---
kind: RAG 原子知识卡
name: 知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头
category: 知识图谱与记忆沉淀 / 全局图谱可视化
scope:
  - "src/handlers/hr/agent/get_knowledge_graph.rs"
  - "src/service/dal/memory.rs"
  - "src/service/domain/runtime/**"
  - "src/models/memory.rs"
  - "src/router.rs"
  - "common/src/api/neural_tools.rs"
  - "common/src/ontology.rs"
  - "migrations/20260930000001_add_direction_to_ontology_relation_types.sql"
  - "frontend/src/pages/hr/knowledge_graph.rs"
  - "frontend/src/api/hr.rs"
  - "frontend/src/components/graph.rs"
  - "frontend/src/components/graph_canvas.rs"
  - "frontend/src/components/canvas_scene.rs"
source_files:
  - 'src/handlers/hr/agent/get_knowledge_graph.rs#L27-L64 (get_knowledge_graph handler：#[register_handler_tool(id="get_knowledge_graph", ..., neural, tags="memory")] + #[generate_http_handler]；调 runtime_domain().memory().get_knowledge_graph(ctx, params.agent_id) → 装配度数 + 逐边取方向 → GetKnowledgeGraphResponse)'
  - 'src/handlers/hr/agent/get_knowledge_graph.rs#L71-L104 (to_node_api / to_edge_api：全局视图字段裁剪（节点不含 node_description/summary/updated_at）+ 边 direction 由 edge_directions 映射兜底 "undirected")'
  - common/src/api/neural_tools.rs#L259-L318 (DTO：GetKnowledgeGraphParams{agent_id:Option<String>} / GetKnowledgeGraphResponse{nodes,edges,generated_at} / GraphNode{id,node_name,node_type,tags,agent_id,is_published,degree,incoming_count,outgoing_count,created_at} / GraphEdge{id,source,target,relation_type,direction,weight})
  - 'src/service/dal/memory.rs#L514-L638 (DAL 实现：R1 全量活跃知识节点 MemoryQuery{KnowledgeNode,Active,exclude Forgotten,limit/offset=None 无 LIMIT 子句} → R2 严格双端活跃 active_ids 过滤悬挂边 → R4 degrees:HashMap<String,(in,out)> 只按生效边统计 → try_join! 并行拉 classes/relation_types/synonyms 装配 OntologyLexicon（含 relation_directions）→ 逐边 common::ontology::resolve 得 edge_directions)'
  - src/models/memory.rs#L619-L638 (领域结构 KnowledgeGraphData{nodes,edges,degrees,edge_directions}；degrees 仅计生效边；edge_directions key=边 ID value="directed"/"undirected")
  - src/service/domain/runtime/mod.rs#L138-L143 (RuntimeMemory trait 新增 get_knowledge_graph)
  - src/service/domain/runtime/memory.rs#L103-L110 (MemoryDomain 一行委托 dal().get_knowledge_graph)
  - src/router.rs#L763-L764 (路由 POST /agents/get_knowledge_graph → get_knowledge_graph_handler)
  - common/src/ontology.rs#L142-L186 (Direction 二值枚举 Directed/Undirected；serde lowercase 序列化 "directed"/"undirected"；FromStr + parse_or_default 兜底无向)
  - common/src/ontology.rs#L71-L95 (ResolvedTerm::{Canonical,ViaSynonym,Drift} 各带 direction 字段；Drift 恒 Undirected)
  - common/src/ontology.rs#L101-L118 (OntologyLexicon.relation_directions: HashMap<String, Direction> 平行结构，未登记 key 解析兜底无向)
  - common/src/ontology.rs#L375-L384 (PresetRelationType.direction: String 带 #[serde(default="default_preset_direction")] 兜底旧 seed 快照为 "undirected")
  - migrations/20260930000001_add_direction_to_ontology_relation_types.sql#L1-L25 (ALTER ontology_relation_types 增 direction TEXT NOT NULL DEFAULT 'undirected' CHECK(direction IN ('directed','undirected')) + multiplicity 可空占位 + 三段幂等回填)
  - frontend/src/pages/hr/knowledge_graph.rs#L45-L53 (ViewMode 两态 Global/Local，默认 Global)
  - frontend/src/pages/hr/knowledge_graph.rs#L279-L305 (load_global_graph：use_effect 默认装载 + agent_id 变化重拉；node_id→degree 映射进 global_degrees)
  - frontend/src/api/hr.rs#L394-L400 (API 客户端 get_knowledge_graph，POST /api/v1/hr/agents/get_knowledge_graph)
  - frontend/src/components/graph.rs#L208-L230 (global_node_radius 度数→半径 SSOT 7~27.9px + global_node_label 半径限绘 3~12 字符，SVG/Canvas 同源)
  - frontend/src/components/graph.rs#L636-L668 (SVG marker defs + edge_marker 按 edge.direction=="directed" 条件挂载 marker_end，undirected 纯线)
  - frontend/src/components/graph.rs#L723-L790 (SVG 全局态圆点分支：global_node_radius 半径 + 选中虚线扫描环 + global_node_label 免绘名/截断守卫，对齐 Canvas)
  - frontend/src/components/canvas_scene.rs#L73-L90 (CanvasEdge.directional: bool，Default=false)
  - frontend/src/components/canvas_scene.rs#L749-L771 (draw_edges 箭头原语：directed 边 target 端小三角、沿 source→target、尺寸随线宽派生、填充复用边色)
  - frontend/src/components/graph_canvas.rs#L79-L90 (Canvas 适配层 global_mode 时 radius=global_node_radius(degree))
  - frontend/src/components/graph_canvas.rs#L140 (GraphEdge→CanvasEdge 映射透传 directional = e.direction=="directed")
  - docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/知识图谱可视化.md
  - 【总卡/兄弟卡】docs/wiki/knowledge/zh/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕/Canvas HUD 可视化：GraphCanvas 知识图谱 + 图表场景LineDonut + 仪表盘Gauge双版 + HudPalette橙光光晕.md
  - 【兄弟卡】docs/wiki/knowledge/zh/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账/本体词表漂移治理与 Prompt 注入：OntologyDal load_lexicon_summary + common 纯函数解析 + 写后认证 + DuckDB 漂移记账.md
  - 【平行卡】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md
