---
kind: RAG 原子知识卡
name: recommend_seed_nodes 种子节点推荐：三因子打分 0.45 连通度 0.35 内容丰富度 0.2 分享权重 + KnowledgeGraph 组件两端复用
category: 记忆系统 / 前端组件复用
scope:
  - "src/service/dal/memory.rs"
  - "src/handlers/hr/agent/recommend_seed_nodes.rs"
  - "common/src/api/memory.rs"
  - "frontend/src/pages/hr/knowledge_graph.rs"
  - "frontend/src/pages/hr/agent_detail.rs"
source_files:
  - src/service/dal/memory.rs#L97-L138 (MemoryDal trait：recommend_seed_nodes 签名：agent_id(Option, None=全局+published) + limit(默认5 / max 50) → Vec<RecommendedSeedNode>)
  - src/service/dal/memory.rs#L314-L398 (recommend_seed_nodes 应用层计算：DAL 先拉节点 → 再拉所有关系 → HashMap 统计每个节点入边+出边度数 → 三因子加权排序 → truncate(limit))
  - src/handlers/hr/agent/recommend_seed_nodes.rs#L21-L80 (HTTP Handler：RecommendSeedNodesParams { agent_id, limit } 结构化；agent_id 鉴权（只能查自己的 Agent）；limit min(5, 50) 双保护)
  - frontend/src/pages/hr/knowledge_graph.rs#L115-L210 (KnowledgeGraph 可复用子组件：agent_id: Option<String> 唯一 prop；use_effect 自动重拉推荐；种子卡片区渲染 + 点击卡片 → seed_node_ids → 调 traverse_knowledge_graph)
  - frontend/src/pages/hr/knowledge_graph.rs#L688-L770 (HrKnowledgeGraph 路由入口：AppLayout + Agent 选择器 + KnowledgeGraph(agent_id=None))
  - frontend/src/pages/hr/agent_detail.rs#L1090-L1120 (Agent 详情页 Tab5：内嵌 KnowledgeGraph { agent_id: Some(current_agent_id.clone()) } —— 证明「组件两端复用」生效)
  - common/src/api/memory.rs (RecommendSeedNodesParams / RecommendedSeedNode { node, score, reasons: Vec<String> }：reasons 数组给前端展示「为什么推荐这个节点」)
  - frontend/src/components/graph_canvas/knowledge_graph_canvas.rs (画布渲染子组件：接收 nodes + edges + levels，Dioxus 独立 state，与推荐逻辑完全解耦)
  - docs/archive/plan-archive/知识图谱推荐起点与组件复用重构.md（完整 7 章：度数统计 + 前端组件拆分 HrKnowledgeGraph vs KnowledgeGraph 两端复用 + agent_id Option 语义）
  - docs/archive/design-archive/memory_search_enhancement_design.md（§1 决策表 扩展；§3 涉及文件清单包含前端知识图谱页面）
  - （占位：待 ai-orz-doc-maintainer 落地后回填真实 Design 路径 design/seed_node_recommendation_and_component_reuse.md → 目前只有 Plan，后续需补齐决策表与红线）
  - docs/wiki/zh/content/功能模块/AI Agent 管理/记忆系统管理.md（种子节点推荐面板：reasons 列展示每个节点的推荐原因 + 点击按钮后跳画布定位节点）
  - docs/wiki/zh/content/架构设计/记忆系统架构.md（§图谱可视化子系统：推荐起点 + 画布渲染 + 链式探索三段）
  - docs/wiki/zh/content/项目概述/核心功能特性/四层记忆系统/长期记忆 (Long-term Memory)/知识节点管理.md（长期知识节点 PO 字段：tags 数量 + summary 长度 = 内容丰富度打分依据）
  - docs/wiki/zh/content/前端应用/页面结构与路由/知识图谱页面与Agent详情页复用链路.md（两端复用架构图：HrKnowledgeGraph 路由 vs Agent 详情 Tab 嵌入）
  - 【平行卡 1】docs/wiki/knowledge/zh/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出/知识图谱 traverse：BFS levels 深度返回 + DFS 栈批量预取 edge_cache + IN 列表 400 分块防 999 溢出.md（用户点击推荐卡片后，实际调用 traverse_knowledge_graph 链路 = 本卡推荐的下游）
  - 【平行卡 2】docs/wiki/knowledge/zh/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐/记忆搜索增强三合一：FTS5 tags 语义过滤 + 图谱 traverse BFS／DFS 遍历 + recommend_seed_nodes 三因子推荐.md（三位一体搜索增强；本卡是 recommend_seed_nodes 细节拆解的独立卡，与那张形成总-分关系）
