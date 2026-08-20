---
kind: wiki_knowledge_card
name: 三位一体混合搜索：FTS5 关键词 + 向量语义 + 合并排序（6 DAO 统一 search 模式 + 向量失败降级）
category: DAL 层 6 业务统一混合搜索
scope:
  - "src/service/dal/agent.rs"
  - "src/service/dal/tool.rs"
  - "src/service/dal/skill.rs"
  - "src/service/dal/task.rs"
  - "src/service/dal/project.rs"
  - "src/service/dao/*/mod.rs"（6 DAO Search 入参结构体：AgentSearch/ToolSearch 等）
source_files:

  - src/service/dal/task.rs:Ln-Lm（TaskDal.search：向量搜索→FTS5→合并/归一化/加权/分页；向量失败降级；并发并行策略）
  - 'src/service/dal/project.rs:Ln-Lm（ProjectDal.search：3 步同模式；try_build_vector_params_for_search 公共辅助）'
  - 'src/service/dal/tool.rs:Ln-Lm（ToolDal.search：向量距离阈值默认 0.8；FTS5 关键词 rank 归一化）'
  - 'src/service/dal/skill.rs:Ln-Lm（SkillDal.search：同 3 步模式；vector_scores/vector_ids HashMap/HashSet 容器）'
  - 'src/service/dao/task/mod.rs:Ln-Lm（TaskSearch 统一入参：keyword + query_vector Option + top_k + 业务 filters TaskQuery 复用）'
  - 'src/service/dao/agent/mod.rs:Ln-Lm（AgentSearch：同模式；vector_distance_threshold 可选覆盖）'
  - 'src/service/dao/tool/mod.rs:Ln-Lm（ToolSearch）、src/service/dao/skill/mod.rs:Ln-Lm（SkillSearch）、src/service/dao/project/mod.rs:Ln-Lm（ProjectSearch）、src/service/dao/message/mod.rs:Ln-Lm（MessageSearch）'

  - docs/archive/design-archive/vector_search_architecture.md

  - docs/archive/design-archive/entity_list_query_search_design.md

  - docs/archive/design-archive/full_entity_fts5_search_design.md
  - '（占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径）'

  - docs/wiki/zh/content/基础设施/存储系统/存储系统.md

  - docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 搜索与查询.md

  - docs/wiki/zh/content/功能模块/AI Agent 管理/Agent 搜索与推荐.md

  - docs/wiki/zh/content/架构设计/记忆系统架构.md---

# 三位一体混合搜索（FTS5 + 向量 + 合并排序）

## §1 整体方案
所有业务实体（Agent/Tool/Skill/Task/Project/Message 共 6 类）提供统一的混合搜索接口（DAL 层 `.search()`），内部统一 5 步：①参数判断选择搜索模式 → ②可选生成查询向量 + 向量搜索 → ③FTS5 关键词全文搜索 → ④归一化加权融合排序 → ⑤分页截取。**6 业务 DAO 统一设计（Search 结构体同构），6 业务 DAL 同模式（5 步骨架一致，细节各调参数），实现"搜索体验一致 + 代码复用高 + 新增搜索实体零发散"。**

(a) **统一搜索入参（6 DAO Search 结构体同构设计）**：每个 DAO（Agent/Tool/Skill/Task/Project/Message）各自定义 `XxxSearch` 结构体，字段完全同构（禁止某业务独有字段导致搜索模式漂移）：
   - `keyword: Option<String>`：用户输入的原始关键词（FTS5 用）
   - `query_vector: Option<Vec<f32>>`：调用方预生成的查询向量（有则跳过关键词→向量转换，直接用）
   - `top_k: Option<i32>`（或 usize）：向量搜索 Top K，默认 20（与 FTS5 默认 LIMIT 对齐，避免两边差数量级）
   - `vector_distance_threshold: Option<f32>`：余弦距离阈值（0~2，越小越相似，默认 0.8，调用方可覆盖收紧）
   - **`filters: XxxQuery`（核心设计！）**：业务过滤条件（组织、用户、状态、时间范围、分页等）直接复用 DAO 已有的 XxxQuery 结构体，**不再复制一份过滤字段到 Search**。复用的好处：Query 新增过滤字段（例如按创建时间 range）时 Search 零改动自动继承；分页参数 pagination 也挂在 filters.pagination 里统一透传（和 query/list 接口完全一致的分页语义）。

