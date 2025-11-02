use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub anylist: AnyListConfig,
    pub cache: CacheConfig,
    pub ntfy: NtfyConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AnyListConfig {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CacheConfig {
    pub database_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NtfyConfig {
    pub base_url: String,
    pub topic: String,
    #[serde(default)]
    pub priorities: NtfyPriorities,
    #[serde(default)]
    pub tags: NtfyTags,
}

#[derive(Debug, Deserialize, Clone)]
pub struct NtfyPriorities {
    #[serde(default = "default_priority")]
    pub item_added: String,
    #[serde(default = "low_priority")]
    pub item_checked: String,
    #[serde(default = "default_priority")]
    pub item_unchecked: String,
    #[serde(default = "default_priority")]
    pub item_removed: String,
    #[serde(default = "default_priority")]
    pub item_modified: String,
}

impl Default for NtfyPriorities {
    fn default() -> Self {
        Self {
            item_added: default_priority(),
            item_checked: low_priority(),
            item_unchecked: default_priority(),
            item_removed: default_priority(),
            item_modified: default_priority(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct NtfyTags {
    #[serde(default = "default_added_tags")]
    pub item_added: String,
    #[serde(default = "default_checked_tags")]
    pub item_checked: String,
    #[serde(default = "default_unchecked_tags")]
    pub item_unchecked: String,
    #[serde(default = "default_removed_tags")]
    pub item_removed: String,
    #[serde(default = "default_modified_tags")]
    pub item_modified: String,
}

impl Default for NtfyTags {
    fn default() -> Self {
        Self {
            item_added: default_added_tags(),
            item_checked: default_checked_tags(),
            item_unchecked: default_unchecked_tags(),
            item_removed: default_removed_tags(),
            item_modified: default_modified_tags(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct LoggingConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
}

// Default value functions
fn default_priority() -> String {
    "default".to_string()
}

fn low_priority() -> String {
    "low".to_string()
}

fn default_added_tags() -> String {
    "heavy_plus_sign,shopping_cart".to_string()
}

fn default_checked_tags() -> String {
    "white_check_mark".to_string()
}

fn default_unchecked_tags() -> String {
    "arrow_backward".to_string()
}

fn default_removed_tags() -> String {
    "x,shopping_cart".to_string()
}

fn default_modified_tags() -> String {
    "pencil2".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Config {
    /// Load configuration from file and environment variables
    /// Environment variables override config file values
    pub fn load() -> Result<Self> {
        // Load .env file if it exists (doesn't error if missing)
        dotenvy::dotenv().ok();

        let mut builder = config::Config::builder()
            // Start with default values
            .set_default("cache.database_path", "./anylist.db")?
            .set_default("ntfy.base_url", "https://ntfy.sh")?
            .set_default("logging.level", "info")?;

        // Try to load config.toml if it exists
        if let Ok(config_path) = std::env::current_dir() {
            let config_file = config_path.join("config.toml");
            if config_file.exists() {
                builder = builder.add_source(config::File::from(config_file));
            }
        }

        // Manually set values from environment variables
        // This is more explicit and reliable than using Environment source
        if let Ok(email) = std::env::var("ANYLIST_EMAIL") {
            builder = builder.set_override("anylist.email", email)?;
        }
        if let Ok(password) = std::env::var("ANYLIST_PASSWORD") {
            builder = builder.set_override("anylist.password", password)?;
        }
        if let Ok(url) = std::env::var("NTFY_URL") {
            builder = builder.set_override("ntfy.base_url", url)?;
        }
        if let Ok(topic) = std::env::var("NTFY_TOPIC") {
            builder = builder.set_override("ntfy.topic", topic)?;
        }
        if let Ok(db_path) = std::env::var("DATABASE_PATH") {
            builder = builder.set_override("cache.database_path", db_path)?;
        }
        if let Ok(log_level) = std::env::var("RUST_LOG") {
            builder = builder.set_override("logging.level", log_level)?;
        }

        let config = builder
            .build()
            .context("Failed to build configuration")?;

        config
            .try_deserialize()
            .context("Failed to deserialize configuration")
    }

    /// Validate that required fields are present
    pub fn validate(&self) -> Result<()> {
        if self.anylist.email.is_empty() {
            anyhow::bail!("AnyList email is required");
        }
        if self.anylist.password.is_empty() {
            anyhow::bail!("AnyList password is required");
        }
        if self.ntfy.topic.is_empty() {
            anyhow::bail!("ntfy topic is required");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_priorities() {
        let priorities = NtfyPriorities::default();
        assert_eq!(priorities.item_added, "default");
        assert_eq!(priorities.item_checked, "low");
    }

    #[test]
    fn test_default_tags() {
        let tags = NtfyTags::default();
        assert_eq!(tags.item_added, "heavy_plus_sign,shopping_cart");
        assert_eq!(tags.item_checked, "white_check_mark");
    }
}
