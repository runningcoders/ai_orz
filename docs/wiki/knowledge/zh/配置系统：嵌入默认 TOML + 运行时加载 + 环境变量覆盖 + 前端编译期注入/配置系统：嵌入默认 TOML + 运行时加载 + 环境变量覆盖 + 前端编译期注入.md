---
kind: configuration_system
name: 配置系统：嵌入默认 TOML + 运行时加载 + 环境变量覆盖 + 前端编译期注入
category: configuration_system
scope:
    - '**'
source_files:
    - common/src/config.rs
    - common/config/ai_orz.toml
    - src/config.rs
    - src/lib.rs
    - src/pkg/mod.rs
    - frontend/src/config.rs
    - frontend/build.rs
    - frontend/Dioxus.toml
    - scripts/start.sh
    - .env.example
    - tests/common/env.rs
---

## 1. 整体方案

项目采用「编译时嵌入默认配置 + 首次运行自动写出 + 运行时解析 TOML + 环境变量覆盖 + dx dev proxy」的分层配置体系，后端与前端共享同一份 `AppConfig` 结构体（定义在 `common/src/config.rs`），并通过 `frontend/build.rs` 在构建时将后端配置序列化为 JSON 常量注入前端二进制。开发模式下由 `frontend/Dioxus.toml` 的 `[[web.proxy]]` 将 `/api/*` 反代到后端 3000，解决 dx 同源占位页问题。

- **默认配置源**：`common/config/ai_orz.toml`，通过 `include_str!` 编译进后端二进制（`src/config.rs` 中的 `DEFAULT_CONFIG_EMBEDDED`）。
- **持久化配置**：位于固定基础目录 `.ai_orz/ai_orz.toml`，由程序首次启动时从嵌入的默认值写出；用户修改后下次启动生效。
- **环境变量覆盖**：关键项通过 `std::env::var` 读取，如 `AI_ORZ_BASE_PATH`、`JWT_SECRET`、`JWT_EXPIRY_HOURS`、`FRONTEND_DIST_DIR`、`TEST_*` 等测试变量；**dev 模式专用变量 `DX_BACKEND_URL`** 用于覆盖 `Dioxus.toml` 中 dx 视角的 proxy backend（前后端分机部署逃生舱）。
- **单例访问**：后端通过 `OnceLock<Arc<AppConfig>>` 暴露 `config::get()` / `try_get()`，全局唯一且线程安全。
- **前端配置**：`frontend/src/config.rs` 中 `FrontendConfig` 优先级为 `localStorage > 浏览器 origin 动态探测 > 编译期注入的后端配置`，支持 `save()` / `reset_to_default()` / `clear_saved()`（删除 localStorage 键，恢复 origin 动态探测，而非持久化点击瞬间快照）。

## 2. 核心文件与职责

| 文件 | 职责 |
|---|---|
| `common/src/config.rs` | 定义全部配置结构体（`AppConfig`、`ServerConfig`、`DatabaseConfig`、`LoggingConfig`、`JwtConfig`、`ConsumerConfig`、`A2aServerConfig`、`LarkConfig`、`StatsConfig`、`FrontendConfig`）、默认值函数、路径计算辅助方法（`db_path`、`log_dir`、`artifacts_dir`、`agent_data_dir`、`skills_root_dir`、`tool_call_trace_dir` 等） |
| `common/config/ai_orz.toml` | 默认配置模板，注释说明各字段含义及环境变量覆盖关系 |
| `src/config.rs` | 后端配置加载器：检查并创建 `.ai_orz` 目录、不存在则写出默认 TOML、解析 TOML 到 `AppConfig`、确保日志目录存在，并通过 `OnceLock` 提供全局单例 |
| `src/lib.rs` | 应用启动顺序：`config::init` → `pkg::init_all` → `service::init` → `producer/consumer::init` → AOP 调度器 → HTTP server；前端静态目录 `FRONTEND_DIST_DIR` 覆盖 |
| `src/pkg/mod.rs` | JWT 模块通过环境变量 `JWT_SECRET`、`JWT_EXPIRY_HOURS` 覆盖 JWT 密钥与过期时间（`get_env` 辅助函数） |
| `frontend/build.rs` | 构建期脚本：读取 `.ai_orz/ai_orz.toml`（不存在则回退到嵌入默认值），反序列化为 `AppConfig`，再序列化为 JSON 常量 `COMPILED_CONFIG` 写入 `OUT_DIR/compiled_config.rs`，供前端 `get_config()` 使用；声明 `cargo:rerun-if-changed` 监听两个配置文件变化 |
| `frontend/src/config.rs` | 前端配置管理：`FrontendConfig` 将后端监听地址转换为 `api_base_url`（`0.0.0.0` → `localhost`），通过 `localStorage` 持久化用户覆盖；优先级 localStorage > 浏览器 origin 动态探测 > 编译期注入配置；提供 `clear_saved()` 删除 localStorage 键恢复自动探测（区别于 `reset_to_default()` + `save` 会持久化 origin 快照） |
| `frontend/Dioxus.toml` | Dioxus CLI 配置：`[[web.proxy]] backend="http://localhost:3000/api"` 将 dx dev server 的 `/api/*` 反代到后端 3000（**dx 进程视角的 localhost**，不是浏览器视角）；dev 模式专用，`dx build --release` 不受影响；`watch_path=["src","styles","index.html"]` 避免监听 build.rs 产物导致多余重编译 |
| `scripts/start.sh` | dev 模式入口：`DX_BACKEND_URL` 环境变量临时覆盖 `Dioxus.toml` proxy backend（dx 进程视角地址），支持前后端分机部署（如 `DX_BACKEND_URL=http://192.168.1.5:3000/api ./scripts/start.sh dev`）；启动前 `preflight_deps` + `preflight_cleanup` 保证环境就绪；`--interactive=false` 禁用 dx TUI 避免 Ctrl+C 卡死 |
| `.env.example` | 测试环境所需的环境变量清单（`DATABASE_URL`、`SQLX_OFFLINE`、`TEST_*` 模型集成变量） |
| `tests/common/env.rs` | 集成测试初始化流程，调用 `ai_orz::config::init()` 复用同一套配置加载逻辑；通过设置 `AI_ORZ_BASE_PATH` 指向临时目录实现数据隔离 |

