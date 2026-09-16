# Fast Load Balancing (flb)

[English](README.md) | [中文](README_ZH.md)

Reverse proxy and load balancer built on [Pingora](https://github.com/cloudflare/pingora). The proxy and admin UI ship in one binary: HTTP/HTTPS traffic uses the proxy ports; configuration uses the admin port.

## Features

- HTTP / HTTPS reverse proxy with host, path, and method routing
- Upstreams: weighted selection; HTTP/HTTPS per backend; empty port defaults to 80/443; empty SNI uses the backend address
- Hosts: HTTP and HTTPS can both be enabled; HTTPS requires a certificate; HTTP-to-HTTPS redirect when both are on
- Certificates: upload PEM, or issue with Let's Encrypt (HTTP-01 / DNS-01); wildcards require DNS-01
- DNS providers: Alibaba Cloud, Wanwang, GoDaddy (for DNS-01)
- Automatic renewal at a random time between 00:00 and 01:00
- TCP / UDP port forwarding
- Vue 3 + Element Plus admin UI, served by the same process (Chinese/English)

## Architecture

| Component | Role |
| --- | --- |
| `flb` | Binary (`crates/flb-cli`) |
| Pingora | HTTP/HTTPS proxy |
| Axum | Admin API `/api` + embedded UI |
| Storage | `{data-dir}/config.yaml` |

Default ports:

| Port | Role |
| --- | --- |
| `80` | HTTP proxy |
| `443` | HTTPS proxy |
| `9000` | Admin UI |

## Requirements

- Rust stable (workspace edition 2024)
- Node.js 20+ and [pnpm](https://pnpm.io/) 10
- System OpenSSL (Pingora)

## Run locally

Build the frontend first so `cargo` can embed `frontend/dist` into the binary:

```bash
cd frontend
pnpm install
pnpm run build
cd ..

cargo run -p flb -- \
  --data-dir data \
  --admin-listen 0.0.0.0:9000 \
  --http-listen 0.0.0.0:80 \
  --https-listen 0.0.0.0:443
```

Open `http://127.0.0.1:9000`. After changing the UI, run `pnpm run build` and rebuild `flb`. To serve files from disk instead of the embed, pass `--www-dir` pointing at a directory that contains `index.html`.

Frontend development:

```bash
# terminal 1: proxy and API
cargo run -p flb -- --data-dir data --admin-listen 127.0.0.1:9000

# terminal 2: Vite proxies /api to 9000
cd frontend && pnpm run dev
```

## Docker

```bash
docker compose up -d --build
```

Mappings:

| Host | Container |
| --- | --- |
| `80` | `80` HTTP proxy |
| `443` | `443` HTTPS proxy |
| `9000` | `9000` admin UI |
| `./data` | `/flb/data` |

Admin UI: `http://127.0.0.1:9000`.

If you already have a musl binary, `docker-compose.local.yaml` mounts `target/x86_64-unknown-linux-musl/release/flb` without rebuilding the image. The admin UI is already inside that binary.

## CLI

Flags also accept matching environment variables.

| Flag | Env | Default | Description |
| --- | --- | --- | --- |
| `--admin-listen` | `FLB_ADMIN_LISTEN` | `0.0.0.0:9000` | Admin UI |
| `--http-listen` | `FLB_HTTP_LISTEN` | `0.0.0.0:80` | HTTP proxy |
| `--https-listen` | `FLB_HTTPS_LISTEN` | `0.0.0.0:443` | HTTPS proxy |
| `--data-dir` | `FLB_DATA_DIR` | `data` | Config and ACME data |
| `--www-dir` | `FLB_WWW_DIR` | `www` | Override embedded UI when `index.html` exists |
| `--acme-staging` | `FLB_ACME_STAGING` | `false` | Let's Encrypt staging |
| `--admin-user` | `FLB_ADMIN_USER` | `admin` | Admin username |
| `--admin-password` | `FLB_ADMIN_PASSWORD` | `admin` | Admin password |
| `--jwt-secret` | `FLB_JWT_SECRET` | derived from user/password | JWT signing key (stateless admin login) |

Logs: `{data-dir}/logs/access.log`, `error.log`, and `flb.log` (async writes; rotated by day and at 100 MiB). Console still follows `RUST_LOG` (default `info`).

## Data directory

`--data-dir` (container path `/flb/data`):

```text
data/
  config.yaml              # certs, domains, upstreams, hosts, streams
  logs/
    access.log             # current access log
    error.log              # current warnings and errors
    flb.log                # admin audit log (JSON lines)
    access.YYYY-MM-DD.log  # rotated by date / size
    error.YYYY-MM-DD.log
    flb.YYYY-MM-DD.log
  letsencrypt/
    acme-account.json      # Let's Encrypt account
```

Existing `config.json` is imported once and rewritten as `config.yaml`. Issued certificate PEMs are stored in `config.yaml`, not as separate files.

## Development

```bash
cargo fmt --all --
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd frontend && pnpm run typecheck && pnpm run build
```

## License

MIT OR Apache-2.0

