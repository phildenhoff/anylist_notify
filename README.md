# AnyList Notification Service

A Rust service that monitors your [AnyList](https://www.anylist.com/) shopping lists in real-time and sends detailed notifications via [ntfy.sh](https://ntfy.sh) when items are added, removed, checked off, or modified.

Built with the [anylist_rs](https://github.com/phildenhoff/anylist_rs) library.

## Features

- **Real-time monitoring**: Uses WebSocket connection to AnyList for instant updates
- **Detailed notifications**: Includes item details, quantity, category, and more
- **Smart diff detection**: Compares cached state to detect exactly what changed
- **SQLite caching**: Maintains local state to enable accurate change detection
- **Flexible configuration**: TOML config file with environment variable overrides
- **Comprehensive logging**: Structured logging with configurable levels
- **Docker ready**: Easy deployment with included Dockerfile

## Prerequisites

**For Docker deployment (recommended):**
- Docker and Docker Compose
- AnyList account
- ntfy.sh topic

**For local development:**
- Rust 1.70+ (install from [rustup.rs](https://rustup.rs/))
- AnyList account
- ntfy.sh topic (use the public server or self-hosted instance)

## Installation

### Building from source

```bash
cd anylist_notify
cargo build --release
```

The binary will be available at `target/release/anylist_notify`.

## Configuration

The service can be configured using a combination of a TOML configuration file and environment variables (which override config file values).

### Option 1: Environment Variables

Create a `.env` file (copy from `.env.example`):

```bash
cp .env.example .env
```

Edit `.env` with your credentials:

```env
# AnyList credentials
ANYLIST_EMAIL=your-email@example.com
ANYLIST_PASSWORD=your-password

# ntfy.sh configuration
NTFY_URL=https://ntfy.sh
NTFY_TOPIC=anylist-updates

# Database
DATABASE_PATH=./anylist.db

# Logging level
RUST_LOG=info
```

### Option 2: Configuration File

Create a `config.toml` file (copy from `config.example.toml`):

```bash
cp config.example.toml config.toml
```

Edit `config.toml`:

```toml
[anylist]
email = "your-email@example.com"
password = "your-password"

[cache]
database_path = "./anylist.db"

[ntfy]
base_url = "https://ntfy.sh"
topic = "anylist-updates"

[ntfy.priorities]
item_added = "default"
item_checked = "low"
item_unchecked = "default"
item_removed = "default"
item_modified = "default"

[ntfy.tags]
item_added = "heavy_plus_sign,shopping_cart"
item_checked = "white_check_mark"
item_unchecked = "arrow_backward"
item_removed = "x,shopping_cart"
item_modified = "pencil2"

[logging]
level = "info"
```

### Option 3: Hybrid (Recommended)

Use `config.toml` for static settings and environment variables for sensitive data:

```bash
# In .env
ANYLIST_EMAIL=your-email@example.com
ANYLIST_PASSWORD=your-password
NTFY_TOPIC=my-secret-topic
```

Environment variables will override any values in `config.toml`.

## Usage

### Running the service

```bash
# Using cargo
cargo run --release

# Or directly
./target/release/anylist_notify
```

The service will:
1. Load configuration
2. Authenticate with AnyList
3. Initialize SQLite cache with current list state
4. Connect to AnyList WebSocket
5. Monitor for changes and send notifications

Press `Ctrl+C` to stop the service gracefully.

### Logging

Control log output with the `RUST_LOG` environment variable:

```bash
# Show only warnings and errors
RUST_LOG=warn ./anylist_notify

# Show debug information
RUST_LOG=debug ./anylist_notify

# Show trace-level details
RUST_LOG=trace ./anylist_notify
```

## Notification Examples

### Item Added
```
Title: ➕ Milk added to Groceries
Message:
  Quantity: 1 gallon
  Details: Whole milk
  Category: Dairy
Tags: heavy_plus_sign, shopping_cart
Priority: default
```

### Item Checked
```
Title: ✅ Milk checked off in Groceries
Message: Checked off in Groceries
Tags: white_check_mark
Priority: low
```

### Item Removed
```
Title: ❌ Milk removed from Groceries
Message: Removed from Groceries
Tags: x, shopping_cart
Priority: default
```

### Item Modified
```
Title: ✏️ Milk modified in Groceries
Message:
  Quantity: 1 gallon → 2 gallons
  Category: none → Dairy
Tags: pencil2
Priority: default
```

## How It Works

1. **Initialization**: Fetches all current lists and stores them in SQLite
2. **WebSocket Monitoring**: Connects to AnyList's WebSocket for real-time updates
3. **Event Handling**: When a `shopping-lists-changed` event is received:
   - Fetches updated lists from the API
   - Compares with cached state
   - Detects changes (additions, removals, checks, modifications)
   - Sends notifications via ntfy.sh
   - Updates cache with new state
4. **Diff Detection**: Compares items by ID to accurately track:
   - New items (not in cache)
   - Removed items (not in current state)
   - Check state changes
   - Field modifications (name, quantity, details, category)

## Cache Database

The service maintains a SQLite database to track list state. The schema includes:

**lists table**:
- `id` - List UUID
- `name` - List name
- `last_updated` - Unix timestamp

**items table**:
- `id` - Item UUID
- `list_id` - Foreign key to lists
- `name` - Item name
- `details` - Item details
- `quantity` - Optional quantity
- `category` - Optional category
- `is_checked` - Check state
- `last_seen` - Unix timestamp

The cache is automatically updated as changes are detected.

## ntfy.sh Setup

### Using Public Server

Simply set your topic in the configuration:

```toml
[ntfy]
topic = "my-unique-anylist-topic"
```

Subscribe to notifications:
- **Web**: Visit `https://ntfy.sh/my-unique-anylist-topic`
- **Mobile**: Install the ntfy app and subscribe to your topic
- **Desktop**: Use the ntfy CLI or desktop app

### Using Self-Hosted Server

Set your server URL in the configuration:

```toml
[ntfy]
base_url = "https://ntfy.example.com"
topic = "anylist"
```

See [ntfy.sh documentation](https://docs.ntfy.sh/) for self-hosting instructions.

## Troubleshooting

### Authentication Fails
- Verify your AnyList email and password are correct
- Check if you can log in to AnyList's website with the same credentials

### No Notifications Received
- Check that your ntfy topic is correct
- Verify you're subscribed to the topic in ntfy
- Check the service logs for errors (`RUST_LOG=debug`)
- Test ntfy manually: `curl -d "test" https://ntfy.sh/your-topic`

### Database Errors
- Ensure the database path is writable
- Delete `anylist.db` to start fresh (will re-sync on next startup)

### WebSocket Connection Issues
- Check your internet connection
- The service will automatically reconnect on connection loss
- Look for reconnection messages in the logs

## Development

### Running Tests

```bash
cargo test
```

### Code Structure

- `src/main.rs` - Service entry point and orchestration
- `src/config.rs` - Configuration management
- `src/cache/` - SQLite cache implementation
  - `models.rs` - Database models
  - `sqlite.rs` - SQLite operations
- `src/sync/` - WebSocket sync and diff detection
  - `diff.rs` - Change detection logic
  - `handler.rs` - Event handling
- `src/notify/` - Notification delivery
  - `ntfy.rs` - ntfy.sh client

## Contributing

This is a personal project, but suggestions and bug reports are welcome via GitHub issues.

## License

This project is provided as-is for personal use. This is an unofficial tool and is not affiliated with AnyList.

## Disclaimer

This service uses the unofficial AnyList API via the [anylist_rs](../anylist_rs) library. Please use responsibly and do not abuse the AnyList service.
