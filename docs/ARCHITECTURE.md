# 架构说明

## 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务，产生价值。

---

## 核心概念

### Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信
- **关系**：直接持有装配好的 `Brain`（动态装配，不持久化）

### Model Provider（模型提供商）
- **定义**：独立实体，保存 LLM 模型配置（提供商类型、模型名称、API Key、自定义地址）
- **关系**：一个 Model Provider 可以被多个 Agent 复用
- **位置**：`models/model_provider.rs`

### Brain（大脑）
- **定义**：顶层实体，包装 `Cortex` 思考模块
- **关系**：直接持有 `Cortex` 实体
- **位置**：`models/brain.rs`

### Cortex 🧠（大脑皮层）
- **定义**：具体的思考推理组合，包含 `ModelProvider` + `CortexTrait`
- **关系**：`Cortex` → 实体结构体，持有 `ModelProvider` 业务对象 + `Box<dyn CortexTrait>`
- **CortexTrait** → 推理 trait，不同 LLM 提供商实现不同
- **位置**：`models/brain.rs`

### Organization（组织）
- **定义**：Agent 的集合，管理成员和任务分配

### Task（任务）
- **定义**：需要完成的工作单元

### Message（消息）
- **定义**：Agent 间的通信单元

### HR (Human Resources)
- **定义**：人力资源领域模块，统一管理所有「人」相关资源
  - **Agent** - AI 智能体
  - **Employee** - 人类员工（预留未来扩展）

### Finance (财务管理)
- **定义**：财务管理领域模块，统一管理所有「资源供给」相关资源
  - **Model Provider** - LLM 模型提供商（付费资源）

---

## 最终实体层次关系 🎯

经过多次重构，我们得到了清晰、职责明确、层次直观的最终实体关系：

```
Agent (po + brain: Option<Brain>)
  └─► Brain (cortex: Cortex)
       └─► Cortex (model_provider: ModelProvider, cortex: Box<dyn CortexTrait + Send + Sync>)
            ├─► ModelProvider (po: ModelProviderPo)
            └─► dyn CortexTrait (async fn prompt(&self, prompt: &str) -> Result<String>)
```

### 层次职责说明

| 层次 | 职责 |
|------|------|
| **Agent** | 顶级智能体，持有装配好的 Brain |
| **Brain** | 聚合根，持有完整的 Cortex 思考模块 |
| **Cortex** | 思考组合，持有 ModelProvider 配置 + 具体推理实现 |
| **ModelProvider** | 模型配置，保存持久化信息 |
| **CortexTrait** | 推理接口，不同提供商不同实现 |

### 命名规范 ✅

| 名称 | 类型 | 位置 | 职责 |
|------|------|------|------|
| **Cortex** | `struct` | `models/brain.rs` | 实体结构体，组合 `ModelProvider + Box<dyn CortexTrait>` |
| **CortexTrait** | `trait` | `models/brain.rs` | 推理接口 trait，定义 `prompt` 方法 |
| **CortexDao** | `trait` | `service/dao/cortex/mod.rs` | DAO 工厂，创建 `CortexTrait` + 执行 `prompt` |
| **CortexDal** | `trait` | `service/dal/cortex.rs` | DAL 业务层，组装完整 `Cortex` 实体 |

遵循 Rust 命名规范：trait 后缀 `Trait` ✨

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

## 目录结构（后端）

