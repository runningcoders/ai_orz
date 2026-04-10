//! # 架构说明

## 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务。

---

## 项目整体架构：三级 cargo workspace

```
ai_orz/
├── **common** 🎯 独立公共 crate
│   ├── src/api/              # 所有前后端共用 API DTO（按功能分组）
│   ├── src/constants/        # 公共常量、基础类型（ApiResponse、状态枚举等）
│   └── src/enums/            # 公共枚举（UserRole 等）
│
├── **src** 后端服务
│   ├── models/               # 持久化实体 PO
│   ├── handlers/             # HTTP 接口层（按业务域/功能分组，每个方法一个文件）
│   ├── service/
│   │   ├── dao/              # 数据访问层 DAO（单一数据源操作）
│   │   ├── dal/              # 业务数据访问层 DAL（组合 DAO 提供业务级数据操作）
│   │   └── domain/           # 领域层（核心业务逻辑）
│   ├── middleware/           # Axum 中间件（JWT认证、RequestContext注入）
│   └── pkg/                  # 公共工具包
│
└── **frontend** 前端 Dioxus 应用
    ├── src/
    │   ├── api/              # API 客户端（调用后端接口，所有 DTO 从 common 导入）
    │   └── components/        # UI 组件（每个页面一个组件）
    └── ...
```

**common crate 设计原则：**
- ✅ 所有前后端共用的 request/response DTO 都放在 `common/src/api/`，消除重复定义
- ✅ 所有公共枚举都放在 common，保证前后端类型一致
- ✅ PO 实体保持在后端 `models/`，不移动到 common（只需要前端看到 DTO）
- ✅ 后端数据库枚举字段直接使用 common 中的枚举类型，实现编译期类型安全

---

## 核心概念

### 1. Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信
- **关系**：直接持有装配好的 Brain，每个 Agent 属于一个组织，有角色字段

### 2. Brain（大脑）
- **定义**：聚合根，包含思考 + 记忆
- **结构**：
```rust
pub struct Brain {
    pub cortex: Cortex,           // 思考推理
    pub memory: Memory,         // 记忆系统 🧠
}
```

### 3. Memory（记忆系统）
- **定义**：分层记忆系统，按照人类认知设计
- **结构**：
```rust
pub struct Memory {
    pub core: CoreMemory,       // 核心认知 → soul + capabilities
    pub working: Vec<MemoryTrace>, // 当前会话工作记忆
}

pub struct CoreMemory {
    pub soul: String,           // 灵魂/性格/角色设定
    pub capabilities: String,   // 能力列表 JSON
}
```

### 4. Cortex（大脑皮层）
- **定义**：具体的思考推理执行，包含模型配置 + 推理实例
- **关系**：一个 ModelProvider 对应一个 Cortex

### 5. ModelProvider（模型提供商）
- **定义**：保存 LLM 模型配置信息，可以被多个 Agent 复用，属于一个组织

### 6. Organization（组织）
- **定义**：顶级租户，包含多个用户、多个 Agent、多个 ModelProvider
- **角色体系**：SuperAdmin → Admin → Member，支持权限控制

### 7. User（用户）
- **定义**：登录用户，属于一个组织，有角色和状态

### 8. EventQueue（事件总线）
- **定义**：轻量级内存事件队列，支持优先级排序和顺序保证
- **设计文档**：详见 [docs/event_design.md](./event_design.md)

---

## 组织用户权限体系

```
Organization (组织)
  └─► User (用户，通过 organization_id 关联)
       ├─► SuperAdmin (超级管理员 - 系统初始化时创建)
       ├─► Admin (管理员)
       └─► Member (普通成员)
```

**认证方案：** JWT + HttpOnly Cookie，适配单实例部署场景
- 公共路由：健康检查、初始化、登录、登出、获取组织列表 → 无需认证
- 保护路由：所有业务接口 → 需要 JWT 认证
- RequestContext 自动注入当前登录用户信息和组织 ID

---

## 🧠 记忆系统最终架构

记忆系统按照人类认知分为四层，设计原则：
- ✅ 核心认知在 Brain 内存，每次调用全部拼入 prompt
- ✅ 当前会话工作记忆在 Brain 内存，每次调用全部拼入 prompt
- ✅ 短期记忆索引存在 SQLite，需要时检索相关片段拼入
- ✅ 长期记忆知识图谱存在 SQLite，需要时检索相关片段拼入
- ✅ 原始细节按天存储为 markdown 文件，人类可读

| 层级 | 位置 | 存储 | 访问方式 | 内容 |
|------|------|------|----------|------|
| **Core Memory** 🎨 | Brain 内存 | 内存 + AgentPo 数据库 | 每次调用 **全部拼入 prompt** | 我是谁，我会做什么，我的性格 → 基础认知底色 |
| **Working Memory** ⚡ | Brain 内存 | 只在内存 | 每次调用 **全部拼入 prompt** | 当前会话正在进行的对话 |
| **Short-Term Memory** 📝 | SQLite 索引 + 按天文件存储原始细节 | 需要时检索相关摘要拼入 | 最近一段时间对话的归纳摘要 |
| **Long-Term Knowledge** 📚 | SQLite 知识图谱 + 按天文件存储原始细节 | 需要时检索相关知识拼入 | 归纳总结后的知识图谱节点，包含关系 |

### 文件存储结构（原始细节）

```
data/
  ├── ai_orz.db              # 主数据库（存索引和知识图谱）
  └── long_term_memory/       # 长期记忆原始细节
        └── {agent_id}/      # 按 Agent 分目录
              ├── 2026-04-07.md  # 一天一个 markdown 文件，追加写入，人类可读
              ├── 2026-04-06.md
              └── ...
```

