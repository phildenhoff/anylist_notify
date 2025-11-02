use chrono::{DateTime, Utc};
use sqlx::FromRow;

/// Database representation of a shopping list
#[derive(Debug, Clone, FromRow)]
pub struct DbList {
    pub id: String,
    pub name: String,
    pub last_updated: i64, // Unix timestamp
}

/// Database representation of a list item
#[derive(Debug, Clone, FromRow)]
pub struct DbItem {
    pub id: String,
    pub list_id: String,
    pub name: String,
    pub details: String,
    pub quantity: Option<String>,
    pub category: Option<String>,
    pub is_checked: bool,
    pub last_seen: i64, // Unix timestamp
}

impl DbList {
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            last_updated: Utc::now().timestamp(),
        }
    }

    pub fn last_updated_datetime(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.last_updated, 0).unwrap_or_default()
    }
}

impl DbItem {
    pub fn new(
        id: String,
        list_id: String,
        name: String,
        details: String,
        quantity: Option<String>,
        category: Option<String>,
        is_checked: bool,
    ) -> Self {
        Self {
            id,
            list_id,
            name,
            details,
            quantity,
            category,
            is_checked,
            last_seen: Utc::now().timestamp(),
        }
    }

    pub fn last_seen_datetime(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.last_seen, 0).unwrap_or_default()
    }
}

/// Convert anylist_rs::ListItem to DbItem
impl From<&anylist_rs::ListItem> for DbItem {
    fn from(item: &anylist_rs::ListItem) -> Self {
        DbItem::new(
            item.id.clone(),
            item.list_id.clone(),
            item.name.clone(),
            item.details.clone(),
            item.quantity.clone(),
            item.category.clone(),
            item.is_checked,
        )
    }
}

/// Convert anylist_rs::List to DbList
impl From<&anylist_rs::List> for DbList {
    fn from(list: &anylist_rs::List) -> Self {
        DbList::new(list.id.clone(), list.name.clone())
    }
}