---

# 知识图谱全局点线视图：get_knowledge_graph 全量聚合端点 + R1R2R4 口径 + 边方向服务端 resolve + 双渲染器箭头

**关联声明（Level 5 新卡）**：本卡是 Canvas 渲染总卡 [Canvas HUD 可视化…](docs/wiki/knowledge/zh/Canvas%20HUD%20%E5%8F%AF%E8%A7%86%E5%8C%96%EF%BC%9AGraphCanvas%20%E7%9F%A5%E8%AF%86%E5%9B%BE%E8%B0%B1%20+%20%E5%9B%BE%E8%A1%A8%E5%9C%BA%E6%99%AFLineDonut%20+%20%E4%BB%AA%E8%A1%A8%E7%9B%98Gauge%E5%8F%8C%E7%89%88%20+%20HudPalette%E6%A9%99%E5%85%89%E5%85%89%E6%99%95/Canvas%20HUD%20%E5%8F%AF%E8%A7%86%E5%8C%96%EF%BC%9AGraphCanvas%20%E7%9F%A5%E8%AF%86%E5%9B%BE%E8%B0%B1%20+%20%E5%9B%BE%E8%A1%A8%E5%9C%BA%E6%99%AFLineDonut%20+%20%E4%BB%AA%E8%A1%A8%E7%9B%98Gauge%E5%8F%8C%E7%89%88%20+%20HudPalette%E6%A9%99%E5%85%89%E5%85%89%E6%99%95.md) 的细卡/兄弟卡（总卡管渲染基础设施与力导向/图表，本卡管「全量聚合端点 + 三口径 + 边方向契约 + 双渲染器箭头一致性」）；本体侧方向登记与漂移治理见 [本体词表漂移治理与 Prompt 注入…](docs/wiki/knowledge/zh/%E6%9C%AC%E4%BD%93%E8%AF%8D%E8%A1%A8%E6%BC%82%E7%A7%BB%E6%B2%BB%E7%90%86%E4%B8%8E%20Prompt%20%E6%B3%A8%E5%85%A5%EF%BC%9AOntologyDal%20load_lexicon_summary%20+%20common%20%E7%BA%AF%E5%87%BD%E6%95%B0%E8%A7%A3%E6%9E%90%20+%20%E5%86%99%E5%90%8E%E8%AE%A4%E8%AF%81%20+%20DuckDB%20%E6%BC%82%E7%A7%BB%E8%AE%B0%E8%B4%A6/%E6%9C%AC%E4%BD%93%E8%AF%8D%E8%A1%A8%E6%BC%82%E7%A7%BB%E6%B2%BB%E7%90%86%E4%B8%8E%20Prompt%20%E6%B3%A8%E5%85%A5%EF%BC%9AOntologyDal%20load_lexicon_summary%20+%20common%20%E7%BA%AF%E5%87%BD%E6%95%B0%E8%A7%A3%E6%9E%90%20+%20%E5%86%99%E5%90%8E%E8%AE%A4%E8%AF%81%20+%20DuckDB%20%E6%BC%82%E7%A7%BB%E8%AE%B0%E8%B4%A6.md)（TBox 词表视角）。ABox 遍历搜索视角见 [知识图谱 traverse…](docs/wiki/knowledge/zh/%E7%9F%A5%E8%AF%86%E5%9B%BE%E8%B0%B1%20traverse%EF%BC%9ABFS%20levels%20%E6%B7%B1%E5%BA%A6%E8%BF%94%E5%9B%9E%20+%20DFS%20%E6%A0%88%E6%89%B9%E9%87%8F%E9%A2%84%E5%8F%96%20edge_cache%20+%20IN%20%E5%88%97%E8%A1%A8%20400%20%E5%88%86%E5%9D%97%E9%98%B2%20999%20%E6%BA%A2%E5%87%BA/%E7%9F%A5%E8%AF%86%E5%9B%BE%E8%B0%B1%20traverse%EF%BC%9ABFS%20levels%20%E6%B7%B1%E5%BA%A6%E8%BF%94%E5%9B%9E%20+%20DFS%20%E6%A0%88%E6%89%B9%E9%87%8F%E9%A2%84%E5%8F%96%20edge_cache%20+%20IN%20%E5%88%97%E8%A1%A8%20400%20%E5%88%86%E5%9D%97%E9%98%B2%20999%20%E6%BA%A2%E5%87%BA.md)。

