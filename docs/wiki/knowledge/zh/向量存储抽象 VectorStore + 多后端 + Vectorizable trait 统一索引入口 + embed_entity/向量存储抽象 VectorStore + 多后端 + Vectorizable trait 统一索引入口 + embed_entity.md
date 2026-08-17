---
kind: wiki_knowledge_card
name: 向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity 实现
category: pkg 存储基础设施 + 模型层 PO 索引
scope:
  - "src/pkg/storage/vector.rs"
  - "src/pkg/storage/mem_vector.rs"
  - "src/models/vector.rs"
  - "src/service/dal/**/*.rs"（embed_entity 调用点）
source_files:
  - src/pkg/storage/vector.rs:Ln-Lm（VectorStore trait：init_collection/upsert/search/get/delete/clear_collection/flush；通用 VectorSearchHit/VectorRow/VectorMeta/VectorIndexParams/VectorCollection）
  - src/pkg/storage/mem_vector.rs:Ln-Lm（InMemoryVectorStore 内存实现：HashMap+余弦距离、Bincode Encode/Decode 懒持久化）
  - src/models/vector.rs:Ln-Lm（Vectorizable trait：vectorize_text/vector_collection/vector_content_hash/vector_expire_at/needs_reindex；MatchType/SearchMatchInfo 混合匹配元信息）
  - src/service/dao/cortex/native/mod.rs:Ln-Lm（CortexDao.embed() 向量生成 + embed_text_for_search() 搜索场景查询向量构造）
  - src/pkg/storage/fts5.rs:Ln-Lm（escape_fts5_keyword FTS5 短语匹配转义）
  - src/service/dal/*（各业务 DAL search() 内部统一 embed_entity → upsert → search 调用链路）
  - docs/archive/design-archive/vector_search_architecture.md
  - docs/archive/design-archive/full_entity_fts5_search_design.md
  - docs/plan/系统初始化模型配置策略调整.md（Embedding 创建/更新重建触发条件矩阵）
  - docs/wiki/zh/content/基础设施/存储系统/存储系统.md
  - docs/wiki/zh/content/基础设施/基础设施.md
  - docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md
  - 【平行卡】docs/wiki/knowledge/zh/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵.md（Embedding 业务生命周期与重建触发条件）
---

# 向量存储抽象 + Vectorizable trait 统一索引

## §1 整体方案
向量存储与实体索引**彻底从业务 DAO/DAL 中解耦**：底层 `VectorStore` trait 定义通用向量操作（支持多后端可插拔），上层 `Vectorizable` trait 让每个 PO 自己决定"哪些字段参与向量化"（信息专家原则，与 CredentialDetail 行为下沉同一模式），中间 DAL 层通过统一 `embed_entity(ctx, cortex, po)` 工厂函数把 PO → 向量索引参数并 upsert 到底层 VectorStore。**禁止在 DAL 层手工 format! 拼接向量文本**（AGENTS §4.8 强制约束）。四层分层：

(a) **VectorStore 通用抽象（pkg 层，零业务）**：`src/pkg/storage/vector.rs` 定义通用接口：`init_collection(collection, dimensions)` / `upsert(collection, id, params)` / `search(collection, query_vector, top_k)` → Vec<VectorSearchHit> / `get(collection, id)` / `delete(collection, id)` / `clear_collection(collection)` / `flush()`。关键设计：
   - collection 是逻辑命名空间（如 "agents"/"tools"/"skills"/"tasks"/"memory:short_term"/"memory:knowledge_node"），各业务实体调用时统一 `Po::vector_collection()` 获取，**禁止硬编码字符串**。
   - 通用行数据：VectorRow { id, vector, meta(VectorMeta { content_hash, embedding_model, indexed_at, expire_at }) }。
   - Search 返回 VectorSearchHit { row, distance }（距离越小越相似，余弦距离 0~2；约定阈值 0.8）。
   - 多后端实现（按配置切换，架构图见 基础设施.md）：**InMemoryVectorStore**（纯 Rust HashMap + 余弦距离，Bincode Encode/Decode 懒持久化，dev 默认）、**LanceVectorStore**（LanceDB 嵌入式向量库，复杂查询 + 生产推荐）、**HnswStore**（HNSW 近邻图索引，高维向量大数量场景）、**SqliteVssStore**（SQLite VSS 扩展，与主 DB 同文件）。

(b) **Vectorizable trait（PO 层，信息下沉）**：`src/models/vector.rs` 定义，所有支持向量索引的 PO **必须实现**（AGENTS §4.8 强制）。核心契约 2 个 + 默认实现 3 个：
   - **必写** `fn vectorize_text(&self) -> String`：PO 自己决定「哪些字段拼接成待向量化文本」（如 AgentPo 拼接 name + description + role_setting + capabilities，用清晰分隔符分隔避免字段边界融合导致语义漂移）。
   - **必写** `fn vector_collection() -> &'static str`：返回 collection 名（全局唯一，与业务一一对应）。
   - **默认实现** `vector_content_hash(&self) -> String`（sha256(vectorize_text)）、`vector_expire_at(&self) -> Option<i64>`（None=永不过期，短期记忆如 ShortTermMemoryIndexPo 可返回 created_at+7d）、`needs_reindex(&self, existing_hash) -> bool`（hash 变了或过期了才重索引，相等 + 未过期直接跳过 upsert，避免无意义计算）。
   - 已实现 Vectorizable 的实体 7 类：AgentPo(agents) / ToolPo(tools) / SkillPo(skills) / TaskPo(tasks) / ProjectPo(projects) / ShortTermMemoryIndexPo(memory:short_term) / LongTermKnowledgeNodePo(memory:knowledge_node)。

(c) **统一索引入口 embed_entity(ctx, cortex, po) → Result<()>`（DAL 层，禁止各 DAO 手写）**：逻辑 = ① po.vector_content_hash() + VectorStore.get(collection, id) 取旧 row.meta.content_hash → ② po.needs_reindex(&old_hash) 判断是否需要重索引 → ③ 不需要 → 立即返回；需要 → ④ cortex.embed_text([po.vectorize_text()]) 生成向量 → ⑤ 构造 VectorIndexParams(vector, hash, model_provider_id, model_name, po.vector_expire_at()) → ⑥ VectorStore.upsert(collection, po.id, params)。整套流程封装成 DAL 层复用函数，**任何 PO 创建/更新后只需一行 `embed_entity(ctx, cortex, po).await?`**，无重复样板。

(d) **搜索场景查询向量构造 embed_text_for_search**：`CortexDao.embed_text_for_search(ctx, provider, keyword)` 与 embed_entity 使用同一模型（保证 query 向量和索引向量处于同一向量空间），返回 VectorIndexParams 供 DAL 层直接取 params.vector 传给 VectorStore.search()。

(e) **FTS5 全文搜索通用工具（与向量搜索互补）**：`src/pkg/storage/fts5.rs` 提供统一 `escape_fts5_keyword(keyword)` 工具：用户原始关键词 → 内部双引号双写转义 → 最外层用双引号包裹成**短语匹配**（phrase match），禁止把空格解释成 FTS5 AND 操作符。空关键词直接返回空串（DAO 层检查空串不触发 FTS5 MATCH，避免 SQL 语法错误）。所有业务 DAO 全文检索一律复用此函数，禁止各自手写转义（SQL 注入/语法错单入口收敛）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [pkg/storage/vector.rs](src/pkg/storage/vector.rs) | VectorStore 抽象 + 通用行/参数 | `trait VectorStore`；VectorIndexParams(VectorIndexParams 构造）；VectorMeta/Bincode Encode Decode |
| [pkg/storage/mem_vector.rs](src/pkg/storage/mem_vector.rs) | InMemoryVectorStore 实现（dev 默认）| HashMap 存 collection；search 计算余弦距离 top-k；Bincode + fs 懒持久化 |
| [models/vector.rs](src/models/vector.rs) | Vectorizable trait + MatchType + SearchMatchInfo | `trait Vectorizable { vectorize_text, vector_collection, content_hash, expire_at, needs_reindex }`；MatchType(Vector/Keyword/Hybrid)；SearchMatchInfo（向量距离/fts_rank/命中字段统一记录）|
| [pkg/storage/fts5.rs](src/pkg/storage/fts5.rs) | FTS5 关键词转义 | `escape_fts5_keyword(keyword) -> String`（短语匹配；含 6 组单元测试覆盖空串/转义/空格/特殊字符）|
| [dao/cortex/native/mod.rs](src/service/dao/cortex/native/mod.rs) | cortex 向量生成 | `embed(ctx, provider, texts: &[String]) -> Vec<Vec<f32>>` 主入口；`embed_text_for_search(ctx, provider, keyword) -> VectorIndexParams` 搜索场景专用（带 hash + 模型信息）|
| [models/agent.rs](src/models/agent.rs) + memory/... 等 | Vectorizable 7 类实现 | AgentPo::vectorize_text（name+description+role_setting）、ToolPo（name+description+input_schema）、TaskPo（title+description+execution_plan）、ShortTermMemoryIndexPo、LongTermKnowledgeNodePo 等 |
| 【① Design 1】vector_search_architecture.md | 为什么选 VectorStore trait + LanceDB 默认后端 + 为什么 collection 按业务分 | docs/archive/design-archive/vector_search_architecture.md |
| 【① Design 2】full_entity_fts5_search_design.md | escape_fts5_keyword 设计动机（FTS5 SQL 注入防护）| docs/archive/design-archive/full_entity_fts5_search_design.md |
| 【③ Wiki 长文 1】存储系统.md §向量存储抽象与多后端 | VectorStore 类图、后端对比表、init_collection 时序 | docs/wiki/zh/content/基础设施/存储系统/存储系统.md |
| 【③ Wiki 长文 2】记忆和向量系统.md §Vectorizable | 7 PO Vectorizable 列表 | docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md |
| 【平行卡】三位一体混合搜索（FTS5 + 向量 + 合并排序） | DAL 层 search() 统一策略 | docs/wiki/knowledge/zh/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）.md |

## §3 架构约定

1. **Vectorizable.vectorize_text 必须保持"字段边界可辨"**：字段之间用明确的分隔符（如 `\n## name:\n{name}\n## description:\n{description}`），不能直接字符串拼接 `format!("{name}{description}")`。否则 embedding 模型会把"最后一个词+下一个字段名"混成一个新词，导致边界语义漂移（查询"name: xxx"匹配不到）。
2. **collection 名用冒号表达层级**：记忆系统长期记忆 = `"memory:knowledge_node"`、短期记忆 = `"memory:short_term"`，业务实体直接用复数 `"agents"` / `"skills"`。禁止出现驼峰 `memoryKnowledgeNode`（不直观）。新增实体时在命名评审会上确定 collection 名，**避免未来迁移 collection 名（旧数据 vector 全部作废）**。
3. **向量失败不阻塞主业务流程（降级原则）**：embed_entity 或 VectorStore.upsert 失败时：创建/更新 PO 本身照常成功（业务数据绝不因向量问题丢），只写 log_warn! 告警 + 挂一个后续异步重建向量索引的 AOP 事件兜底。search() 时向量搜索失败（VSS 扩展未安装、Embedding Provider 不可用）→ 降级为纯 FTS5 关键词搜索（保留 keyword_results，vector_scores 空 HashMap，合并照常进行）。
4. **SearchMatchInfo 必须随业务实体一起透传到返回响应**：查询侧每个返回的 Tool/Skill/Agent/Memory 条目均携带 SearchMatchInfo { match_type, vector_distance, keyword_fields, embedding_model, indexed_at, content_hash, fts_rank }。前端可据此做 UI 语义标注（"向量命中 相似度 0.72"或"关键词命中 字段 name"）。match_type 三态 Vector/Keyword/Hybrid 精确标记，方便统计分析"不同关键词下向量 vs FTS5 命中率"。
5. **向量过期 expire_at：仅短期记忆类实体使用**：ShortTermMemoryIndexPo expire_at = created_at + 7d（长期不需要）；其他 PO 默认 None（永不过期，重建索引时通过 needs_reindex() hash 判断，不依赖过期）。过期不是软删除，仅表示"过了这个时间点下次搜索可能不返回"——数据行仍保留直到手动 rebuild。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止在 DAL/DAO 层手写 `format!("{} {}", po.name, po.description)` 当向量文本**（违反 AGENTS §4.8）。所有支持向量索引的业务实体必须在 models 文件中实现 Vectorizable trait；向量文本生成只能走 `po.vectorize_text()` 一处入口。否则新增字段时 6 个 DAL 各改一处 = 5 处漏掉 1 处 = 索引内容漂移 = 搜索命中不到。
2. ❌ **禁止 VectorStore.search 返回 distance > 阈值（默认 0.8）的结果**：search() 内部应先 top-k 然后再 distance 过滤，DAL 层再次双重过滤。超过阈值的向量结果语义意义不大（像乱匹配），会污染用户搜索体验并稀释 FTS5 关键词结果的权重。
3. ❌ **禁止新增 PO 支持向量索引时漏实现 Vectorizable**：若某 PO（如 OrganizationPo、AttachmentPo）被业务方要求搜索，但未实现 Vectorizable → 编译不会报错，但 DAL 调用 embed_entity 会无可用 impl。推荐用 custom test 断言检查「所有标记 #[searchable] 的 PO 均已实现 Vectorizable」（clippy 未来可 lint）。
4. ✅ **escape_fts5_keyword 强约束：所有 FTS5 MATCH 查询必须先过此函数**。禁止 DAO 层直接把用户输入 keyword 拼进 `"SELECT ... WHERE fts MATCH '{keyword}'"` SQL 字符串。未转义 = SQL 注入 + 语法错误 + 关键词含双引号直接报错。
5. ✅ **向量维度严格对齐强约束**：同一 collection 所有向量维度必须与 init_collection 传入的 dimensions 完全一致。embed_entity 内部必须读取 cortex provider 的 model_name（即 embedding model），若模型变动（换 provider 维度变）→ 整个 collection 必须 rebuild（clear_collection + 批量 re-index）。混用不同维度向量进同一 collection = 向量距离计算完全失真 = 搜索结果不可用（隐式 bug 极难查）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 3 篇 wiki 长文（存储系统/基础设施/记忆和向量系统）+ 2 Design + Plan 占位 + 1 平行卡（混合搜索）；对应 Wiki 长文 cite 段回链本卡 + 2 Design + 平行卡。
