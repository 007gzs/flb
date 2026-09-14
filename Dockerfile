FROM node:24-bookworm-slim AS frontend_builder
RUN npm config set registry https://registry.npmmirror.com/
RUN corepack enable && corepack prepare pnpm@10.14.0 --activate
WORKDIR /src
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend .
RUN pnpm run build

FROM ghcr.io/rust-cross/cargo-zigbuild AS rust_builder
ENV RUSTUP_DIST_SERVER="https://rsproxy.cn"
ENV RUSTUP_UPDATE_ROOT="https://rsproxy.cn/rustup"
RUN rustup update stable && rustup default stable && rustup target add x86_64-unknown-linux-musl
RUN apt-get update && apt-get install -y cmake
WORKDIR /src
COPY . .
COPY --from=frontend_builder /src/dist /src/frontend/dist
RUN cargo zigbuild --release --target x86_64-unknown-linux-musl

FROM debian:bullseye-slim
WORKDIR /flb
COPY --from=rust_builder /src/target/x86_64-unknown-linux-musl/release/flb /bin/
RUN mkdir -p /flb/data/
EXPOSE 80 443 9000
VOLUME /flb/data/
CMD ["flb", "--data-dir", "/flb/data"]