---

## §1 概述

**本卡角色**：图谱页面「起步入口」推荐算法 + 前端组件复用拆分的一张双域（后端算法 + 前端组件）综合卡。覆盖后端 DAL 层的三因子加权打分算法（连通度 0.45 + 内容丰富度 0.35 + 分享权重 0.2）、HTTP Handler 的参数双保护、以及前端 `KnowledgeGraph { agent_id: Option<String> }` 可复用子组件——既能在 HrKnowledgeGraph 路由入口以「全局+Agent选择器」模式用，也能在 Agent 详情页第 5 个 Tab 直接嵌入固定 agent_id 的单 Agent 模式。

- **三因子算法设计**：选择在 DAL 应用层做统计（而非纯 SQL 窗口函数），原因：SQLite 对窗口函数支持有限，跨 SQLite 版本兼容性差。做法：两步 DAO 查询 → ① `query_knowledge_nodes` 拉候选节点（受 agent_id + published 过滤）→ ② `list_relations_batch` 一次拉候选节点所有出入边 → HashMap 汇总每节点的入度 + 出度 → 内容丰富度按节点 tags.len() + summary.chars().count() 归一化 → 分享权重按 `published=true` 给 0.2 额外分、共享给团队的额外 0.1。
- **reasons 数组语义**：`RecommendedSeedNode.reasons: Vec<String>` 每一项对应一个子得分（如「连通度高：24 条关联（入8出16）/ 100 归一化得分 0.42」「内容完整：8 个标签 + 摘要 320 字 / 得分 0.31」「已发布共享：+0.2」），前端卡片按 reasons 逐行渲染，用户知道为什么推荐。
- **组件拆分硬约束**：`KnowledgeGraph` 子组件对外只暴露一个 `agent_id: Option<String>` prop。其他所有内部状态（推荐结果、搜索参数、画布节点坐标、levels）必须全部收敛在组件内部的 `use_signal` 里，调用方不能传自定义状态。原因：Agent 详情页调用一行 `KnowledgeGraph { agent_id: Some(x) }` 就够，零心智负担。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 锚点 |
|------|------|---------|------|
| memory.rs (DAL trait) | 对外签名 | agent_id: Option<String>（None = 全局 + published；Some = 指定 Agent 的私有 + published）；limit: Option<usize>（内部 min(user_limit, 50, 500_total_cap)）| `:L97-L138` |
| memory.rs (DAL impl) | 三因子打分核心 | ① 拉节点 → ② 批量拉边 → ③ HashMap<NodeId, InOutDegree> 统计 → ④ 每个节点 score=0.45×norm(连通度) + 0.35×norm(内容丰富度) + 0.2×share_weight → ⑤ sort_by score rev → truncate(limit) → ⑥ 生成 reasons 数组 | `:L314-L398` |
| recommend_seed_nodes.rs (Handler) | HTTP 鉴权 + 参数保护 | agent_id 必做「当前用户对该 Agent 有 view 权限」检查；limit 先 `unwrap_or(5)` 再 `min(50)` 再传给 DAL（DAL 内再做一次 min(500) 总上限，两层防穿透）| `:L21-L80` |
| knowledge_graph.rs (子组件) | KnowledgeGraph 复用子组件 | 单 prop `agent_id: Option<String>`；内部 use_signal(recommended)、use_effect(deps=[agent_id])：一变就自动重新拉推荐；推荐卡片网格区渲染；点击卡片 seed_ids → 调用 search_memory_with_traversal 查图谱 → 给画布组件 | `:L115-L210` |
| knowledge_graph.rs (路由入口) | HrKnowledgeGraph 页面 | 包裹 AppLayout + 顶部 Agent 选择器（下拉，选中后存入 selected_agent_id signal）；调用 KnowledgeGraph { agent_id: selected_agent_id() }（selected 为「不选」= None 语义）| `:L688-L770` |
| agent_detail.rs (Agent 详情页) | Tab5 嵌入证明 | Agent 详情页 5 个 Tab：概览/工具技能/状态图/对话与记忆/知识图谱；第 5 个 Tab = `KnowledgeGraph { agent_id: Some(current_id.clone()) }` —— 组件两端复用的典型落地 | `:L1090-L1120` |
| common/api/memory.rs | DTO | RecommendedSeedNode { node: KnowledgeNode, score: f32, reasons: Vec<String> }；reasons 数组给前端展示「推荐原因清单」| 见 common DTO |

