> 📦 归档标记（2026-08-15）：被 [配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入](docs/wiki/knowledge/zh/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入/配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: configuration_system
name: 基于 TOML + 环境变量覆盖的运行时配置系统
category: configuration_system
scope:
    - '**'
source_files:
    - common/src/config.rs
    - common/config/ai_orz.toml
    - src/config.rs
    - src/pkg/mod.rs
    - frontend/src/config.rs
    - .env.example
    - tests/common/env.rs
---

## 1. 采用的方案

项目采用 **TOML 配置文件 + 环境变量覆盖** 的双层配置加载机制，使用 `serde`/`toml` crate 进行序列化与解析。默认配置在编译期通过 `include_str!` 嵌入二进制，首次启动时自动写出到磁盘；用户可编辑外部 `ai_orz.toml` 文件并配合环境变量调整行为。

- 后端：`common/src/config.rs` 定义全部配置结构体（`AppConfig`、`ServerConfig`、`DatabaseConfig`、`LoggingConfig`、`JwtConfig`、`ConsumerConfig`、`LarkConfig`、`A2aServerConfig` 等），并通过 `#[serde(default)]` 提供默认值。
- 前端：`frontend/src/config.rs` 实现独立的 `FrontendConfig`，优先级为 `localStorage > 编译时嵌入的后端 listen_addr`。

## 2. 关键文件与包

- `common/src/config.rs`：所有配置结构体、路径计算辅助方法（`db_path`、`log_dir`、`artifacts_dir`、`agent_data_dir`、`skills_root_dir` 等）。
- `common/config/ai_orz.toml`：编译时嵌入的默认配置模板。
- `src/config.rs`：应用级配置加载器，维护全局 `OnceLock<Arc<AppConfig>>` 单例，提供 `init()` / `get()` / `try_get()`。
- `src/pkg/mod.rs`：JWT 模块通过环境变量 `JWT_SECRET`、`JWT_EXPIRY_HOURS` 覆盖 JWT 密钥与过期时间。
- `frontend/src/config.rs`：前端配置管理，从 `localStorage` 读取用户覆盖，默认值来自后端 `listen_addr`。
- `.env.example`：测试/集成环境所需的环境变量模板（`DATABASE_URL`、`SQLX_OFFLINE`、各类 `TEST_*` 模型 API Key）。
- `tests/common/env.rs`：集成测试初始化流程，调用 `ai_orz::config::init()` 复用同一套配置加载逻辑。

## 3. 架构与约定

### 3.1 基础数据目录与配置文件位置
- 固定根目录常量 `BASE_DATA_PATH = ".ai_orz"`，可通过环境变量 `AI_ORZ_BASE_PATH` 覆盖。
- 配置文件名固定为 `ai_orz.toml`，位于 `BASE_DATA_PATH` 下。
- 启动流程：
  1. 若 `BASE_DATA_PATH` 不存在则创建。
  2. 若 `ai_orz.toml` 不存在，将嵌入的默认配置写入该文件。
  3. 读取并解析 TOML 为 `AppConfig`。
  4. 根据 `logging.enable_file_log` 确保日志目录存在。

### 3.2 配置分层与覆盖顺序
| 层级 | 来源 | 说明 |
|---|---|---|
| 默认值 | `AppConfig` 字段上的 `#[serde(default)]` 及 `Default` impl | 零配置即可运行 |
| 嵌入模板 | `common/config/ai_orz.toml`（`include_str!`） | 首次运行自动写出 |
| 用户配置 | 磁盘上的 `ai_orz.toml` | 最终生效的配置源 |
| 环境变量 | `AI_ORZ_BASE_PATH`、`JWT_SECRET`、`JWT_EXPIRY_HOURS` | 运行时覆盖，不修改磁盘文件 |

### 3.3 路径组织约定
`AppConfig` 集中提供路径构造方法，所有子模块统一通过 `config.db_path()`、`config.log_dir()`、`config.artifact_path(...)` 等方式获取路径，避免硬编码字符串。路径均以 `BASE_DATA_PATH` 为根，按语义划分子目录：`agents/{id}`、`artifacts/projects/{project_id}`、`tools/{tool_id}/call_trace`、`skills`、`attachments` 等。

### 3.4 前端配置
前端 `FrontendConfig` 独立于后端，仅包含 `api_base_url`。默认值由后端 `server.listen_addr` 推导（将 `0.0.0.0:` 替换为 `localhost:` 并补全 `http://` 前缀），用户可通过 `localStorage` 中的 `ai_orz_config` 键持久化覆盖。

## 4. 约定与约束

- **配置文件格式**：必须为 TOML，使用 `toml::from_str` 解析，解析失败会映射为 `ErrorCode::ConfigInvalid`。
- **基础数据目录不可变**：`BASE_DATA_PATH` 常量固定为 `.ai_orz`，只能通过 `AI_ORZ_BASE_PATH` 环境变量整体迁移。
- **首次运行自生成**：默认配置以 `DEFAULT_CONFIG_EMBEDDED` 常量形式嵌入二进制，程序保证首次运行写出，后续不再覆盖用户修改的文件。
- **日志目录按需创建**：仅在 `logging.enable_file_log == true` 时创建日志目录。
- **JWT 敏感信息必须通过环境变量注入**：注释明确要求生产环境修改 `JWT_SECRET`，且支持 `JWT_EXPIRY_HOURS` 覆盖默认 168 小时过期。
- **测试隔离**：集成测试通过设置 `AI_ORZ_BASE_PATH` 指向临时目录，并使用 `VectorStoreType::InMemory` 避免污染开发环境。
- **消费者配置继承**：`ConsumerConfig.for_topic(topic)` 采用 topic 专属配置覆盖全局配置的合并策略，未设置的字段继承全局值。
- **向量存储后端选择**：通过 `database.vector_store_type` 枚举（`LanceDb`、`InMemory`、`Hnsw`、`SqliteVss`）切换，默认 LanceDB。
- **A2A Server 开关**：通过 `[a2a_server] enabled` 控制是否启用 JSON-RPC 端点 `/a2a`，默认关闭。

## 5. 适用性判断

本仓库实现了完整的后端运行时配置系统（TOML + 环境变量 + 嵌入式默认值 + 路径抽象），同时前端也有独立的轻量配置管理。因此该类别完全适用。