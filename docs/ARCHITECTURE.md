# 架构说明

## 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务，产生价值。

---

## 核心概念

### Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信

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
 ┌───────────────────────────────────────────────────────────────────┐
 │                        浏览器 / 客户端                                │
 └───────────────────────────────────────────────────────────────────┘
                              ↓ HTTP
 ┌───────────────────────────────────────────────────────────────────┐
 │                 Axum 后端 (Rust)                                   │
 │  - REST API                                                      │
 │  - 静态文件服务 (dist 目录)                                       │
 │  - SQLite 存储                                                   │
 └───────────────────────────────────────────────────────────────────┘
                              ↓
 ┌───────────────────────────────────────────────────────────────────┐
 │                 Dioxus 前端 (WebAssembly)                          │
 │  - 组件化开发                                                    │
 │  - 响应式状态管理                                                │
 │  - 浏览器运行                                                    │
 └───────────────────────────────────────────────────────────────────┘
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
├── frontend/                      # 前端源码 (Dioxus 0.7 WebAssembly)
│   ├── src/
│   │   └── main.rs                # 入口 + 页面组件
│   ├── Cargo.toml
│   └── index.html                 # HTML 入口
│
├── dist/                          # 生产构建输出（前端静态文件）
├── docs/                          # 文档
├── build-full.sh                   # 全量构建脚本
├── start-dev.sh                   # 开发启动脚本
└── Cargo.toml                     # 工作空间配置
```

---

## 分层关系

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

## 后端分层架构

### 分层

| 层 | 位置 | 职责 | 调用关系 |
|---|------|------|------|
| **Handler** | `src/handlers/` | 接收请求，返回响应 | 调用 domain |
| **Domain** | `src/service/domain/` | 抽象业务逻辑、领域规则、调度 dal | 调用 dal |
| **DAL** | `src/service/dal/` | 具体业务逻辑，组合 dao/远程接口 | 调用 dao |
| **DAO** | `src/service/dao/` | 数据增删查改，操作 models | 被 dal 调用 |
| **Model** | `src/models/` | 实体定义，被各层广泛使用 | 被 dao 使用 |
| **PKG** | `src/pkg/` | 存储驱动、工具函数 | 被 dao/handler 引用 |

### 设计原则

1. **清晰分层** - 每层只负责自己的职责，不越界
2. **依赖方向** - Handler → Domain → DAL → DAO → Storage，单向依赖
3. **Context 传递** - 所有公共方法第一个参数是 `RequestContext`，保存用户信息和日志上下文
4. **命名一致** - 遵循 [NAMING_CONVENTION.md](./NAMING_CONVENTION.md)

### 初始化流程

```rust
// 按顺序初始化，保证依赖正确
dao::init_all();      // 初始化所有 DAO
dal::init_all();      // 依赖 DAO
domain::init_all();    // 依赖 DAL
```

## 各层职责详解 ★

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

## Models 层说明 ★

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

## Handler 层规范

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

## API 设计规范

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

## 前端架构

### 技术栈

- **框架**: Dioxus 0.7 (Web)
- **编译目标**: `wasm32-unknown-unknown`
- **构建工具**: dioxus-cli (`dx`)
- **开发模式**: `dx serve` 热重载
- **生产构建**: `dx build --release` 输出优化 WASM

### 项目结构

```
frontend/
├── src/
│   └── main.rs         # 入口组件，当前包含健康检查页面
├── Cargo.toml         # 依赖配置
├── index.html         # HTML 入口模板
└── target/           # 编译输出
```

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

不设置环境变量时使用合理的默认值，方便开发：

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

## 构建流程

### 开发构建

```bash
./start-dev.sh
# 1. 启动后端 cargo run
# 2. 启动前端 dx serve (热重载)
```

### 生产构建

```bash
./build-full.sh
# 1. dx build --release 编译前端
# 2. 复制产物到 dist/
# 3. cargo build --release 编译后端
```

## 技术选型理由

### 为什么选 Rust + Dioxus?

1. **全栈 Rust** - 前后端同一种语言，减少上下文切换
2. **内存安全** - Rust 内存安全，无 GC，适合长期运行的服务
3. **单二进制部署** - 编译一个二进制文件就能运行，包含前端
4. **WASM 前端** - 一次类型，前端后端共享类型定义（未来可扩展）
5. **热重载开发** - dx serve 支持热重载，开发体验好

### 为什么选 Axum?

- 官方维护，活跃开发
- 异步清晰，API 友好
- 生态完善，支持 static file service

### 为什么选 SQLite?

- 单文件数据库，不需要额外服务
- 适合中小型项目，部署简单
- 足够稳定，性能满足需求

## 扩展性

- 新增实体 → 按分层依次添加
- 新增页面 → 前端直接添加组件
- 更换存储 → 只需要换 DAO 实现
- 前后端分离部署 → 前端独立部署，后端只提供 API

## 下一步

- [x] 创建 `models/` 目录，定义实体 ✅
- [x] 重命名 `service/domain/*.rs` → `service/domain/*_domain.rs` ✅
- [x] 搭建 Dioxus 前端 ✅
- [ ] 配置 SQLite（pkg/storage/）
- [ ] 实现 Agent DAO + CRUD API

