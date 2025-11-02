use crate::cache::SqliteCache;
use crate::config::Config;
use crate::notify::NtfyClient;
use crate::sync::diff::{detect_changes, ListChange};
use anyhow::{Context, Result};
use anylist_rs::{AnyListClient, SyncEvent};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub struct SyncHandler {
    client: Arc<AnyListClient>,
    cache: Arc<SqliteCache>,
    notifier: Arc<NtfyClient>,
    config: Arc<Config>,
    authenticated_user_id: String,
    user_names: Arc<tokio::sync::RwLock<HashMap<String, String>>>,
}

impl SyncHandler {
    pub fn new(
        client: Arc<AnyListClient>,
        cache: Arc<SqliteCache>,
        notifier: Arc<NtfyClient>,
        config: Arc<Config>,
        user_names: Arc<tokio::sync::RwLock<HashMap<String, String>>>,
    ) -> Self {
        let authenticated_user_id = client.user_id();
        Self {
            client,
            cache,
            notifier,
            config,
            authenticated_user_id,
            user_names,
        }
    }

    /// Get the user name for a given user ID
    pub async fn get_user_name(&self, user_id: &str) -> Option<String> {
        let names = self.user_names.read().await;
        names.get(user_id).cloned()
    }

    /// Initialize the cache with current list state
    /// This should be called once at startup before starting WebSocket sync
    pub async fn initialize_cache(&self) -> Result<()> {
        info!("Initializing cache with current list state");

        let lists = self
            .client
            .get_lists()
            .await
            .context("Failed to fetch initial lists")?;

        // Update user names mapping
        self.update_user_names(&lists).await;

        for list in &lists {
            self.cache
                .sync_list(list)
                .await
                .context(format!("Failed to sync list: {}", list.name))?;
            debug!("Cached list: {} ({} items)", list.name, list.items.len());
        }

        info!("Cache initialized with {} lists", lists.len());
        Ok(())
    }

    /// Handle a sync event from the WebSocket
    pub async fn handle_event(&self, event: SyncEvent) -> Result<()> {
        match event {
            SyncEvent::ShoppingListsChanged => {
                info!("Shopping lists changed - processing updates");
                self.handle_shopping_lists_changed().await?;
            }
            SyncEvent::Heartbeat => {
                debug!("Heartbeat received");
            }
            _ => {
                debug!("Ignoring event: {:?}", event);
            }
        }
        Ok(())
    }

    /// Handle shopping list changes by fetching updates and detecting diffs
    async fn handle_shopping_lists_changed(&self) -> Result<()> {
        // Fetch current lists from API
        let current_lists = self
            .client
            .get_lists()
            .await
            .context("Failed to fetch updated lists")?;

        // Update user names mapping from all lists
        self.update_user_names(&current_lists).await;

        // Process each list
        for current_list in &current_lists {
            if let Err(e) = self.process_list_changes(current_list).await {
                error!(
                    "Error processing changes for list {}: {}",
                    current_list.name, e
                );
                // Continue processing other lists even if one fails
            }
        }

        // Check for deleted lists
        self.detect_deleted_lists(&current_lists).await?;

        Ok(())
    }

    /// Update user names mapping from list shared users
    async fn update_user_names(&self, lists: &[anylist_rs::List]) {
        let mut names = self.user_names.write().await;

        for list in lists {
            for user in &list.shared_users {
                // Use full name if available, otherwise email, otherwise user_id
                let display_name = if let Some(ref name) = user.full_name {
                    if !name.is_empty() {
                        name.clone()
                    } else if let Some(ref email) = user.email {
                        email.clone()
                    } else {
                        user.user_id.clone()
                    }
                } else if let Some(ref email) = user.email {
                    email.clone()
                } else {
                    user.user_id.clone()
                };

                names.insert(user.user_id.clone(), display_name);
            }
        }

        debug!("Updated user names mapping with {} users", names.len());
    }

    /// Process changes for a single list
    async fn process_list_changes(&self, current_list: &anylist_rs::List) -> Result<()> {
        debug!("Processing changes for list: {}", current_list.name);

        // Get cached items for this list
        let cached_items = self
            .cache
            .get_items(&current_list.id)
            .await
            .context("Failed to get cached items")?;

        // Detect changes
        let mut changes = detect_changes(
            &current_list.id,
            &current_list.name,
            &cached_items,
            &current_list.items,
        );

        // Filter out own changes if configured
        if self.config.notifications.filter_own_changes {
            let original_count = changes.len();
            changes = self.filter_own_changes(changes);
            let filtered_count = original_count - changes.len();
            if filtered_count > 0 {
                debug!(
                    "Filtered out {} change(s) made by authenticated user in list: {}",
                    filtered_count, current_list.name
                );
            }
        }

        if !changes.is_empty() {
            info!(
                "Detected {} change(s) in list: {}",
                changes.len(),
                current_list.name
            );

            // Send notifications for each change
            for change in &changes {
                debug!("Change detected: {:?}", change);
                if let Err(e) = self.notifier.notify(change).await {
                    error!("Failed to send notification: {}", e);
                    // Continue processing other changes even if notification fails
                }
            }
        } else {
            debug!("No changes detected in list: {}", current_list.name);
        }

        // Update cache with current state
        self.cache
            .sync_list(current_list)
            .await
            .context("Failed to sync list to cache")?;

        Ok(())
    }

    /// Filter out changes made by the authenticated user
    fn filter_own_changes(&self, changes: Vec<ListChange>) -> Vec<ListChange> {
        changes
            .into_iter()
            .filter(|change| {
                let user_id = match change {
                    ListChange::ItemAdded { user_id, .. } => user_id,
                    ListChange::ItemRemoved { user_id, .. } => user_id,
                    ListChange::ItemChecked { user_id, .. } => user_id,
                    ListChange::ItemUnchecked { user_id, .. } => user_id,
                    ListChange::ItemModified { user_id, .. } => user_id,
                };

                // Keep the change if user_id doesn't match authenticated user
                match user_id {
                    Some(id) => id != &self.authenticated_user_id,
                    None => {
                        warn!("Change has no user_id, including in notifications");
                        true // Include changes with no user_id
                    }
                }
            })
            .collect()
    }

    /// Detect lists that have been deleted
    async fn detect_deleted_lists(&self, current_lists: &[anylist_rs::List]) -> Result<()> {
        let cached_lists = self
            .cache
            .get_all_lists()
            .await
            .context("Failed to get cached lists")?;

        let current_ids: std::collections::HashSet<_> =
            current_lists.iter().map(|l| l.id.as_str()).collect();

        for cached_list in cached_lists {
            if !current_ids.contains(cached_list.id.as_str()) {
                info!("List deleted: {} ({})", cached_list.name, cached_list.id);
                self.cache
                    .delete_list(&cached_list.id)
                    .await
                    .context("Failed to delete list from cache")?;

                // Optionally: send a notification about list deletion
                // (not implemented in current design but could be added)
            }
        }

        Ok(())
    }
}
