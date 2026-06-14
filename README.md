# ai_orz

AI 代理执行框架 - Full-stack Rust + Dioxus

![GitHub last commit](https://img.shields.io/github/last-commit/runningcoders/ai_orz)
![GitHub license](https://img.shields.io/github/license/runningcoders/ai_orz)
![Rust](https://img.shields.io/badge/Rust-1.85+-000000?logo=rust)
![Tests](https://img.shields.io/badge/tests-267%20%E2%9C%94-brightgreen)
[![GitHub stars](https://img.shields.io/github/stars/runningcoders/ai_orz?style=social)](https://github.com/runningcoders/ai_orz)

---

## 技术栈

- **后端**: Rust + Axum + SQLite + [rig-core](https://docs.rs/rig-core/latest/rig/) (LLM 调用框架)
- **前端**: Dioxus 0.7 (WebAssembly)
- **common**: 独立 crate，存放前后端共享的 DTO、枚举、常量（API 契约统一）
- **构建**: dioxus-cli + cargo workspace

---

## 已实现功能特性 ✅

| 模块 | 状态 | 说明 |
|------|------|------|
| 👥 组织用户权限 | ✅ | 多级组织、用户角色、JWT 认证 |
| 🤖 Agent 完整生命周期 | ✅ | 创建、配置、绑定工具、唤醒执行 |
| ⚙️ Agent 运行时配置 | ✅ | JSON 格式可扩展配置，支持最大思考深度等参数 |
| 🧠 四层记忆系统 | ✅ | Core/Working/Short-term/Long-term 分级存储 |
| 💬 全功能消息对话 | ✅ | 用户 ↔ Agent 双向对话，支持项目上下文，工具调用消息复用消息表 |
| 📨 消息渠道系统 | ✅ | 多渠道消息接入，支持启用/禁用/连接测试 |
| 🛠️ 混合模式工具调用 | ✅ | 简单工具走 rig 原生 auto，关键工具走自建 manual 可控链路 |
| 📚 技能库 | ✅ | 可复用技能和工作流管理，支持搜索和分类 |
| 📋 任务项目管理 | ✅ | 项目聚合对话，任务跟踪进度、思考深度、优先级 |
| 📎 统一附件存储 | ✅ | 消息附件和项目产物统一存储 |
| 🚀 异步消费者系统 | ✅ | 通用消费者框架 + Message Topic 三层分发（Agent/User/System） |
| 📝 结构化日志系统 | ✅ | JSON 格式，自动上下文关联，日志自动清理 |
| 🏗️ 严格分层架构 | ✅ | DAO/DAL/Domain/Handler/Consumer 五层职责清晰 |
| 🔍 单元测试覆盖 | ✅ | 数据层 + 领域层 100% 测试覆盖 |

> 📝 **开发规范与详细架构** 请查看 [AGENTS.md](./AGENTS.md) - AI 助手专属快速入门手册

---

## 项目结构

```
ai_orz/
├── common/             # 公共共享类型（前后端共用 DTO、枚举、配置、常量）
│   ├── config/         # 默认配置模板 ai_orz.toml
│   └── src/
│       ├── api/        # API 请求响应 DTO 按功能分组
│       ├── config.rs   # 应用配置结构体定义 + 默认常量
│       ├── constants/  # 公共常量、基础类型
│       └── enums/      # 公共枚举
│
├── src/                # 后端源码
│   ├── handlers/       # HTTP 接口层（按业务域分组，每个方法一个文件）
│   ├── service/
│   │   ├── dao/        # 数据访问层 DAO（单一数据源操作）
│   │   ├── dal/        # 业务数据访问层 DAL（组合 DAO）
│   │   └── domain/     # 领域层（核心业务逻辑编排）
│   ├── middleware/     # Axum 中间件
│   ├── models/         # 持久化实体 PO
│   ├── consumer/       # 异步消费者系统
│   └── pkg/            # 公共工具包
│
├── frontend/            # Dioxus 前端源码
│   ├── build.rs        # 编译时读取配置嵌入前端
│   └── src/
│       ├── config.rs   # 前端运行时配置管理（支持 localStorage 读写）
│       └── components/  # UI 组件
│
├── dist/               # 前端静态文件（生产构建输出）
├── docs/               # 详细设计文档
├── build-full.sh       # 全量构建脚本
├── start-dev.sh        # 一键启动开发环境
├── AGENTS.md           # 🎯 AI 开发规范总览 + 文档索引（必读）
└── README.md
```

---

## 文档快速索引

| 文档 | 说明 | 优先级 |
|------|------|--------|
| [AGENTS.md](./AGENTS.md) | 🎯 **AI 开发规范总览** - 项目是什么、怎么开发、完整文档索引 | ⭐⭐⭐ |
| [docs/ARCHITECTURE.md](./docs/ARCHITECTURE.md) | 完整架构说明、核心概念解释、实体关系 | ⭐⭐⭐ |
| [docs/LAYERED_ARCHITECTURE_PRACTICE.md](./docs/LAYERED_ARCHITECTURE_PRACTICE.md) | **开发记录** - 分层架构落地、反模式、避坑指南、经验总结 | ⭐⭐⭐ |
| [docs/sqlx_guide.md](./docs/sqlx_guide.md) | SQLx 0.8 + SQLite 开发规范、枚举映射、测试隔离 | ⭐⭐⭐ |
| [docs/tool_design.md](./docs/tool_design.md) | 混合模式工具调用、工具注册表、调用追踪 | ⭐⭐ |
| [docs/message_interaction_design.md](./docs/message_interaction_design.md) | 消息交互架构、工具调用复用消息表 | ⭐⭐ |
| [docs/consumer_architecture.md](./docs/consumer_architecture.md) | 异步消费者框架、按 to_role 分层分发 | ⭐⭐ |

> 📖 完整文档列表请查看 [AGENTS.md](./AGENTS.md)

---

## 配置

项目使用 **TOML 配置文件** 管理配置，默认配置嵌入在二进制中：

### 配置机制

1. 首次运行时，如果项目根目录不存在 `ai_orz.toml`，程序会自动从嵌入的默认配置生成一份
2. 你可以修改 `ai_orz.toml` 自定义配置
3. 前端在编译时会自动读取配置文件，将默认配置嵌入前端产物
4. **前端运行时可修改配置**：在浏览器设置页面修改 API 地址，修改保存到浏览器 `localStorage`，刷新页面生效，无需重新编译

### 配置文件格式

```toml
base_data_path = "data"

[server]
listen_addr = "0.0.0.0:3000"

[database]
db_file_name = "ai_orz.db"

[frontend]
dist_dir = "dist"

[logging]
enable_file_log = true
log_subdir = "logs"
format = "json"           # 日志格式: "json" (默认) 或 "text"
retention_days = 30       # 日志保留天数，0 表示不清理

[jwt]
# JWT签名密钥（生产环境务必修改！也可以通过环境变量 JWT_SECRET 设置）
# secret = "your-secret-key-here"
# default_expiry_hours = 168
```

### 环境变量覆盖

| 环境变量 | 对应配置项 | 说明 |
|----------|------------|------|
| `JWT_SECRET` | `jwt.secret` | JWT 签名密钥 |
| `JWT_EXPIRY_HOURS` | `jwt.default_expiry_hours` | JWT 过期时间（小时） |

---

## 快速开始

### 开发模式

```bash
./start-dev.sh
```

- 后端服务: http://localhost:3000
- 前端开发服务器 (热重载): http://localhost:8080

### 生产构建

```bash
./build-full.sh
```

输出：
- 后端二进制: `target/release/ai_orz`
- 前端静态文件: `dist/`

### 运行生产版本

```bash
./target/release/ai_orz
```

服务启动后监听 `0.0.0.0:${SERVER_PORT:-3000}`，前端静态文件从 `dist/` 目录提供。

---

## 端口说明

| 服务 | 默认端口 | 说明 |
|------|----------|------|
| 后端 API | 3000 | 提供 REST API 和静态文件服务 |
| 前端开发服务器 | 8080 | dx serve 热重载开发服务器 |

---

## 单元测试

运行所有单元测试：

```bash
cargo test
```

**测试状态：** **323 个测试全部通过 ✅**

---

## 日志系统使用规范 📝

### 重要：必须使用封装的日志库

**所有业务代码必须使用 `crate::pkg::logging` 模块提供的日志函数，禁止直接使用 `tracing::info!`/`tracing::error!` 等宏。**

### 为什么？

1. **自动上下文关联**：每条日志自动携带完整的 RequestContext 7 个字段（log_id/user_id/username/organization_id/agent_id/task_id/project_id + operation）
2. **结构化 JSON 输出**：便于 ELK/Loki 等日志分析系统采集和查询
3. **日志自动清理**：自动清理过期日志，避免磁盘空间耗尽
4. **统一格式**：所有日志格式一致，便于排查问题

### 正确使用方式（宏，推荐）

```rust
// 直接使用宏，无需导入（已在 crate 根导出）
// 支持格式化字符串，自动携带正确的文件/行号信息

// 简单消息
log_info!(ctx, "send_message", "消息发送成功");
log_warn!(ctx, "memory_warning", "Agent 内存使用率超过 80%");
log_error!(ctx, "db_error", "数据库查询超时");
log_debug!(ctx, "debug_info", "调试信息");

// 支持格式化
log_info!(ctx, "create_agent", "用户 {} 创建了 Agent {}", username, agent_id);

// 系统启动/无上下文场景（仅在 main/框架初始化时使用）
use crate::pkg::logging::{system_info, system_warn, system_error};
system_info("服务启动成功");
```

### 为什么要用宏？

| 特性 | 函数封装 | 宏封装 |
|------|---------|--------|
| 文件/行号信息 | ❌ 全部显示 logging.rs 位置 | ✅ 正确显示调用位置 |
| 格式化支持 | ✅ `format!()` 后传入 | ✅ 原生支持 `log_info!(ctx, "op", "a={}", a)` |
| 导入便利性 | 需要导入函数 | 无需导入，宏在 crate 根可用 |
| 性能 | 相同 | 相同 |

### 日志字段说明（JSON 格式）

```json
{
  "timestamp": "2026-05-14T15:30:45.123456Z",
  "level": "INFO",
  "target": "ai_orz::service::domain::message",
  "filename": "src/service/domain/message/management.rs",
  "line_number": 123,
  "span": {
    "log_id": "abc-123-xyz-456",
    "user_id": "user_123",
    "username": "张三",
    "organization_id": "org_456",
    "agent_id": "agent_789",
    "task_id": "task_abc",
    "project_id": "proj_def",
    "operation": "send_message"
  },
  "message": "消息发送成功"
}
```

---

## 前端已实现功能

- ✅ 顶部导航栏（权限控制下拉菜单）
  - 前台接待
  - 人力资源 → Agent 管理
  - 财务管理 → 模型管理
  - 项目管理 → 项目列表
  - 任务管理 → 任务列表
- ✅ 前台接待/登录流程（系统初始化检测 + 组织选择）
- ✅ 个人信息页（查看修改）
- ✅ 组织信息页（仅管理员）
- ✅ ⚙️ 设置页（运行时修改 API 地址，保存到 localStorage）
- ✅ 用户管理页（仅管理员）
- ✅ Agent 管理列表
- ✅ Model Provider 管理列表（自动测试连通性）
- ✅ 项目管理列表
- ✅ 任务管理列表（状态更新）

---

## API 接口速览

| 领域 | 路径前缀 | 功能 |
|------|---------|------|
| HR (人力资源) | `/api/v1/hr/agents` | Agent CRUD |
| Finance (财务管理) | `/api/v1/finance/model-providers` | LLM 模型配置管理、调用测试 |
| 组织用户权限 | `/api/v1/organization/*`, `/api/v1/auth/*` | 系统初始化、登录、组织/用户管理 |
| 项目管理 | `/api/v1/projects` | 项目 CRUD |
| 任务管理 | `/api/v1/tasks` | 任务 CRUD、状态更新 |
| 消息对话 | `/api/v1/projects/{id}/messages` | 发送消息、接收回复、触发 Agent 执行 |
| 消息渠道 | `/api/v1/finance/message-channels` | 消息渠道 CRUD、状态更新、连接测试 |
| 技能库 | `/api/v1/skills` | 技能 CRUD、搜索 |
| 工具管理 | `/api/v1/finance/tools` | 工具 CRUD、状态更新、Agent 绑定/解绑工具 |

---

## License

[Apache License 2.0](LICENSE)
