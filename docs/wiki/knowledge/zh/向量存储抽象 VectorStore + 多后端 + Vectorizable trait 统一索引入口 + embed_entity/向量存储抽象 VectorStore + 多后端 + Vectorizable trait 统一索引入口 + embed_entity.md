---
kind: wiki_knowledge_card
name: 向量存储抽象 VectorStore + 多后端 + Vectorizable trait 统一索引入口 + embed_entity 实现
category: pkg 存储基础设施 + 模型层 PO 索引
scope:
  - "src/pkg/storage/vector.rs"
  - "src/pkg/storage/mem_vector.rs"
  - "src/pkg/storage/lance.rs"
  - "src/pkg/storage/hnsw.rs"
  - "src/pkg/storage/sqlite_vss.rs"
  - "src/models/vector.rs"
  - "src/service/dal/**/*.rs"（embed_entity 调用点 + 7 域 rebuild_vectors）
  - "src/service/dao/**/vector.rs"（7 域 Vector DAO：业务 Query → VectorFilter 转译）
  - "src/handlers/system/vector_rebuild.rs"（SuperAdmin 全量重建入口）
  - "src/handlers/finance/model_provider/rebuild_vectors_task.rs"（RebuildVectorsTask 后台任务）
  - "src/pkg/background_task/progress.rs"（TaskProgressCounter 细粒度进度计数）
  - "src/router.rs"（/vector-rebuild 路由注册）
  - "common/src/api/system.rs"（RebuildVectorsRequest DTO）
  - "frontend/src/api/system.rs"（rebuild_vectors 前端 API）