## 3. 架构与约定

### 3.1 数据目录约定
- 固定根目录常量 `BASE_DATA_PATH = ".ai_orz"`，可通过环境变量 `AI_ORZ_BASE_PATH` 覆盖。
- 所有运行时产物（数据库、日志、附件、技能、Agent 数据、工具追踪日志等）均位于该目录下，由 `AppConfig` 的路径方法统一生成，避免散落硬编码路径。

### 3.2 配置加载流程
1. 进程启动调用 `src/config::init()`。
2. 读取 `AI_ORZ_BASE_PATH` 或默认 `.ai_orz`，确保目录存在。
3. 若 `.ai_orz/ai_orz.toml` 不存在，写入嵌入的默认配置。
4. 用 `toml::from_str` 解析为 `AppConfig`，错误映射为 `ErrorCode::ConfigInvalid`。
5. **安全配置强制校验**：解析后检查 `security.secret_key` 是否为空，若为空则自动生成默认值 `ai-orz-default-secret-key-change-me` 并持久化回配置文件，同时打印警告（用于数据库敏感字段加密）。
6. 根据 `logging.enable_file_log` 创建日志子目录。
7. 存入 `OnceLock` 全局单例，后续通过 `config::get()` 获取。

### 3.3 环境变量覆盖策略
- 应用级：`AI_ORZ_BASE_PATH` 覆盖数据根目录（每次调用 `base_data_path()` 都读环境变量，支持运行时切换）。
- JWT：`JWT_SECRET`、`JWT_EXPIRY_HOURS` 直接读环境变量（见 `src/pkg/mod.rs` 中的 `get_env` 辅助函数）。
- 前端静态资源：`FRONTEND_DIST_DIR` 可覆盖 `dist` 目录名（见 `src/lib.rs`）。
- 测试：`TEST_LLM_*`、`TEST_EMBEDDING_*`、`DATABASE_URL`、`SQLX_OFFLINE` 等控制可选测试行为。
- 跨模块共享的环境变量名集中在 `common/src/config.rs` 中以 `pub const` 形式声明（如 `BASE_DATA_PATH_ENV`），其他模块引用常量而非硬编码字符串。

### 3.4 前后端配置同步
后端 `AppConfig` 是单一事实来源。`frontend/build.rs` 在构建时读取同一份 TOML，将其序列化为 JSON 常量嵌入前端二进制。前端 `FrontendConfig::default()` 优先使用浏览器当前 origin 作为 `api_base_url`（同源部署假设），无 window 环境再回退到编译期嵌入的配置。用户可在前端设置页通过 `localStorage` 覆盖 API 地址；**重置行为分两档**：`reset_to_default()` 仅清空内存表单值，`clear_saved()` 真正删除 `ai_orz_config` 键以恢复 origin 动态探测。

### 3.5 dev 模式 API 代理（dx 专用）
`frontend/Dioxus.toml` 的 `[[web.proxy]] backend="http://localhost:3000/api"` 将 dx dev server 的 `/api/*` 反代到后端 3000。设计要点：
- **dx 进程视角**：`backend` 是 dx serve 进程发起转发连接的目标地址，不是浏览器视角——`start.sh` 同机启动 dx 与后端时 `localhost` 恒正确，本地/远程沙箱/新机器场景均无需修改。
- **逃生舱 `DX_BACKEND_URL`**：前后端分机部署（如前端 dev 在本机、后端在远端）时通过 `DX_BACKEND_URL=http://192.168.1.5:3000/api ./scripts/start.sh dev` 临时覆盖；启动前备份 `Dioxus.toml`，退出时自动恢复。
- **不影响 prod**：`dx build --release` 不受 `[[web.proxy]]` 影响；prod 同源部署下前端直接以浏览器 origin 为 `api_base_url`。
- **解决的问题**：无代理时 dev 模式前端 API 请求打到 dx 的 `index_on_404` 占位页（200+HTML），触发 "200: error decoding response body"。

