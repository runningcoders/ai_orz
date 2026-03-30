# ai_orz 项目架构设计

## 1. 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务，产生价值。

---

## 2. 核心概念

### 2.1 Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信
- **属性**：id、name、role、capabilities、status

### 2.2 Organization（组织）
- **定义**：Agent 的集合，管理成员和任务分配
- **属性**：id、name、members、tasks

### 2.3 Task（任务）
- **定义**：需要完成的工作单元
- **属性**：id、title、description、assigned_to、status、priority

### 2.4 Message（消息）
- **定义**：Agent 间的通信单元
- **属性**：id、from、to、content、timestamp

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
│   ├── handlers/                   # HTTP 层：接收请求 → 调用 service → 返回响应
│   │   ├── mod.rs
│   │   ├── agent.rs                # Agent 接口（含 Req/Resp 结构体）
│   │   ├── organization.rs         # Organization 接口
│   │   ├── task.rs                 # Task 接口
│   │   └── health.rs               # 健康检查
│   │
│   ├── service/                    # 业务逻辑层（三层结构）
│   │   ├── mod.rs
│   │   ├── domain/                 # 领域层：定义领域模型和领域方法
│   │   │   ├── mod.rs
│   │   │   ├── agent.rs            # Agent 领域模型
│   │   │   ├── organization.rs     # Organization 领域模型
│   │   │   └── task.rs             # Task 领域模型
│   │   ├── dal/                    # 具体业务层：组合 dao，完成特定业务
│   │   │   ├── mod.rs
│   │   │   ├── agent_dal.rs        # Agent 业务（如：本地存储 + 远程 API 组合）
│   │   │   ├── org_dal.rs          # Organization 业务
│   │   │   └── task_dal.rs         # Task 业务
│   │   └── dao/                    # 数据层：以业务视角增删查改数据
│   │       ├── mod.rs
│   │       ├── agent_dao.rs        # Agent 数据操作
│   │       ├── org_dao.rs          # Organization 数据操作
│   │       └── task_dao.rs         # Task 数据操作
│   │
│   └── pkg/                        # 公共包（存储、外部 API、常量等）
│       ├── mod.rs
│       ├── storage/                # 数据存储（SQLite）
│       │   ├── mod.rs
│       │   └── sqlite.rs
│       ├── external/               # 外部 API 封装及其数据结构
│       │   └── mod.rs
│       └── constants/              # 项目常量
│           └── mod.rs
│
├── tests/                          # 集成测试
├── docs/                           # 文档
│   ├── architecture/               # 架构设计
│   ├── api/                        # API 文档
│   └── rfc/                        # RFC 提案
└── Cargo.toml
```

---

## 4. 分层架构

```
┌──────────────────────────────────────────────────┐
│   handlers/   HTTP 层                             │
│   接收 Req → 调用 domain 方法 → 返回 Resp         │
├──────────────────────────────────────────────────┤
│   service/domain/   领域层                        │
│   定义领域模型和核心领域方法                       │
├──────────────────────────────────────────────────┤
│   service/dal/   具体业务层                       │
│   组合 dao，完成特定业务（本地 + 远程）            │
├──────────────────────────────────────────────────┤
│   service/dao/   数据层                           │
│   以业务视角增删查改数据                           │
├──────────────────────────────────────────────────┤
│   pkg/   公共包                                   │
│   storage / external API / constants             │
└──────────────────────────────────────────────────┘
```

### 各层职责说明

| 层 | 位置 | 职责 |
|----|------|------|
| **Handler** | `handlers/` | 接收 HTTP 请求，参数校验，调用 domain，返回 JSON |
| **Domain** | `service/domain/` | 领域模型定义，核心领域方法 |
| **DAL** | `service/dal/` | 组合 DAO，完成具体业务（可混合本地+远程） |
| **DAO** | `service/dao/` | 数据增删查改，屏蔽底层存储细节 |
| **PKG** | `pkg/` | 存储驱动、外部 API 封装、常量管理 |

---

## 5. Handler 层规范

Handler 的核心逻辑固定为三步：

```rust
// 1. 接收输入 Req
// 2. 调用 service domain 方法
// 3. 将结果转换为 Resp 返回

async fn create_agent(
    Json(req): Json<CreateAgentReq>,
) -> Result<Json<ApiResponse<CreateAgentResp>>, AppError> {
    let agent = AgentDomain::create(req.into()).await?;
    Ok(Json(ApiResponse::success(agent.into())))
}
```

每个 handler 文件中包含：
- `XxxReq` — 请求结构体
- `XxxResp` — 响应结构体
- handler 函数

---

## 6. API 设计规范

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

### 错误响应格式
```json
{
  "code": 400,
  "message": "Bad Request",
  "error": "详细错误信息"
}
```

---

## 7. 存储方案

- **当前**：SQLite（轻量、无需独立服务、方便开发）
- **位置**：`pkg/storage/sqlite.rs`
- **未来**：可扩展为 PostgreSQL 等

---

## 8. pkg 包规范

`pkg/` 是项目的公共基础包，包含：

| 子包 | 内容 |
|------|------|
| `pkg/storage/` | SQLite 连接、迁移、通用查询封装 |
| `pkg/external/` | 外部 API 客户端及其请求/响应数据结构 |
| `pkg/constants/` | 项目全局常量（状态码、枚举值等） |

**原则**：`pkg/` 不依赖 `service/` 和 `handlers/`，只被上层依赖。

---

## 9. 开发流程

1. 在 `pkg/constants/` 定义相关常量
2. 在 `service/domain/` 定义领域模型
3. 在 `pkg/storage/` 实现数据库 schema
4. 在 `service/dao/` 实现数据操作
5. 在 `service/dal/` 组合业务逻辑
6. 在 `handlers/` 定义 Req/Resp 并实现 handler
7. 在 `router.rs` 注册路由
8. 在 `tests/` 编写测试

---

## 10. 下一步

- [ ] 搭建目录结构骨架
- [ ] 配置 SQLite（rusqlite 或 sqlx）
- [ ] 定义 Agent 领域模型
- [ ] 实现 Agent CRUD API