```
ai_orz/src/
├── main.rs                     # 应用入口
├── router.rs                   # 路由配置
├── error.rs                    # 统一错误处理
│
├── handlers/                 # HTTP 层 ★ 按业务分组 + 方法粒度拆分 ✨
│   ├── mod.rs                 # 导出 + 通用 ApiResponse + 公共 extract_ctx 方法
│   ├── health.rs             # 健康检查
│   ├── hr/                  # HR (Human Resources) 模块入口
│   │   └── agent/            # Agent 管理 ✨ 每个方法一个文件
│   │       ├── mod.rs         # 仅导出
│   │       ├── create_agent.rs # DTO + handler
│   │       ├── get_agent.rs    # DTO + handler
│   │       ├── list_agents.rs # DTO + handler
│   │       ├── update_agent.rs # DTO + handler
│   │       └── delete_agent.rs # handler
│   └── finance/             # Finance (财务管理) 模块入口
│       └── model_provider/  # Model Provider 管理 ✨ 结构与 hr/agent 完全对齐
│           ├── mod.rs                       # 仅导出
│           ├── create_model_provider.rs      # DTO + handler
│           ├── get_model_provider.rs         # DTO + handler
│           ├── list_model_providers.rs       # DTO + handler
│           ├── update_model_provider.rs      # DTO + handler
│           ├── delete_model_provider.rs      # handler
│           ├── test_connection.rs           # 测试连通性 handler
│           └── call_model.rs                 # 通用调用模型 handler
│
├── models/                 # 实体模型层 ★
│   ├── mod.rs
│   ├── brain.rs             # 🧠 Brain 实体 + Cortex 实体 + CortexTrait 接口
│   ├── agent.rs             # Agent 实体
│   ├── model_provider.rs   # 模型提供商实体
│   ├── organization.rs     # 组织实体
│   ├── task.rs             # 任务实体
│   └── message.rs          # 消息实体
│
├── service/                # 业务逻辑层
│   ├── mod.rs
│   ├── domain/             # 领域层：领域行为、领域规则
│   │   ├── mod.rs
│   │   ├── hr/             # HR (Human Resources) 领域 ✨
│   │   │   ├── mod.rs       # 单例管理 + 结构体定义 + trait 总定义
│   │   │   └── agent.rs     # AgentManage trait 具体实现
│   │   └── finance/        # Finance (财务管理) 领域 ✨
│   │       ├── mod.rs       # 单例管理 + 结构体定义 + trait 总定义
│   │       └── model_provider.rs # ModelProviderManage trait 具体实现
│   │
│   ├── dal/                 # 具体业务层：组合 dao，完成特定业务
│   │   ├── mod.rs
│   │   ├── agent.rs         # Agent 业务
│   │   ├── cortex.rs        # Cortex 业务 ✨ 创建完整 Cortex 实体
│   │   ├── model_provider.rs # 模型提供商业务
│   │   └── org.rs           # Organization 业务
│   │
│   └── dao/                 # 数据层：持久化，与 models 交互
│       ├── mod.rs
│       ├── cortex/             🧠 Cortex DAO - 大脑皮层工厂
│       │   ├── mod.rs         # CortexDao trait + OnceLock 单例
│       │   ├── rig.rs         # Rig 框架实现 (RigCortexDao)
│       │   ├── rig/          # 具体 CortexTrait 实现
│       │   │   ├── openai.rs
│       │   │   ├── openai_compatible.rs
│       │   │   └── ollama.rs
│       │   └── rig_test.rs     # 单元测试
│       ├── agent/             # Agent 存储 DAO
│       │   ├── mod.rs         # 接口定义
│       │   └── sqlite.rs     # SQLite 实现
│       ├── model_provider/   # 模型提供商存储 DAO
│       │   ├── mod.rs         # 接口定义
│       │   ├── sqlite.rs     # SQLite 实现
│       │   └── sqlite_test.rs # 单元测试
│       └── org/             # Organization 存储 DAO
│           ├── mod.rs         # 接口定义
│           └── sqlite.rs     # SQLite 实现
│
└── pkg/                    # 公共包
    ├── mod.rs
    ├── constants/            # 常量（状态枚举）
    │   ├── mod.rs
    │   ├── status.rs         # 状态枚举（命名格式：实体名+Po+Status → ModelProviderPoStatus）
    │   ├── provider_type.rs  # ProviderType 枚举
    │   └── request_context.rs # RequestContext 定义
    ├── storage/              # 存储层（SQLite）
    │   ├── mod.rs             # 全局连接管理
    │   └── sql.rs            # SQL 建表语句常量
    └── logging.rs             # 日志（带 Context）
```

---

## 设计原则：模块划分与封装

### 1. 严格分层，不允许跨层级调用

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