## §1 概述

**本卡角色**：知识图谱「全局点线视图」全链路知识卡。覆盖后端全量聚合端点 `get_knowledge_graph`（handler / common DTO / domain / DAL）、R1/R2/R4 三口径、全局视图字段裁剪、边方向的服务端 resolve 契约，以及 SVG/Canvas 双渲染器的方向性边箭头。**定位：新增/调整全局图谱端点、排查全局视图「线飞出去/度数不对/箭头只在一种渲染器出现」时读。**

- **为什么要有「全局点线视图」这个默认视图**：图谱页此前的入口只能「先搜索 / 先点推荐起点」才出图——用户第一次打开页面看到的是一片空画布，必须先做一次输入动作才有内容。全局点线视图把「全量活跃知识节点 + 双端活跃关系边」一次性拉全，作为**无需任何筛选条件的默认装载视图**：用户先看到全景，再点节点或搜索进入以该节点为中心的局部卡片页（ViewMode 两态：Global 默认 ↔ Local）。这是「先全景后聚焦」的浏览顺序，而不是「先筛选后出图」。
- **为什么边必须「严格双端活跃」（R2）**：关系表里存着指向已遗忘/已删除节点的边（悬挂边）。渲染器若拿到端点缺失的边，只能把缺失端点画成占位节点或直接画一条没有端点的飞线——前者让一屏全是无内容卡片，后者在画布上就是一条不知从哪到哪的线。R2 在 DAL 单点收敛「只保留两端节点都在活跃节点集内的边」，保证渲染器拿到的每条边两端都有实体节点。
- **为什么度数只计「生效边」（R4）**：节点度数（入边 + 出边）是全局视图里**节点半径的唯一输入**（度数越大点越大）。若把被 R2 过滤掉的悬挂边也算进度数，节点会因「看不见的边」而虚胖——视觉上一个大点却只有一两条可见连线，读数与图形自相矛盾。R4 规定度数只在**通过 R2 的生效边**上统计。
- **为什么方向在服务端 resolve（前端零词表映射）**：边的方向（是否有向）由本体词表决定（`ontology_relation_types.direction`），而词表是后端资产。让前端自己维护「哪些关系词是有向的」等价于把词表复制一份到前端，任一侧新增关系词就漂移。三期方案 a′ 把方向在 DAL 聚合时逐边 `common::ontology::resolve` 带出，`GraphEdge.direction` 是**已解析值**（"directed"/"undirected"），前端纯透传——零词表映射这条不变式同时被后端 DTO 注释与前端映射层测试兜底。