(b) **搜索模式自动判定（3 种）**：根据入参自动选择：
   - **纯关键词模式**（keyword Some 且 query_vector None 或 Embedding Provider 不可用）→ 只跑 FTS5；
   - **纯向量模式**（query_vector Some 且 keyword None）→ 只跑向量搜索；
   - **混合模式**（两者都有 → 两条并行跑，最终合并融合）。并发实现：FTS5 与向量搜索用 tokio::join! 并行（IO 密集，并行节省一半延迟），失败则降级（向量搜索失败 → 纯 FTS5）。

(c) **Step 1~3 具体执行（TaskDal 为例，其他同构）**：
   - **(Step 0 准备容器)** HashMap<String, f32> vector_scores 存向量 id→distance；HashSet<String> vector_ids 存向量命中 id 全集；提前取 filters.pagination（后续 params 会被 move）。
   - **(Step 2 向量搜索)**：keyword 存在 → try_build_vector_params_for_search(ctx, cortex_dao, model_provider_dao, keyword)：此函数内部 ① get_default_embedding_provider（无可用 provider → Ok(None) = 跳过向量搜索，不报错）→ ② cortex.embed_text_for_search（关键词 → query_vector）→ ③ project_vector_dao.search_vector(ctx, &vector, top_k) → ④ 过滤 distance < 阈值，灌入 vector_scores / vector_ids。向量搜索内部失败（VSS 未装、LanceDB 启动失败、embed 超时任意原因）→ log_warn! + 降级 = vector_scores 空，不影响后续。
   - **(Step 3 FTS5 关键词搜索)**：直接调 `task_dao.search_tasks(ctx, search.clone())` → 返回 `Vec<(TaskPo, fts_rank: f32)>`。fts_rank 是 SQLite FTS5 BM25 评分（越小越相关，0~∞，越小越相关，需在下一步归一化翻转方向）。

(d) **Step 4 合并融合排序（核心）**：这一步把"向量 distance（越小越好 0~2）"和"FTS5 rank（越小越好，量纲不定）"统一成"综合分（越大越好 0~1）"。统一归一化加权公式：
   - **向量分**：`vector_score(po_id) = 1.0 - min(distance / 2.0, 1.0)`（距离 0 → 分 1.0 满分；距离 ≥ 2 → 分 0.0）。仅命中向量的 id 有值，否则 None。
   - **FTS 分**：先取本次搜索的 FTS5 rank_max = max(fts_ranks.iter())，如果 rank_max == 0 则全部 0；否则 `fts_score(po_id) = 1.0 - min(fts_rank / rank_max, 1.0)`（rank 最小最相关 → 分最大=1.0，相对归一化到本批次结果内）。
   - **综合分**：`hybrid_score(po) = α * vector_score.unwrap_or(0.0) + β * fts_score.unwrap_or(0.0)`（权重 α=0.55，β=0.45：向量语义略占优，解决"同义词/相关词 FTS5 永远搜不到"的问题；同时关键词权重不太低，精确匹配用户输入字段时排名更靠前）。匹配类型标记：两种都命中 → Hybrid，仅向量 → Vector，仅 FTS → Keyword，写入 SearchMatchInfo.match_type。
   - **去重 + 排序**：vector_ids ∪ fts_ids（所有命中 id），按 hybrid_score DESC 排序 → 截取 filters.pagination 对应偏移范围（offset..offset+limit）→ DAL 再批量 get PO（保证返回是完整实体，不含中间引用）→ 组装 PagedResult { items, total }（total = 去重后 id 总数，分页友好）。

