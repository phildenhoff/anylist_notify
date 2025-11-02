mod cache;
mod config;
mod notify;
mod sync;

use anyhow::{Context, Result};
use anylist_rs::AnyListClient;
use cache::SqliteCache;
use config::Config;
use notify::NtfyClient;
use sync::SyncHandler;
use std::sync::Arc;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    info!("Starting AnyList Notification Service");

    // Load configuration
    let config = Config::load().context("Failed to load configuration")?;
    config.validate().context("Invalid configuration")?;

    info!("Configuration loaded successfully");
    info!("ntfy topic: {}", config.ntfy.topic);
    info!("Database path: {}", config.cache.database_path);

    // Initialize SQLite cache
    let cache = SqliteCache::new(&config.cache.database_path)
        .await
        .context("Failed to initialize cache")?;
    let cache = Arc::new(cache);

    info!("Cache initialized");

    // Authenticate with AnyList
    info!("Authenticating with AnyList...");
    let client = AnyListClient::login(&config.anylist.email, &config.anylist.password)
        .await
        .context("Failed to authenticate with AnyList")?;
    let client = Arc::new(client);

    info!("Authenticated successfully");

    // Initialize ntfy client
    let notifier = Arc::new(NtfyClient::new(config.ntfy.clone()));

    // Create sync handler
    let handler = Arc::new(SyncHandler::new(
        client.clone(),
        cache.clone(),
        notifier.clone(),
    ));

    // Initialize cache with current state
    info!("Fetching initial list state...");
    handler
        .initialize_cache()
        .await
        .context("Failed to initialize cache")?;

    info!("Cache initialized with current list state");

    // Set up WebSocket event handler
    let handler_clone = handler.clone();
    let event_callback = move |event| {
        let handler = handler_clone.clone();
        tokio::spawn(async move {
            if let Err(e) = handler.handle_event(event).await {
                error!("Error handling event: {}", e);
            }
        });
    };

    // Start real-time sync
    info!("Connecting to AnyList WebSocket...");
    let mut sync = client
        .start_realtime_sync(event_callback)
        .await
        .context("Failed to start real-time sync")?;

    info!("WebSocket connected - monitoring for changes");
    info!("Press Ctrl+C to stop");

    // Wait for Ctrl+C
    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            info!("Received shutdown signal");
        }
        Err(err) => {
            error!("Unable to listen for shutdown signal: {}", err);
        }
    }

    // Gracefully disconnect
    info!("Disconnecting...");
    sync.disconnect()
        .await
        .context("Failed to disconnect gracefully")?;

    info!("Service stopped");
    Ok(())
}
