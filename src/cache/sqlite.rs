use super::models::{DbItem, DbList};
use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::str::FromStr;
use tracing::{debug, info};

pub struct SqliteCache {
    pool: SqlitePool,
}

impl SqliteCache {
    /// Create a new SQLite cache and initialize the database
    pub async fn new(database_path: &str) -> Result<Self> {
        // Check if database file already exists
        let db_exists = std::path::Path::new(database_path).exists();

        if db_exists {
            info!("Using existing database at: {}", database_path);
        } else {
            info!("Creating new database at: {}", database_path);
        }

        let options = SqliteConnectOptions::from_str(database_path)?
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await
            .context("Failed to connect to SQLite database")?;

        let cache = Self { pool };
        cache.run_migrations().await?;

        // Log cache statistics if database existed
        if db_exists {
            let stats = cache.get_stats().await?;
            info!(
                "Cache loaded: {} lists with {} total items",
                stats.total_lists, stats.total_items
            );
        } else {
            info!("New database initialized successfully");
        }

        Ok(cache)
    }

    /// Run database migrations to create tables
    async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations");

        // Create lists table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS lists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                last_updated INTEGER NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create lists table")?;

        // Create items table
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS items (
                id TEXT PRIMARY KEY,
                list_id TEXT NOT NULL,
                name TEXT NOT NULL,
                details TEXT NOT NULL,
                quantity TEXT,
                category TEXT,
                is_checked BOOLEAN NOT NULL,
                last_seen INTEGER NOT NULL,
                FOREIGN KEY (list_id) REFERENCES lists(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create items table")?;

        // Create index on list_id for faster lookups
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_items_list_id ON items(list_id)
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create index on items")?;

        info!("Database migrations completed");
        Ok(())
    }

    /// Get a cached list by ID
    pub async fn get_list(&self, list_id: &str) -> Result<Option<DbList>> {
        let list = sqlx::query_as::<_, DbList>(
            "SELECT id, name, last_updated FROM lists WHERE id = ?",
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch list from cache")?;

        Ok(list)
    }

    /// Get all cached items for a list
    pub async fn get_items(&self, list_id: &str) -> Result<Vec<DbItem>> {
        let items = sqlx::query_as::<_, DbItem>(
            "SELECT id, list_id, name, details, quantity, category, is_checked, last_seen FROM items WHERE list_id = ?",
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch items from cache")?;

        Ok(items)
    }

    /// Get all cached lists
    pub async fn get_all_lists(&self) -> Result<Vec<DbList>> {
        let lists = sqlx::query_as::<_, DbList>(
            "SELECT id, name, last_updated FROM lists ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch all lists from cache")?;

        Ok(lists)
    }

    /// Upsert a list (insert or update)
    pub async fn upsert_list(&self, list: &DbList) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO lists (id, name, last_updated)
            VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                last_updated = excluded.last_updated
            "#,
        )
        .bind(&list.id)
        .bind(&list.name)
        .bind(list.last_updated)
        .execute(&self.pool)
        .await
        .context("Failed to upsert list")?;

        debug!("Upserted list: {} ({})", list.name, list.id);
        Ok(())
    }

    /// Upsert an item (insert or update)
    pub async fn upsert_item(&self, item: &DbItem) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO items (id, list_id, name, details, quantity, category, is_checked, last_seen)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                list_id = excluded.list_id,
                name = excluded.name,
                details = excluded.details,
                quantity = excluded.quantity,
                category = excluded.category,
                is_checked = excluded.is_checked,
                last_seen = excluded.last_seen
            "#,
        )
        .bind(&item.id)
        .bind(&item.list_id)
        .bind(&item.name)
        .bind(&item.details)
        .bind(&item.quantity)
        .bind(&item.category)
        .bind(item.is_checked)
        .bind(item.last_seen)
        .execute(&self.pool)
        .await
        .context("Failed to upsert item")?;

        debug!("Upserted item: {} in list {}", item.name, item.list_id);
        Ok(())
    }

    /// Sync a complete list with the cache
    /// This will upsert the list and all its items, and mark items as seen
    pub async fn sync_list(&self, list: &anylist_rs::List) -> Result<()> {
        let db_list = DbList::from(list);
        self.upsert_list(&db_list).await?;

        for item in &list.items {
            let db_item = DbItem::from(item);
            self.upsert_item(&db_item).await?;
        }

        debug!("Synced list: {} ({} items)", list.name, list.items.len());
        Ok(())
    }

    /// Delete items that haven't been seen since the given timestamp
    /// This is used to detect removed items
    pub async fn delete_stale_items(&self, list_id: &str, since: i64) -> Result<Vec<DbItem>> {
        // First, fetch the items that will be deleted
        let stale_items = sqlx::query_as::<_, DbItem>(
            "SELECT id, list_id, name, details, quantity, category, is_checked, last_seen FROM items WHERE list_id = ? AND last_seen < ?",
        )
        .bind(list_id)
        .bind(since)
        .fetch_all(&self.pool)
        .await
        .context("Failed to fetch stale items")?;

        // Then delete them
        if !stale_items.is_empty() {
            sqlx::query("DELETE FROM items WHERE list_id = ? AND last_seen < ?")
                .bind(list_id)
                .bind(since)
                .execute(&self.pool)
                .await
                .context("Failed to delete stale items")?;

            debug!(
                "Deleted {} stale items from list {}",
                stale_items.len(),
                list_id
            );
        }

        Ok(stale_items)
    }

    /// Delete a list and all its items
    pub async fn delete_list(&self, list_id: &str) -> Result<()> {
        // Due to FOREIGN KEY constraint with ON DELETE CASCADE,
        // deleting the list will automatically delete all items
        sqlx::query("DELETE FROM lists WHERE id = ?")
            .bind(list_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete list")?;

        debug!("Deleted list: {}", list_id);
        Ok(())
    }

    /// Get the current timestamp for marking items as seen
    pub fn current_timestamp() -> i64 {
        Utc::now().timestamp()
    }

    /// Get cache statistics
    pub async fn get_stats(&self) -> Result<CacheStats> {
        let total_lists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM lists")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count lists")?;

        let total_items: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM items")
            .fetch_one(&self.pool)
            .await
            .context("Failed to count items")?;

        Ok(CacheStats {
            total_lists: total_lists as usize,
            total_items: total_items as usize,
        })
    }
}

/// Cache statistics
pub struct CacheStats {
    pub total_lists: usize,
    pub total_items: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_operations() {
        // Use in-memory database for testing
        let cache = SqliteCache::new("sqlite::memory:")
            .await
            .expect("Failed to create cache");

        // Test list operations
        let list = DbList::new("test-list-1".to_string(), "Test List".to_string());
        cache.upsert_list(&list).await.expect("Failed to upsert list");

        let fetched = cache
            .get_list("test-list-1")
            .await
            .expect("Failed to get list")
            .expect("List not found");
        assert_eq!(fetched.name, "Test List");

        // Test item operations
        let item = DbItem::new(
            "item-1".to_string(),
            "test-list-1".to_string(),
            "Milk".to_string(),
            "Whole milk".to_string(),
            Some("1 gallon".to_string()),
            Some("Dairy".to_string()),
            false,
        );
        cache.upsert_item(&item).await.expect("Failed to upsert item");

        let items = cache
            .get_items("test-list-1")
            .await
            .expect("Failed to get items");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Milk");
    }
}