**严格规则**：不允许跨层级调用，只能逐层依赖 ✅

### 2. 强约定：所有 service 层方法都必须传递 `RequestContext` ✨

这是强制约定，必须遵守：

> **所有 service 层（DAO/DAL/Domain）公共方法都必须传递 `ctx: RequestContext` 作为第一个参数**
> 即使方法当前不使用，也必须传递，方便后续日志串联和扩展

示例：

```rust
// ✅ 正确
fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;

// ❌ 错误 - 缺少 ctx
fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;
```

### 3. Domain 领域层设计： trait 分层架构 ✨

经过讨论和实践，我们得到了更合理的模块划分设计：

**核心思想：高内聚低耦合，易于扩展**

```
service/domain/hr/
├── mod.rs          # 👉 这里放：
│   1. 单例管理（static OnceLock + 初始化方法）放在最**上部**
│   2. pub struct HrDomainImpl 结构体定义
│   3. pub trait HrDomain 总 trait 定义（声明所有子功能入口）
│   4. pub trait AgentManage Agent 管理 trait 定义
│   5. impl HrDomain for HrDomainImpl 总实现放在这里
└── agent.rs       # 👉 这里放：
    - impl AgentManage for HrDomainImpl Agent 管理具体方法实现
```

**未来添加员工管理只需要：**

```
service/domain/hr/
├── mod.rs          # 添加 pub trait EmployeeManage 定义
└── employee.rs     # 添加 impl EmployeeManage for HrDomainImpl 实现
```

不需要修改其他文件，完全符合**开放封闭原则**，易于扩展 ✅

### 4. 方法命名规范

子功能 trait 中的方法必须**完整命名**，一眼就能知道操作的是什么对象：

| 不推荐 | 推荐 ✅ |
|--------|------|
| `create` | `create_agent` |
| `get` | `get_agent` |
| `list` | `list_agents` |
| `update` | `update_agent` |
| `delete` | `delete_agent` |

因为同一个 `HrDomainImpl` 会实现多个子 trait（AgentManage + EmployeeManage），完整命名避免混淆，可读性更好 ✅

### 5. DAO 层规范：OnceLock 单例模式

所有 DAO 都遵循统一单例模式：

```rust
static INSTANCE: OnceLock<Arc<dyn XyzDao + Send + Sync>> = OnceLock::new();

pub fn dao() -> Arc<dyn XyzDao + Send + Sync> {
    INSTANCE.get().cloned().unwrap()
}

pub fn init() {
    let _ = INSTANCE.set(Arc::new(XyzDaoImpl::new()));
}
```

**特点：**
- 不持有数据库连接，从全局存储模块获取
- OnceLock 保证线程安全，惰性初始化
- 对外只暴露 trait，隐藏具体实现
- 方便替换实现，易于测试

### 6. 枚举命名规范

所有状态枚举都放在 `pkg::constants::status`，命名格式：**实体名 + Po + Status**

| 错误 | 正确 ✅ |
|------|--------|
| `ModelProviderStatus` | `ModelProviderPoStatus` |

清晰归属，便于查找 ✅

### 7. Handler 层设计：业务分组 + 方法粒度拆分 ✨

经过讨论和实践，我们得到了更清晰的 Handler 架构设计：

**设计原则：**
1. **先按业务分组** → 对应 `domain` 分组（`hr` / `finance`）
2. **组内按方法粒度拆分** → **每个方法一个单独文件**，文件包含：
   - 完整的**输入 DTO** 定义
   - 完整的**输出 DTO** 定义
   - 完整的**业务 handler** 实现

**目录结构示例（Agent 管理）：**
```
handlers/hr/agent/
├── mod.rs               # 仅导出，不包含实现（~10 行）
├── create_agent.rs      # CreateAgentRequest + CreateAgentResponse + create_agent 实现
├── get_agent.rs         # GetAgentResponse + get_agent 实现
├── list_agents.rs       # AgentListItem + list_agents 实现
├── update_agent.rs      # UpdateAgentRequest + UpdateAgentResponse + update_agent 实现
└── delete_agent.rs      # delete_agent 实现
```

