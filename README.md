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
| Storage | `{data-dir}/config.json` |

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
| `22080` | `80` HTTP proxy |
| `22443` | `443` HTTPS proxy |
| `22081` | `9000` admin UI |
| `./data` | `/flb/data` |

Admin UI: `http://127.0.0.1:22081`.

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

Logs: `RUST_LOG=info` (or `debug`).

## Data directory

`--data-dir` (container path `/flb/data`):

```text
data/
  config.json              # certs, domains, upstreams, hosts, streams
  letsencrypt/
    acme-account.json      # Let's Encrypt account
```

Issued certificate PEMs are stored in `config.json`, not as separate files.

## Development

```bash
cargo fmt --all --
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd frontend && pnpm run typecheck && pnpm run build
```

## License

MIT