### 3.6 配置结构组织
所有配置段通过 `#[serde(default)]` 实现字段级默认值，每个段都有独立的 `Default` 实现和 `default_xxx()` 函数，新增配置段需遵循相同模式：定义结构体 → 添加 `AppConfig` 字段 → 提供默认值函数 → 如需路径派生则添加 `AppConfig` 方法。**完整配置结构分层**：
- `ServerConfig`：监听地址
- `DatabaseConfig`：SQLite 文件名、向量库文件名、**向量存储后端（`VectorStoreType` 枚举：LanceDb/InMemory/Hnsw/SqliteVss）**、HNSW 索引目录
- `StatsConfig`：DuckDB 统计库路径与批量大小
- `FrontendConfig`：静态资源目录
- `LoggingConfig`：是否写文件、日志子目录、格式（text/json）、保留天数
- `JwtConfig`：签名密钥、默认过期小时数
- `ConsumerConfig`：全局并发、空队列睡眠、错误重试睡眠、**Topic 专属覆盖（`for_topic(topic)` 合并策略：topic 字段优先，未设置继承全局）**
- `LarkConfig`：飞书 App ID/Secret/加密密钥/验证令牌
- `A2aServerConfig`：协议版本、JSON-RPC 端点、Agent Card 路径、**`enabled` 开关控制是否启用 JSON-RPC 端点 `/a2a`，默认关闭**
- `SecurityConfig`：敏感字段加密密钥 secret_key

### 3.7 消费者配置继承
`ConsumerConfig` 支持全局默认值 + `topics` 哈希表 per-topic 覆盖，通过 `for_topic(topic)` 合并得到最终配置（topic 字段优先，未设置则继承全局），新增 topic 无需修改全局并发参数即可单独调优。

## 4. 约束与规则

1. **数据目录不可变**：`.ai_orz` 是固定基础目录（除非通过 `AI_ORZ_BASE_PATH` 显式覆盖），所有路径必须通过 `AppConfig` 的方法派生，禁止散落的字符串拼接。
2. **配置文件自动生成**：首次运行自动写出默认 TOML，用户不应手动编辑嵌入模板，应编辑 `.ai_orz/ai_orz.toml`。
3. **TOML 解析失败即报错**：解析错误统一映射为 `ErrorCode::ConfigInvalid`，不会静默降级。
4. **JWT 密钥必须覆盖**：`jwt.secret` 注释明确要求生产环境修改，或通过 `JWT_SECRET` 环境变量注入。
5. **前端 API 地址推导规则**：监听地址以 `http://` 或 `https://` 开头则直接使用，否则前缀补 `http://`；`0.0.0.0` 被替换为 `localhost`。
6. **测试隔离**：真实模型相关测试通过 `TEST_*` 环境变量开关，未设置时跳过，保证 CI 安全；集成测试通过 `std::env::set_var("AI_ORZ_BASE_PATH", ...)` 指向临时目录，确保测试不污染真实数据。
7. **配置变更触发重建**：`frontend/build.rs` 声明 `cargo:rerun-if-changed=../.ai_orz/ai_orz.toml` 与 `../common/config/ai_orz.toml`，任一变化都会重新生成前端配置常量。
8. **secret_key 不可为空**：启动时强制校验并自动补写，属于硬性安全约束；`jwt.secret`、`lark.app_secret`、`lark.encrypt_key`、`lark.verification_token` 等敏感字段建议通过环境变量注入，而非写入配置文件。
9. **配置单例不可变**：通过 `Arc<AppConfig>` + `OnceLock` 暴露只读引用，运行时不允许修改。
10. **新增配置项必须带默认值**：所有子结构字段通过 `#[serde(default)]` 或 `Default` 实现，禁止出现必填但无默认值的字段。
11. **敏感信息不入库**：JWT 密钥等敏感配置仅存于配置文件或环境变量，不在数据库或日志中记录明文。
12. **配置文件位置固定**：始终位于 `BASE_DATA_PATH`（默认 `.ai_orz/`）下的 `ai_orz.toml`，不允许自定义文件名。
13. **向量存储后端选择**：通过 `database.vector_store_type` 枚举切换，默认 LanceDB；测试使用 `InMemory`。
14. **构建期依赖**：前端构建会读取 `../.ai_orz/ai_orz.toml`，若不存在则回退到源码中的默认模板；修改默认模板需重新构建前端才能生效。
15. **dev proxy 不可删除**：`frontend/Dioxus.toml` 中的 `[[web.proxy]] backend="http://localhost:3000/api"` 是 dev 模式 API 请求打通的必要配置，禁止删除；前后端分机部署时用 `DX_BACKEND_URL` 环境变量覆盖，不要直接修改文件。
16. **重置默认 = 删除键而非保存快照**：前端设置页「重置为默认」必须调用 `clear_saved()` 删除 `ai_orz_config` 键（恢复 origin 动态探测），不能用 `reset_to_default() + save`（会把点击瞬间的 origin 快照持久化，换环境仍被旧快照粘住）。
