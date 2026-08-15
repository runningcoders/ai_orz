> 📦 归档标记（2026-08-15）：被 [记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐](docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md) 取代。保留原因：历史参考，主卡已吸收本卡独有源码锚点与硬约束。生效方案：主卡真实路径作为唯一 RAG 召回目标。
---
kind: wiki_knowledge_card
name: 记忆领域搜索增强：FTS5 tags 语义过滤 + knowledge graph traverse 图谱遍历 + recommend_seed_nodes 种子节点推荐
category: memory DAL + DAO（搜索扩展）
scope:
  - "src/service/dal/memory.rs"
  - "src/service/dao/memory/sqlite.rs"
  - "src/service/dao/memory/vector.rs"
source_files:
  - src/service/dao/memory/sqlite.rs:Ln-Lm（search_knowledge_nodes：FTS5 escape + tags IN 过滤（placeholders 拼接）+ 归属 agent/include_shared 权限过滤 + idx_ltkn_is_published 部分索引加速 + 向量搜索失败降级）
  - src/service/dao/memory/vector.rs:Ln-Lm（search_knowledge_node_vector：知识图谱节点向量语义搜索 LanceDB collection "memory:knowledge_node"）
  - src/service/dal/memory.rs:Ln-Lm（MemoryDal：traverse_knowledge_graph() BFS 遍历图谱（depth max 3；边类型 BELONGS_TO/RELATES_TO/REFERENCES；回边 visited HashSet 防循环）+ recommend_seed_nodes(agent_id, include_shared, top_k) 种子节点推荐（综合 out_degree + 内容长度 + share 加权））
  - common/src/models/graph.rs:Ln-Lm（LongTermKnowledgeGraphEdge / LongTermKnowledgeGraphNode 图谱结构；边类型枚举）
  - src/models/agent.rs:Ln-Lm（LongTermKnowledgeNodePo tags 字段 JSON 数组 + is_published 冗余字段加速索引）
  - migrations/*_memory_fts5.sql:Ln-Lm（知识节点 FTS5 虚拟表 + tags 部分索引）
  - docs/design/memory_search_enhancement_design.md
  - docs/design/vector_search_architecture.md
  - （占位：待 ai-orz-doc-maintainer 落地后回填真实 Plan 路径）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆 (Short-term Memory)/记忆搜索机制.md
  - docs/wiki/zh/content/架构设计/记忆系统架构.md
  - docs/wiki/zh/content/功能模块/AI Agent 管理/记忆系统管理.md
  - docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md
---

# 记忆领域搜索增强（tags 语义过滤 + 图谱遍历 + 种子节点推荐）

## §1 整体方案
长期知识图谱（Long Term Knowledge Graph）是 Agent 的沉淀资产，搜索需要兼顾三种典型使用场景：
- **(A) 用户关键词自由搜索**：输入"用户喜欢红色"→ 既要命中 name/description 含"喜欢红色"的节点（FTS5），也要按 tags=["用户偏好/颜色"] 精准过滤，还要向量语义匹配"用户对颜色的选择"这类近义表述节点。
- **(B) 图谱上下文扩展**：知道一个节点后要扩展关联节点（例如选中「用户A年龄30」节点 → traverse 扩展出「用户A 偏好」「用户A 任务」邻居节点），用于把用户工作记忆从单点扩展到上下文子图。
- **(C) 新 Agent 入职或新会话启动的冷启动**：recommend_seed_nodes 给当前 Agent 推荐"联系最多、内容最丰富、共享度最高"的核心节点，作为 Agent 的初始注意力焦点（避免从零开始瞎搜）。

因此记忆领域搜索在"基础混合搜索"之上增加了 3 个特定能力（对应上述 3 场景）：**tags 语义过滤**（场景 A）、**traverse_knowledge_graph 图谱遍历**（场景 B）、**recommend_seed_nodes 种子节点推荐**（场景 C）。底层仍复用 VectorStore/FTS5 通用基础设施，上层封装记忆专属 DAL 方法。

(a) **FTS5 tags 语义过滤（SQL 级 + 部分索引加速）**：
   - **空关键词直接返回空**（FTS5 MATCH 空串 SQL 报错，DAO 层前置短路）→ escape_fts5_keyword(&keyword) 转义成短语匹配。
   - **tags IN 过滤（占位符拼接，防 SQL 注入）**：如果 MemorySearch.filters.tags = ["用户偏好", "红色"] → 构造 `AND EXISTS (SELECT 1 FROM json_each(m.tags) WHERE json_each.value IN (?, ?))` 子句 + placeholders 数量 = tags 长度，依次 bind 参数。**禁止 format! 把 tags 值直接拼进 SQL**（注入防护）。
   - **归属 + share 权限过滤**：只有 (节点所属 agent_id == 当前 agent) OR (include_shared=true AND 节点 is_published=1) 才能看到。**冗余字段 is_published** 替代了 json_each(tags) 里查询 "public/published" tag 的复杂子查询，配合 DDL 中 `idx_ltkn_is_published` 部分索引（WHERE is_published=1）直接走索引，查询速度提升 10~100 倍。
   - 向量搜索失败（向量后端异常）→ log_warn 降级纯 FTS5。返回 Vec<(Po, Option<f32>)>（第二个元素是向量距离，仅向量命中时 Some），方便上层后续加权融合。

(b) **traverse_knowledge_graph 图谱 BFS 遍历（DAL 层业务方法）**：
   - **输入参数**：start_node_id、max_depth（默认 3，避免全图遍历爆炸）、edge_type_filter（可选，边类型白名单空=全部允许；常见边类型：BELONGS_TO「节点属于 Agent」/RELATES_TO「节点互相关联」/REFERENCES「任务引用知识」/DERIVED_FROM「摘要节点派生自原始记忆」）。
   - **BFS 算法**：VecDeque<(node_id, depth)> 做队列；visited HashSet 去重防止回边（A→B→A 死循环）；每出队一个节点 → 查 graph_edge 表找"起点=当前节点"的所有边 → 满足 edge_type_filter 且 depth<max_depth → 未 visited → 入队、visited 标记、收集结果。
   - **输出结构**：GraphTraverseResult { nodes: Vec<KnowledgeNode>, edges: Vec<KnowledgeGraphEdge>, levels: Vec<Vec<String>>（每层 node_id 列表，供前端按层可视化图谱） }。输出包含边，前端可渲染连接关系。
   - **边界**：max_depth=0 → 空；max_depth=1 → 只有直接邻居；硬上限 max_depth ≤ 6（安全网，防止调用方传 999 打爆 DB）。

(c) **recommend_seed_nodes 冷启动种子节点推荐（综合评分）**：
   - 针对典型"Agent 刚入职想快速了解组织知识"或"新会话开始 Agent 想从重要节点起步"场景。输入 agent_id / include_shared / top_k（默认 10）。
   - **综合评分公式（三要素加权）**：score = 0.45 × connectivity_score + 0.35 × content_richness_score + 0.2 × share_weight。
     - **connectivity_score（联系广度）**：该节点的 out_degree（出边数）在本 Agent 可看节点中的分位数归一化。出边越多 = 越"枢纽节点"（像"项目A总览"节点挂了大量任务/文档引用）。
     - **content_richness_score（内容信息量）**：node.description.len()（文本长度）+ 附件数量 加权后相对归一化。内容越丰富 = 节点沉淀价值越高（空壳节点分低）。
     - **share_weight（共享价值）**：is_published=1 → ×1.5 加成；仅自己可见 → ×1.0。公开节点更可能是通用知识，推荐给新 Agent 更合理。
   - **输出结构**：Vec<SeedNodeRecommendation { node_id, node_name, score, reasons: Vec<String> }>，每条推荐给出 reasons 文字化解释（例如「高连接度：关联 18 个节点」「内容丰富：500+ 字详细说明」「共享节点：组织通用知识」），前端可显示"为什么推荐这个节点"（而不是冰冷的排序列表）。
   - **缓存**：同一 agent_id + include_short 1h 内可做内存级缓存（种子节点变化慢，无需每次扫全图）；缓存失效由知识写入事件触发失效。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [dao/memory/sqlite.rs](src/service/dao/memory/sqlite.rs) | 记忆 FTS5 搜索 + tags 过滤 | search_knowledge_nodes：escape_fts5_keyword + has_tags placeholders 拼接 + agent_id/include_shared 权限过滤 + idx_ltkn_is_published；空关键词短路 |
| [dao/memory/vector.rs](src/service/dao/memory/vector.rs) | 记忆向量语义搜索 | search_knowledge_node_vector：`"memory:knowledge_node"` collection 通过 ctx.vector_store().search() |
| [dal/memory.rs](src/service/dal/memory.rs) | traverse_knowledge_graph() BFS + recommend_seed_nodes() 推荐 | BFS：VecDeque + visited + max_depth 上限；推荐：connectivity/content/share 三要素评分 0.45/0.35/0.2 |
| [common/src/models/graph.rs](common/src/models/graph.rs)（或 models/memory/*.rs）| 图谱结构定义 | LongTermKnowledgeNodePo（tags 数组 + is_published）、GraphEdge { from, to, edge_type, created_at }、GraphTraverseResult（nodes+edges+levels）|
| [migrations/*memory_fts5.sql](migrations)（对应编号文件）| 记忆 FTS5 + 部分索引 DDL | FTS5 虚拟表 MATCH；idx_ltkn_is_published WHERE is_published = 1 部分索引 |
| 【① Design 1】memory_search_enhancement_design.md | tags 部分索引动机 + 推荐评分 3 因子权重 | docs/design/memory_search_enhancement_design.md |
| 【① Design 2】vector_search_architecture.md §LongTermKnowledge collection | 为什么图谱节点 collection 叫 "memory:knowledge_node" | docs/design/vector_search_architecture.md |
| 【③ Wiki 长文 1】记忆搜索机制.md §FTS5 tags 过滤 + §BFS 图谱扩展 | 搜索用户视角说明 | docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/短期记忆%20(Short-term%20Memory)/记忆搜索机制.md |
| 【③ Wiki 长文 2】记忆系统架构.md §长期知识图谱 §搜索扩展层 | 架构层级分层图 | docs/wiki/zh/content/架构设计/记忆系统架构.md |
| 【③ Wiki 长文 3】记忆系统管理.md §种子节点推荐面板 | 前端 UI 推荐 reasons 展示 | docs/wiki/zh/content/功能模块/AI%20Agent%20管理/记忆系统管理.md |
| 【③ Wiki 长文 4】记忆和向量系统.md §Vectorizable 实现 | LongTermKnowledgeNodePo::vectorize_text 说明 | docs/wiki/zh/content/数据模型/消息和记忆模型/记忆和向量系统.md |
| 【平行卡】向量存储 + Vectorizable | 底层向量基础设施 | docs/wiki/knowledge/zh/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity/向量存储抽象%20VectorStore%20+%20多后端%20+%20Vectorizable%20trait%20统一索引入口%20+%20embed_entity.md |
| 【平行卡】三位一体混合搜索 | 6 实体通用混合搜索样板（记忆额外扩展）| docs/wiki/knowledge/zh/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）/三位一体混合搜索：FTS5%20关键词%20+%20向量语义%20+%20合并排序%20（6%20DAO%20统一%20search%20模式%20+%20向量失败降级）.md |

## §3 架构约定

1. **tags 过滤：永远是 AND（所有 tags 都要有）不是 OR**：tags = ["红色", "用户偏好"] → 要求节点的 tags JSON 数组同时包含这两个值（AND 语义）。不支持 OR 是有意为之（OR 语义用关键词/向量搜索已经覆盖，避免能力重叠复杂化）。前端 UI tags 多选组件旁边注明"多标签同时满足"。
2. **is_published 冗余字段只在节点创建/更新时同步，绝不从 tags 动态查询**：创建节点时如果 tags 包含 "published" / "public" → 自动置 is_published = 1；更新 tags 时同步刷新。读查询永远查 is_published 冗余字段（部分索引），不要 `WHERE EXISTS (SELECT 1 FROM json_each(tags) ... = 'public')`（性能差 10-100x，复杂查询扫全表）。
3. **BFS 遍历禁止深度过大硬上限**：DAL 层入口对 max_depth 做 clamp(1, 6)。即使调用方传 max_depth=1000 也会被强制截断到 6，避免 DBA 投诉"全图 10w 节点扫爆 IO"。
4. **recommend_seed_nodes 评分权重可调但必须三要素**：connectivity/content/share 三者都要有（不允许只按一个指标推荐=偏科严重），具体权重 0.45/0.35/0.2 是实测推荐效果折中。调参后务必跑回归：老 Agent（图谱边丰富）种子 top10 应是 8~9 条"枢纽型 + 内容丰富"节点，不能全是短节点。
5. **推荐 reasons 文案必须自解释、不能暴露内部量纲原始值**：只输出"高连接度：关联 18 个节点"或"共享节点：组织通用知识"；不要把 connectivity_score=0.9213 这种内部归一化值直接写出去（用户看不懂）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 tags 过滤 SQL 中直接 format! 把 tags 值拼进 SQL**。必须用占位符 `?` + bind 参数；placeholders 数量与 tags 长度动态生成。反例：`format!("AND json_each.value IN ({})", tags.join(","))` = SQL 注入（tags = "'); DROP TABLE knowledge_nodes; --" 直接删库）。
2. ❌ **禁止 traverse_knowledge_graph 输出 levels 顺序不一致**：BFS 结果 levels[0] = [start_node_id]、levels[1]=直接邻居、levels[2]=邻居的邻居… 严格按层级对应；前端图谱可视化依赖此顺序，错乱会导致边连不到正确层级节点。
3. ❌ **禁止 recommend_seed_nodes 返回重复节点或 include_shared=false 时出现外部节点**：seed_id 列表做 dedup 再排序；权限过滤和 search_knowledge_nodes 走同一套（agent_id + is_published AND include_shared），不要复制粘贴另一套（避免一套改了另一套没改 = 越权看到不该看的节点）。
4. ✅ **搜索记忆节点的"空关键词短路"强约束**：keyword.trim().is_empty() 且 query_vector None 时，返回空 Vec（不要返回全表数据，否则前端点"搜索"按钮啥都不填就全量节点，10k+ 条前端卡顿+DB 扫表）。
5. ✅ **图谱节点写入后必须同步 tags 变更触发 is_published 刷新 + 推荐缓存失效**：创建/更新节点 tags 时 → DAO 同步 is_published；任何节点写入/删除/边变更 AOP 事件 → recommend_seed_nodes 缓存（若未来接入）立即失效（旧缓存会导致新节点推荐不到）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 4 篇 wiki 长文（记忆搜索机制/记忆系统架构/记忆管理/记忆向量系统）+ 2 Design + Plan 占位 + 2 平行卡（向量基础设施/混合搜索样板）；对应 Wiki 长文 cite 段回链本卡 + Design + 平行卡。
