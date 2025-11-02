use crate::cache::DbItem;
use anylist_rs::ListItem;
use std::collections::HashMap;

/// Represents a change detected between cached and current list state
#[derive(Debug, Clone, PartialEq)]
pub enum ListChange {
    /// An item was added to the list
    ItemAdded {
        list_id: String,
        list_name: String,
        item: ItemInfo,
        user_id: Option<String>,
    },
    /// An item was removed from the list
    ItemRemoved {
        list_id: String,
        list_name: String,
        item_name: String,
        user_id: Option<String>,
    },
    /// An item was checked off
    ItemChecked {
        list_id: String,
        list_name: String,
        item_name: String,
        user_id: Option<String>,
    },
    /// An item was unchecked
    ItemUnchecked {
        list_id: String,
        list_name: String,
        item_name: String,
        user_id: Option<String>,
    },
    /// An item's fields were modified
    ItemModified {
        list_id: String,
        list_name: String,
        item_name: String,
        changes: Vec<FieldChange>,
        user_id: Option<String>,
    },
}

/// Information about a list item
#[derive(Debug, Clone, PartialEq)]
pub struct ItemInfo {
    pub id: String,
    pub name: String,
    pub details: String,
    pub quantity: Option<String>,
    pub category: Option<String>,
    pub user_id: Option<String>,
}

/// Represents a change to a specific field
#[derive(Debug, Clone, PartialEq)]
pub enum FieldChange {
    Name { old: String, new: String },
    Details { old: String, new: String },
    Quantity { old: Option<String>, new: Option<String> },
    Category { old: Option<String>, new: Option<String> },
}

impl ItemInfo {
    pub fn from_list_item(item: &ListItem) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            details: item.details.clone(),
            quantity: item.quantity.clone(),
            category: item.category.clone(),
            user_id: item.user_id.clone(),
        }
    }

    pub fn from_db_item(item: &DbItem) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            details: item.details.clone(),
            quantity: item.quantity.clone(),
            category: item.category.clone(),
            user_id: item.user_id.clone(),
        }
    }
}

/// Detect changes between cached items and current items
pub fn detect_changes(
    list_id: &str,
    list_name: &str,
    cached_items: &[DbItem],
    current_items: &[ListItem],
) -> Vec<ListChange> {
    let mut changes = Vec::new();

    // Create lookup maps by item ID
    let cached_map: HashMap<&str, &DbItem> =
        cached_items.iter().map(|item| (item.id.as_str(), item)).collect();
    let current_map: HashMap<&str, &ListItem> =
        current_items.iter().map(|item| (item.id.as_str(), item)).collect();

    // Detect added items (in current but not in cached)
    for current_item in current_items {
        if !cached_map.contains_key(current_item.id.as_str()) {
            changes.push(ListChange::ItemAdded {
                list_id: list_id.to_string(),
                list_name: list_name.to_string(),
                item: ItemInfo::from_list_item(current_item),
                user_id: current_item.user_id.clone(),
            });
        }
    }

    // Detect removed items (in cached but not in current)
    for cached_item in cached_items {
        if !current_map.contains_key(cached_item.id.as_str()) {
            changes.push(ListChange::ItemRemoved {
                list_id: list_id.to_string(),
                list_name: list_name.to_string(),
                item_name: cached_item.name.clone(),
                user_id: cached_item.user_id.clone(),
            });
        }
    }

    // Detect modifications (items in both, but with different values)
    for current_item in current_items {
        if let Some(cached_item) = cached_map.get(current_item.id.as_str()) {
            // Check for check state changes
            if cached_item.is_checked != current_item.is_checked {
                if current_item.is_checked {
                    changes.push(ListChange::ItemChecked {
                        list_id: list_id.to_string(),
                        list_name: list_name.to_string(),
                        item_name: current_item.name.clone(),
                        user_id: current_item.user_id.clone(),
                    });
                } else {
                    changes.push(ListChange::ItemUnchecked {
                        list_id: list_id.to_string(),
                        list_name: list_name.to_string(),
                        item_name: current_item.name.clone(),
                        user_id: current_item.user_id.clone(),
                    });
                }
            }

            // Check for field changes
            let field_changes = detect_field_changes(cached_item, current_item);
            if !field_changes.is_empty() {
                changes.push(ListChange::ItemModified {
                    list_id: list_id.to_string(),
                    list_name: list_name.to_string(),
                    item_name: current_item.name.clone(),
                    changes: field_changes,
                    user_id: current_item.user_id.clone(),
                });
            }
        }
    }

    changes
}