**模型提供商管理（finance domain）结构完全对齐：**
```
handlers/finance/model_provider/
├── mod.rs                       # 仅导出
├── create_model_provider.rs      # DTO + handler
├── get_model_provider.rs         # DTO + handler
├── list_model_providers.rs       # DTO + handler
├── update_model_provider.rs      # DTO + handler
├── delete_model_provider.rs      # handler
├── test_connection.rs           # 测试连通性
└── call_model.rs                 # 通用调用模型
```

**优点：**
- ✅ 每个文件短小单一职责，不超过 100 行，阅读清晰
- ✅ 找到代码更容易，增删改某一个方法只需要改一个文件
- ✅ 输入输出定义和实现在一起，不用翻多个文件
- ✅ 结构与 domain 完全对齐，便于查找
- ✅ 符合单一职责原则，高内聚低耦合

路由路径也完全对齐 domain：

| 层级 | 路径 |
|------|------|
| Domain | `service/domain/finance/model_provider.rs` |
| Handler | `handlers/finance/model_provider/` 按方法拆分 |
| Router | `/api/v1/finance/model-providers` |

完全一致，结构清晰 ✅

### 8. 公共方法抽取

公共工具方法抽取到上层复用：
- `extract_ctx` 从 HeaderMap 提取 RequestContext → 放在 `handlers/mod.rs`，所有 handler 复用 ✅

避免重复代码，保持干净 ✅

---

## 分层架构详解

### Domain 层（领域层）
- **职责**：核心业务逻辑、领域规则、调度 dal 组织业务
- **特点**：
  - 最接近业务本质
  - 不直接操作数据库
  - 编排 dal 完成业务
- **调用规则**：`wake_cortex` 只接收已经查询好的 `ModelProvider`，上层 Handler 负责查询 ✅

### DAL 层（具体业务层）
- **职责**：具体业务逻辑，组合 dao/外部服务
- **特点**：
  - 面向特定业务场景
  - 可组合多个 dao
  - 可调用外部 API

```rust
// dal 层示例
pub struct AgentDal {
    agent_dao: Arc<dyn AgentDaoTrait>,
}

impl AgentDal {
    pub fn create(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError> {
        // 调用 dao 持久化
        agent_dao.insert(ctx, agent.po())?;
        Ok(())
    }
}
```

**`AgentDal.wake_brain` 最新设计 ✨**

`Brain` 已经完整持有 `Cortex` → `ModelProvider`，所以 `wake_brain` 不再需要传入 `model_provider` 参数：

```rust
// 最新设计 ✅
fn wake_brain(&self, ctx: RequestContext, agent: &mut Agent, brain: Brain) -> Result<(), AppError> {
    // 从 brain.cortex.model_provider.po.id 获取 model_provider_id
    // 如果和当前 agent.po.model_provider_id 不同 → 更新并写入数据库
}
```

移除了多余参数，接口更干净 ✅

### DAO 层（数据层）
- **职责**：数据增删查改，与 models 映射
- **特点**：
  - 最底层的数据访问
  - 只做数据操作，不含业务逻辑
  - 操作 models 与数据库转换
  - delete 方法统一接收完整 PO 对象，接口风格一致

**`CortexDao` 最新设计 ✨**

| 方法 | 职责 |
|------|------|
| `create_cortex_trait(ctx, provider)` → 创建 `Box<dyn CortexTrait>` | 工厂方法创建推理实例 |
| `prompt(ctx, cortex, prompt)` → `Result<String>` | 执行推理获取回答 |

所有方法都传递 `ctx: RequestContext` ✅

### Models 层说明

`models/` 是项目的**实体模型层**，被 service 三层广泛使用：

```rust
// models/agent.rs
pub struct Agent {
    pub po: AgentPo,
    pub brain: Option<Brain>, // 装配好的 Brain，动态装配，不持久化
}
```

**特点：**
- 被 domain/dal/dao 各层使用
- 实现数据库与实体对象的转换
- 不包含业务逻辑
- 可序列化/反序列化

