---
kind: configuration_system
name: 基于 TOML + 环境变量 + 编译期嵌入的三层配置系统
category: configuration_system
scope:
    - '**'
source_files:
    - common/src/config.rs
    - common/config/ai_orz.toml
    - src/config.rs
    - frontend/src/config.rs
    - frontend/build.rs
    - .env.example
---

## 1. 整体方案

AI Orz 采用 **TOML 配置文件 + 环境变量覆盖 + 编译期嵌入默认值** 的三层配置体系，后端与前端共享同一份 `AppConfig` 结构定义（位于 `common/src/config.rs`），通过 workspace 机制在多个 crate 间复用。

- **运行时配置源**：`.ai_orz/ai_orz.toml`（用户可编辑）
- **默认配置源**：`common/config/ai_orz.toml`（编译时嵌入二进制，首次运行自动写出）
- **环境变量覆盖**：启动时读取特定环境变量覆盖关键路径和密钥

## 2. 核心文件与职责

| 文件 | 职责 |
|---|---|
| `common/src/config.rs` | 定义 `AppConfig` 及所有子配置结构（Server、Database、Logging、Jwt、Consumer、Lark、A2aServer 等），提供 `base_data_path()` / `db_path()` / `log_dir()` / `attachments_dir()` / `artifacts_dir()` / `agent_data_dir()` / `skill_*` 等路径构造方法 |
| `src/config.rs` | 应用级配置加载器：使用 `OnceLock<Arc<AppConfig>>` 作为进程内单例，实现 `init()` / `get()` / `try_get()`；负责创建 `.ai_orz/` 目录、首次运行写出默认配置、解析 TOML、确保日志目录存在 |
| `common/config/ai_orz.toml` | 默认配置模板（server、database、frontend、logging、a2a_server、jwt 段） |
| `frontend/build.rs` | 构建期脚本：读取 `../.ai_orz/ai_orz.toml`（不存在则回退到 `common/config/ai_orz.toml`），反序列化为 `AppConfig`，再序列化生成 `compiled_config.rs` 注入 `COMPILED_CONFIG` 常量与 `get_config()` 函数 |
| `frontend/src/config.rs` | 前端配置管理：优先级为 localStorage > 编译期嵌入配置；将 `0.0.0.0` 替换为 `localhost` 以适配浏览器访问 |
| `.env.example` | 开发环境变量模板（DATABASE_URL、SQLX_OFFLINE、测试模型 API Key 等） |

## 3. 架构与约定

### 3.1 数据根目录固定化
- 基础数据目录固定为 `.ai_orz/`，可通过环境变量 `AI_ORZ_BASE_PATH` 覆盖（见 `BASE_DATA_PATH_ENV` 常量）
- 所有持久化产物（SQLite DB、向量库、日志、附件、技能、Agent 数据、工具调用追踪）均相对该根目录组织，由 `AppConfig` 上的方法统一生成路径，避免散落的路径拼接逻辑

### 3.2 默认配置“零感”初始化
- `load_config()` 检查 `.ai_orz/ai_orz.toml` 是否存在，不存在即从 `DEFAULT_CONFIG_EMBEDDED`（`include_str!("../common/config/ai_orz.toml")`）写出
- 同时确保日志目录存在（当 `logging.enable_file_log == true`）
- 这意味着部署后无需手动准备配置文件即可直接启动

### 3.3 环境变量覆盖策略
- 全局路径覆盖：`AI_ORZ_BASE_PATH` 影响 `AppConfig::base_data_path()`（每次调用都读环境变量，支持运行时切换）
- JWT 密钥与过期时间：`JWT_SECRET`、`JWT_EXPIRY_HOURS` 在 `src/pkg/mod.rs` 中通过 `get_env_or_default` 读取并覆盖配置中的对应字段
- 前端静态目录：`FRONTEND_DIST_DIR` 在 `src/lib.rs` 中覆盖 `config.frontend.dist_dir`
- 测试隔离：测试通过 `std::env::set_var("AI_ORZ_BASE_PATH", ...)` 将每个用例的数据落盘到独立临时目录

### 3.4 前后端配置同步
- 后端与前端共享 `common::config::AppConfig` 类型定义，保证配置结构一致
- 前端构建期通过 `build.rs` 将当前生效的 `ai_orz.toml` 序列化为 JSON 并嵌入前端二进制，前端运行时用 `web_sys::window().location.origin` 优先推导 `api_base_url`，无 window 环境回退到编译期嵌入的配置

### 3.5 配置结构分层
`AppConfig` 按功能域拆分子结构，全部使用 `#[serde(default)]` 保证向后兼容：
- `ServerConfig`：监听地址
- `DatabaseConfig`：SQLite 文件名、向量库文件名、向量存储后端（`VectorStoreType` 枚举：LanceDb/InMemory/Hnsw/SqliteVss）、HNSW 索引目录
- `StatsConfig`：DuckDB 统计库路径与批量大小
- `FrontendConfig`：静态资源目录
- `LoggingConfig`：是否写文件、日志子目录、格式（text/json）、保留天数
- `JwtConfig`：签名密钥、默认过期小时数
- `ConsumerConfig`：全局并发、空队列睡眠、错误重试睡眠、Topic 专属覆盖
- `LarkConfig`：飞书 App ID/Secret/加密密钥/验证令牌
- `A2aServerConfig`：协议版本、JSON-RPC 端点、Agent Card 路径

## 4. 约束与约定

- **配置文件位置固定**：始终位于 `BASE_DATA_PATH`（默认 `.ai_orz/`）下的 `ai_orz.toml`，不允许自定义文件名
- **新增配置项必须带默认值**：所有子结构字段通过 `#[serde(default)]` 或 `Default` 实现，禁止出现必填但无默认值的字段
- **敏感信息不入库**：JWT 密钥等敏感配置仅存于配置文件或环境变量，不在数据库或日志中记录明文
- **路径构造集中化**：所有磁盘路径必须通过 `AppConfig` 上的方法生成，禁止在业务代码中手写路径拼接
- **环境变量命名规范**：跨模块共享的环境变量名集中在 `common/src/config.rs` 中以 `pub const` 形式声明（如 `BASE_DATA_PATH_ENV`），其他模块引用常量而非硬编码字符串
- **前端配置优先级**：localStorage > 编译期嵌入配置；且 `0.0.0.0` 会被替换为 `localhost`，这是硬性行为，不可通过配置关闭
- **构建期依赖**：前端构建会读取 `../.ai_orz/ai_orz.toml`，若不存在则回退到源码中的默认模板；修改默认模板需重新构建前端才能生效