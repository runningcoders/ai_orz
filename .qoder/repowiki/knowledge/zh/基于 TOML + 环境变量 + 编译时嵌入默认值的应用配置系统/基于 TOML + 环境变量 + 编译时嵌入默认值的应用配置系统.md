---
kind: configuration_system
name: 基于 TOML + 环境变量 + 编译时嵌入默认值的应用配置系统
category: configuration_system
scope:
    - '**'
source_files:
    - src/config.rs
    - common/src/config.rs
    - common/config/ai_orz.toml
    - src/lib.rs
    - frontend/src/config.rs
    - .env.example
---

## 1. 整体方案

应用采用 **TOML 配置文件 + 环境变量覆盖 + 编译时嵌入默认值** 的三层加载策略，通过 `OnceLock<Arc<AppConfig>>` 作为进程内全局单例提供。

- 配置结构体定义集中在 `common/src/config.rs`（`AppConfig`、`ServerConfig`、`DatabaseConfig`、`LoggingConfig`、`JwtConfig`、`ConsumerConfig`、`SecurityConfig`、`A2aServerConfig` 等），使用 `serde` 反序列化。
- 默认模板文件 `common/config/ai_orz.toml` 在编译期通过 `include_str!` 嵌入到二进制中（`src/config.rs` 中的 `DEFAULT_CONFIG_EMBEDDED`）。
- 运行时由 `src/config.rs::load_config()` 负责：读取 `AI_ORZ_BASE_PATH` 环境变量确定基础数据目录 `.ai_orz/`，若 `ai_orz.toml` 不存在则自动写出默认模板；解析后校验并补写缺失的 `[security] secret_key`；确保日志目录存在。
- 启动入口 `src/lib.rs::run()` 首先调用 `config::init()` 完成加载，再通过 `config::get()` 获取全局配置，随后传递给 `pkg::init_all`、`service::init`、`router::create_router` 等模块。

## 2. 关键文件与职责

| 文件 | 职责 |
|---|---|
| `common/src/config.rs` | 所有配置结构体定义、默认值函数、路径派生方法（`db_path`、`log_dir`、`artifacts_dir`、`agent_data_dir`、`skills_root_dir` 等） |
| `common/config/ai_orz.toml` | 可编辑的默认配置模板（首次运行自动生成到 `.ai_orz/ai_orz.toml`） |
| `src/config.rs` | 配置加载逻辑、`OnceLock` 单例管理、默认模板注入、安全密钥自动补写 |
| `src/lib.rs::run()` | 应用启动顺序：`config::init` → `pkg::init_all` → `service::init` → `producer/consumer::init` → AOP 调度器 → HTTP server |
| `frontend/src/config.rs` | 前端 Dioxus WASM 侧配置：优先级为 `localStorage` > 编译时嵌入的后端 listen_addr（将 `0.0.0.0` 替换为 `localhost`） |
| `.env.example` | 测试用环境变量示例（SQLX、真实模型集成测试密钥等） |

## 3. 架构与设计约定

### 3.1 配置来源优先级

后端：
1. 环境变量 `AI_ORZ_BASE_PATH` 覆盖基础数据根目录（默认 `.ai_orz`）。
2. 文件系统 `ai_orz.toml`（位于 base data path 下）是主配置源。
3. 编译时嵌入的默认模板作为兜底，首次启动自动写出。
4. 部分字段支持环境变量覆盖（如 `JWT_SECRET`、`JWT_EXPIRY_HOURS`，见 `ai_orz.toml` 注释）。
5. 前端静态目录可通过环境变量 `FRONTEND_DIST_DIR` 覆盖（见 `src/lib.rs::run()`）。

前端（Dioxus WASM）：
1. `localStorage` 中 `ai_orz_config` 键值最高优先。
2. 回退到编译时嵌入的后端 `listen_addr`，并将 `0.0.0.0` 替换为 `localhost` 以适配浏览器访问。
3. 无 `window` 环境（单元测试）直接回退到编译时默认值。

### 3.2 安全配置强制校验

`load_config()` 在解析后检查 `security.secret_key` 是否为空：若为空则自动生成默认值 `ai-orz-default-secret-key-change-me` 并持久化回配置文件，同时打印警告。该机制确保数据库敏感字段加密始终可用，但生产环境必须修改该密钥。

### 3.3 路径组织约定

所有持久化数据统一位于 base data path（`.ai_orz/`）下，通过 `AppConfig` 的方法集中派生：
- 数据库：`ai_orz.db`、`ai_orz_vector.db`
- 向量索引：`hnsw_index/`
- 附件：`attachments/`（按日期分层）
- 项目产物：`artifacts/projects/{project_id}/{artifact_id}`
- Agent 数据：`agents/{agent_id}/memory`、`agents/{agent_id}/skills`
- 共享技能：`skills/{skill_id}/skill.md`
- 工具追踪：`tools/{tool_id}/call_trace`、`tools/{tool_id}/logs`

### 3.4 消费者配置继承

`ConsumerConfig` 支持全局默认值 + `topics` 哈希表 per-topic 覆盖，通过 `for_topic(topic)` 合并得到最终配置（topic 字段优先，未设置则继承全局）。

## 4. 约束与规则

- **base data path 固定**：默认 `.ai_orz/`，仅能通过 `AI_ORZ_BASE_PATH` 环境变量覆盖，代码中多处硬编码此约定。
- **配置文件自动生成**：首次运行若不存在 `ai_orz.toml`，程序会静默写出默认模板，用户无需手动创建。
- **secret_key 不可为空**：启动时强制校验并自动补写，属于硬性安全约束。
- **前端 API 地址同源优先**：WASM 前端优先使用浏览器当前 origin 作为 `api_base_url`，避免端口变更导致请求打偏。
- **配置单例不可变**：通过 `Arc<AppConfig>` + `OnceLock` 暴露只读引用，运行时不允许修改。
- **测试隔离**：测试通过 `DATABASE_URL=sqlite://./.ai_orz/test.db` 和 `SQLX_OFFLINE=true` 环境变量隔离数据与构建依赖。

## 5. 适用性说明

该系统覆盖了后端 Rust 服务的全部运行时配置（服务器、数据库、日志、JWT、安全密钥、A2A、消费者并发等），并通过独立的前端配置模块处理 WASM 侧的 API 地址问题，构成完整的跨端配置体系。