---

## LLM 调用架构 🧠

### 核心概念（最终版）

| 概念 | 位置 | 职责 |
|------|------|------|
| **ModelProvider** | `models/model_provider.rs` | 独立实体，保存模型配置 |
| **Brain** | `models/brain.rs` | 顶层实体，包装 `Cortex` |
| **Cortex** | `models/brain.rs` | 实体结构体，持有 `ModelProvider + Box<dyn CortexTrait>` |
| **CortexTrait** | `models/brain.rs` | 推理 trait，不同提供商实现不同 |
| **CortexDao** | `service/dao/cortex/` | 工厂 DAO，统一创建 `CortexTrait` + 执行 `prompt` |
| **CortexDal** | `service/dal/cortex.rs` | DAL 业务层，组装完整 `Cortex` 实体 |

### 分层职责清晰化

| 层级 | 职责 |
|------|------|
| **DAO 层 (CortexDao)** | 创建 `CortexTrait` 实例，执行 `prompt` |
| **DAL 层 (CortexDal)** | 调用 DAO 创建 `CortexTrait`，组装完整 `Cortex` 实体 |

### LLM 调用流程

```rust
// Handler 层调用方式（最新版 ✨）
use crate::service::domain::finance::domain;

// 1. Handler 先查询 Model Provider（已经找到实例）
let provider = domain().model_provider_manage().get_model_provider(ctx, id)?
    .ok_or_else(|| AppError::NotFound(...))?;

// 2. 调用 domain 直接唤醒执行
let result = domain().model_provider_manage().wake_cortex(ctx, &provider, prompt)?;
```

### 职责清晰

- **Handler 层**：负责参数解析，查询 `ModelProvider`，然后传递给 domain
- **Domain 层**：只负责业务逻辑，直接使用上层查询好的 `ModelProvider`
- **不需要 domain 重复查询** ✅

### 支持的模型提供商

| 提供商 | 实现文件 | 支持 |
|--------|------|------|
| OpenAI 官方 | `service/dao/cortex/rig/openai.rs` | ✅ |
| DeepSeek | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| 阿里云通义千问 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| 字节跳动豆包 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| Ollama 本地 | `service/dao/cortex/rig/ollama.rs` | ✅ |
| 自定义 OpenAI 兼容 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |

---

## Handler 层规范

Handler 的核心逻辑固定为三步：

```rust
// 1. 解析输入请求
// 2. 调用 service domain 方法
// 3. 封装结果返回统一响应

async fn create_agent(
    headers: HeaderMap,
    Json(req): Json<CreateAgentReq>,
) -> Result<(StatusCode, Json<ApiResponse<AgentResp>>), AppError> {
    let ctx = extract_ctx(&headers);
    let agent_po = AgentPo::new(...);
    let agent = Agent::from_po(agent_po);

    domain().agent_manage().create_agent(ctx, &agent)?;

    Ok((StatusCode::CREATED, Json(ApiResponse::success(...))))
}
```

---

## API 设计规范

### 路由命名（对齐 HR 模块）

现在所有 Agent 相关路由都在 HR 分组下：

```
POST   /api/v1/hr/agents          创建
GET    /api/v1/hr/agents          列表
GET    /api/v1/hr/agents/{id}     详情
PUT    /api/v1/hr/agents/{id}     更新
DELETE /api/v1/hr/agents/{id}     删除
```

Model Provider 在 Finance 分组下：

```
POST   /api/v1/finance/model-providers          创建
GET    /api/v1/finance/model-providers          列表
GET    /api/v1/finance/model-providers/{id}     详情
PUT    /api/v1/finance/model-providers/{id}     更新
DELETE /api/v1/finance/model-providers/{id}     删除
POST   /api/v1/finance/model-providers/{id}/test # 测试连通性
POST   /api/v1/finance/model-providers/{id}/call # 调用模型
```

### 统一响应格式

```json
{
  "code": 0,
  "message": "success",
  "data": {}
}
```