## §2 关键文件与职责表

| 文件 | 角色 | 关键内容 | 源码锚点 |
|------|------|---------|---------|
| `src/handlers/hr/agent/get_knowledge_graph.rs` | Adapter（handler） | 神经工具 `get_knowledge_graph`（neural, tags=memory）+ HTTP；装配度数 + 逐边取方向 → 响应；字段裁剪在 `to_node_api` / `to_edge_api` | `#L27-L64` / `#L71-L104` |
| `common/src/api/neural_tools.rs` | DTO（前后端 SSOT） | `GetKnowledgeGraphParams` / `GetKnowledgeGraphResponse` / `GraphNode` / `GraphEdge`（含 `direction`） | `#L259-L318` |
| `src/service/dal/memory.rs` | DAL | `get_knowledge_graph`：R1 全量活跃知识节点 → R2 严格双端活跃 → R4 度数仅计生效边 → 逐边 resolve 方向 | `#L514-L638` |
| `src/models/memory.rs` | 领域结构 | `KnowledgeGraphData { nodes, edges, degrees, edge_directions }` | `#L619-L638` |
| `src/service/domain/runtime/mod.rs` + `memory.rs` | Domain | trait 新增 `get_knowledge_graph` + Domain 一行委托（不夹带业务） | `#L138-L143` / `#L103-L110` |
| `src/router.rs` | 路由 | `POST /agents/get_knowledge_graph` | `#L763-L764` |
| `common/src/ontology.rs` | 本体纯函数层 | `Direction` 二值枚举 + `OntologyLexicon.relation_directions` + `ResolvedTerm.*.direction` + seed `PresetRelationType.direction` serde 兜底 | `#L142-L186` / `#L101-L118` / `#L71-L95` / `#L375-L384` |
| `migrations/20260930000001_add_direction_to_ontology_relation_types.sql` | 迁移 | `direction` CHECK 二值 + `multiplicity` 占位 + 三段幂等回填 | `#L1-L25` |
| `frontend/src/pages/hr/knowledge_graph.rs` | 页面入口 | ViewMode 两态（Global 默认 / Local）+ `load_global_graph` 默认装载 + 搜索/点选切局部 +「返回全局」 | `#L45-L53` / `#L279-L305` |
| `frontend/src/components/graph.rs` | SVG 渲染 | `global_node_radius` / `global_node_label` SSOT（Canvas 同源）+ marker 按 `edge.direction` 条件挂载 + 全局态圆点分支 | `#L208-L230` / `#L636-L668` / `#L723-L790` |
| `frontend/src/components/canvas_scene.rs` | Canvas 渲染 | `CanvasEdge.directional` + `draw_edges` 箭头原语（target 端小三角） | `#L73-L90` / `#L749-L771` |
| `frontend/src/components/graph_canvas.rs` | Canvas 适配层 | global_mode 时 `radius=global_node_radius(degree)`；`GraphEdge→CanvasEdge` 透传 `directional` | `#L79-L90` / `#L140` |
| `frontend/src/api/hr.rs` | API 客户端 | `get_knowledge_graph` → `POST /api/v1/hr/agents/get_knowledge_graph` | `#L394-L400` |
| 【Wiki 长文】知识图谱可视化.md | 系统化上下文 | [docs/wiki/zh/content/前端应用/页面模块/HR 管理页面/知识图谱可视化.md](docs/wiki/zh/content/前端应用/页面模块/HR%20管理页面/知识图谱可视化.md) | — |

