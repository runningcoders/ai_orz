---
kind: RAG 原子知识卡
name: 容器化与网关部署：Dockerfile 多阶段 + Caddyfile reverse_proxy + release.yml 多架构镜像
category: 基础设施 / 部署运维
scope:
  - "Dockerfile"
  - "docker-compose.yml"
  - "Caddyfile"
  - ".github/workflows/release.yml"
  - "src/config.rs"
  - "src/middleware/proxy.rs"
  - "src/middleware/sse_headers.rs"
  - "src/middleware/api_notice.rs"
  - "common/src/config.rs"
  - "common/config/ai_orz.toml"
  - ".env.deploy.example"
source_files:
  - Dockerfile (多阶段构建：backend Rust builder → runtime slim + frontend Dioxus WASM 构建产物注入；gh cli 安装；非 root 用户)
  - docker-compose.yml (backend + caddy 两服务；/data 卷挂载；环境变量注入)
  - Caddyfile (reverse_proxy 后端服务 + trust_proxy 安全前提 + SSE 缓冲禁止)
  - .github/workflows/release.yml (buildx 多架构镜像 amd64/arm64 + push registry)
  - src/config.rs (AI_ORZ_TRUST_PROXY + AI_ORZ_PUBLIC_BASE_URL 环境变量；ServerConfig::trust_proxy)
  - src/middleware/proxy.rs (client_ip 从 X-Forwarded-For 最右取；trust_proxy 安全前提)
  - src/middleware/sse_headers.rs (X-Accel-Buffering: no 防止 nginx 缓冲 SSE)
  - src/middleware/api_notice.rs (API notice 中间件)
  - common/src/config.rs (AppConfig + ServerConfig 结构)
  - common/config/ai_orz.toml (嵌入默认配置，新增 server.trust_proxy)
  - .env.deploy.example (部署环境变量模板)
  - .dockerignore (排除 target/ node_modules/ .git/)
  - docs/plan/容器化与网关部署方案.md (SSOT 设计稿)
  - docs/wiki/zh/content/配置与部署.md
  - docs/wiki/zh/content/基础设施/持续集成与发布工作流.md

---

## §1 概述

AI Orz 单二进制后端（Rust axum）+ Dioxus WASM 前端 + SQLite 的容器化部署方案。Dockerfile 采用三阶段构建：backend Rust builder（cargo build --release）→ runtime slim 镜像注入产物 + gh cli + 非 root 用户；frontend Dioxus WASM 产物在 backend 构建阶段一并注入到 runtime 镜像中。Caddyfile 作为 dumb reverse_proxy 网关，统一处理 HTTPS（自动证书）+ SSE 缓冲禁止（proxy_buffering off）+ 静态前端 WASM 资源。多架构镜像通过 `.github/workflows/release.yml` 的 docker buildx 同时构建 amd64/arm64 并 push registry。安全前提是 `server.trust_proxy`：直连暴露场景（无反向代理）必须为 false，此时 middleware/proxy.rs 的 client_ip() 禁止信任 X-Forwarded-* 头。

**定位：部署到服务器、排查 SSE 被缓冲（前端收不到实时消息）、配置 trust_proxy 被绕过安全红线、release.yml 镜像构建失败时读。**

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| Dockerfile 多阶段构建 | 镜像构建 | Stage1 builder（rust:alpine + cargo build --release backend）→ Stage2 frontend（dioxus build --release wasm）→ Stage3 runtime（debian:slim + 注入 binary + WASM dist + gh cli + user ai-orz 非 root） | 见 Dockerfile |
| docker-compose.yml | 编排 | backend（ai-orz:latest + 端口 8080 + /data 卷）+ caddy（caddy:latest + 端口 80/443 + Caddyfile 挂载 + backend 网络依赖） | 见 docker-compose.yml |
| Caddyfile | 反向代理 + 静态托管 | reverse_proxy localhost:8080；root * /usr/share/caddy/static（前端 WASM dist）；try_files {path} /index.html（SPA fallback）；proxy_buffering off（SSE 必配）；encoding zstd gzip | 见 Caddyfile |
| .github/workflows/release.yml | 多架构镜像 | docker buildx build --platform linux/amd64,linux/arm64 --push；tag=git SHA + latest；push GitHub Container Registry | 见 release.yml |
| src/config.rs + common/src/config.rs | 配置解析 | ServerConfig 新增 trust_proxy: bool 字段；环境变量 AI_ORZ_TRUST_PROXY（默认 false）+ AI_ORZ_PUBLIC_BASE_URL；嵌入默认配置从 common/config/ai_orz.toml | 见 src/config.rs |
| src/middleware/proxy.rs | 真实客户端 IP | client_ip() 从 X-Forwarded-For 最右取（最接近反代理的是真实 IP）；仅当 trust_proxy=true 时生效，否则返回 None | 见 src/middleware/proxy.rs |
| src/middleware/sse_headers.rs | SSE 缓冲防止 | 所有 SSE 响应无条件追加 X-Accel-Buffering: no；Caddy/Traefik 虽不缓冲但该头无害；nginx 默认缓冲会吞掉 SSE 直到连接结束才发 | 见 src/middleware/sse_headers.rs |
| src/middleware/api_notice.rs | API 公告 | 中间件：从 system_config 表读 api_notice JSON → 注入响应头 X-Api-Notice；前端据此显示公告横幅 | 见 src/middleware/api_notice.rs |
| common/config/ai_orz.toml | 默认配置 | 嵌入编译期默认值；新增 [server] 段 + trust_proxy = false；sqlite_path、jwt_secret、model_provider 等 | 见 common/config/ai_orz.toml |
| .env.deploy.example | 部署模板 | 列出所有生产环境变量：AI_ORZ_DB_PATH、AI_ORZ_JWT_SECRET_KEY（占位符）、AI_ORZ_TRUST_PROXY、AI_ORZ_PUBLIC_BASE_URL；敏感值留 ${PLACEHOLDER} | 见 .env.deploy.example |
| .dockerignore | 构建缓存 | 排除 target/、node_modules/、.git/、.sqlx/、docs/、tests/；加速构建并减小上下文 | 见 .dockerignore |
| docs/plan/容器化与网关部署方案.md | 设计 SSOT | 为什么选 Caddy 而非 nginx（自动证书极简）；三阶段构建权衡；非 root 用户理由；SSE 缓冲坑说明 | 见 plan |

