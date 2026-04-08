# Agent 开发规范与最佳实践

本文总结了 AI_orz 项目中 Agent 和记忆系统开发过程中总结出的最佳实践和规范。

## 一、记忆系统架构设计 🧠

### 1.1 记忆分层（最终设计）

记忆系统按照人类认知分为四层，符合人类记忆运行机制：

| 层级 | 位置 | 存储 | 访问方式 | 内容 |
|------|------|------|----------|------|
| **Core Memory** 🎨 | Brain 内存 + `AgentPo` 持久化 | 内存 + 数据库 | 每次调用 **全部拼入 prompt** | 我是谁，我会做什么，我的性格，行事规则 → 基础认知底色 |
| **Working Memory** ⚡ | Brain 内存 | 只在内存 | 每次调用 **全部拼入 prompt** | 当前会话正在进行的对话 |
| **Short-Term Memory** 📝 | SQLite 索引 + 按天文件存储原始细节 | 需要时检索相关片段拼入 | 最近一段时间的对话，归纳总结后的关键信息 |
| **Long-Term Memory** 📚 | SQLite 知识图谱 + 引用短期索引 | 需要时检索相关片段拼入 | 归纳总结后的知识图谱节点，包含关系和引用 |

### 1.2 核心设计原则

1. **核心认知在 Agent 自身** → `core = soul + capability` 直接存储在 `AgentPo`，每次全拼入 ✅
2. **短期/长期都不占内存** → 只在用户提问时实时检索相关内容，内存永远干净 ✅
3. **原始细节按天归档** → `data/long_term_memory/{agent_id}/{YYYY-MM-DD}.md`，一天一个文件，人类可读，文件数量极少 ✅
4. **知识图谱渐进式演进** → 短期索引积累到一定数量，自动归纳更新知识图谱，长期数量可控 ✅
5. **ID = 内容 hash** → 自动去重，相同内容不会重复存储 ✅

### 1.3 文件存储结构

```
data/
  ├── ai_orz.db              # 主数据库（存索引和知识图谱）
  └── agent/                # Agent 相关数据
        └── {agent_id}/       # 按 Agent ID 分目录
              └── memory/      # 记忆原始细节
                    ├── 2026-04-07.md  # 一天一个 markdown 文件，追加写入
                    ├── 2026-04-06.md
                    └── ...
```

**优点：**
- 文件数量极少，一年才 365 个文件
- 原始细节人类可读，直接打开就能看
- append-only 写入，不覆盖历史，天然版本控制

---

## 二、代码分层规范

### 2.1 实体定义

所有记忆相关实体统一放在 `models/memory.rs`：

```rust
// models/memory.rs
pub struct MemoryTrace;         // 一条原始记忆，既可以在内存也可以写入文件
pub struct ShortTermMemoryIndexPo;  // 短期索引，SQLite 持久化
pub struct LongTermKnowledgeNodePo; // 长期知识图谱节点，SQLite 持久化
pub struct KnowledgeReferencePo;    // 知识节点引用原始短期索引
pub enum MemoryRole;               // 记忆角色：User / Assistant / System / Summary
```

### 2.2 数据访问层（DAO）

所有记忆相关 DAO 统一放在 `service/dao/memory/`：

```
service/dao/memory/
├── mod.rs          # 定义 trait + 单例 + 导出
├── sqlite.rs       # SQLite 特定实现
└── sqlite_test.rs  # 单元测试
```

### 2.3 SQL 建表语句规范

- 所有建表语句统一放在 `pkg/storage/sql.rs` 作为常量
- 每个常量对应一个实体，注释标明对应实体
- 单元测试也使用同一个常量，不重复写建表语句
- 启动时自动创建所有表，在 `pkg/storage/sqlite.rs::init_db` 中统一初始化

```rust
// pkg/storage/sql.rs
/// SQLite: 短期记忆索引表建表语句
///
/// 对应实体: [crate::models::memory::ShortTermMemoryIndexPo]
pub const SQLITE_CREATE_TABLE_SHORT_TERM_MEMORY_INDEX: &str = r#"..."#;
```

### 2.4 数据库架构分层（可扩展设计）

```
pkg/storage/
├── mod.rs      # 公共接口：全局 Storage 单例 + init
├── sql.rs       # 所有建表 SQL 常量集中定义
└── sqlite.rs    # SQLite 特定初始化，调用 sql 常量建表
```

**优点：**
- 预留扩展其他数据库，以后添加 PostgreSQL 只需要添加 `pkg/storage/postgres.rs`
- SQL 常量集中管理，不会重复
- 公共接口保持不变，上层不需要改动

---

## 三、单元测试规范

### 3.1 测试位置

- 每个 DAO 模块下直接放置测试文件：`service/dao/xxx/sqlite_test.rs`
- 测试使用内存数据库，不依赖全局连接池，独立可运行

### 3.2 测试规范

```rust
// 错误：硬编码建表语句 ❌
conn.execute("CREATE TABLE ...", ()).unwrap();

// 正确：使用定义好的常量 ✅
conn.execute(crate::pkg::storage::sql::SQLITE_CREATE_TABLE_XXX, ()).unwrap();
```

### 3.3 当前测试统计

- **总测试数**: 33 个
- **通过率**: 100% (33/33)
- **记忆系统新增测试**: 4 个

---

## 四、API 设计规范

