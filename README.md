# ai_orz

AI 代理执行框架 - Full-stack Rust + Dioxus

## 技术栈

- **后端**: Rust + Axum + SQLite + [rig-core](https://docs.rs/rig-core/latest/rig/) (LLM 调用框架)
- **前端**: Dioxus 0.7 (WebAssembly)
- **构建**: dioxus-cli + cargo workspace

## 项目结构

```
ai_orz/
├── src/                # 后端源码
├── frontend/            # Dioxus 前端源码
├── dist/               # 编译好的前端静态文件（生产构建输出）
├── build-full.sh        # 全量构建脚本（后端 + 前端）
├── start-dev.sh         # 一键启动开发环境（后端 + dx serve 热重载）
└── README.md
```

## 架构（后端）

```
src/
├── models/              # 实体定义
│   ├── brain.rs         # 🧠 Brain 实体 + Cortex 思考接口
│   ├── agent.rs         # Agent 实体
│   ├── model_provider.rs # 模型提供商实体
│   ├── organization.rs  # 组织实体
│   └── task.rs          # 任务实体
├── pkg/                # 工具和基础设施
│   ├── constants/      # 常量定义（状态枚举）
│   ├── logging.rs       # 日志模块（带 Context）
│   └── storage/        # 存储管理
│       ├── mod.rs       # 全局连接管理
│       └── sql.rs      # SQL 常量定义（SQLITE_CREATE_TABLE_*）
├── service/
│   └── dao/            # DAO 层（数据访问操作）
│       ├── brain/       # 🧠 Brain DAO - 大脑工厂
│       │   ├── mod.rs     # BrainDao trait + OnceLock 单例
│       │   ├── rig.rs    # Rig 框架实现 (RigBrainDao)
│       │   ├── rig/     # 具体 Cortex 实现
│       │   │   ├── openai.rs
│       │   │   ├── openai_compatible.rs
│       │   │   └── ollama.rs
│       │   └── rig_test.rs # 单元测试
│       ├── agent/       # Agent 存储 DAO
│       │   ├── mod.rs   # 接口定义
│       │   └── sqlite.rs # SQLite 实现
│       ├── model_provider/ # 模型提供商存储 DAO
│       │   ├── mod.rs   # 接口定义
│       │   └── sqlite.rs # SQLite 实现
│       └── org/         # 组织存储 DAO
│           ├── mod.rs   # 接口定义
│           └── sqlite.rs # SQLite 实现
└── handlers/          # HTTP 接口
    ├── mod.rs           # 导出 + 通用 ApiResponse
    └── health.rs       # 健康检查
```

## 核心概念

| 概念 | 职责 |
|------|------|
| **ModelProvider** | 独立实体，保存模型配置（提供商类型、模型名称、API Key、自定义地址） |
| **Brain** | 顶层实体，包装 `Cortex`，由 `BrainDao` 根据 `ModelProvider` 创建 |
| **Cortex** 🧠 | 思考推理接口，不同提供商实现不同 |
| **BrainDao** | 工厂 DAO，统一创建 `Brain` 和执行 `prompt` |

## LLM 调用流程

```rust
// 业务层调用方式
use service::dao::{brain_dao, model_provider_dao};

// 1. 查询模型配置（从数据库）
let provider = model_provider_dao.find_by_id(ctx, "provider-id")?;

// 2. 创建 Brain（通过工厂）
let brain = brain_dao.create_brain(&provider)?;

// 3. 执行推理（通过 BrainDao）
let result = brain_dao.prompt(&brain, "你好").await?;
```

## 支持的模型提供商

| 提供商 | 支持 |
|--------|------|
| OpenAI 官方 | ✅ |
| DeepSeek | ✅ |
| 阿里云通义千问 | ✅ |
| 字节跳动豆包 | ✅ |
| Ollama 本地 | ✅ |
| 自定义 OpenAI 兼容接口 | ✅ |

## 设计原则

1. **分层清晰**
   - DAO：只负责底层操作，不关心业务
   - DAL：组合 DAO，构建业务对象，不关心业务逻辑
   - Domain：实现业务逻辑，不关心 HTTP 和存储
   - Handler：只负责 HTTP 接口处理，不关心业务

2. **优雅命名 🧠**
   - **Brain** → 整个智能体大脑
   - **Cortex** → 大脑皮层，负责具体思考推理
   > "brain 里用来思考的部分就是大脑皮层"

3. **编码规范**
   - 文件：`snake_case.rs`
   - 函数/变量：`snake_case`
   - 类型/结构体：`PascalCase`
   - 常量：`SQLITE_CREATE_TABLE_AGENTS`（全大写+下划线）

4. **测试分离**
   - 核心逻辑：`xxx.rs`
   - 单元测试：`xxx_test.rs`
   - 保持核心文件干净

5. **Context 传递**
   - 所有公共方法第一个参数都是 `ctx: RequestContext`
   - `created_by`/`modified_by` 从 `ctx.uid()` 获取
   - 保证链路完整性，便于日志追踪

6. **分层初始化**
   - `dao::init_all()` 初始化所有 DAO
   - `dal::init_all()` 初始化所有 DAL（依赖已初始化的 DAO）
   - `domain::init_all()` 初始化所有 Domain（依赖已初始化的 DAL）
   - `service::init()` 按顺序调用三层初始化
   - 每层只负责自己的实例，结构清晰，易于扩展

## 配置

项目配置通过环境变量读取：

| 环境变量 | 默认值 | 说明 |
|----------|--------|------|
| `SERVER_HOST` | `0.0.0.0` | 服务监听地址 |
| `SERVER_PORT` | `3000` | 服务监听端口 |
| `DATABASE_URL` | `data/ai_orz.db` | SQLite 数据库路径 |

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

## API 接口

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/agents` | 创建 Agent |
| GET | `/api/v1/agents` | 列出所有 Agent |
| GET | `/api/v1/agents/:id` | 获取单个 Agent |
| PUT | `/api/v1/agents/:id` | 更新 Agent |
| DELETE | `/api/v1/agents/:id` | 删除 Agent |
| POST | `/api/v1/model-providers` | 创建模型提供商 |
| GET | `/api/v1/model-providers` | 列出所有模型提供商 |
| GET | `/api/v1/model-providers/:id` | 获取单个模型提供商 |
| PUT | `/api/v1/model-providers/:id` | 更新模型提供商 |
| DELETE | `/api/v1/model-providers/:id` | 删除模型提供商 |
| GET | `/health` | 健康检查 |

## 前端架构

```
frontend/src/
├── main.rs              # 入口，App 组件
├── api/                # API 调用模块
│   └── health.rs       # 健康检查 API
└── components/         # UI 组件
    ├── navbar.rs        # 顶部导航栏
    ├── reception.rs     # 前台接待欢迎页
    └── agent_management.rs # Agent 管理页
```

前端已经实现：
- ✅ 顶部导航栏（前台接待 + 人力资源下拉菜单 → 员工管理 / Agent 管理）
- ✅ 前台接待欢迎页
- ✅ Agent 管理列表 + 创建弹窗

## 前端开发

项目使用 Dioxus 0.7 Web 框架开发前端：

- **开发**: `cd frontend && dx serve` 支持热重载
- **构建**: `dx build --release` 输出生成版本
- **打包**: `./build-full.sh` 自动编译前后端并打包到 `dist/`

## 端口说明

| 服务 | 默认端口 | 说明 |
|------|----------|------|
| 后端 API | 3000 | 提供 REST API 和静态文件服务 |
| 前端开发服务器 | 8080 | dx serve 热重载开发服务器 |

## License

MIT