**优点**：
- ✅ 文件数量极少 → 一年才 365 个文件，完全不会多
- ✅ 原始细节人类可读 → 直接打开就能看今天所有对话
- ✅ append-only 写入 → 不覆盖历史，天然版本控制
- ✅ 迁移简单 → 整个 data 目录打包就带走

---

## 事件总线架构

详见独立设计文档：[docs/event_design.md](./event_design.md)

### 核心设计要点

| 设计点 | 实现方案 |
|--------|----------|
| 持久化 | 所有事件先存入 SQLite `messages` 表，总线只存 `message_id` 元数据 |
| 崩溃恢复 | 服务启动自动从数据库恢复所有 pending 事件 |
| 优先级排序 | 按 `priority DESC, created_at ASC` 排序，高优先级先出队 |
| 顺序保证 | 相同 `order_key` 保证顺序处理，不同 `order_key` 可以并行 |
| 并发模型 | Tokio 任务调度，相同 key 顺序锁保证顺序 |

---

## 最终实体层次关系

```
Agent (po + brain: Option<Brain>)
  └─► Brain 🧠
       ├─► Cortex (model_provider: ModelProvider, cortex: Box<dyn CortexTrait>)
       └─► Memory
            ├─► CoreMemory (soul: String, capabilities: String)
            └─► working: Vec<MemoryTrace>
```

---

## 分层职责清晰化

| 层级 | 模块 | 职责 |
|------|------|------|
| **models** | 实体定义 | 定义所有持久化对象和业务实体 |
| **service/dao** | 数据访问层 | 数据库访问，文件读写 |
| **service/dal** | 业务逻辑层 | 组合 dao 完成业务逻辑 |
| **service/domain** | 领域层 | 核心业务规则，编排 dal |
| **handlers** | HTTP 接口层 | 接收请求，调用 domain，返回响应 |

---

## 设计原则

1. **严格分层不跨级调用** → 遵循 `handlers → domain → dal → dao → models` 层级依赖
2. **所有 service 层方法必须传递 RequestContext** → 方便日志追踪和扩展
3. **原始细节不占内存** → 短期长期都在数据库，只在需要时检索
4. **渐进式演进** → 短期积累到一定数量触发归纳，不断更新核心记忆和知识图谱
5. **人类可读** → 原始细节按天 markdown 存储，不需要工具直接查看

---

## 支持的模型提供商

| 提供商 | 实现文件 | 支持 |
|--------|----------|------|
| OpenAI 官方 | `service/dao/cortex/rig/openai.rs` | ✅ |
| DeepSeek | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| 阿里云通义千问 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| 字节跳动豆包 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |
| Ollama 本地 | `service/dao/cortex/rig/ollama.rs` | ✅ |
| 自定义 OpenAI 兼容接口 | `service/dao/cortex/rig/openai_compatible.rs` | ✅ |

---

## 类型安全设计

### 枚举类型安全

项目中所有存储为整数的枚举字段，现在都直接使用 Rust 枚举类型存储：

| PO 实体 | 枚举字段 | 枚举类型 | 说明 |
|---------|----------|----------|------|
| `AgentPo` | `status` | `AgentStatus` | 已完成 |
| `ModelProviderPo` | `status` | `ModelProviderStatus` | 已完成 |
| `ModelProviderPo` | `provider_type` | `ProviderType` | 已完成 |
| `OrganizationPo` | `status` | `OrganizationStatus` | 已完成 |
| `UserPo` | `role` | `UserRole` (common) | 已完成 |
| `UserPo` | `status` | `UserStatus` | 已完成 |

**实现方式：**
- `common` 中定义枚举，为枚举实现 `rusqlite::ToSql` 和 `rusqlite::FromSql` trait
- 存储到 SQLite 自动转换为 `i32`，读取自动转换为枚举
- 编译期类型检查，避免 magic number 错误
- serde 序列化保持整数输出，API 契约不变

## 数据库设计

所有建表语句都统一放在 `src/pkg/storage/sql.rs` 作为常量，每个常量注释对应到实体：

| 表名 | 对应实体 |
|------|----------|
| `agents` | `AgentPo` |
| `model_providers` | `ModelProviderPo` |
| `organizations` | `OrganizationPo` |
| `users` | `UserPo` |
| `messages` | `MessagePo` (事件总线消息) |
| `tasks` | `Task` |
| `short_term_memory_index` | `ShortTermMemoryIndexPo` |
| `long_term_knowledge_node` | `LongTermKnowledgeNodePo` |
| `knowledge_reference` | `KnowledgeReferencePo` |
| `knowledge_node_relation` | `KnowledgeNodeRelationPo` |

---

## 单元测试规范

- 每个 DAO/DAL/Domain 模块对应一个单元测试文件
- 每个单元测试独立，使用随机临时 SQLite 文件，互不干扰
- 每个测试在执行前重新初始化 storage，保证干净环境
- 所有建表使用定义好的常量，不重复写 SQL
- 当前项目总测试数：**66 个** → **全部通过** ✅

### 测试设计要点

| 问题 | 解决方案 |
|------|----------|
| OnceLock 只能初始化一次 | 每个测试重新初始化 storage，使用随机数据库文件名 → 完全独立 |
| 一个测试 panic 影响其他 | 每个测试独立运行，互不干扰 → 失败只影响自己 |
| 代码可读性 | 每个测试短小精悍，独立清晰 → 好维护 |