**章节来源**
- [memory.rs:L314-L398](src/service/dal/memory.rs#L314-L398)
- [recommend_seed_nodes.rs:L21-L80](src/handlers/hr/agent/recommend_seed_nodes.rs#L21-L80)
- [knowledge_graph.rs:L115-L210](frontend/src/pages/hr/knowledge_graph.rs#L115-L210)
- [knowledge_graph.rs:L688-L770](frontend/src/pages/hr/knowledge_graph.rs#L688-L770)

---

## §3 架构约定与扩展模式

### 3.1 双端复用数据流

```
后端推荐算法（DAL 应用层 HashMap 统计）
  query_nodes(agent_id_filter) + query_relations_bulk
        │
        ▼  HashMap 统计 + 三因子加权 + reasons 生成
  Vec<RecommendedSeedNode> → 倒序 → truncate(min(limit,50))
        │
        └── HTTP Handler /api/v1/hr/agents/recommend_seed_nodes
                ▲
                │ 两种调用者：
前端路由页 HrKnowledgeGraph ─┘   └── Agent 详情页 Tab5
      agent_id = None / 选择器值         agent_id = Some(current_agent_id)
                │                           │
                └─────────┬─────────────────┘
                          ▼  统一 KnowledgeGraph { agent_id: Option<String> } 子组件
                          │  use_effect(agent_id): 自动拉推荐
                          │  推荐卡片网格：点击 → seed_node_ids → 图谱 traverse
                          ▼
                   KnowledgeGraphCanvas 画布
                (nodes + edges + ordered_levels)
```

### 3.2 扩展模式：新增第 4 个推荐因子（如「最近 7 天被浏览次数」）

1. **后端加因子字段**：在 DAL impl `recommend_seed_nodes` 内部新增 factor4_score，权重建议从原有三因子中分摊（比如把 0.45 + 0.35 + 0.2 → 0.35 + 0.3 + 0.15 + 0.2 浏览热度）。
2. **reasons 追加文案**：对应 factor4 命中节点的 reasons 数组必须追加对应一行（如「近期热门：近 7 天被 12 次浏览 / 最高 32 次 → 得分 0.18」），保证 UI 透明。
3. **前端零改动**：组件不改，DTO 不用扩展（reasons 是 Vec<String>），只需要后端多返回一条 reason 文本——这就是为什么 DTO 设计成 Vec<String> 而不是结构化字段的原因：未来加因子不改 DTO，前端无感知。
4. **组件新增嵌入位置**：比如在 Project 详情页要嵌入项目维度的知识图谱 → 新建一行 `KnowledgeGraph { agent_id: find_project_owner_agent_id(project_id) }`，就搞定了。**组件不提供自定义筛选 prop**，因为业务差异（项目/团队维度）应该通过「过滤 agent_id」实现，不应该污染通用组件。

---

## §4 硬约束与故障排查

### 4.1 必守红线

1. **红线 1**：**三因子权重之和必须 = 1.00（±0.01 容差）**，绝不能 0.45 + 0.35 + 0.2 = 1.02 或 0.98。代码评审里必须加 `debug_assert!((0.45 + 0.35 + 0.2 - 1.0).abs() < 1e-6)`，否则未来加新因子时没人记得归一化，导致 score 线性叠加爆炸，推荐结果完全不可解释。
2. **红线 2**：**limit 双保护，Handler 层一层（min 50）+ DAL 层一层（min 500）**，禁止只做一层。原因：万一 HTTP 路由改了，绕过 Handler 直接进 DAL，仍然有一层兜底，避免一次拉 10 万个节点把前端卡成死机。
3. **红线 3**：**KnowledgeGraph 子组件绝不暴露除 agent_id 外的自定义状态**。如果未来有人为了复用想要「自定义推荐过滤条件」，应该：(a) 在后端 DTO `RecommendSeedNodesParams` 加字段；(b) 在组件内部按 agent_id 派生。绝不把内部 signal 通过 prop 暴露出去——否则两端调用方会开始写大量 `if (custom_mode) { ... }`，组件复用会彻底变成复制粘贴。
4. **红线 4**：**published 节点的分享权重永不等于 0**。即使这个节点连通度 0、内容丰富度 0（空节点），只要 published=true，分享权重 0.2 就能让它出现在全局推荐（None agent_id 模式下）的尾部。否则用户「我明明发布了一个节点，为什么全局图谱推荐一个都看不到？」永远是个玄学 bug。

### 4.2 故障排查路径

| 症状 | 起点锚点 | 次级排查 |
|------|---------|---------|
| 全局图谱（HrKnowledgeGraph 不选 Agent）页面打开，推荐卡片空（0 个节点） | [memory.rs:L314-L398](src/service/dal/memory.rs#L314-L398) 检查 published_flag 过滤 | 典型：知识库初始为空（所有节点都是私有），没一条 published=true。临时排查：手动去 DB 手动把一条知识节点 UPDATE `is_published = 1` 后刷新验证 |
| 同一 Agent 详情页嵌入的 KnowledgeGraph 推荐节点 ≠ 全局页选同一个 Agent 下拉的推荐节点（顺序差很多） | [knowledge_graph.rs:L115-L210](frontend/src/pages/hr/knowledge_graph.rs#L115-L210) 检查两端调用参数 | 典型：全局页传了 `limit = 5`，详情页 `limit = 10`，推荐排序截断点不同 → 节点重叠但顺序不保证；或者一端走了缓存（没走网络）另一端没走，use_resource 缓存 key 漏了 agent_id |
| 推荐 reasons 数组里的得分加起来 ≠ 最终 score | [memory.rs:L350-L390](src/service/dal/memory.rs#L350-L390) 三因子权重计算处 | 典型：某因子归一化公式改了（比如 max_degree 从全局 max 改成 候选集 max），但 reasons 文案还按旧公式打印，显示得分与实际 score 对不上 |
| Agent 详情页 Tab 切换到「知识图谱」，整个页面白屏 5 秒后才渲染 | 检查 use_effect 是否在每次 Tab 切换时都重复拉推荐 + traverse 两遍 | [agent_detail.rs:L1090-L1120](frontend/src/pages/hr/agent_detail.rs#L1090-L1120) 确认：是否给 KnowledgeGraph 子组件传了稳定的 agent_id（如果每次 render 生成新 String 实例，use_effect 会认为依赖变了，不停重拉推荐） |
| 新增因子后 DTO/Handler 编译通过但前端显示 reasons 全空 | 检查 reasons 数组的 push 代码 | 典型：新增因子 push reason 时写错了 `if factor4_score > 0 { reasons.push(...) }` 但 factor4_score 默认 0.0，实际上所有节点都没过条件；应改为无论得分多少都要 push reason（至少显示 "最近浏览：无记录，得分 0.0"），保证 UI 透明性 |
