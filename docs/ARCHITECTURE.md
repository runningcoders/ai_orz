# 架构说明

## 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务，产生价值。

---

## 核心概念

### Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信

### Model Provider（模型提供商）
- **定义**：独立实体，保存 LLM 模型配置（提供商类型、模型名称、API Key、自定义地址）
- **关系**：一个 Model Provider 可以被多个 Agent 复用

### Brain（大脑）
- **定义**：顶层实体，包装 Cortex 思考模块
- **关系**：由 BrainDao 根据 Model Provider 创建

### Cortex 🧠（大脑皮层）
- **定义**：具体的思考推理接口
- **关系**：不同 LLM 提供商实现不同的 Cortex

### Organization（组织）
- **定义**：Agent 的集合，管理成员和任务分配

### Task（任务）
- **定义**：需要完成的工作单元

### Message（消息）
- **定义**：Agent 间的通信单元

---

## 整体架构

**全栈 Rust 单仓库项目**

```
 ┌───────────────────────────────────────────────────────────────────────────┐
 │                        浏览器 / 客户端                                │
 └───────────────────────────────────────────────────────────────────────────┘
                              ↓ HTTP
 ┌───────────────────────────────────────────────────────────────────────────┐
 │                 Axum 后端 (Rust)                                   │
 │  - REST API                                                      │
 │  - 静态文件服务 (dist 目录)                                       │
 │  - SQLite 存储                                                   │
 └───────────────────────────────────────────────────────────────────────────┘
                              ↓
 ┌───────────────────────────────────────────────────────────────────────────┐
 │                    Dioxus 前端 (WebAssembly)                          │
 │  - 组件化开发                                                    │
 │  - 响应式状态管理                                              │
 │  - 浏览器运行                                                      │
 └───────────────────────────────────────────────────────────────────────────┘
```

---

## 目录结构

```
ai_orz/
├── src/                          # 后端源码
│   ├── main.rs                     # 应用入口
│   ├── router.rs                   # 路由配置
│   ├── error.rs                    # 统一错误处理
│   │
│   ├── handlers/                 # HTTP 层
│   │   ├── mod.rs
│   │   ├── agent.rs              # Agent 接口
│   │   ├── model_provider.rs     # 模型提供商接口
│   │   ├── organization.rs       # 组织接口
│   │   ├── task.rs              # 任务接口
│   │   └── health.rs           # 健康检查
│   │
│   ├── models/                 # 实体模型层 ★
│   │   ├── mod.rs
│   │   ├── brain.rs             # 🧠 Brain 实体 + Cortex trait
│   │   ├── agent.rs             # Agent 实体
│   │   ├── model_provider.rs   # 模型提供商实体
│   │   ├── organization.rs     # 组织实体
│   │   ├── task.rs             # 任务实体
│   │   └── message.rs          # 消息实体
│   │
│   ├── service/                # 业务逻辑层
│   │   ├── mod.rs
│   │   ├── domain/             # 领域层：领域行为、领域规则
│   │   │   ├── mod.rs
│   │   │   ├── agent_domain.rs     # Agent 领域逻辑
│   │   │   ├── org_domain.rs       # Organization 领域逻辑
│   │   │   └── task_domain.rs      # Task 领域逻辑
│   │   ├── dal/                 # 具体业务层：组合 dao，完成特定业务
│   │   │   ├── mod.rs
│   │   │   ├── agent_dal.rs     # Agent 业务
│   │   │   ├── org_dal.rs       # Organization 业务
│   │   │   └── task_dal.rs      # Task 业务
│   │   └── dao/                 # 数据层：持久化，与 models 交互
│   │       ├── mod.rs
│   │       ├── brain/             🧠 Brain DAO - 大脑工厂
│   │       │   ├── mod.rs         # BrainDao trait + OnceLock 单例
│   │       │   ├── rig.rs         # Rig 框架实现 (RigBrainDao)
│   │       │   ├── rig/          # 具体 Cortex 实现
│   │       │   │   ├── openai.rs
│   │       │   │   ├── openai_compatible.rs
│   │       │   │   └── ollama.rs
│   │       │   └── rig_test.rs     # 单元测试
│   │       ├── agent/             # Agent 存储 DAO
│   │       │   ├── mod.rs
│   │       │   └── sqlite.rs
│   │       ├── model_provider/   # 模型提供商存储 DAO
│   │       │   ├── mod.rs
│   │       │   └── sqlite.rs
│   │       └── org/             # Organization 存储 DAO
│   │           ├── mod.rs
│   │           └── sqlite.rs
│   │
│   └── pkg/                    # 公共包
│       ├── mod.rs
│       ├── constants/            # 常量（状态枚举）
│       │   └── mod.rs
│       ├── storage/              # 存储层（SQLite）
│       │   ├── mod.rs             # 全局连接管理
│       │   └── sql.rs            # SQL 建表语句常量
│       ├── external/             # 外部 API
│       │   └── mod.rs
│       └── logging.rs             # 日志（带 Context）
├── frontend/                      # 前端源码 (Dioxus 0.7 WebAssembly)
│   ├── src/
│   │   ├── main.rs               # 入口 + 页面组件
│   │   ├── api/                 # API 调用模块
│   │   │   └── health.rs         # 健康检查 API
│   │   └── components/         # UI 组件
│   │       ├── navbar.rs        # 顶部导航栏
│   │       ├── reception.rs     # 前台接待欢迎页
│   │       └── agent_management.rs # Agent 管理页
├── dist/                          # 生产构建输出（前端静态文件）
├── docs/                          # 文档
├── build-full.sh                   # 全量构建脚本
├── start-dev.sh                   # 开发启动脚本
└── README.md
```

