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
| Axum | 管理 API `/api` + 内嵌前端 |
| 存储 | `{data-dir}/config.yaml` |

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

先编译前端，`cargo` 会把 `frontend/dist` 打进二进制：

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

浏览器打开 `http://127.0.0.1:9000`。改完前端后需要再执行 `pnpm run build` 并重新编译 `flb`。若目录里有 `index.html`，可用 `--www-dir` 覆盖内嵌界面。

开发前端时：

```bash
# 终端 1：代理与 API
cargo run -p flb -- --data-dir data --admin-listen 127.0.0.1:9000

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
| `80` | `80` HTTP 代理 |
| `443` | `443` HTTPS 代理 |
| `9000` | `9000` 管理界面 |
| `./data` | `/flb/data` |

管理界面：`http://127.0.0.1:9000`。

本地已编好 musl 二进制时，可用 `docker-compose.local.yaml`，挂载 `target/x86_64-unknown-linux-musl/release/flb`，不必重新构建镜像。管理界面已打进该二进制。

## 命令行

参数也可用同名环境变量。

| 参数 | 环境变量 | 默认 | 说明 |
| --- | --- | --- | --- |
| `--admin-listen` | `FLB_ADMIN_LISTEN` | `0.0.0.0:9000` | 管理界面 |
| `--http-listen` | `FLB_HTTP_LISTEN` | `0.0.0.0:80` | HTTP 代理 |
| `--https-listen` | `FLB_HTTPS_LISTEN` | `0.0.0.0:443` | HTTPS 代理 |
| `--data-dir` | `FLB_DATA_DIR` | `data` | 配置与 ACME 数据 |
| `--www-dir` | `FLB_WWW_DIR` | `www` | 目录内有 `index.html` 时覆盖内嵌界面 |
| `--acme-staging` | `FLB_ACME_STAGING` | `false` | Let's Encrypt 预发环境 |
| `--admin-user` | `FLB_ADMIN_USER` | `admin` | 管理界面用户名 |
| `--admin-password` | `FLB_ADMIN_PASSWORD` | `admin` | 管理界面密码 |
| `--jwt-secret` | `FLB_JWT_SECRET` | 由用户名/密码派生 | JWT 签名密钥（无状态登录） |

日志：`{data-dir}/logs/access.log`、`error.log` 与 `flb.log`（异步落盘；按日期和 100 MiB 拆分）。控制台仍可用 `RUST_LOG`（默认 `info`）。

## 数据目录

`--data-dir`（容器内 `/flb/data`）大致如下：

```text
data/
  config.yaml              # 证书、域名、后端、主机、数据流
  logs/
    access.log             # 当前访问日志
    error.log              # 当前告警与错误
    flb.log                # 管理后台审计日志（JSON 行）
    access.YYYY-MM-DD.log  # 按日期 / 大小滚动
    error.YYYY-MM-DD.log
    flb.YYYY-MM-DD.log
  letsencrypt/
    acme-account.json      # Let's Encrypt 账号
```

已有 `config.json` 会在首次启动时导入并写成 `config.yaml`。签发后的证书内容写在 `config.yaml`，不单独落成 PEM 文件。

## 开发

```bash
cargo fmt --all --
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

cd frontend && pnpm run typecheck && pnpm run build
```

## License

MIT OR Apache-2.0
