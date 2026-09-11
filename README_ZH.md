# Fast Load Balancing (flb)

[English](README.md) | [中文](README_ZH.md)

基于 [Pingora](https://github.com/cloudflare/pingora) 的反向代理与负载均衡。代理和管理界面打在同一个二进制里：HTTP/HTTPS 流量走代理端口，配置走管理端口。

## 功能

- HTTP / HTTPS 反向代理，按主机名、路径、方法路由
- 后端服务组：加权选择；每个后端可单独选 HTTP/HTTPS；端口可留空（HTTP 默认 80，HTTPS 默认 443）；SNI 可留空，连接时使用后端地址
- 主机配置：可同时开启 HTTP 和 HTTPS；HTTPS 需选证书；两者都开时支持 HTTP 转 HTTPS
- 证书：手动上传 PEM，或 Let's Encrypt 自动签发（HTTP-01 / DNS-01）；泛域名只能走 DNS-01
- DNS 提供商：阿里云、万网、GoDaddy（用于 DNS-01）
- 每天 0:00–1:00 随机时间自动续签
- TCP / UDP 端口转发
- Vue 3 + Element Plus 管理界面，由同一进程提供静态文件

## 架构

| 组件 | 说明 |
| --- | --- |
| `flb` | 可执行文件（`crates/flb-cli`） |
| Pingora | HTTP/HTTPS 代理 |
| Axum | 管理 API `/api` + 前端静态资源 |
| 存储 | `{data-dir}/config.json` |

默认端口：

| 端口 | 用途 |
| --- | --- |
| `80` | HTTP 代理 |
| `443` | HTTPS 代理 |
| `9000` | 管理界面 |

## 要求

- Rust stable（workspace edition 2024）
- Node.js 20+、[pnpm](https://pnpm.io/) 10
- 系统 OpenSSL（Pingora）

## 本地运行

先编译前端，再启动 `flb`：

```bash
cd frontend
pnpm install
pnpm run build
cd ..

cargo run -p flb -- \
  --www-dir frontend/dist \
  --data-dir data \
  --admin-listen 0.0.0.0:9000 \
  --http-listen 0.0.0.0:80 \
  --https-listen 0.0.0.0:443
```

浏览器打开 `http://127.0.0.1:9000`。

开发前端时：

```bash
# 终端 1：代理与 API
cargo run -p flb -- --www-dir frontend/dist --data-dir data --admin-listen 127.0.0.1:9000

# 终端 2：Vite，/api 会转到 9000
cd frontend && pnpm run dev
```

## Docker

```bash
docker compose up -d --build
```

映射：

| 宿主机 | 容器 |
| --- | --- |
| `22080` | `80` HTTP 代理 |
| `22443` | `443` HTTPS 代理 |
| `22081` | `9000` 管理界面 |
| `./data` | `/flb/data` |

管理界面：`http://127.0.0.1:22081`。

本地已编好 musl 二进制时，可用 `docker-compose.local.yaml`，挂载 `target/x86_64-unknown-linux-musl/release/flb` 和 `frontend/dist`，不必重新构建镜像。

## 命令行

参数也可用同名环境变量。

| 参数 | 环境变量 | 默认 | 说明 |
| --- | --- | --- | --- |
| `--admin-listen` | `FLB_ADMIN_LISTEN` | `0.0.0.0:9000` | 管理界面 |
| `--http-listen` | `FLB_HTTP_LISTEN` | `0.0.0.0:80` | HTTP 代理 |
| `--https-listen` | `FLB_HTTPS_LISTEN` | `0.0.0.0:443` | HTTPS 代理 |
| `--data-dir` | `FLB_DATA_DIR` | `data` | 配置与 ACME 数据 |
| `--www-dir` | `FLB_WWW_DIR` | `www` | 前端静态目录 |
| `--acme-staging` | `FLB_ACME_STAGING` | `false` | Let's Encrypt 预发环境 |

日志：`RUST_LOG=info`（或 `debug`）。

## 数据目录

`--data-dir`（容器内 `/flb/data`）大致如下：

```text
data/
  config.json              # 证书、域名、后端、主机、数据流
  letsencrypt/
    acme-account.json      # Let's Encrypt 账号
```

签发后的证书内容写在 `config.json`，不单独落成 PEM 文件。

## 开发

```bash
cargo fmt --all --
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd frontend && pnpm run typecheck && pnpm run build
```

## License

MIT