---

## 分层关系

```
┌─────────────────────────────────────────────────────────────┐
│   handlers/   HTTP 层                                        │
│   接收 Req → 调用 domain → 返回 Resp                             │
├─────────────────────────────────────────────────────────────┤
│   service/domain/   领域层                                       │
│   核心业务逻辑、领域规则、调度 dal                                 │
├─────────────────────────────────────────────────────────────┤
│   service/dal/   具体业务层                                       │
│   组合 dao/远程接口，完成特定业务                                  │
├─────────────────────────────────────────────────────────────┤
│   service/dao/   数据层                                        │
│   数据增删查改，操作 models，与数据库交互                            │
├─────────────────────────────────────────────────────────────┤
│   models/   实体模型层 ★                                      │
│   数据库实体定义，dao 用这些对象和数据库映射                     │
├─────────────────────────────────────────────────────────────┤
│   pkg/   公共包                                                  │
│   storage / external / constants                                 │
└─────────────────────────────────────────────────────────────┘
```

---

## LLM 调用架构 🧠

### 核心概念

| 概念 | 位置 | 职责 |
|------|------|------|
| **ModelProvider** | `models/model_provider.rs` | 独立实体，保存模型配置 |
| **Brain** | `models/brain.rs` | 顶层实体，包装 `Cortex` |
| **Cortex** 🧠 | `models/brain.rs` trait | 思考推理接口，不同提供商实现不同 |
| **BrainDao** | `service/dao/brain/` | 工厂 DAO，统一创建 `Brain` + 执行 `prompt` |

### LLM 调用流程

```rust
// 业务层调用方式
use service::dao::{brain_dao, model_provider_dao};

// 1. 查询模型配置（从数据库）
let provider = model_provider_dao.find_by_id(ctx, "id")?;

// 2. 创建 Brain
let brain = brain_dao.create_brain(&provider)?;

// 3. 执行推理
let result = brain_dao.prompt(&brain, "你好").await?;
```

### 支持的模型提供商

| 提供商 | 实现文件 | 支持 |
|--------|------|------|
| OpenAI 官方 | `service/dao/brain/rig/openai.rs` | ✅ |
| DeepSeek | `service/dao/brain/rig/openai_compatible.rs` | ✅ |
| 阿里云通义千问 | `service/dao/brain/rig/openai_compatible.rs` | ✅ |
| 字节跳动豆包 | `service/dao/brain/rig/openai_compatible.rs` | ✅ |
| Ollama 本地 | `service/dao/brain/rig/ollama.rs` | ✅ |
| 自定义 OpenAI 兼容 | `service/dao/brain/rig/openai_compatible.rs` | ✅ |

---

## 分层架构详解 ★

### Domain 层（领域层）
- **职责**：核心业务逻辑、领域规则、调度 dal 组织业务
- **特点**：
  - 最接近业务本质
  - 不直接操作数据库
  - 编排 dal 完成业务

```rust
// domain 层示例
pub struct AgentDomain;

impl AgentDomain {
    pub async fn create(cmd: CreateAgentCmd) -> Result<Agent> {
        // 校验业务规则
        // 调用 dal 执行具体业务
        AgentDal::create(cmd).await
    }
}
```

### DAL 层（具体业务层）
- **职责**：具体业务逻辑，组合 dao/外部服务
- **特点**：
  - 面向特定业务场景
  - 可组合多个 dao
  - 可调用外部 API

```rust
// dal 层示例
pub struct AgentDal;

impl AgentDal {
    pub async fn create(cmd: CreateAgentCmd) -> Result<Agent> {
        // 调用 dao 持久化
        AgentDao::insert(&agent)?;
        // 发送通知等
        Ok(agent)
    }
}
```

