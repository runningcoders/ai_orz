---
kind: configuration_system
name: 配置系统：嵌入式默认 TOML + 运行时环境变量覆盖
category: configuration_system
scope:
    - '**'
source_files:
    - common/src/config.rs
    - common/config/ai_orz.toml
    - src/config.rs
    - src/lib.rs
    - frontend/src/config.rs
    - .env.example
---

## 1. 使用的方案

后端采用 **TOML 配置文件 + 环境变量覆盖** 的双层配置模型，基于 `serde`/`toml` crate 实现。前端（Dioxus WASM）通过独立的前端配置模块从 `localStorage` 与编译时嵌入的后端默认配置派生 API 地址。

## 2. 关键文件

- `common/src/config.rs`：定义所有配置结构体（`AppConfig`、`ServerConfig`、`DatabaseConfig`、`LoggingConfig`、`JwtConfig`、`ConsumerConfig`、`LarkConfig`、`A2aServerConfig`）、默认值函数、路径计算辅助方法（`db_path`、`attachments_dir`、`artifacts_dir`、`agent_data_dir`、`skills_root_dir`、`tool_call_trace_dir` 等）。
- `common/config/ai_orz.toml`：编译时嵌入的默认配置文件模板。
- `src/config.rs`：应用启动时的配置加载器，维护进程级单例 `OnceLock<Arc<AppConfig>>`，提供 `init()` / `get()` / `try_get()` / `load_config()`。
- `src/lib.rs`：`run()` 中第一个调用 `config::init()`，随后将配置注入 `pkg::init_all`、`router::create_router` 等子系统。
- `frontend/src/config.rs`：前端配置，优先级为 `localStorage` > 编译时嵌入的后端默认配置。
- `.env.example`：测试/构建相关的环境变量示例（`DATABASE_URL`、`SQLX_OFFLINE`、`TEST_*` 系列），非运行时配置。

## 3. 架构与设计约定

### 3.1 基础数据目录与配置文件位置
- 固定根目录常量 `BASE_DATA_PATH = ".ai_orz"`，可通过环境变量 `AI_ORZ_BASE_PATH` 覆盖（见 `common/src/config.rs` 第 15-16 行及 `base_data_path()` 方法）。
- 配置文件名常量 `CONFIG_FILE_NAME = "ai_orz.toml"`，位于 `${BASE_DATA_PATH}/ai_orz.toml`。
- 首次运行若文件不存在，`load_config()` 使用 `include_str!("../common/config/ai_orz.toml")` 写入默认配置到磁盘（`src/config.rs` 第 54-58 行）。
- 启动时确保 `.ai_orz` 目录存在；若启用文件日志则自动创建 `logs/` 子目录。

### 3.2 配置加载顺序与覆盖机制
1. 进程启动调用 `config::init()` → `load_config()`。
2. `load_config()` 先读 `AI_ORZ_BASE_PATH` 环境变量确定基础目录，再读取该目录下 `ai_orz.toml`，用 `toml::from_str` 反序列化为 `AppConfig`。
3. 部分字段支持环境变量覆盖：
   - `JWT_SECRET` 覆盖 `jwt.secret`（在 `src/pkg/mod.rs` 中通过 `std::env::var("JWT_SECRET")` 获取）。
   - `JWT_EXPIRY_HOURS` 覆盖 `jwt.default_expiry_hours`（同上）。
   - `FRONTEND_DIST_DIR` 覆盖 `frontend.dist_dir`（在 `src/lib.rs` 的 `run()` 中读取）。
   - `AI_ORZ_BASE_PATH` 覆盖整个基础数据路径。
4. 未显式设置的字段走 `#[serde(default)]` + 对应 `default_*` 函数（如 `default_listen_addr = "0.0.0.0:3000"`、`default_db_file_name = "ai_orz.db"`、`default_log_format = "json"`、`default_retention_days = 30`）。

### 3.3 单例与生命周期
- 配置以 `Arc<AppConfig>` 形式存入全局 `OnceLock`，通过 `config::get()` 获取，保证线程安全且只加载一次。
- 测试场景提供 `config::try_get()` 避免 panic。
- 启动流程：`main` → `ai_orz::run()` → `config::init()` → `pkg::init_all(&config)` → `service::init()` → `producer/consumer::init()` → `service::init_base_data()` → `aop::init_all()` → 启动 Axum 服务器。

### 3.4 路径抽象
`AppConfig` 集中暴露所有持久化路径的计算方法（`db_path`、`vector_db_path`、`hnsw_index_dir`、`attachments_dir`、`artifact_project_dir`、`agent_data_dir`、`agent_memory_dir`、`skills_root_dir`、`tool_call_trace_dir`、`tool_logs_dir` 等），业务代码不直接拼接路径字符串，统一通过 `base_data_path()` 派生，避免硬编码。

### 3.5 前端配置
- 前端 `FrontendConfig` 仅包含 `api_base_url`。
- 默认值从编译时嵌入的后端 `ai_orz.toml` 中的 `server.listen_addr` 推导，并将 `0.0.0.0` 替换为 `localhost`。
- 用户可通过 `localStorage["ai_orz_config"]` 覆盖，保存键名为 `ai_orz_config`。

## 4. 约定与约束

- **配置文件格式**：必须为 TOML，字段名与 `common/src/config.rs` 中 `AppConfig` 及其子结构体严格一致，新增配置需同时添加 `#[serde(default)]` 和对应的 `default_*` 函数。
- **默认配置来源**：`common/config/ai_orz.toml` 是单一事实源，通过 `include_str!` 编译进二进制，禁止在部署环境修改此文件——应修改生成的 `.ai_orz/ai_orz.toml`。
- **敏感信息**：`jwt.secret`、`lark.app_secret`、`lark.encrypt_key`、`lark.verification_token` 等敏感字段建议通过环境变量注入，而非写入配置文件。
- **基础数据隔离**：所有运行时产物（SQLite、DuckDB、向量索引、附件、技能、工具日志）均位于 `BASE_DATA_PATH` 下，不同部署通过设置 `AI_ORZ_BASE_PATH` 隔离。
- **消费者配置合并策略**：`ConsumerConfig::for_topic(topic)` 按 topic 专属配置优先、全局配置兜底的方式合并，新增 topic 无需修改全局并发参数即可单独调优。
- **测试隔离**：测试通过 `std::env::set_var("AI_ORZ_BASE_PATH", ...)` 指向临时目录，确保测试不污染真实数据。
- **前端与后端共享配置**：前端通过读取后端嵌入的默认配置来推断 `api_base_url`，保持前后端监听地址一致，避免手动同步。

## 5. 适用性说明

本仓库实现了完整的配置系统：有明确的配置结构定义、默认模板、运行时加载逻辑、环境变量覆盖机制以及路径抽象层，属于高置信度的配置系统实现。