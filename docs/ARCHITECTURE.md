//! # 架构说明

## 项目愿景

将 Agent 以组织化形式管理，可以共同完成任务。组织可以通过组网的形式完成更高级别的协作任务。

---

## 核心概念

### 1. Agent（智能体）
- **定义**：独立的执行单元，可以接收任务、执行操作、与其他 Agent 通信
- **关系**：直接持有装配好的 Brain，每个 Agent 有自己独立的记忆

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
- **定义**：保存 LLM 模型配置信息，可以被多个 Agent 复用

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

## 数据库设计

所有建表语句都统一放在 `src/pkg/storage/sql.rs` 作为常量，每个常量注释对应到实体：

| 表名 | 对应实体 |
|------|----------|
| `agents` | `AgentPo` |
| `model_providers` | `ModelProviderPo` |
| `organizations` | `OrganizationPo` |
| `tasks` | `Task` |
| `short_term_memory_index` | `ShortTermMemoryIndexPo` |
| `long_term_knowledge_node` | `LongTermKnowledgeNodePo` |
| `knowledge_reference` | `KnowledgeReferencePo` |

---

## 单元测试规范

- 每个 DAO 模块对应一个单元测试文件
- 单元测试使用内存数据库，不依赖全局连接池
- 所有建表使用定义好的常量，不重复写 SQL
- 当前项目总测试数：**33 个** → **全部通过** ✅