**章节来源**
- [Dockerfile](Dockerfile)
- [Caddyfile](Caddyfile)
- [src/middleware/proxy.rs](src/middleware/proxy.rs)
- [src/middleware/sse_headers.rs](src/middleware/sse_headers.rs)

---

## §3 架构约定

**网关 dumb 原则**：Caddy 只做三层事——① HTTPS 终结 + 自动证书签发续期；② reverse_proxy 转发 HTTP 请求到 backend:8080；③ 静态托管前端 WASM dist 目录。业务逻辑（认证/鉴权/SSE 路由）全在 backend Rust 里完成。未来要换 nginx/Traefik 只需替换 Caddyfile，无需改任何后端代码。

**trust_proxy 安全前提**：`server.trust_proxy = true` 意味着"我知道前面有可信的反向代理（Caddy），它会正确设置 X-Forwarded-For/X-Forwarded-Proto 头"。此时 middleware/proxy.rs 的 client_ip() 从 X-Forwarded-For 最右端取（最接近代理的那个 IP）。如果 backend 直接暴露到公网（无代理）却误设 trust_proxy=true，攻击者可以伪造 X-Forwarded-For 头绕过 IP 限制。默认值必须是 false。

**SSE 缓冲的坑**：nginx 默认 proxy_buffering = on，会把 SSE 响应全读完再一次性返回——SSE 实时推送失效，前端收到的是一堆 JSON 一起冒出来。Caddy/Traefik 天生不缓冲流式响应，但 middleware/sse_headers.rs 仍必须无条件追加 `X-Accel-Buffering: no`（对 nginx 生效），兼容用户自行替换网关场景。

**三阶段构建权衡**：backend Rust 编译慢（~5-10min release），builder 镜像体积大（2GB+ 含 rustup），runtime 镜像只复制二进制 + WASM dist + gh cli，最终镜像 < 100MB。非 root 用户 ai-orz 运行进程——安全基线，防止二进制被劫持后容器宿主机被 root 权限搞坏。

---

## §4 硬约束与回归红线

1. **server.trust_proxy = false 时 middleware/proxy.rs 的 client_ip() 必须返回 None**：直连暴露场景禁止信任 X-Forwarded-*，任何人都能伪造头绕过 IP 限制。trust_proxy=true 必须由运维显式配置，默认 false 永不自动开启。
2. **SSE 响应必须无条件下 X-Accel-Buffering: no**：Caddy/Traefik 虽不缓冲但该头无害；nginx 默认缓冲会吞掉 SSE。middleware/sse_headers.rs 在 middleware 链上对所有 SSE 路径（/api/v1/sse/*）响应追加，永不允许关闭。
3. **Dockerfile 必须用非 root 用户**：安全基线。Stage3 runtime 最后必须 `RUN useradd -r ai-orz && USER ai-orz`，禁止以 root 运行二进制。
4. **.env.deploy.example 中敏感配置必须用占位符**：AI_ORZ_JWT_SECRET_KEY、AI_ORZ_MODEL_PROVIDER_API_KEY 等必须写 `${PLACEHOLDER}` 或 `REPLACE_WITH_YOUR_VALUE`，禁止提交真实 key；检查 CI 加 `grep -rE '(secret|key|token)' .env.deploy.example | grep -v PLACEHOLDER`。
5. **release.yml 多架构 buildx 必须 --push**：不能只 `--load`（单架构），必须同时构建 amd64 + arm64 并推送 registry；否则 ARM Mac 用户 `docker pull` 会报 image not found。
6. **docker-compose.yml /data 卷持久化 SQLite**：SQLite 必须挂到宿主机 /data 卷，禁止放在容器内（容器重建即丢数据）；compose 里必须显式声明 `volumes: - /data:/var/lib/ai-orz`。

---

## §5 扩展入口速查

| 扩展需求 | 改动位置 | 参考锚点 |
|---------|---------|---------|
| 换 nginx 代替 Caddy | 新建 nginx.conf（server 块 + proxy_pass + proxy_buffering off + / 静态托管前端 dist）→ docker-compose.yml caddy 服务替换为 nginx | [Caddyfile](Caddyfile) |
| 加 Prometheus metrics | Caddyfile 加 `metrics` 指令 → backend 端加 `/metrics` endpoint → release.yml 镜像加 prometheus 二进制 | [release.yml](.github/workflows/release.yml) |
| Kubernetes Helm Chart | 新建 charts/ 目录：Deployment（两份 backend + caddy container）+ Service + Ingress + ConfigMap；复用 Dockerfile 多阶段产物 | [docker-compose.yml](docker-compose.yml) |
| 加 WAF（Cloudflare） | Caddyfile 加 `@waf` route 配合 cloudflare waf；backend src/config.rs 新增 trust_proxy_cloudflare_ip_ranges | [src/config.rs](src/config.rs) |
