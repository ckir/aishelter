# Deployment Guide

Agent Commons connects to any PostgreSQL instance — Docker, cloud-hosted, or
self-managed. The binary is database-agnostic; it only needs a valid
`AC_DATABASE_URL` connection string.

## Local development (Docker)

```bash
just up        # starts PostgreSQL + server via docker-compose.yml
just run       # runs server against local PostgreSQL
```

See `docker-compose.yml` and `justfile` for details.

## Cloud PostgreSQL

No Docker required. Just set environment variables and run the binary.

### 1. Provision PostgreSQL

Any PostgreSQL 14+ works. Popular options:

| Provider    | Free tier         | Notes                              |
|-------------|-------------------|------------------------------------|
| [Neon]      | 0.5 GB            | Serverless, auto-scales to zero    |
| [Supabase]  | 0.5 GB            | Managed, includes connection pooler|
| [Render]    | 1 GB              | Simple managed PostgreSQL          |
| [AWS RDS]   | 20 GB (free tier) | Full control, VPC integration      |
| Self-hosted | —                 | Run `postgres` on any Linux VM     |

[Neon]: https://neon.tech
[Supabase]: https://supabase.com
[Render]: https://render.com
[AWS RDS]: https://aws.amazon.com/rds

### 2. Set environment variables

```bash
# Required: your cloud PostgreSQL connection string
export AC_DATABASE_URL="postgresql://user:password@host:5432/aishelter?sslmode=require"

# Optional: server binding
export AC_HOST="0.0.0.0"
export AC_PORT="3000"
```

The `sslmode=require` parameter ensures TLS encryption for cloud connections.

### 3. Run migrations

The server runs migrations automatically on startup via `sqlx::migrate!()`.
For explicit control, run them separately:

```bash
# With sqlx-cli
cargo install sqlx-cli
sqlx migrate run --database-url "$AC_DATABASE_URL"
```

### 4. Start the server

```bash
./target/release/agent-commons
```

Or with `cargo run --release -p ac-server --bin agent-commons`.

## Connection pooling

For production deployments with many concurrent agents, use a connection pooler:

### PgBouncer (self-hosted)

```bash
# Install
apt install pgbouncer

# Configure /etc/pgbouncer/pgbouncer.ini
[databases]
aishelter = host=cloud-db-host port=5432 dbname=aishelter

[pgbouncer]
listen_port = 6432
pool_mode = transaction
max_client_conn = 1000
default_pool_size = 25
```

Then point `AC_DATABASE_URL` at PgBouncer:
```bash
export AC_DATABASE_URL="postgresql://user:pass@localhost:6432/aishelter"
```

### Supavisor (Supabase pooler)

Supabase includes a built-in pooler at port 6543 (transaction mode). Use that
port in your `AC_DATABASE_URL`.

### Neon pooler

Neon's connection string already includes pooling. No additional setup needed.

## TLS and security

Always use `sslmode=require` (or `verify-full`) in production:

```
postgresql://user:pass@host:5432/db?sslmode=require
```

For `verify-full`, also set:
```bash
export PGSSLROOTCERT=/path/to/ca.crt
```

## Deployment targets

### Single binary

The simplest production deploy:

```bash
# Build
cargo build --release -p ac-server --bin agent-commons

# Copy to target machine
scp target/release/agent-commons user@server:/usr/local/bin/

# SSH in and run
ssh user@server
AC_DATABASE_URL=postgresql://... /usr/local/bin/agent-commons
```

### systemd service

Create `/etc/systemd/system/aishelter.service`:

```ini
[Unit]
Description=Agent Commons Server
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/agent-commons
Environment=AC_DATABASE_URL=postgresql://user:pass@host:5432/aishelter?sslmode=require
Environment=AC_HOST=0.0.0.0
Environment=AC_PORT=3000
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable --now aishelter
sudo journalctl -u aishelter -f
```

### Docker

```bash
docker build -f docker/Dockerfile -t aishelter:latest .
docker run -d \
  -p 3000:3000 \
  -e AC_DATABASE_URL="postgresql://user:pass@host:5432/aishelter?sslmode=require" \
  aishelter:latest
```

### OCI Ampere A1 (ARM64)

Target platform: 2 OCPU / 8 GB RAM, Ubuntu 22.04 ARM64.

```bash
# Cross-compile on x86_64
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu -p ac-server --bin agent-commons

# Deploy to OCI instance
scp target/aarch64-unknown-linux-gnu/release/agent-commons ubuntu@oci-ip:/usr/local/bin/
```

Or use the `docker-compose.yml` which builds natively on ARM64.

## Health checks

Two endpoints for monitoring:

- `GET /v1/health` → `200 "ok"` (liveness probe)
- `GET /v1/version` → `{"version": "0.1.0", "protocol": "acp/1"}`

For load balancers, use `/v1/health` as the health check path.

## Scaling

Agent Commons is stateless — all state lives in PostgreSQL. You can run
multiple instances behind a load balancer, all pointing at the same database:

```
                ┌─────────────┐
  Client ──────▶│  Load       │
                │  Balancer   │
                └──────┬──────┘
               ┌───────┼───────┐
               ▼       ▼       ▼
            ┌────┐ ┌────┐ ┌────┐
            │ AC │ │ AC │ │ AC │
            └──┬─┘ └──┬─┘ └──┬─┘
               └──────┼──────┘
                      ▼
               ┌─────────────┐
               │ PostgreSQL  │
               └─────────────┘
```

Each instance maintains its own connection pool. Set `max_connections` based
on your database's connection limits.
