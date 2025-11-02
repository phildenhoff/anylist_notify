# Deployment Guide

## Docker Deployment (Recommended for Homelab)

### Prerequisites
- Docker and Docker Compose installed
- Your AnyList credentials
- A ntfy.sh topic name

### Deployment Steps

#### 1. Clone/Copy the project to your server

```bash
# On your Unraid/Dokploy server
git clone <your-repo> anylist_notify
cd anylist_notify
```

#### 2. Create environment file

```bash
cp .env.example .env
```

Edit `.env` with your credentials:

```env
ANYLIST_EMAIL=your-email@example.com
ANYLIST_PASSWORD=your-password
NTFY_TOPIC=your-unique-topic-name
NTFY_URL=https://ntfy.sh
RUST_LOG=info
```

#### 3. Build and run with Docker Compose

```bash
# Build the image
docker-compose build

# Start the service
docker-compose up -d

# View logs
docker-compose logs -f

# Stop the service
docker-compose down
```

#### 4. Subscribe to notifications

- **Web**: Visit `https://ntfy.sh/your-unique-topic-name`
- **Mobile**: Install the ntfy app and subscribe to your topic
- **Desktop**: Use the ntfy desktop app

### Dokploy Deployment

If using Dokploy on your Unraid server:

1. **Create a new service** in Dokploy
2. **Connect your Git repository** or upload the project
3. **Set environment variables** in Dokploy UI:
   - `ANYLIST_EMAIL`
   - `ANYLIST_PASSWORD`
   - `NTFY_TOPIC`
   - `NTFY_URL` (optional, defaults to https://ntfy.sh)
   - `RUST_LOG` (optional, defaults to info)
4. **Configure volume mount**:
   - Mount `/data` to persist the SQLite database
5. **Deploy**

### Database Persistence

The SQLite database is stored in the `./data` directory (mounted to `/data` in the container). This ensures:
- List state persists across container restarts
- No duplicate notifications on restart
- Historical change tracking

**Backup the database:**
```bash
# Copy database to backup location
cp data/anylist.db data/anylist.db.backup

# Or use docker cp
docker cp anylist_notify:/data/anylist.db ./backup/
```

### Monitoring

**Check if the service is running:**
```bash
docker-compose ps
```

**View real-time logs:**
```bash
docker-compose logs -f anylist_notify
```

**Check resource usage:**
```bash
docker stats anylist_notify
```

### Troubleshooting

**Service won't start:**
```bash
# Check logs for errors
docker-compose logs anylist_notify

# Verify environment variables
docker-compose config
```

**Database issues:**
```bash
# Reset database (WARNING: loses change history)
rm -rf data/anylist.db
docker-compose restart
```

**WebSocket connection issues:**
- Ensure your server has outbound internet access
- Check firewall rules
- The service will automatically reconnect on connection loss

### Updates

To update to a new version:

```bash
# Pull latest code
git pull

# Rebuild and restart
docker-compose build
docker-compose up -d

# Database persists automatically
```

## Alternative: Render.com Deployment

If you prefer cloud hosting (requires paid tier for persistent connections):

1. Create a new **Web Service** on Render
2. Connect your Git repository
3. Set **Build Command**: `cargo build --release`
4. Set **Start Command**: `./target/release/anylist_notify`
5. Add environment variables (same as above)
6. Add a **Disk** for `/data` (to persist SQLite database)
7. Deploy

**Cost:** ~$7/month for persistent service

## Resource Requirements

- **Memory**: ~50-100MB
- **CPU**: Minimal (mostly idle, spikes on list updates)
- **Disk**: <100MB (mostly for the binary)
- **Network**: Low bandwidth (WebSocket + occasional ntfy.sh requests)

Perfect for a homelab alongside other services!
