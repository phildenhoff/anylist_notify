use crate::config::NtfyConfig;
use crate::sync::diff::{FieldChange, ListChange};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;
use tracing::{debug, error, info};

pub struct NtfyClient {
    client: Client,
    config: NtfyConfig,
}

#[derive(Debug, Serialize)]
struct NtfyMessage {
    topic: String,
    title: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    priority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
}

impl NtfyClient {
    pub fn new(config: NtfyConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    /// Send a notification for a list change
    pub async fn notify(&self, change: &ListChange) -> Result<()> {
        let (title, message, priority, tags) = self.format_notification(change);

        let ntfy_msg = NtfyMessage {
            topic: self.config.topic.clone(),
            title,
            message,
            priority: Some(priority),
            tags: Some(tags),
        };

        self.send_message(&ntfy_msg).await
    }

    /// Send the actual HTTP request to ntfy.sh
    async fn send_message(&self, message: &NtfyMessage) -> Result<()> {
        let url = format!("{}/{}", self.config.base_url, message.topic);

        debug!("Sending notification to ntfy: {}", message.title);

        let response = self
            .client
            .post(&url)
            .header("Title", &message.title)
            .header("Priority", message.priority.as_deref().unwrap_or("default"))
            .header(
                "Tags",
                message
                    .tags
                    .as_ref()
                    .map(|t| t.join(","))
                    .unwrap_or_default(),
            )
            .body(message.message.clone())
            .send()
            .await
            .context("Failed to send notification to ntfy.sh")?;

        if response.status().is_success() {
            info!("Notification sent: {}", message.title);
            Ok(())
        } else {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            error!("Failed to send notification: {} - {}", status, body);
            anyhow::bail!("ntfy.sh returned error: {} - {}", status, body)
        }
    }

    /// Format a list change into notification components
    fn format_notification(&self, change: &ListChange) -> (String, String, String, Vec<String>) {
        match change {
            ListChange::ItemAdded {
                list_name,
                item,
                user_id,
                ..
            } => {
                let title = format!("➕ {} added to {}", item.name, list_name);
                let mut message_parts = Vec::new();

                if let Some(quantity) = &item.quantity {
                    message_parts.push(format!("Quantity: {}", quantity));
                }
                if !item.details.is_empty() {
                    message_parts.push(format!("Details: {}", item.details));
                }
                if let Some(category) = &item.category {
                    message_parts.push(format!("Category: {}", category));
                }
                if let Some(uid) = user_id {
                    message_parts.push(format!("Changed by: {}", format_user_id(uid)));
                }

                let message = if message_parts.is_empty() {
                    format!("Added to {}", list_name)
                } else {
                    message_parts.join("\n")
                };

                let priority = self.config.priorities.item_added.clone();
                let tags = parse_tags(&self.config.tags.item_added);

                (title, message, priority, tags)
            }

            ListChange::ItemRemoved {
                list_name,
                item_name,
                user_id,
                ..
            } => {
                let title = format!("❌ {} removed from {}", item_name, list_name);
                let mut message = format!("Removed from {}", list_name);
                if let Some(uid) = user_id {
                    message.push_str(&format!("\nChanged by: {}", format_user_id(uid)));
                }
                let priority = self.config.priorities.item_removed.clone();
                let tags = parse_tags(&self.config.tags.item_removed);

                (title, message, priority, tags)
            }

            ListChange::ItemChecked {
                list_name,
                item_name,
                user_id,
                ..
            } => {
                let title = format!("✅ {} checked off in {}", item_name, list_name);
                let mut message = format!("Checked off in {}", list_name);
                if let Some(uid) = user_id {
                    message.push_str(&format!("\nChanged by: {}", format_user_id(uid)));
                }
                let priority = self.config.priorities.item_checked.clone();
                let tags = parse_tags(&self.config.tags.item_checked);

                (title, message, priority, tags)
            }

            ListChange::ItemUnchecked {
                list_name,
                item_name,
                user_id,
                ..
            } => {
                let title = format!("◀️ {} unchecked in {}", item_name, list_name);
                let mut message = format!("Unchecked in {}", list_name);
                if let Some(uid) = user_id {
                    message.push_str(&format!("\nChanged by: {}", format_user_id(uid)));
                }
                let priority = self.config.priorities.item_unchecked.clone();
                let tags = parse_tags(&self.config.tags.item_unchecked);

                (title, message, priority, tags)
            }

            ListChange::ItemModified {
                list_name,
                item_name,
                changes,
                user_id,
                ..
            } => {
                let title = format!("✏️ {} modified in {}", item_name, list_name);
                let mut message = format_field_changes(changes);
                if let Some(uid) = user_id {
                    message.push_str(&format!("\nChanged by: {}", format_user_id(uid)));
                }
                let priority = self.config.priorities.item_modified.clone();
                let tags = parse_tags(&self.config.tags.item_modified);

                (title, message, priority, tags)
            }
        }
    }
}

/// Format a user ID into a more readable string
/// Currently just returns the ID, but could be extended to look up user names
fn format_user_id(user_id: &str) -> String {
    // For now, just return the user ID
    // In the future, this could map user IDs to friendly names
    user_id.to_string()
}

/// Parse comma-separated tags into a vector
fn parse_tags(tags: &str) -> Vec<String> {
    tags.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Format field changes into a readable message
fn format_field_changes(changes: &[FieldChange]) -> String {
    let mut parts = Vec::new();

    for change in changes {
        match change {
            FieldChange::Name { old, new } => {
                parts.push(format!("Name: {} → {}", old, new));
            }
            FieldChange::Details { old, new } => {
                if old.is_empty() {
                    parts.push(format!("Details added: {}", new));
                } else if new.is_empty() {
                    parts.push(format!("Details removed: {}", old));
                } else {
                    parts.push(format!("Details: {} → {}", old, new));
                }
            }
            FieldChange::Quantity { old, new } => {
                let old_str = old.as_deref().unwrap_or("none");
                let new_str = new.as_deref().unwrap_or("none");
                parts.push(format!("Quantity: {} → {}", old_str, new_str));
            }
            FieldChange::Category { old, new } => {
                let old_str = old.as_deref().unwrap_or("none");
                let new_str = new.as_deref().unwrap_or("none");
                parts.push(format!("Category: {} → {}", old_str, new_str));
            }
        }
    }

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::diff::ItemInfo;

    #[test]
    fn test_parse_tags() {
        let tags = parse_tags("tag1,tag2,tag3");
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);

        let tags = parse_tags("tag1, tag2 , tag3");
        assert_eq!(tags, vec!["tag1", "tag2", "tag3"]);

        let tags = parse_tags("");
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_format_field_changes() {
        let changes = vec![
            FieldChange::Quantity {
                old: Some("1".to_string()),
                new: Some("2".to_string()),
            },
            FieldChange::Category {
                old: None,
                new: Some("Dairy".to_string()),
            },
        ];

        let message = format_field_changes(&changes);
        assert!(message.contains("Quantity: 1 → 2"));
        assert!(message.contains("Category: none → Dairy"));
    }

    #[test]
    fn test_format_added_notification() {
        use crate::config::{NtfyPriorities, NtfyTags};

        let config = NtfyConfig {
            base_url: "https://ntfy.sh".to_string(),
            topic: "test".to_string(),
            priorities: NtfyPriorities::default(),
            tags: NtfyTags::default(),
        };

        let client = NtfyClient::new(config);

        let change = ListChange::ItemAdded {
            list_id: "list-1".to_string(),
            list_name: "Groceries".to_string(),
            item: ItemInfo {
                id: "item-1".to_string(),
                name: "Milk".to_string(),
                details: "Whole milk".to_string(),
                quantity: Some("1 gallon".to_string()),
                category: Some("Dairy".to_string()),
            },
        };

        let (title, message, priority, tags) = client.format_notification(&change);

        assert!(title.contains("Milk"));
        assert!(title.contains("Groceries"));
        assert!(message.contains("Quantity: 1 gallon"));
        assert!(message.contains("Details: Whole milk"));
        assert!(message.contains("Category: Dairy"));
        assert_eq!(priority, "default");
        assert!(!tags.is_empty());
    }
}