## §3 架构约定

1. **R1 / R2 / R4 三口径（DAL 单点收敛）**：R1 = 只取**活跃知识节点**（`MemoryQuery{ memory_type: KnowledgeNode, status: Active, exclude_status: Forgotten, limit/offset: None }` → SQL 不带 `LIMIT` 子句，真正全量）；R2 = **严格双端活跃**（`active_ids.contains(source) && active_ids.contains(target)`，悬挂边一律丢弃）；R4 = **度数只按生效边统计**（`degrees: HashMap<node_id, (in, out)>`，遍历的是 R2 之后的 `edges`，不是原始关系表）。空节点集直接返回空结构，早退不再查关系。
2. **全局视图字段裁剪清单**：`GraphNode` 只含 `id / node_name / node_type / tags / agent_id / is_published / degree / incoming_count / outgoing_count / created_at`；**刻意不含** `node_description` / `summary` / `updated_at`——全局视角关心「点与线」而非细节，正文/摘要/时间在点击节点进入卡片页时经既有通道按需加载。裁剪依据写在 `to_node_api` 注释与 handler 工具描述里。
3. **方向 resolve 词表口径**：DAL 用 `tokio::try_join!` 并行拉 `list_all_classes` / `list_all_relation_types` / `list_all_synonyms` 装配 `OntologyLexicon`（含 `relation_directions`）；对每条边 `common::ontology::resolve(&lexicon, TermKind::Relation, &rel.relation_type)`：`Canonical` / `ViaSynonym` 取登记方向，`Drift`（词表外/漂移词）兜底 `Undirected`。产出 `edge_directions: HashMap<edge_id, String>`（`Direction::as_str()` → `"directed"`/`"undirected"`）。
4. **DAL 之间禁止互引**：`get_knowledge_graph` 处于 `MemoryDalImpl`，**不调用** `dal::ontology::load_lexicon`——而是直接走 `OntologyDao` 原始行装配词表，避免 DAL 层互相依赖（与 `OntologyDalImpl` 同构）。
5. **方向契约的序列化口径**：`Direction` 枚举 `#[serde(rename_all = "lowercase")]` → `"directed"` / `"undirected"`，与迁移 `CHECK(direction IN ('directed','undirected'))` 逐字一致；`FromStr` 非法值 → `Err`，`parse_or_default` 兜底 `Undirected`（脏数据/自拟词退化为无向，不 panic 不阻断）。
6. **双渲染器 SSOT（半径 + 箭头）**：① 半径：`graph::global_node_radius(degree)`（底 7px、每度 +1.1px、19 度封顶 ≈27.9px、28px 防御上限）由 SVG 与 Canvas 共用（Canvas 适配层 `graph_canvas.rs` 调用它，此前内联在 `graph_canvas.rs` 会漂移）；`global_node_label`（半径 <14 不绘名，否则按半径 ×1.5 估宽截断 3~12 字符）同样双端共用。② 箭头：**SVG** 用 `<marker id="arrowhead">` + `marker_end` 按 `edge.direction == "directed"` **条件挂载**（undirected 纯线）；**Canvas** 用 `draw_edges` 内箭头原语（directed 边在 target 端画小三角、方向沿 source→target、尺寸随线宽派生、填充复用边色）。两渲染器的方向输入同源（`GraphEdge.direction` → `CanvasEdge.directional`）。
7. **全局态两者形态对齐 + 全局态固定 Canvas 的例外**：全局态下 SVG 走**圆点分支**（对齐 Canvas 全局模式：圆点 + 度数半径 + 名称限绘，跳过矩形卡片/竖条/正文/标签）；边标签 overlay 在全局态跳过（对齐 Canvas 全局态无边标签）。「返回全局」按钮 onclick 同步切回 Canvas（全局态默认渲染器固定 Canvas，SVG 的圆点分支主要服务对拍与兜底）。
8. **ViewMode 两态语义**：默认 `ViewMode::Global`（`use_effect` 装载全量）；搜索或点选节点 → 切 `ViewMode::Local`（装载以该节点为中心的局部卡片页）；「返回全局」→ 切回 `ViewMode::Global` 并复用已拉取的全局数据。`agent_id` 变化时重新拉取全局数据刷新归属过滤。

