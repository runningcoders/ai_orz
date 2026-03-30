# ai_orz 项目架构设计

## 1. 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务，产生价值。

---

## 2. 核心概念

### 2.1 Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信

### 2.2 Organization（组织）
- **定义**：Agent 的集合，管理成员和任务分配

### 2.3 Task（任务）
- **定义**：需要完成的工作单元

### 2.4 Message（消息）
- **定义**：Agent 间的通信单元

---

## 3. 目录结构

```
ai_orz/
├── src/
│   ├── main.rs                     # 应用入口
│   ├── lib.rs                      # 库入口
│   ├── router.rs                   # 路由配置
│   ├── error.rs                    # 统一错误处理
│   │
│   ├── handlers/                   # HTTP 层
│   │   ├── mod.rs
│   │   ├── agent.rs                # Agent 接口（Req/Resp 结构体）
│   │   ├── organization.rs         # Organization 接口
│   │   ├── task.rs                 # Task 接口
│   │   └── health.rs               # 健康检查
│   │
│   ├── models/                     # 实体模型层 ★
│   │   ├── mod.rs
│   │   ├── agent.rs                # Agent 实体
│   │   ├── organization.rs         # Organization 实体
│   │   ├── task.rs                 # Task 实体
│   │   └── message.rs              # Message 实体
│   │
│   ├── service/                    # 业务逻辑层
│   │   ├── mod.rs
│   │   ├── domain/                 # 领域层：领域行为、领域规则
│   │   │   ├── mod.rs
│   │   │   ├── agent_domain.rs     # Agent 领域逻辑
│   │   │   ├── org_domain.rs       # Organization 领域逻辑
│   │   │   └── task_domain.rs      # Task 领域逻辑
│   │   ├── dal/                    # 具体业务层：组合 dao，完成特定业务
│   │   │   ├── mod.rs
│   │   │   ├── agent_dal.rs        # Agent 业务
│   │   │   ├── org_dal.rs          # Organization 业务
│   │   │   └── task_dal.rs         # Task 业务
│   │   └── dao/                    # 数据层：持久化，与 models 交互
│   │       ├── mod.rs
│   │       ├── agent_dao.rs        # Agent 数据操作
│   │       ├── org_dao.rs          # Organization 数据操作
│   │       └── task_dao.rs         # Task 数据操作
│   │
│   └── pkg/                        # 公共包
│       ├── mod.rs
│       ├── storage/                # 存储层（SQLite）
│       │   ├── mod.rs
│       │   └── sqlite.rs
│       ├── external/               # 外部 API
│       │   └── mod.rs
│       └── constants/              # 常量
│           └── mod.rs
│
├── tests/
├── docs/
└── Cargo.toml
```

---

## 4. 分层关系

```
┌─────────────────────────────────────────────────────────────┐
│   handlers/   HTTP 层                                        │
│   接收 Req → 调用 domain → 返回 Resp                         │
├─────────────────────────────────────────────────────────────┤
│   service/domain/   领域层                                   │
│   核心业务逻辑、领域规则                                      │
├─────────────────────────────────────────────────────────────┤
│   service/dal/   具体业务层                                   │
│   组合 dao/远程服务，完成特定业务                              │
├─────────────────────────────────────────────────────────────┤
│   service/dao/   数据层                                      │
│   与 models 交互，完成数据库持久化                             │
├─────────────────────────────────────────────────────────────┤
│   models/   实体模型层 ★                                      │
│   数据库实体定义，dao 用这些对象与数据库映射                     │
├─────────────────────────────────────────────────────────────┤
│   pkg/   公共包                                              │
│   storage / external / constants                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 5. 各层职责

| 层 | 位置 | 职责 | 调用关系 |
|---|---|---|---|
| **Handler** | `handlers/` | 接收请求，返回响应 | 调用 domain |
| **Domain** | `service/domain/` | 抽象业务逻辑、领域规则、调度 dal | 调用 dal |
| **DAL** | `service/dal/` | 具体业务逻辑，组合 dao/远程接口 | 调用 dao |
| **DAO** | `service/dao/` | 数据增删查改，操作 models | 被 dal 调用 |
| **Model** | `models/` | 实体定义，被各层广泛使用 | 被 dao 使用 |
| **PKG** | `pkg/` | 存储驱动、工具函数 | 被 dao/handler 引用 |

---

## 6. Service 三层详解 ★

### Domain 层（领域层）
- **职责**：核心业务逻辑、领域规则、调度 dal 组织业务
- **特点**：
  - 最接近业务本质
  - 不直接操作数据库
  - 编排 dal 完成业务
  - 可被多个 handler 复用

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
  - 事务边界

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

```rust
// dao 层示例
pub struct AgentDao;

impl AgentDao {
    pub fn insert(conn: &Connection, agent: &Agent) -> Result<()> { ... }
    pub fn find_by_id(conn: &Connection, id: &str) -> Result<Option<Agent>> { ... }
    pub fn update(conn: &Connection, agent: &Agent) -> Result<()> { ... }
    pub fn delete(conn: &Connection, id: &str) -> Result<()> { ... }
}
```

---

## 7. Models 层说明 ★

`models/` 是项目的**实体模型层**，被 service 三层广泛使用：

```rust
// models/agent.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub role: String,
    pub capabilities: Vec<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}
```

**特点：**
- 被 domain/dal/dao 各层使用
- 实现数据库与实体对象的转换
- 不包含业务逻辑
- 可序列化/反序列化

---

## 8. Handler 层规范

Handler 的核心逻辑固定为三步：

```rust
// 1. 接收输入 Req
// 2. 调用 service domain 方法
// 3. 将结果转换为 Resp 返回

async fn create_agent(
    Json(req): Json<CreateAgentReq>,
) -> Result<Json<ApiResponse<AgentResp>>, AppError> {
    let agent = AgentDomain::create(req.into()).await?;
    Ok(Json(ApiResponse::success(agent.into())))
}
```

---

## 9. API 设计规范

### 路由命名
```
GET    /api/v1/agents          # 列表
POST   /api/v1/agents          # 创建
GET    /api/v1/agents/{id}     # 详情
PUT    /api/v1/agents/{id}     # 更新
DELETE /api/v1/agents/{id}     # 删除
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

## 10. API 设计规范

### 路由命名
```
GET    /api/v1/agents          # 列表
POST   /api/v1/agents          # 创建
GET    /api/v1/agents/{id}     # 详情
PUT    /api/v1/agents/{id}     # 更新
DELETE /api/v1/agents/{id}     # 删除
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

## 11. 下一步

- [x] 创建 `models/` 目录，定义实体 ✅
- [x] 重命名 `service/domain/*.rs` → `service/domain/*_domain.rs` ✅
- [ ] 配置 SQLite（pkg/storage/）
- [ ] 实现 Agent DAO + CRUD API
