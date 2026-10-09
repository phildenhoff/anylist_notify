use super::models::{DbItem, DbList};
use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use sqlx::SqliteConnection;
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
                user_id TEXT,
                last_seen INTEGER NOT NULL,
                FOREIGN KEY (list_id) REFERENCES lists(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .context("Failed to create items table")?;

        // Migration: Add user_id column to existing databases
        // This will silently fail if the column already exists, which is fine
        let _ = sqlx::query("ALTER TABLE items ADD COLUMN user_id TEXT")
            .execute(&self.pool)
            .await;

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
            "SELECT id, list_id, name, details, quantity, category, is_checked, user_id, last_seen FROM items WHERE list_id = ?",
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
        let mut connection = self.pool.acquire().await?;
        Self::write_item(&mut connection, item).await
    }

    async fn write_item(connection: &mut SqliteConnection, item: &DbItem) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO items (id, list_id, name, details, quantity, category, is_checked, user_id, last_seen)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                list_id = excluded.list_id,
                name = excluded.name,
                details = excluded.details,
                quantity = excluded.quantity,
                category = excluded.category,
                is_checked = excluded.is_checked,
                user_id = excluded.user_id,
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
        .bind(&item.user_id)
        .bind(item.last_seen)
        .execute(connection)
        .await
        .context("Failed to upsert item")?;

        debug!("Upserted item: {} in list {}", item.name, item.list_id);
        Ok(())
    }

    /// Replace a list's cached snapshot atomically, including removals.
    pub async fn sync_list(&self, list: &anylist_rs::List) -> Result<()> {
        let mut transaction = self.pool.begin().await.context("Failed to begin cache sync")?;
        let db_list = DbList::from(list);
        sqlx::query(
            r#"
            INSERT INTO lists (id, name, last_updated) VALUES (?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name, last_updated = excluded.last_updated
            "#,
        )
        .bind(&db_list.id)
        .bind(&db_list.name)
        .bind(db_list.last_updated)
        .execute(&mut *transaction)
        .await
        .context("Failed to upsert list")?;

        // A full snapshot is authoritative. Do not use second-resolution last_seen:
        // two syncs in the same second must still remove absent items.
        sqlx::query("DELETE FROM items WHERE list_id = ?")
            .bind(&list.id)
            .execute(&mut *transaction)
            .await
            .context("Failed to replace cached items")?;
        for item in &list.items {
            Self::write_item(&mut transaction, &DbItem::from(item)).await?;
        }
        transaction.commit().await.context("Failed to commit cache sync")?;

        debug!("Synced list: {} ({} items)", list.name, list.items.len());
        Ok(())
    }

    /// Delete items that haven't been seen since the given timestamp
    /// This is used to detect removed items
    pub async fn delete_stale_items(&self, list_id: &str, since: i64) -> Result<Vec<DbItem>> {
        // First, fetch the items that will be deleted
        let stale_items = sqlx::query_as::<_, DbItem>(
            "SELECT id, list_id, name, details, quantity, category, is_checked, user_id, last_seen FROM items WHERE list_id = ? AND last_seen < ?",
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

    fn snapshot(items: Vec<anylist_rs::ListItem>) -> anylist_rs::List {
        anylist_rs::List {
            id: "list-1".into(), name: "Groceries".into(), items,
            shared_users: vec![],
        }
    }

    fn item(id: &str) -> anylist_rs::ListItem {
        anylist_rs::ListItem {
            id: id.into(), list_id: "list-1".into(), name: id.into(),
            details: String::new(), quantity: None, category: None,
            is_checked: false, user_id: None,
        }
    }

    #[tokio::test]
    async fn removed_items_do_not_repeat_and_empty_lists_are_cleared() {
        let cache = SqliteCache::new("sqlite::memory:").await.unwrap();
        cache.sync_list(&snapshot(vec![item("removed"), item("kept")])).await.unwrap();
        let current = snapshot(vec![item("kept")]);
        let before = cache.get_items("list-1").await.unwrap();
        assert_eq!(crate::sync::diff::detect_changes("list-1", "Groceries", &before, &current.items).len(), 1);
        cache.sync_list(&current).await.unwrap();
        let after = cache.get_items("list-1").await.unwrap();
        assert_eq!(after.len(), 1);
        assert!(crate::sync::diff::detect_changes("list-1", "Groceries", &after, &current.items).is_empty());
        cache.sync_list(&current).await.unwrap();
        cache.sync_list(&snapshot(vec![])).await.unwrap();
        assert!(cache.get_items("list-1").await.unwrap().is_empty());
        assert!(cache.get_list("list-1").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn failed_snapshot_rolls_back_and_other_lists_are_untouched() {
        let cache = SqliteCache::new("sqlite::memory:").await.unwrap();
        cache.sync_list(&snapshot(vec![item("original")])).await.unwrap();
        let mut other = snapshot(vec![]);
        other.id = "other-list".into();
        let mut other_item = item("other-item");
        other_item.list_id = other.id.clone();
        other.items.push(other_item);
        cache.sync_list(&other).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_bad_item BEFORE INSERT ON items WHEN NEW.id = 'bad' BEGIN SELECT RAISE(ABORT, 'test failure'); END")
            .execute(&cache.pool).await.unwrap();
        let mut bad = snapshot(vec![item("new"), item("bad")]);
        bad.name = "Changed name".into();
        assert!(cache.sync_list(&bad).await.is_err());
        let cached = cache.get_items("list-1").await.unwrap();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].id, "original");
        assert_eq!(cache.get_list("list-1").await.unwrap().unwrap().name, "Groceries");
        assert_eq!(cache.get_items("other-list").await.unwrap()[0].id, "other-item");
    }

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
            Some("test-user-id".to_string()),
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