## §4 硬约束与回归红线

1. **全量语义必须真全量**：R1 拉节点必须 `limit: None, offset: None`（SQL 不带 `LIMIT` 子句）；禁止复用带默认分页限的查询构造，否则全局视图只显示前 N 个节点，且度数统计也随之残缺。
2. **R2 严格双端活跃是渲染器前置不变式**：只保留 `active_ids` 同时含 source 与 target 的边；禁止把悬挂边透传给前端（要么画出飞线、要么让渲染器对未知端点索引 panic）。
3. **R4 度数必须只按生效边统计**：`degrees` 的入/出计数遍历的是 R2 过滤后的 `edges`，禁止直接用原始关系表统计——否则节点半径会因「看不见的边」虚胖，读数与图形矛盾。
4. **全局视图禁止回带细节字段**：`GraphNode` 不得加回 `node_description` / `summary` / `updated_at`（全局视图只需要点+线+度数；详情走卡片页通道）。要展示细节就进 Local 卡片页，不要把正文塞进全局响应。
5. **方向必须服务端 resolve，前端零词表映射**：`GraphEdge.direction` 是服务端 resolve 后的已解析值；前端禁止维护「关系词→是否有向」的本地词表，只允许 `e.direction == "directed"` 的纯透传判定（`graph_canvas.rs` 映射 `directional`）。
6. **`direction` 只有 directed/undirected 二值**：DDL `CHECK(direction IN ('directed','undirected'))`、`Direction` 枚举、`as_str()` 序列化三处必须同一口径；解析非法值一律兜底 `Undirected`，禁止出现第三值或 panic。
7. **旧 seed 快照兼容不破**：`PresetRelationType.direction` 必须带 `#[serde(default = "default_preset_direction")]`，旧 seed（无该字段）反序列化落 `"undirected"`，与 DDL `DEFAULT` 同口径；禁止改默认值方向。
8. **有向边箭头必须是双渲染器一致的条件挂载**：SVG 的 `marker_end` 与 Canvas 的箭头原语都只对 `directed` 边生效；**禁止只在单渲染器上加箭头**（同一份数据在 SVG/Canvas 下有无箭头不一致 = 视觉漂移，用户以为方向语义丢了）。
9. **度数→半径公式是双渲染器 SSOT**：`global_node_radius` / `global_node_label` 只在 `graph.rs` 一处实现，SVG 与 Canvas 都引用它；禁止任一渲染器另抄一份系数（一处改一处漏 = 两种渲染器同度数点大小不同）。
10. **DAL 之间禁止互引**：`get_knowledge_graph` 直接走 `OntologyDao` 原始行装配词表，禁止调用 `dal::ontology::load_lexicon`（跨 DAL 依赖会形成 DAL 层环）。
11. **全局态 SVG 残留必须清零**：全局态下 SVG 不得残留矩形卡片/正文/标签等局部态元素，也不得绘边标签 overlay（对齐 Canvas 全局态的纯点线形态）；「返回全局」必须同步切回 Canvas。