### DAO 层（数据层）
- **职责**：数据增删查改，与 models 映射
- **特点**：
  - 最底层的数据访问
  - 只做数据操作，不含业务逻辑
  - 操作 models 与数据库转换

**所有 DAO 都遵循单例模式**：
```rust
static INSTANCE: OnceLock<Arc<dyn XyzDao + Send + Sync>> = OnceLock::new();

pub fn dao() -> Arc<dyn XyzDao + Send + Sync> {
    INSTANCE.get().cloned().unwrap()
}

pub fn init() {
    let _ = INSTANCE.set(Arc::new(XyzDaoImpl::new()));
}
```

### Models 层说明 ★

`models/` 是项目的**实体模型层**，被 service 三层广泛使用：

```rust
// models/agent.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub model_provider_id: String,  ← ✨ 现在只保存 ID
    pub capabilities: Vec<String>,
    pub status: i32,
    pub created_at: i64,
    pub updated_at: i64,
}
```

**特点**：
- 被 domain/dal/dao 各层使用
- 实现数据库与实体对象的转换
- 不包含业务逻辑
- 可序列化/反序列化

---

## Handler 层规范

Handler 的核心逻辑固定为三步：

```rust
// 1. 解析输入请求
// 2. 调用 service domain 方法
// 3. 封装结果返回统一响应

async fn create_agent(
    Json(req): Json<CreateAgentReq>,
) -> Result<Json<ApiResponse<AgentResp>>, AppError> {
    let agent = AgentDomain::create(req.into()).await?;
    Ok(Json(ApiResponse::success(agent.into())))
}
```

---

## API 设计规范

### 路由命名
```
GET    /api/v1/agents          列表
POST   /api/v1/agents          创建
GET    /api/v1/agents/{id}     详情
PUT    /api/v1/agents/{id}     更新
DELETE /api/v1/agents/{id}     删除
```

### 统一响应格式
```json
{
  "code": 0,
  "message": "success",
  "data": {}
}
```

## 前端架构

### 技术栈

- **框架**：Dioxus 0.7 (Web)
- **编译目标**：`wasm32-unknown-unknown`
- **构建工具**：dioxus-cli (`dx`)
- **开发模式**：`dx serve` 热重载
- **生产构建**：`dx build --release` 输出优化 WASM

### 项目结构

```
frontend/src/
├── main.rs              # 入口组件，当前包含导航 + 页面路由
├── api/                 # API 调用模块
│   └── health.rs         # 健康检查 API
└── components/         # UI 组件
    ├── navbar.rs        # 顶部导航栏
    ├── reception.rs     # 前台接待欢迎页
    └── agent_management.rs # Agent 管理页
```

前端已经实现：
- ✅ 顶部导航栏（前台接待 + 人力资源下拉 → 员工管理 / Agent 管理）
- ✅ 前台接待欢迎页
- ✅ Agent 管理列表 + 创建弹窗

---

## 配置管理

### 环境变量

所有配置通过环境变量注入，不硬编码：

```bash
# 示例
export SERVER_HOST=0.0.0.0
export SERVER_PORT=3000
export DATABASE_URL=./data/ai_orz.db
./target/release/ai_orz
```

### 默认值

不设置环境变量时使用合理默认值，方便开发：

| 环境变量 | 默认值 |
|----------|--------|
| `SERVER_HOST` | `0.0.0.0` |
| `SERVER_PORT` | `3000` |
| `DATABASE_URL` | `data/ai_orz.db` |

## 静态文件服务

生产模式下，后端自动提供 `dist/` 目录下的前端静态文件：

- `GET /` → `dist/index.html`
- `GET /wasm/*.wasm` → 编译好的 WebAssembly
- `GET /api/*` → 后端 API
- `GET /health` → 健康检查

---

## 技术选型理由

### 为什么选 Rust + Dioxus?

1. **全栈 Rust** - 前后端同一种语言，减少上下文切换
2. **内存安全** - Rust 内存安全，无 GC，适合长期运行的服务
3. **单二进制部署** - 编译一个二进制文件就能运行，包含前端
4. **WASM 前端** - 一次编译，到处运行，类型安全
5. **热重载开发** - dx serve 支持热重载，开发体验好

### 为什么选 Axum?

- 官方维护，活跃开发
- 异步清晰，API 友好
- 生态完善，支持静态文件服务

### 为什么选 SQLite?

- 单文件数据库，不需要额外服务
- 适合中小型项目，部署简单
- 足够稳定，性能满足需求

---

## 扩展性

- **新增实体** → 按分层依次添加即可
- **新增页面** → 前端直接添加组件
- **更换存储** → 只需要换 DAO 实现
- **前后端分离部署** → 前端独立部署，后端只提供 API

