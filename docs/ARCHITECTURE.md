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

### HR (Human Resources)
- **定义**：人力资源领域模块，统一管理所有「人」相关资源
  - **Agent** - AI 智能体
  - **Employee** - 人类员工（预留未来扩展）

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
├── handlers/                 # HTTP 层
│   ├── mod.rs                 # 导出 + 通用 ApiResponse + 公共 extract_ctx 方法
│   ├── health.rs             # 健康检查
│   ├── organization.rs       # 组织接口
│   ├── hr.rs                 # HR (Human Resources) 模块入口
│   └── hr/                  # HR 模块子功能
│       └── agent/            # Agent 管理
│           ├── mod.rs         # Agent HTTP 接口实现
│           └── dto.rs         # 请求/响应 DTO
│
├── models/                 # 实体模型层 ★
│   ├── mod.rs
│   ├── brain.rs             # 🧠 Brain 实体 + Cortex trait
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
│   │   └── hr/             # HR (Human Resources) 领域 ✨ 新架构
│   │       ├── mod.rs       # 单例管理 + 结构体定义 + trait 总定义 ✨
│   │       │   - static HR_DOMAIN: OnceLock<...> 单例放在最上部
│   │       │   - pub struct HrDomainImpl 结构体定义
│   │       │   - pub trait HrDomain 总 trait 定义
│   │       │   - pub trait AgentManage Agent 管理 trait 定义
│   │       └── agent.rs     # AgentManage trait 具体实现 ✨
│   │
│   ├── dal/                 # 具体业务层：组合 dao，完成特定业务
│   │   ├── mod.rs
│   │   ├── agent.rs         # Agent 业务
│   │   ├── model_provider.rs # 模型提供商业务
│   │   └── org.rs           # Organization 业务
│   │
│   └── dao/                 # 数据层：持久化，与 models 交互
│       ├── mod.rs
│       ├── brain/             🧠 Brain DAO - 大脑工厂
│       │   ├── mod.rs         # BrainDao trait + OnceLock 单例
│       │   ├── rig.rs         # Rig 框架实现 (RigBrainDao)
│       │   ├── rig/          # 具体 Cortex 实现
│       │   │   ├── openai.rs
│       │   │   ├── openai_compatible.rs
│       │   │   └── ollama.rs
│       │   └── rig_test.rs     # 单元测试
│       ├── agent/             # Agent 存储 DAO
│       │   ├── mod.rs         # 接口定义
│       │   └── sqlite.rs     # SQLite 实现
│       ├── model_provider/   # 模型提供商存储 DAO
│       │   ├── mod.rs         # 接口定义
│       │   └── sqlite.rs     # SQLite 实现
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

### 1. 分层清晰，严格不跨层调用

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

**严格规则**：不允许跨层级调用，只能逐层依赖

### 2. Domain 领域层设计： trait 分层架构 ✨

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
├── employee.rs     # 添加 impl EmployeeManage for HrDomainImpl 实现
```

不需要修改其他文件，完全符合**开放封闭原则**，易于扩展 ✅

### 3. 方法命名规范

子功能 trait 中的方法必须**完整命名**，一眼就能知道操作的是什么对象：

| 不推荐 | 推荐 |
|--------|------|
| `create` | `create_agent` ✅ |
| `get` | `get_agent` ✅ |
| `list` | `list_agents` ✅ |
| `update` | `update_agent` ✅ |
| `delete` | `delete_agent` ✅ |

因为同一个 `HrDomainImpl` 会实现多个子 trait（AgentManage + EmployeeManage），完整命名避免混淆，可读性更好 ✅

### 4. DAO 层规范：OnceLock 单例模式

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

### 5. 枚举命名规范

所有状态枚举都放在 `pkg::constants::status`，命名格式：**实体名 + Po + Status**

| 错误 | 正确 ✅ |
|------|--------|
| `ModelProviderStatus` | `ModelProviderPoStatus` |

清晰归属，便于查找 ✅

### 6. Handler 层设计：业务分组 + 方法粒度拆分 ✨

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
└── delete_model_provider.rs      # handler
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

### 7. 公共方法抽取

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

### DAO 层（数据层）
- **职责**：数据增删查改，与 models 映射
- **特点**：
  - 最底层的数据访问
  - 只做数据操作，不含业务逻辑
  - 操作 models 与数据库转换
  - delete 方法统一接收完整 PO 对象，接口风格一致

### Models 层说明

`models/` 是项目的**实体模型层**，被 service 三层广泛使用：

```rust
// models/agent.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub model_provider_id: String,
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
│   └── health.rs         # 健康检查 API
└── components/         # UI 组件
    ├── navbar.rs        # 顶部导航栏
    ├── reception.rs     # 前台接待欢迎页
    └── agent_management.rs # Agent 管理页
```

前端已经实现：
- ✅ 顶部导航栏（前台接待 + 人力资源下拉 → 员工管理 / Agent 管理）
- ✅ 前台接待欢迎页
- ✅ Agent 管理列表 + 创建弹窗（目前使用示例数据模拟，后续对接真实 API）

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