/// Detect changes to specific fields
fn detect_field_changes(cached: &DbItem, current: &ListItem) -> Vec<FieldChange> {
    let mut changes = Vec::new();

    if cached.name != current.name {
        changes.push(FieldChange::Name {
            old: cached.name.clone(),
            new: current.name.clone(),
        });
    }

    if cached.details != current.details {
        changes.push(FieldChange::Details {
            old: cached.details.clone(),
            new: current.details.clone(),
        });
    }

    if cached.quantity != current.quantity {
        changes.push(FieldChange::Quantity {
            old: cached.quantity.clone(),
            new: current.quantity.clone(),
        });
    }

    if cached.category != current.category {
        changes.push(FieldChange::Category {
            old: cached.category.clone(),
            new: current.category.clone(),
        });
    }

    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_db_item(id: &str, name: &str, is_checked: bool) -> DbItem {
        DbItem {
            id: id.to_string(),
            list_id: "list-1".to_string(),
            name: name.to_string(),
            details: "".to_string(),
            quantity: None,
            category: None,
            is_checked,
            user_id: Some("test-user".to_string()),
            last_seen: 0,
        }
    }

    fn create_list_item(id: &str, name: &str, is_checked: bool) -> ListItem {
        ListItem {
            id: id.to_string(),
            list_id: "list-1".to_string(),
            name: name.to_string(),
            details: "".to_string(),
            quantity: None,
            category: None,
            is_checked,
            user_id: Some("test-user".to_string()),
        }
    }

    #[test]
    fn test_detect_added_item() {
        let cached = vec![];
        let current = vec![create_list_item("item-1", "Milk", false)];

        let changes = detect_changes("list-1", "Groceries", &cached, &current);

        assert_eq!(changes.len(), 1);
        match &changes[0] {
            ListChange::ItemAdded { item, .. } => {
                assert_eq!(item.name, "Milk");
            }
            _ => panic!("Expected ItemAdded"),
        }
    }

    #[test]
    fn test_detect_removed_item() {
        let cached = vec![create_db_item("item-1", "Milk", false)];
        let current = vec![];

        let changes = detect_changes("list-1", "Groceries", &cached, &current);

        assert_eq!(changes.len(), 1);
        match &changes[0] {
            ListChange::ItemRemoved { item_name, .. } => {
                assert_eq!(item_name, "Milk");
            }
            _ => panic!("Expected ItemRemoved"),
        }
    }

    #[test]
    fn test_detect_checked_item() {
        let cached = vec![create_db_item("item-1", "Milk", false)];
        let current = vec![create_list_item("item-1", "Milk", true)];

        let changes = detect_changes("list-1", "Groceries", &cached, &current);

        assert_eq!(changes.len(), 1);
        match &changes[0] {
            ListChange::ItemChecked { item_name, .. } => {
                assert_eq!(item_name, "Milk");
            }
            _ => panic!("Expected ItemChecked"),
        }
    }

    #[test]
    fn test_detect_unchecked_item() {
        let cached = vec![create_db_item("item-1", "Milk", true)];
        let current = vec![create_list_item("item-1", "Milk", false)];

        let changes = detect_changes("list-1", "Groceries", &cached, &current);

        assert_eq!(changes.len(), 1);
        match &changes[0] {
            ListChange::ItemUnchecked { item_name, .. } => {
                assert_eq!(item_name, "Milk");
            }
            _ => panic!("Expected ItemUnchecked"),
        }
    }

    #[test]
    fn test_detect_modified_quantity() {
        let cached = vec![DbItem {
            id: "item-1".to_string(),
            list_id: "list-1".to_string(),
            name: "Milk".to_string(),
            details: "".to_string(),
            quantity: Some("1 gallon".to_string()),
            category: None,
            is_checked: false,
            user_id: Some("test-user".to_string()),
            last_seen: 0,
        }];

        let current = vec![ListItem {
            id: "item-1".to_string(),
            list_id: "list-1".to_string(),
            name: "Milk".to_string(),
            details: "".to_string(),
            quantity: Some("2 gallons".to_string()),
            category: None,
            is_checked: false,
            user_id: Some("test-user".to_string()),
        }];

        let changes = detect_changes("list-1", "Groceries", &cached, &current);

        assert_eq!(changes.len(), 1);
        match &changes[0] {
            ListChange::ItemModified { changes, .. } => {
                assert_eq!(changes.len(), 1);
                match &changes[0] {
                    FieldChange::Quantity { old, new } => {
                        assert_eq!(old, &Some("1 gallon".to_string()));
                        assert_eq!(new, &Some("2 gallons".to_string()));
                    }
                    _ => panic!("Expected Quantity change"),
                }
            }
            _ => panic!("Expected ItemModified"),
        }
    }

    #[test]
    fn test_no_changes() {
        let cached = vec![create_db_item("item-1", "Milk", false)];
        let current = vec![create_list_item("item-1", "Milk", false)];

        let changes = detect_changes("list-1", "Groceries", &cached, &current);

        assert_eq!(changes.len(), 0);
    }

    #[test]
    fn test_multiple_changes() {
        let cached = vec![
            create_db_item("item-1", "Milk", false),
            create_db_item("item-2", "Bread", false),
        ];

        let current = vec![
            create_list_item("item-1", "Milk", true), // checked
            create_list_item("item-3", "Eggs", false), // added
            // item-2 removed
        ];

        let changes = detect_changes("list-1", "Groceries", &cached, &current);

        assert_eq!(changes.len(), 3);
        // Should have: ItemAdded, ItemRemoved, ItemChecked
    }
}