(e) **失败降级原则（高可用）**：
   - Embedding Provider 挂了 → log_debug + 跳过向量，纯 FTS5；
   - VectorStore 后端异常（如 LanceDB 打不开文件）→ log_warn + 跳过向量；
   - FTS5 虚拟表损坏（极少见）→ 不降级，直接返回 Err（核心搜索能力丢失必须让上层感知）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [dal/task.rs](src/service/dal/task.rs) | 任务混合搜索实现（完整 5 步样板）| search()：容器准备→向量→FTS5→合并/加权/翻转→分页；向量失败降级 log_warn + 注释标记 |
| [dal/project.rs](src/service/dal/project.rs) | 项目混合搜索（try_build_vector_params_for_search 公共辅助）| 同 5 步；提取公共辅助 `try_build_vector_params_for_search(ctx, cortex, mp_dao, keyword) -> Option<VectorIndexParams>` |
| [dal/tool.rs](src/service/dal/tool.rs) | 工具混合搜索（top_k/阈值逻辑）| DEFAULT_VECTOR_DISTANCE_THRESHOLD = 0.8；top_k 默认 20 |
| [dal/skill.rs](src/service/dal/skill.rs) | 技能混合搜索（容器声明）| `vector_scores: HashMap` / `vector_ids: HashSet` 模式 |
| [dao/task/mod.rs](src/service/dao/task/mod.rs) | TaskSearch 入参 | `keyword/query_vector/top_k/filters: TaskQuery` 四字段同构 |
| [dao/agent/mod.rs](src/service/dao/agent/mod.rs) | AgentSearch | 同模式 + vector_distance_threshold 可选覆盖 |
| [dao/tool/mod.rs](src/service/dao/tool/mod.rs) | ToolSearch；[dao/skill/mod.rs](src/service/dao/skill/mod.rs) SkillSearch；[dao/project/mod.rs](src/service/dao/project/mod.rs) ProjectSearch；[dao/message/mod.rs](src/service/dao/message/mod.rs) MessageSearch | 6 类 Search 结构体统一字段 |
| 【① Design 1】vector_search_architecture.md §混合搜索策略 | 为什么 α=0.55/β=0.45 权重、为什么合并分要越大越好 | docs/archive/design-archive/vector_search_architecture.md |
| 【① Design 2】entity_list_query_search_design.md §search 模式统一 | 为什么 search 接口复用 filters XxxQuery（不复制一份字段）| docs/archive/design-archive/entity_list_query_search_design.md |
| 【① Design 3】full_entity_fts5_search_design.md §FTS5 rank 归一化 | BM25 评分跨实体量纲不一为什么要批次内相对归一化 | docs/archive/design-archive/full_entity_fts5_search_design.md |
| 【③ Wiki 长文 1】存储系统.md §混合检索 | 合并排序归一化公式说明 | docs/wiki/zh/content/基础设施/存储系统/存储系统.md |
| 【③ Wiki 长文 2】Agent 搜索与查询.md §AgentSearch 三场景 | list(无)/query(过滤)/search(关键词+向量) | docs/wiki/zh/content/项目概述/核心功能特性/Agent 全生命周期管理/Agent 搜索与查询.md |
| 【③ Wiki 长文 3】Agent 搜索与推荐.md §混合搜索 UI | 前端展示 match_type 徽标 | docs/wiki/zh/content/功能模块/AI Agent 管理/Agent 搜索与推荐.md |
| 【平行卡】VectorStore + Vectorizable 统一索引 | 向量/fts5 底层基础设施 | docs/wiki/knowledge/zh/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity.md |
| 【平行卡】记忆知识图谱搜索增强（tags 过滤 + 图谱遍历） | 记忆领域搜索扩展（tags 语义 + 图谱路径）| docs/wiki/knowledge/zh/记忆领域搜索增强：FTS5%20tags%20语义过滤%20+%20knowledge%20graph%20traverse%20图谱遍历%20+%20recommend_seed_nodes%20种子节点推荐/记忆领域搜索增强：FTS5%20tags%20语义过滤%20+%20knowledge%20graph%20traverse%20图谱遍历%20+%20recommend_seed_nodes%20种子节点推荐.md |

## §3 架构约定

