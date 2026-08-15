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
    - frontend/src/config.rs
    - frontend/build.rs
    - .env.example
---

## 1. 整体方案

项目采用「编译时嵌入默认配置 + 首次运行自动写出 + 运行时解析 TOML + 环境变量覆盖」的分层配置体系，后端与前端共享同一份 `AppConfig` 结构体（定义在 `common/src/config.rs`），并通过 `frontend/build.rs` 在构建时将后端配置序列化为 JSON 常量注入前端二进制。

- **默认配置源**：`common/config/ai_orz.toml`，通过 `include_str!` 编译进后端二进制（`src/config.rs` 中的 `DEFAULT_CONFIG_EMBEDDED`）。
- **持久化配置**：位于固定基础目录 `.ai_orz/ai_orz.toml`，由程序首次启动时从嵌入的默认值写出；用户修改后下次启动生效。
- **环境变量覆盖**：关键项通过 `std::env::var` 读取，如 `AI_ORZ_BASE_PATH`、`JWT_SECRET`、`JWT_EXPIRY_HOURS`、`FRONTEND_DIST_DIR`、`TEST_*` 等测试变量。
- **单例访问**：后端通过 `OnceLock<Arc<AppConfig>>` 暴露 `config::get()` / `try_get()`，全局唯一且线程安全。
- **前端配置**：`frontend/src/config.rs` 中 `FrontendConfig` 优先级为 `localStorage > 编译期注入的后端配置`，支持 `save()` / `reset_to_default()`。

## 2. 核心文件与职责

| 文件 | 职责 |
|---|---|
| `common/src/config.rs` | 定义全部配置结构体（`AppConfig`、`ServerConfig`、`DatabaseConfig`、`LoggingConfig`、`JwtConfig`、`ConsumerConfig`、`A2aServerConfig`、`LarkConfig`、`StatsConfig`、`FrontendConfig`）、默认值函数、路径计算辅助方法（`db_path`、`log_dir`、`artifacts_dir`、`agent_data_dir`、`skills_root_dir`、`tool_call_trace_dir` 等） |
| `common/config/ai_orz.toml` | 默认配置模板，注释说明各字段含义及环境变量覆盖关系 |
| `src/config.rs` | 后端配置加载器：检查并创建 `.ai_orz` 目录、不存在则写出默认 TOML、解析 TOML 到 `AppConfig`、确保日志目录存在，并通过 `OnceLock` 提供全局单例 |
| `frontend/build.rs` | 构建期脚本：读取 `.ai_orz/ai_orz.toml`（不存在则回退到嵌入默认值），反序列化为 `AppConfig`，再序列化为 JSON 常量 `COMPILED_CONFIG` 写入 `OUT_DIR/compiled_config.rs`，供前端 `get_config()` 使用 |
| `frontend/src/config.rs` | 前端配置管理：`FrontendConfig` 将后端监听地址转换为 `api_base_url`（`0.0.0.0` → `localhost`），通过 `localStorage` 持久化用户覆盖 |
| `.env.example` | 测试环境所需的环境变量清单（`DATABASE_URL`、`SQLX_OFFLINE`、`TEST_*` 模型集成变量） |

## 3. 架构与约定

### 3.1 数据目录约定
- 固定根目录常量 `BASE_DATA_PATH = ".ai_orz"`，可通过环境变量 `AI_ORZ_BASE_PATH` 覆盖。
- 所有运行时产物（数据库、日志、附件、技能、Agent 数据、工具追踪日志等）均位于该目录下，由 `AppConfig` 的路径方法统一生成，避免散落硬编码路径。

### 3.2 配置加载流程
1. 进程启动调用 `src/config::init()`。
2. 读取 `AI_ORZ_BASE_PATH` 或默认 `.ai_orz`，确保目录存在。
3. 若 `.ai_orz/ai_orz.toml` 不存在，写入嵌入的默认配置。
4. 用 `toml::from_str` 解析为 `AppConfig`，错误映射为 `ErrorCode::ConfigInvalid`。
5. 根据 `logging.enable_file_log` 创建日志子目录。
6. 存入 `OnceLock` 全局单例，后续通过 `config::get()` 获取。

### 3.3 环境变量覆盖策略
- 应用级：`AI_ORZ_BASE_PATH` 覆盖数据根目录。
- JWT：`JWT_SECRET`、`JWT_EXPIRY_HOURS` 直接读环境变量（见 `src/pkg/mod.rs` 中的 `get_env` 辅助函数）。
- 前端静态资源：`FRONTEND_DIST_DIR` 可覆盖 `dist` 目录名（见 `src/lib.rs`）。
- 测试：`TEST_LLM_*`、`TEST_EMBEDDING_*`、`DATABASE_URL`、`SQLX_OFFLINE` 等控制可选测试行为。

### 3.4 前后端配置同步
后端 `AppConfig` 是单一事实来源。`frontend/build.rs` 在构建时读取同一份 TOML，将其序列化为 JSON 常量嵌入前端二进制。前端 `FrontendConfig::default()` 基于后端 `server.listen_addr` 推导 `api_base_url`，并将 `0.0.0.0` 替换为 `localhost` 以适配浏览器访问。用户可在前端设置页通过 `localStorage` 覆盖 API 地址。

### 3.5 配置结构组织
所有配置段通过 `#[serde(default)]` 实现字段级默认值，每个段都有独立的 `Default` 实现和 `default_xxx()` 函数，新增配置段需遵循相同模式：定义结构体 → 添加 `AppConfig` 字段 → 提供默认值函数 → 如需路径派生则添加 `AppConfig` 方法。

## 4. 约束与规则

- **数据目录不可变**：`.ai_orz` 是固定基础目录（除非通过 `AI_ORZ_BASE_PATH` 显式覆盖），所有路径必须通过 `AppConfig` 的方法派生，禁止散落的字符串拼接。
- **配置文件自动生成**：首次运行自动写出默认 TOML，用户不应手动编辑嵌入模板，应编辑 `.ai_orz/ai_orz.toml`。
- **TOML 解析失败即报错**：解析错误统一映射为 `ErrorCode::ConfigInvalid`，不会静默降级。
- **JWT 密钥必须覆盖**：`jwt.secret` 注释明确要求生产环境修改，或通过 `JWT_SECRET` 环境变量注入。
- **前端 API 地址推导规则**：监听地址以 `http://` 或 `https://` 开头则直接使用，否则前缀补 `http://`；`0.0.0.0` 被替换为 `localhost`。
- **测试隔离**：真实模型相关测试通过 `TEST_*` 环境变量开关，未设置时跳过，保证 CI 安全。
- **配置变更触发重建**：`frontend/build.rs` 声明 `cargo:rerun-if-changed=../.ai_orz/ai_orz.toml` 与 `../common/config/ai_orz.toml`，任一变化都会重新生成前端配置常量。