### 4.1 所有 service 层方法必须传递 `RequestContext`

**强制约定**：所有 service 层（DAO/DAL/Domain）公共方法都必须传递 `ctx: RequestContext` 作为第一个参数 ✅

```rust
// ✅ 正确
fn wake_cortex(&self, ctx: RequestContext, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;

// ❌ 错误 - 缺少 ctx
fn wake_cortex(&self, provider: &ModelProvider, prompt: &str) -> Result<String, AppError>;
```

**原因**：方便后续日志串联、追踪、权限扩展，即使当前不用也必须传递

### 4.2 Memory DAO API

```rust
pub trait MemoryDaoTrait: Send + Sync {
    // 追加写入记忆追踪到每日文件，并插入短期索引
    fn append_memory_trace(
        &self,
        ctx: RequestContext,
        trace: &MemoryTrace,
        summary: String,
        tags: Vec<String>,
    ) -> Result<ShortTermMemoryIndexPo, AppError>;

    // 按 ID 查询短期索引
    fn get_short_term_index(
        &self,
        ctx: RequestContext,
        id: &str,
    ) -> Result<Option<ShortTermMemoryIndexPo>, AppError>;

    // 全文检索短期索引
    fn search_short_term(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ShortTermMemoryIndexPo>, AppError>;

    // 读取记忆原始内容（从日期文件按偏移读取）
    fn read_memory_content(&self, index: &ShortTermMemoryIndexPo) -> Result<String, AppError>;

    // 创建/更新知识节点（upsert）
    fn save_knowledge_node(
        &self,
        ctx: RequestContext,
        node: &LongTermKnowledgeNodePo,
    ) -> Result<(), AppError>;

    // 批量创建/更新知识节点
    fn batch_save_knowledge_nodes(
        &self,
        ctx: RequestContext,
        nodes: &[LongTermKnowledgeNodePo],
    ) -> Result<(), AppError>;

    // 添加知识引用
    fn add_knowledge_reference(
        &self,
        ctx: RequestContext,
        reference: &KnowledgeReferencePo,
    ) -> Result<(), AppError>;

    // 批量添加知识引用
    fn batch_add_knowledge_references(
        &self,
        ctx: RequestContext,
        references: &[KnowledgeReferencePo],
    ) -> Result<(), AppError>;
}
```

---

## 五、调用流程设计

### 5.1 完整调用流程

```
用户提问 → Agent/Brain:
  1. 🎨 Core Memory → 全部拼入 prompt (soul + capability)
  2. ⚡ Working Memory → 当前会话全部拼入 prompt
  3. 🔍 Short-Term Memory → 全文检索相关摘要，拼入 prompt
  4. 🧩 Long-Term Memory → 全文检索相关知识节点，拼入 prompt
  ↓
  5. 🧠 Cortex 推理 → 生成回答
  ↓
  6. 💾 保存本轮对话:
     - 添加到 WorkingMemory.conversation
     - 生成摘要，追加写入日期文件
     - 插入短期索引到 SQLite
  ↓
  7. 🧹 触发归纳（当短期索引达到阈值）:
     - LLM 归纳短期对话 → 更新 Core Memory / 生成/更新知识图谱节点
     - 建立引用关系到原始对话
  ↓
  8. 📤 返回结果给用户
```

### 5.2 渐进式归纳

- 短期索引积累到 N 条 → 触发归纳
- 归纳结果更新 Core Memory → 写入 `AgentPo.soul` / `AgentPo.capability`
- 归纳生成/更新知识图谱节点 → 建立引用到原始短期索引
- 旧的短期索引保留不变，知识图谱不断演进

---

## 六、已完成 TODO ✅

- [x] 定义所有记忆实体：`MemoryTrace` / `ShortTermMemoryIndexPo` / `LongTermKnowledgeNodePo` / `KnowledgeReferencePo`
- [x] `MemoryTrace` 一条记忆既可以在内存也可以写入文件，ID = 内容 hash
- [x] 实现 `MemoryDao` 完整 API：追加/查询/检索/知识节点批量 upsert
- [x] 原始细节按天存储为 markdown 文件，人类可读
- [x] `pkg/storage` 重构：sql 常量集中 + sqlite 特定初始化，预留扩展其他数据库
- [x] 所有单元测试使用 SQL 常量，不重复建表语句
- [x] 所有 service 层方法都传递 `RequestContext`
- [x] 所有测试全部通过：33/33 ✅

## 七、下一步 TODO 🚧

- [ ] 修改 `models/agent.rs` → `AgentPo` 添加 `soul` / `capability` 字段
- [ ] 修改 `models/brain.rs` → 添加 `AgentMemory` 结构 `core + working`
- [ ] 在 `AgentDal.assemble_brain` 中装配记忆系统
- [ ] 集成到调用流程
- [ ] 实现自动归纳触发逻辑

---

## 八、经验总结

1. **设计对齐人类认知** → 架构更容易理解，也更容易演进
2. **渐进式实现** → 先做基础架构，再慢慢完善功能
3. **分层清晰** → 每个层职责单一，方便测试和替换
4. **常量集中管理** → SQL 建表语句统一放在 `pkg/storage/sql.rs`，避免重复
5. **预留扩展** → 数据库层分离，方便以后支持其他数据库
6. **原始细节人类可读** → 按天 markdown 存储，不需要工具直接就能看