---

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
│   ├── health.rs         # 健康检查 API
│   ├── agent.rs        # Agent 管理 API
│   └── model_provider.rs # Model Provider 管理 API
└── components/         # UI 组件
    ├── navbar.rs        # 顶部导航栏
    ├── reception.rs     # 前台接待欢迎页
    ├── agent_management.rs # Agent 管理页
    └── model_provider_management.rs # Model Provider 管理页
```

前端已经实现：
- ✅ 顶部导航栏（前台接待 + 人力资源下拉 + 财务管理下拉）
  - 人力资源 → Agent 管理
  - 财务管理 → Model Provider 管理
- ✅ 前台接待欢迎页
- ✅ Agent 管理列表 + 创建弹窗 + 删除功能
- ✅ Model Provider 管理列表 + 创建弹窗 + 删除功能 + 自动测试连通性 ✨

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

## 单元测试规范

我们要求：

- ✅ 每个业务 DAO/DAL 都应该有单元测试
- ✅ 单元测试放在模块同级目录，文件名以 `_test.rs` 结尾
- ✅ 当前测试覆盖：
  - `service/dao/cortex/rig_test.rs` - 6 测试 ✅
  - `service/dal/cortex_test.rs` - 2 测试 ✅
  - `service/dao/model_provider/sqlite_test.rs` - 3 测试 ✅
- ✅ 总共 **31** 个单元测试，全部通过 ✅

运行测试：

```bash
cargo test
```

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

---

## 重构历史 (2026-04-07)

今日完成了大规模重构，最终得到清晰简洁的完美架构：

### 核心重构决策

1. **实体层次重构**
   - ✅ `Agent` 直接持有 `Option<Brain>`，动态装配，不持久化
   - ✅ `Brain` 直接持有 `Cortex`
   - ✅ `Cortex` 直接持有 `ModelProvider + Box<dyn CortexTrait>`
   - ✅ `ModelProvider` 只持有配置 (`po`)，不持有执行实例
   - ✅ 移除了 `Brain` 中重复的 `cortex` 字段，避免冗余

2. **分层职责清晰化**
   - ✅ DAO 创建 `CortexTrait`，DAL 创建 `Cortex` 实体
   - ✅ 所有调用收敛到 DAO 层
   - ✅ 简单模块保持单文件，不需要分文件夹

3. **命名修正**
   - ✅ `BrainDao` → `CortexDao`（Brain 是聚合根，CortexDao 只创建 Cortex）
   - ✅ `create_brain` → `create_cortex_trait`
   - ✅ `wake_brain` → `wake_cortex`（实际唤醒的是 Cortex，不是 Brain）
   - ✅ `Cortex` trait → `CortexTrait`（遵循 Rust 命名规范，Cortex 现在是实体结构体）
   - ✅ `wake_cortex` in CortexDao → `prompt`（不再有"唤醒"概念，就是执行 prompt）

4. **依赖注入**
   - ✅ `ModelProviderDal` 持有 `CortexDao`，构造函数注入
   - ✅ `AgentDal` 不再依赖 `CortexDao`，上层组装好 Brain 传入，只做赋值和更新
   - ✅ `Agent` 业务实体持 `brain` 可选字段，持久化不存储，只通过 DAL 动态装配

5. **强规范推行**
   - ✅ **所有 service 层方法都传递 `ctx: RequestContext` 参数**（强制约定）
   - ✅ 即使当前不使用，也必须传递，方便未来日志串联扩展

6. **接口简化**
   - ✅ `FinanceDomain.wake_cortex` 只接收 `ModelProvider`，上层 Handler 负责查询 ✅
   - ✅ `AgentDal.wake_brain` 移除 `model_provider` 参数，从 `Brain.cortex.model_provider` 获取 ✅
   - ✅ 移除冗余参数，接口更干净，职责更清晰

7. **单元测试补充**
   - ✅ `cortex_dao` + `cortex_dal` + `model_provider_dao` 都有单元测试
   - ✅ 31/31 全部测试通过

### 最终验证

```
running 31 tests
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

✅ 编译成功，所有测试通过，满足所有规范要求