source_files:
  - 'src/pkg/storage/vector.rs:Ln-Lm（VectorStore trait：init_collection/upsert/search/get/delete/clear_collection/flush；search 新增 filter 参数 + update_payload；通用 VectorSearchHit/VectorRow/VectorMeta/VectorIndexParams/VectorCollection）'
  - 'src/pkg/storage/mem_vector.rs:Ln-Lm（InMemoryVectorStore 内存实现：HashMap+余弦距离、Bincode Encode/Decode 懒持久化；payload + filter 内存求值 + update_payload）'
  - 'src/pkg/storage/lance.rs:Ln-Lm（LanceVectorStore：payload 列平铺落库 + SQL only_if 谓词下推 + upsert/get/search 读写 payload + update_payload）'
  - 'src/pkg/storage/hnsw.rs:Ln-Lm（HnswStore：filter take×4 放大补偿 + update_payload）'
  - 'src/pkg/storage/sqlite_vss.rs:Ln-Lm（SqliteVssStore：建表/upsert 落 payload + get/search 读侧解析）'
  - 'src/models/vector.rs:Ln-Lm（Vectorizable trait + VectorPayload/VectorFilter/ReindexDecision：vectorize_text/vector_collection/vector_content_hash/vector_expire_at/needs_reindex + vector_payload/vector_id/reindex_decision；MatchType/SearchMatchInfo 混合匹配元信息）'
  - 'src/service/dao/cortex/native/mod.rs:Ln-Lm（CortexDao.embed() 向量生成 + embed_text_for_search() 搜索场景查询向量构造）'
  - 'src/pkg/storage/fts5.rs:Ln-Lm（escape_fts5_keyword FTS5 短语匹配转义）'
  - 'src/service/dao/memory/vector.rs（MemoryVectorDao：接收 MemoryQuery → DAO 内转译 VectorFilter）'
  - 'src/service/dao/message/vector.rs（MessageVectorDao：接收 MessageQuery → DAO 内转译 VectorFilter）'
  - 'src/service/dao/agent/vector.rs（AgentVectorDao：接收 AgentQuery → DAO 内转译 VectorFilter）'
  - 'src/service/dao/project/vector.rs + src/service/dao/task/vector.rs（Project/Task VectorDao：接收业务 Query → DAO 内转译谓词下推）'
  - 'src/service/dao/skill/vector.rs + src/service/dao/tool/vector.rs（Skill/Tool VectorDao：接收业务 Query → DAO 内转译谓词下推）'

  - src/service/dal/*（各业务 DAL search() 内部统一 embed_entity → upsert → search 调用链路）

  - docs/archive/design-archive/vector_search_architecture.md

  - docs/archive/design-archive/full_entity_fts5_search_design.md

  - docs/plan/系统初始化模型配置策略调整.md（Embedding 创建/更新重建触发条件矩阵）

  - docs/plan/向量搜索Pre-Filter改造.md（Payload 落库 + 谓词下推 + 三态索引实施计划）

  - docs/wiki/zh/content/基础设施/存储系统/存储系统.md

  - docs/wiki/zh/content/基础设施/基础设施.md

  - docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md

  - 'src/handlers/system/vector_rebuild.rs（SuperAdmin 全量重建 handler：POST /api/v1/system/vector-rebuild → check_super_admin → 注册 RebuildVectorsTask 后台任务 → 返回 task_id）'
  - 'src/handlers/finance/model_provider/rebuild_vectors_task.rs（RebuildVectorsTask：BackgroundTask 实现；遍历 7 域 agent/memory/skill/task/project/message/tool 调用各 DAL rebuild_vectors(ctx, progress)；新增 TaskProgressCounter 跨批次上报已处理条数；任务互斥——已有 Running 时返回 409）'
  - 'src/pkg/background_task/progress.rs（TaskProgressCounter：后台任务细粒度进度计数；Arc<AtomicUsize> + Relaxed Ordering；跨 await 可克隆零拷贝；语义与进度条 current_step/total_steps 正交）'
  - 'src/service/dal/mod.rs（VECTOR_REBUILD_PAGE_SIZE const = 200；各 DAL rebuild_vectors trait 签名统一：async fn rebuild_vectors(&self, ctx: RequestContext, progress: &TaskProgressCounter) -> Result<()>）'
  - 'src/service/dal/memory.rs / message.rs / project.rs / skill.rs / task.rs / tool.rs（各域 rebuild_vectors：分页扫描——按 VECTOR_REBUILD_PAGE_SIZE=200 逐条取出实体 → 调用 try_build_vector_params_for_entity → embed → upsert → progress.advance(n)）'
  - 'src/service/dal/agent/impl.rs（AgentDalImpl 同步升级：rebuild_vectors(ctx, progress) 分页扫描 7 类实体）'
  - 'src/router.rs（system_routes() 新增 POST /vector-rebuild → handlers::system::vector_rebuild::rebuild_vectors_handler；路由层 require_role_middleware(UserRole::Admin)）'
  - 'common/src/api/system.rs（RebuildVectorsRequest 无参 DTO：derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)）'
  - 'frontend/src/api/system.rs（rebuild_vectors() async 前端 API：POST /api/v1/system/vector-rebuild → 返回 TaskIdResponse → 拿 task_id 轮询 GET /api/v1/system/tasks/{task_id}/progress）'
  - 'frontend/src/pages/organization/info.rs（组织设置页 SuperAdmin 触发全量重建入口）'

  - 【平行卡】docs/wiki/knowledge/zh/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵/Embedding Provider 生命周期：ModelProviderStatus Disabled(2) + 创建不阻塞策略 + 重建触发条件矩阵.md（Embedding 业务生命周期与重建触发条件）---

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

(f) **Pre-Filter 谓词下推 + Payload 落库 + 三态索引决策**（2026-09-13 大增量，commit 10e6b6ce..5b601b1f）：这是向量索引架构的核心扩展，解决了「全局 Top-K 被其他 Agent/租户污染」的召回缺陷。三层改造：
   - **模型层新增三底座结构**（`src/models/vector.rs`）：`VectorPayload`（Option 宽结构，字段子集，不用 KV map）+ `VectorFilter`（通用谓词表达式，支持等值/范围/IN/AND/OR）+ `ReindexDecision`（三态 Skip/PayloadOnly/FullReindex）。`Vectorizable` trait 扩展三个方法：`vector_payload()` → 实体自己决定哪些字段入 payload；`vector_id()` → 向量索引 ID（默认 po.id）；`reindex_decision(old)` → 三态决策。payload 变但 text 未变 → `PayloadOnly`（刷新 payload 不重算 embedding）；text 变 → `FullReindex`；双 hash 未变 → `Skip`。防止"过滤字段变了但 vectorize_text 没变 → payload 落后 → pre-filter 漏召回"。
   - **4 后端统一落 payload + search 加 filter + update_payload**：`VectorStore::search(collection, vector, top_k, filter: Option<VectorFilter>)` 签名强制带 filter（不留旧签名）。Lance 用 SQL `only_if` 谓词下推（最强）；Hnsw 用 ANN 近似搜索但 pre-filter 让候选集变小 → **take 放大 4 倍**（固定 `take = top_k * 4`）补偿命中率下降；InMemory/SqliteVss 内存求值。新增 `update_payload(collection, id, payload)` 方法——当 reindex_decision 返回 PayloadOnly 时，只刷新 payload 列而不重算向量。
   - **转译下沉 DAO 层（信息专家原则）**：7 域 Vector DAO 各自接收业务 Query（`MemoryQuery`/`MessageQuery`/`AgentQuery`/`ProjectQuery`/`TaskQuery`/`SkillQuery`/`ToolQuery`），在 DAO 内部转译为 `VectorFilter`。哪些字段能下推只有实体 DAO 自己知道（如 MemoryQuery 有 agent_id/is_published/scope 字段）。**禁止 Domain 层或 Service 层直接构造 VectorFilter**——VectorFilter 只暴露通用谓词表达式。
   - **关键设计决策**：payload 不是全字段 → 召回命中后**必须回业务表**取完整 PO（业务表是 SSOT，payload 是召回优化）。tags 落 payload 但第一版不下推（转译白名单不含 tags，回表兜底）。零兼容包袱：旧向量数据目录整体删除，不含任何 schema 迁移逻辑。
   - **消除的历史缺陷**：旧架构是 post-filter——向量库返回全局 Top-K 后在业务层过滤。多租户场景下，A 用户的 Agent 搜索可能被 B 用户的数据占满 Top-K，过滤后所剩无几。pre-filter 在向量搜索时就只考虑符合条件的行，彻底修复这个召回质量问题。

(g) **全量向量索引重建（SuperAdmin 入口 + DAL 分页 + 后台任务进度）**（2026-09-13 增量，commit 04c4e567，Pre-Filter 改造的配套运营能力）：为 Pre-Filter 改造提供全量重建能力，让旧数据（无 payload 行）一次性补齐。四层组件：
   - **SuperAdmin 专用入口**：`src/handlers/system/vector_rebuild.rs` 新增 handler，路由 `POST /api/v1/system/vector-rebuild`。路由层 `require_role_middleware(UserRole::Admin)` + handler 内部 `check_super_admin(&ctx)?` 两道关——普通 Admin（非 SuperAdmin）也无法触发。返回 `TaskIdResponse { task_id }`，前端拿 task_id 轮询 `GET /api/v1/system/tasks/{task_id}/progress` 展示进度条 + 已处理条数。
   - **DAL 7 域统一 rebuild_vectors 签名（带分页）**：`src/service/dal/mod.rs` 新增 `const VECTOR_REBUILD_PAGE_SIZE: usize = 200`。7 域（memory/message/project/skill/task/tool/agent）`rebuild_vectors` trait 签名统一升级为 `async fn rebuild_vectors(&self, ctx: RequestContext, progress: &TaskProgressCounter) -> Result<()>`。实现逻辑：**分页扫描**（`LIMIT 200 OFFSET page*200`）→ 每批取完后 `progress.advance(batch.len())` 上报条数 → 逐条调用 `try_build_vector_params_for_entity`（embed → upsert）。排序键用 `updated_at DESC, id DESC`——重建期间被新写路径自动 upsert 的实体由写路径兜底，偏移漂移无副作用。
   - **后台任务进度基建 TaskProgressCounter**：`src/pkg/background_task/progress.rs` 新增，定位是**任务对象 ↔ 执行方（DAL）之间的进度通道**。内部 `Arc<AtomicUsize>` + `Relaxed` Ordering（计数仅用于展示，不参与同步决策），可克隆、跨 await 零拷贝。与 BackgroundTask 的步骤级进度（`current_step/total_steps`）**正交**——进度条表达「走到第几步」，计数表达「这一步/整体已处理多少条」。为什么不直接把计数放在 RebuildVectorsTask 上：DAL 在 service 层，RebuildVectorsTask 在 handlers 层，service 不能依赖 handler 类型。句柄放 background_task（基建层），两侧都能引用。
   - **RebuildVectorsTask 后台任务**：已存在的任务类型，本次升级携带 TaskProgressCounter 调用 7 域 DAL。遍历顺序 agent → memory → skill → task → project → message → tool，每步进度文案拼「(i/7) 正在重建 X 向量索引（已处理 N 条）」。**互斥语义**：已有 Running 的 RebuildVectorsTask 时，新任务注册会失败返回 409——避免多个重建同时打 Embedding API 限流。完成后 result JSON 附带 `processed` 字段让前端知道总处理条数。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [pkg/storage/vector.rs](src/pkg/storage/vector.rs) | VectorStore 抽象 + 通用行/参数 | `trait VectorStore`：search 新增 filter 参数 + update_payload；VectorIndexParams；VectorMeta/Bincode Encode Decode |
| [pkg/storage/mem_vector.rs](src/pkg/storage/mem_vector.rs) | InMemoryVectorStore 实现（dev 默认）| HashMap 存 collection；payload + filter 内存求值 + update_payload；Bincode + fs 懒持久化 |
| [pkg/storage/lance.rs](src/pkg/storage/lance.rs) | LanceVectorStore（生产推荐） | payload 平铺列随向量落库；upsert + search/get 读写 payload；SQL `only_if` 谓词下推；update_payload 取回旧行重写 |
| [pkg/storage/hnsw.rs](src/pkg/storage/hnsw.rs) | HnswStore（ANN 高维） | filter take×4 放大补偿过滤命中率下降；update_payload |
| [pkg/storage/sqlite_vss.rs](src/pkg/storage/sqlite_vss.rs) | SqliteVssStore（与主 DB 同文件） | 建表/upsert 落 payload 列；get/search 读侧解析 payload |
| [models/vector.rs](src/models/vector.rs) | Vectorizable trait + 三底座结构 + MatchType + SearchMatchInfo | `trait Vectorizable`（含扩展：vector_payload/vector_id/reindex_decision）；`VectorPayload`/`VectorFilter`/`ReindexDecision`（三态 Skip/PayloadOnly/FullReindex）；MatchType；SearchMatchInfo |
| [pkg/storage/fts5.rs](src/pkg/storage/fts5.rs) | FTS5 关键词转义 | `escape_fts5_keyword(keyword) -> String`（短语匹配；含 6 组单元测试覆盖空串/转义/空格/特殊字符）|
| [dao/cortex/native/mod.rs](src/service/dao/cortex/native/mod.rs) | cortex 向量生成 | `embed(ctx, provider, texts: &[String]) -> Vec<Vec<f32>>` 主入口；`embed_text_for_search(ctx, provider, keyword) -> VectorIndexParams` 搜索场景专用（带 hash + 模型信息）|
| [dao/memory/vector.rs](src/service/dao/memory/vector.rs) | MemoryVectorDao | 接收 MemoryQuery → DAO 内转译 VectorFilter（agent_id/is_published/scope 等下推字段）|
| [dao/message/vector.rs](src/service/dao/message/vector.rs) | MessageVectorDao | 接收 MessageQuery → DAO 内转译 VectorFilter |
| [dao/agent/vector.rs](src/service/dao/agent/vector.rs) | AgentVectorDao | 接收 AgentQuery → DAO 内转译 VectorFilter |
| [dao/project/vector.rs](src/service/dao/project/vector.rs) + [dao/task/vector.rs](src/service/dao/task/vector.rs) | Project/Task VectorDao | 接收业务 Query → DAO 内转译谓词下推 |
| [dao/skill/vector.rs](src/service/dao/skill/vector.rs) + [dao/tool/vector.rs](src/service/dao/tool/vector.rs) | Skill/Tool VectorDao | 接收业务 Query → DAO 内转译谓词下推 |
| [models/agent.rs](src/models/agent.rs) + memory/... 等 | Vectorizable 10 类实现（2026-09-13 补齐） | 所有 10 类 Vectorizable 实体补齐 vector_payload + vector_id + reindex_decision |
| 【① Design 1】vector_search_architecture.md | 为什么选 VectorStore trait + LanceDB 默认后端 + 为什么 collection 按业务分 | docs/archive/design-archive/vector_search_architecture.md |
| 【① Design 2】full_entity_fts5_search_design.md | escape_fts5_keyword 设计动机（FTS5 SQL 注入防护）| docs/archive/design-archive/full_entity_fts5_search_design.md |
| 【② Plan】向量搜索Pre-Filter改造.md | Payload 落库 + 谓词下推 + 三态索引完整实施计划 | docs/plan/向量搜索Pre-Filter改造.md |
| 【③ Wiki 长文 1】存储系统.md §向量存储抽象与多后端 | VectorStore 类图、后端对比表、init_collection 时序 | docs/wiki/zh/content/基础设施/存储系统/存储系统.md |
| 【③ Wiki 长文 2】记忆和向量系统.md §Vectorizable | 10 PO Vectorizable 列表 | docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md |
| 【平行卡】三位一体混合搜索（FTS5 + 向量 + 合并排序） | DAL 层 search() 统一策略 | docs/wiki/knowledge/zh/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）.md |
| [handlers/system/vector_rebuild.rs](src/handlers/system/vector_rebuild.rs) | **新增** SuperAdmin 全量重建 handler | `rebuild_vectors()` → `check_super_admin()` → `registry().register(RebuildVectorsTask)` → 返回 `TaskIdResponse` |
| [handlers/finance/model_provider/rebuild_vectors_task.rs](src/handlers/finance/model_provider/rebuild_vectors_task.rs) | RebuildVectorsTask 后台任务（升级） | `BackgroundTask` impl；新增 `TaskProgressCounter progress` 字段；7 域 DAL `rebuild_vectors(ctx, &progress)` 调用；互斥 409；完成 result 含 `processed` 条数 |
| [pkg/background_task/progress.rs](src/pkg/background_task/progress.rs) | **新增** TaskProgressCounter | `Arc<AtomicUsize>`；`new()` / `processed()` / `advance(count)`；Relaxed Ordering；跨 await 可克隆零拷贝 |
| [service/dal/mod.rs](src/service/dal/mod.rs) | **扩展** VECTOR_REBUILD_PAGE_SIZE | `pub const VECTOR_REBUILD_PAGE_SIZE: usize = 200`；各 DAL rebuild_vectors trait 签名统一 |
| [service/dal/memory.rs](src/service/dal/memory.rs) / [message.rs](src/service/dal/message.rs) / [project.rs](src/service/dal/project.rs) / [skill.rs](src/service/dal/skill.rs) / [task.rs](src/service/dal/task.rs) / [tool.rs](src/service/dal/tool.rs) | **扩展** 各域分页 rebuild_vectors | `rebuild_vectors(ctx, progress)` → 分页扫描（LIMIT 200 OFFSET page*200）→ `progress.advance(batch.len())` → embed + upsert |
| [service/dal/agent/impl.rs](src/service/dal/agent/impl.rs) | **扩展** AgentDalImpl rebuild_vectors | 同步升级：rebuild_vectors(ctx, progress) 分页扫描 |
| [router.rs](src/router.rs) | **扩展** /vector-rebuild 路由 | `system_routes()` 新增 `"/vector-rebuild".post(handlers::system::vector_rebuild::rebuild_vectors_handler)`；路由层 require_role_middleware(UserRole::Admin) |
| [common/src/api/system.rs](common/src/api/system.rs) | **扩展** RebuildVectorsRequest DTO | `#[derive(...)] pub struct RebuildVectorsRequest {}`（无参数） |
| [frontend/src/api/system.rs](frontend/src/api/system.rs) | **扩展** rebuild_vectors 前端 API | `pub async fn rebuild_vectors() -> Result<TaskIdResponse, ApiError>` → POST /api/v1/system/vector-rebuild |
| [frontend/src/pages/organization/info.rs](frontend/src/pages/organization/info.rs) | SuperAdmin 触发入口 | 组织设置页 SuperAdmin 权限按钮 → 调用 rebuild_vectors() → 轮询 progress |

## §3 架构约定

1. **Vectorizable.vectorize_text 必须保持"字段边界可辨"**：字段之间用明确的分隔符（如 `\n## name:\n{name}\n## description:\n{description}`），不能直接字符串拼接 `format!("{name}{description}")`。否则 embedding 模型会把"最后一个词+下一个字段名"混成一个新词，导致边界语义漂移（查询"name: xxx"匹配不到）。
2. **collection 名用冒号表达层级**：记忆系统长期记忆 = `"memory:knowledge_node"`、短期记忆 = `"memory:short_term"`，业务实体直接用复数 `"agents"` / `"skills"`。禁止出现驼峰 `memoryKnowledgeNode`（不直观）。新增实体时在命名评审会上确定 collection 名，**避免未来迁移 collection 名（旧数据 vector 全部作废）**。
3. **向量失败不阻塞主业务流程（降级原则）**：embed_entity 或 VectorStore.upsert 失败时：创建/更新 PO 本身照常成功（业务数据绝不因向量问题丢），只写 log_warn! 告警 + 挂一个后续异步重建向量索引的 AOP 事件兜底。search() 时向量搜索失败（VSS 扩展未安装、Embedding Provider 不可用）→ 降级为纯 FTS5 关键词搜索（保留 keyword_results，vector_scores 空 HashMap，合并照常进行）。
4. **SearchMatchInfo 必须随业务实体一起透传到返回响应**：查询侧每个返回的 Tool/Skill/Agent/Memory 条目均携带 SearchMatchInfo { match_type, vector_distance, keyword_fields, embedding_model, indexed_at, content_hash, fts_rank }。前端可据此做 UI 语义标注（"向量命中 相似度 0.72"或"关键词命中 字段 name"）。match_type 三态 Vector/Keyword/Hybrid 精确标记，方便统计分析"不同关键词下向量 vs FTS5 命中率"。
5. **向量过期 expire_at：仅短期记忆类实体使用**：ShortTermMemoryIndexPo expire_at = created_at + 7d（长期不需要）；其他 PO 默认 None（永不过期，重建索引时通过 needs_reindex() hash 判断，不依赖过期）。过期不是软删除，仅表示"过了这个时间点下次搜索可能不返回"——数据行仍保留直到手动 rebuild。
6. **VectorPayload 字段集与实体可见性规则对齐**：payload 里放哪些字段，由实体的 DAO 自己决定（信息专家原则）。典型下推字段：agent_id / organization_id / project_id / is_published / status / scope 等过滤字段；绝对不放全字段（payload 不是业务表 SSOT）。新增实体实现 vector_payload() 时先列出「哪些字段用于过滤」清单，再编码。
7. **reindex_decision 三态必须在 upsert 前调用**：embed_entity 或等价索引链路中，先算 vector_content_hash + payload_hash → 对比旧 row → reindex_decision(old) → Skip/PayloadOnly/FullReindex。PayloadOnly 调 update_payload（skip embedding），FullReindex 走完整 upsert（重新算 embedding + 覆盖 payload），Skip 直接返回。禁止跳过 reindex_decision 判断直接 upsert（payload 变了但 text 没变 → 白算 embedding + payload 可能被旧值覆盖）。
8. **update_payload 语义：刷新 payload 列，不碰 vector**：PayloadOnly 路径下，update_payload 只更新 payload 列（Lance 是取回旧行重写 payload 字段后写回；InMemory/Hnsw 是 HashMap 更新 payload 字段）。向量值和 meta.content_hash 保持不变。**禁止把 FullReindex 简化为只调 update_payload**（text 变了 → embedding 必须重算）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止在 DAL/DAO 层手写 `format!("{} {}", po.name, po.description)` 当向量文本**（违反 AGENTS §4.8）。所有支持向量索引的业务实体必须在 models 文件中实现 Vectorizable trait；向量文本生成只能走 `po.vectorize_text()` 一处入口。否则新增字段时 6 个 DAL 各改一处 = 5 处漏掉 1 处 = 索引内容漂移 = 搜索命中不到。
2. ❌ **禁止 VectorStore.search 返回 distance > 阈值（默认 0.8）的结果**：search() 内部应先 top-k 然后再 distance 过滤，DAL 层再次双重过滤。超过阈值的向量结果语义意义不大（像乱匹配），会污染用户搜索体验并稀释 FTS5 关键词结果的权重。
3. ❌ **禁止新增 PO 支持向量索引时漏实现 Vectorizable**：若某 PO（如 OrganizationPo、AttachmentPo）被业务方要求搜索，但未实现 Vectorizable → 编译不会报错，但 DAL 调用 embed_entity 会无可用 impl。推荐用 custom test 断言检查「所有标记 #[searchable] 的 PO 均已实现 Vectorizable」（clippy 未来可 lint）。
4. ✅ **escape_fts5_keyword 强约束：所有 FTS5 MATCH 查询必须先过此函数**。禁止 DAO 层直接把用户输入 keyword 拼进 `"SELECT ... WHERE fts MATCH '{keyword}'"` SQL 字符串。未转义 = SQL 注入 + 语法错误 + 关键词含双引号直接报错。
5. ✅ **向量维度严格对齐强约束**：同一 collection 所有向量维度必须与 init_collection 传入的 dimensions 完全一致。embed_entity 内部必须读取 cortex provider 的 model_name（即 embedding model），若模型变动（换 provider 维度变）→ 整个 collection 必须 rebuild（clear_collection + 批量 re-index）。混用不同维度向量进同一 collection = 向量距离计算完全失真 = 搜索结果不可用（隐式 bug 极难查）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 3 篇 wiki 长文（存储系统/基础设施/记忆和向量系统）+ 2 Design + 1 Plan（向量搜索Pre-Filter改造）+ 1 平行卡（混合搜索）；对应 Wiki 长文 cite 段回链本卡 + 2 Design + 1 Plan + 平行卡。
7. ✅ **search 签名强制带 filter 参数**：不留旧签名。`VectorStore::search(collection, vector, top_k, filter: Option<VectorFilter>)`——所有调用方必须显式传 `filter: Some(...)` 或 `filter: None`。过滤是 search 的标准能力，不是可选扩展。
8. ✅ **Payload 不是 SSOT**：payload 只存用于过滤的字段子集（Option 宽结构），不存完整业务字段。召回命中后**必须回业务表**取完整 PO。业务表是 SSOT，payload 是召回优化。
9. ✅ **转译下沉 DAO 层**：VectorFilter 只暴露通用谓词表达式，哪些字段能下推由实体 DAO 自己决定。**禁止 Domain 层或 Service 层直接构造 VectorFilter**——转译必须在各业务 DAO 的 `*VectorDao` 内部完成。
10. ✅ **Hnsw take 放大 4 倍补偿命中率**：Hnsw 是 ANN 近似搜索，pre-filter 让候选集比全局 Top-K 更难命中。固定 `take = top_k * 4` 补偿。测试断言 pre-filter 后 recall@k 不低于 post-filter 基线。
11. ✅ **reindex_decision 三态不可跳过**：必须在 upsert 前调用，返回 Skip/PayloadOnly/FullReindex。PayloadOnly → update_payload（skip embedding）；FullReindex → 完整 upsert；Skip → 直接返回。禁止跳过 reindex_decision 判断直接 upsert。
12. ✅ **全量重建必须走后台任务**：不能同步阻塞 HTTP 连接。RebuildVectorsTask 注册到 BackgroundTask registry，返回 task_id 后由前端轮询 GET /api/v1/system/tasks/{task_id}/progress 获取进度。禁止 handlers 层直接 await DAL rebuild_vectors（同步阻塞 → SuperAdmin 页面卡死、Embedding API 超时连锁放大）。
13. ✅ **SuperAdmin guard 强制**：handler 内部 `check_super_admin()` 二次校验，路由层 system 域 + `require_role_middleware(UserRole::Admin)` 两道关。普通 Admin（如模型提供商管理员）也无法触发。前端组织设置页按钮必须带 SuperAdmin 条件渲染。
14. ✅ **重建任务互斥 + Provider 维度**：已有 Running 的 RebuildVectorsTask 时，新任务注册返回 409。避免多个重建同时打 Embedding API 限流。RebuildVectorsTask 内部遍历 provider → 在该 provider 维度完成所有 7 域重建后再切下一个 provider。
15. ✅ **DAL rebuild_vectors 签名统一携带 progress**：7 域 `async fn rebuild_vectors(&self, ctx: RequestContext, progress: &TaskProgressCounter) -> Result<()>` 签名一致。progress 句柄由 background_task 基建层提供（Arc<AtomicUsize> + Relaxed Ordering），service 层和 handler 层都能引用，不产生跨层类型依赖。禁止在 service 层直接写进度文案（handler 层才有业务语义）。