1. **6 类业务 DAL 搜索模式保持同构，不引入"某业务独有特殊流程"**：新增第 7 类搜索对象（例如 Organization）时，必须复用本节 5 步骨架（容器准备→向量→FTS5→合并→分页），**权重 α=0.55 β=0.45 默认值相同**，阈值 0.8 相同。允许微调但必须在代码注释中写明原因（例如某领域 FTS5 精度更高，β=0.58 实测更好）。
2. **filters 永远复用 XxxQuery（禁止 Search 中复制一份字段）**：所有过滤条件（organization_id、status、时间范围、tags）均在 filters: XxxQuery 字段里定义，Search 结构体不得再加「专属过滤字段」。否则会出现"Search 能按 created_at range 查、Query 不能" 这种漂移。新增过滤只改 XxxQuery，Search 自动继承。
3. **归一化必须"本批次结果内相对"**：FTS5 rank / 向量 distance 在不同查询、不同实体间量纲不同，**永远不能用全局固定分母归一化**。必须取本次搜索返回结果集合内的 max/min 做相对翻转归一化到 0~1；避免"某实体 FTS5 rank 全局普遍大 → 该实体所有结果分低，排名永远靠后"。
4. **total 统计要取并集，不要取交集**：分页总数 total = len(vector_ids ∪ fts_ids) = 去重后 id 总个数。不要取 len(vector_ids) + len(fts_ids)（交集会被重复计数，前端翻页永远显示多一倍）。
5. **SearchMatchInfo 必须随 Item 精确赋值**：排序后每个实体必须精准标记 match_type（Hybrid 当且仅当两边都命中，否则 Vector / Keyword）、vector_distance 从 vector_scores[id] 取（仅向量命中时 Some）、fts_rank 从 FTS5 结果元组取（仅关键词命中时 Some）、keyword_fields 从 FTS5 命中字段列表取。前端 UI 依此渲染"语义命中 0.72"徽标。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 search() 返回 Vec 而非 PagedResult<T>**（违反 AGENTS §4.9 分页规范）。搜索接口必须与 query/list 统一返回 `PagedResult { items, total }`，Handler 层用 `.items` 拿结果，`.total` 用于分页 UI 总条数。
2. ❌ **禁止 count 独立拼 WHERE 不复用 filters**：search 结果中的 total 必须基于实际命中（交集并集），禁止另写一个 SQL 独立 count（过滤条件漂移 → total 和实际查到的数量不一致 = 分页 UI 永远跳页错误）。
3. ❌ **禁止 Handler 层把 PagedResult 当 Vec 用（直接 for x in result）**：必须使用 `.items` 遍历。直接迭代 PagedResult 会导致编译错误或逻辑混乱（如果 PagedResult 实现了 IntoIterator 扩展）。
4. ✅ **向量搜索与 FTS5 必须 tokio::join! 并行（IO 密集型）**：串行化会让两次 IO 时间叠加，混合搜索 p95 延迟直接翻倍；join! 失败分支独立 match 各自降级（向量失败向量空，不影响 FTS5 已成功结果；FTS5 失败 FTS5 空，但向量结果有效保留）。
5. ✅ **新增 Search 实体 3 处强绑定同步**：(1) dao/Xxx/mod.rs 加 XxxSearch 结构体（keyword/query_vector/top_k/vector_distance_threshold?/filters XxxQuery）→ (2) dal/Xxx.rs 加 `async fn search(ctx, search: XxxSearch) -> Result<PagedResult<Xxx>>` 实现（复制粘贴 5 步样板 + 改类型名）→ (3) Domain 层 `XxxSearch` trait 中加 `async fn search(...)` 暴露给 Handler。**3 处不同步 = Domain 层 search 调用编译失败（最简单的防护）**。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 4 篇 wiki 长文（存储系统/Agent 搜索与查询/Agent 搜索与推荐/记忆系统架构）+ 3 Design + Plan 占位 + 2 平行卡（向量基础设施/记忆增强）；对应 Wiki 长文 cite 段回链本卡 + 3 Design + 平行卡。
