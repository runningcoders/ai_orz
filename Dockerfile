# syntax=docker/dockerfile:1.7
# AI Orz 生产镜像
#
# 三阶段构建：
#   1. frontend-builder —— Node(Tailwind) + wasm32 + dioxus-cli → /src/dist
#   2. backend-builder  —— 原生 release 构建 → /src/target/release/ai_orz
#   3. runtime          —— debian-slim + git/sh/常用 CLI，只含二进制 + 前端产物
#
# 多架构策略（重要）：**不用交叉编译，也不用 QEMU**。
# duckdb 的 bundled C++ 源码编译 + fastembed 下载 ONNX Runtime 二进制，交叉或模拟构建
# 既慢又容易失败（仓库现状：release.yml 已明确放弃交叉编译，理由 lancedb/ort-sys 太重）。
# 改为由 CI 在 amd64 / arm64 **原生 runner** 上各构建一次并推 by-digest，
# 再用 `docker buildx imagetools create` 合并成多架构 manifest。
# 本地单架构构建：docker build -t ai_orz .

ARG RUST_VERSION=1.97.0
ARG NODE_VERSION=22.22.2
ARG DX_VERSION=0.7.10

# ==================== 构建底座 ====================
FROM rust:${RUST_VERSION}-bookworm AS builder-base

# RUSTUP_TOOLCHAIN 覆盖仓库根 rust-toolchain.toml 的 channel="stable"，
# 保证镜像内工具链版本确定（否则 rustup 会在构建期另下一次 stable）
ARG RUST_VERSION
ENV RUSTUP_TOOLCHAIN=${RUST_VERSION} \
    DEBIAN_FRONTEND=noninteractive \
    CARGO_TERM_COLOR=never \
    SQLX_OFFLINE=true

# protobuf-compiler/libprotobuf-dev：lance-encoding 的 build script 需要 well-known types
# （google/protobuf/empty.proto），缺失会报 empty.proto not found
RUN apt-get update && apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        clang \
        cmake \
        curl \
        git \
        libprotobuf-dev \
        libssl-dev \
        pkg-config \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

# ==================== 阶段 1：前端 WASM ====================
FROM builder-base AS frontend-builder

ARG TARGETARCH
ARG NODE_VERSION
ARG DX_VERSION

# Tailwind 由 frontend/build.rs 调用 npm 包 tailwindcss 编译，故构建期必须可用 Node
RUN case "$TARGETARCH" in \
      amd64) node_arch=x64 ;; \
      arm64) node_arch=arm64 ;; \
      *) echo "unsupported TARGETARCH: $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" \
      | tar -xJ -C /usr/local --strip-components=1 \
    && node --version

RUN rustup target add wasm32-unknown-unknown

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install dioxus-cli --version "${DX_VERSION}" --locked

WORKDIR /src
COPY . .

# 复用仓库既有脚本（与 make build-fe / CI release 同一条链路，唯一事实源）
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    --mount=type=cache,target=/src/frontend/target \
    bash scripts/build_frontend.sh

# ==================== 阶段 2：后端二进制 ====================
FROM builder-base AS backend-builder

WORKDIR /src
COPY . .

# 裸 cargo build 只选 default member（根包 ai_orz）——正是 release 需要的产物
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked

# ==================== 阶段 3：运行时 ====================
FROM debian:bookworm-slim AS runtime

ARG DEBIAN_FRONTEND=noninteractive

# git：Agent 工作区产物版本管理依赖（pkg/git_workspace.rs）
# procps：shell 工具进程管理；unzip/curl：常用 CLI；tzdata：cron 时区解析
RUN apt-get update && apt-get install -y --no-install-recommends \
        bash \
        ca-certificates \
        curl \
        git \
        procps \
        tzdata \
        unzip \
    && rm -rf /var/lib/apt/lists/*

RUN useradd --create-home --uid 10001 --shell /bin/sh ai_orz \
    && mkdir -p /data /app \
    && chown -R ai_orz:ai_orz /data /app

COPY --from=backend-builder /src/target/release/ai_orz /usr/local/bin/ai_orz
COPY --from=frontend-builder /src/dist /app/dist

WORKDIR /app
USER ai_orz

# AI_ORZ_BASE_PATH：所有落盘数据（SQLite / 向量 / 工作区 / 日志 / 配置）都在这一个目录下，
# 只需挂载 /data 即可完成持久化与备份
ENV AI_ORZ_BASE_PATH=/data \
    AI_ORZ_LISTEN_ADDR=0.0.0.0:3000

VOLUME ["/data"]
EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=5s --start-period=40s --retries=3 \
    CMD curl -fsS http://127.0.0.1:3000/health || exit 1

ENTRYPOINT ["/usr/local/bin/ai_orz"]
