# docs/diagrams · 架构图目录

本目录存放**自包含 HTML 架构图**：单个文件内含全部样式、脚本与数据，克隆仓库后本地双击即可交互，也可在 GitHub Pages 落地页 / 前端文档中心在线渲染。

> 更新方式：重新生成后**覆盖同名文件**即可；不要引用外部 CDN、不要把图拆成多文件。

---

## 一、一张图一个抽象层级

- **总图**只回答「系统里有哪些容器、请求怎么流、细节去哪看」（容器级）。
- **专题图**回答「这个子系统的能力线、链路过哪几层、口径是多少」（组件级 + 时序混合）。
- 不把子系统塞进总图：总图连 8 个业务域都只用一行字概括，单拆某一个会破坏层级一致性。

## 二、清单（17 张）

| 文件 | 层级 | 讲什么 | 视图 | 对应知识库模块 |
|---|---|---|---|---|
| [ai-orz-architecture.html](./ai-orz-architecture.html) | 容器级总图 | 三级 workspace、四层单向调用、存储与外部系统、认知架构分层 | 主请求路径 / 异步与 Agent 运行时 / 持久化 | 00-总览与架构 |
| [ai-orz-think-path.html](./ai-orz-think-path.html) | 组件级 | 两阶段唤醒 → 小脑决策契约（entry_gate / entry_pool / chain_reply, close_default）→ 思考中注入新输入 → 三层预算循环（跨周期 RoundCollected 收集）→ 七相位图引擎 → 两段式落盘（增量 + 收尾，TraceFinishGuard 五条退出路径兜底 close 行）→ 前端时间线；轨迹查询走 `.jsonl.idx` 旁车索引（索引过滤 + seek 命中行）；含思考强度判相对偏移、先验与兜底分离 | 一轮思考 / 两阶段唤醒 / 轨迹链路 / 认知分层 | 02-Agent运行时 |
| [ai-orz-tools.html](./ai-orz-tools.html) | 组件级 | CoreTool 三协议路由（Builtin / HTTP / MCP）+ 工具级凭据契约与身份注入 + 策略拦截与六态授权 + 技能包装配 | 三层调用 / 策略与授权 / 注册与装配 | 06-工具与技能 |
| [ai-orz-tool-context.html](./ai-orz-tool-context.html) | 组件级 | 工具调用中的上下文（身份）数据流转：声明（`CredentialKind` 的 `UserInfo` / `AgentInfo` 两个非凭据变体 + `field` 白名单 + `binding` 四注入点）→ 解析（`is_identity` 二元分派 + 未命中按来源分叉）→ 注入（`check` 注入 + `base_env` 九键末位硬 insert）→ 使用（git 子进程 4 个 `GIT_*` / commit-msg hook） | 四层主链路 / 失败语义分叉 / 注入与优先级 / 落点实证 | 06-工具与技能 |
| [ai-orz-aop.html](./ai-orz-aop.html) | 组件级 | pkg/aop 六 Trait + Registry 单例 + 12 类业务消费者（18 处注册 / 8 Sync + 10 Async）+ 3 类 tick 生产者 + context_carrier 贯穿 | 一次事件流转 / 启动顺序 / 生产者与兜底 | 10-异步基建(AOP) |
| [ai-orz-memory.html](./ai-orz-memory.html) | 组件级 | 四层记忆（Core / Working / Short / Long）+ 两条 JSONL（writer + 旁车索引）+ 三路召回 + 短期记忆索引分层 + 睡眠沉淀 | 四层记忆 / 存储面 / 检索与沉淀 | 03-记忆系统 |
| [ai-orz-ontology.html](./ai-orz-ontology.html) | 组件级 | 受控词表 TBox + 属性定义与门闩 + 骨架三元组 + 漂移与有效性治理（口径与现网 `get_ontology_drift_dashboard` / `get_property_effectiveness` 同源） | 读侧链路 / 写入门闩 / 治理面 | 00-总览与架构 |
| [ai-orz-org-federation.html](./ai-orz-org-federation.html) | 组件级 | 多级组织 + UserRole 并查集继承 + JWT 双模式 + 身份凭证 + 配对码建联与跨组织签名 | 组织与角色 / 认证与凭证 / 联邦组网 | 01-组织与权限 |
| [ai-orz-task-project.html](./ai-orz-task-project.html) | 组件级 | TaskStatus 六态（含 PendingReview=5）+ 验收闭环 + dependencies DAG + execution_plan / result 增量 + Project 五字段注入 | 任务状态机 / DAG 与唤醒 / 项目聚合 | 05-任务与项目 |
| [ai-orz-messaging.html](./ai-orz-messaging.html) | 组件级 | 渠道入站归一化 → 路由策略（含回复仲裁与协作压力）→ 入站过滤（规则门 + 小脑相关性门，渠道无关）→ MessageDomain（builder / delivery）→ 出站媒体分类（MIME 单点判定 / 超限跳过）→ 附件字节流与正文引用 → DAL/DAO → SSE 与唤醒 | 入站与路由 / 编排与投递 / 存储与唤醒 | 04-对话与消息 |
| [ai-orz-a2a.html](./ai-orz-a2a.html) | 组件级 | Agent Card + JSON-RPC 四方法 + 协议映射与回调 + 30s 轮询兜底 + 联邦签名 | 协议入口 / 结果与兜底 / 联邦与长连接 | 07-外部Agent(A2A) |
| [ai-orz-search.html](./ai-orz-search.html) | 组件级 | FTS5 全文 + 向量检索 + 图谱遍历三路召回 + 属性精确过滤 + `ids` 直取与 `include_retired` / `include_history` 受控出口 | 工具入口 / 三路召回 / 存储面 | 08-搜索 |
| [ai-orz-stats.html](./ai-orz-stats.html) | 组件级 | 业务事件 → AOP 同步消费 → 四类指标落库（含按时序桶的耗时统计）→ 采集器落 pkg/stats/runtime（全局单例，handler 直读）→ 监控 API 与看板 | 采集 / 聚合 / 读取与看板 | 09-统计与监控 |
| [ai-orz-frontend.html](./ai-orz-frontend.html) | 组件级 | Dioxus WASM 入口 / 路由 / 页面 / HUD / API 客户端 + 产物预览（HtmlPreview 唯一渲染出口） + build.rs 产物 | 应用骨架 / 页面与组件 / 数据与产物 | 11-前端应用 |
| [ai-orz-infra.html](./ai-orz-infra.html) | 组件级 | 配置 → 连接池与迁移 → 路径与 JSONL（pkg/jsonl：writer + sidecar）→ 向量重建三档 → 后台任务 → 打包发布 | 启动与配置 / 运行期资源 / 构建与发布 | 12-基础设施与部署 |
| [ai-orz-data-model.html](./ai-orz-data-model.html) | ER 图（表级） | 三族九表：身份 / 工作 / 知识图谱，字段取自现网 SQLite schema | 身份与组织 / 任务与消息 / 知识图谱三件套 | 13-数据模型 |
| [ai-orz-testing.html](./ai-orz-testing.html) | 组件级 | fmt / clippy / test / docs_lint / inline-test-lint 五道门禁 + 护栏用例 | 门禁组合 / 测试分层 / 文档与索引 | 14-测试 |

## 三、阅读顺序

1. **总图**先建立全局认知（3 分钟）。
2. **运行时六张**：思维轨迹（怎么想）→ 工具与技能（怎么动手）→ 工具调用上下文（身份怎么进 env）→ 异步基建（怎么解耦）→ 记忆（怎么记住）→ 搜索（怎么找得到）。
3. **业务与通道四张**：组织与权限（谁能做什么）→ 任务与项目（活怎么流转）→ 对话与消息（话怎么传）→ 外部 Agent A2A（怎么接外面）。
4. **数据与运行面三张**：本体（图谱的 TBox，写记忆前看）→ 数据模型 ER 图（表与字段）→ 统计与监控（数字从哪来）。
5. **工程面三张**：前端应用（界面怎么搭）→ 基础设施与部署（怎么跑起来、发出去）→ 测试与门禁（怎么保证不坏）。

## 四、选型理由与缺口

**为什么是这 17 张**：优先选「跨层链路长 + 有状态机/多协议 + 看图能明显省读文档」的模块。15 个能力域现已**全部有图**（00-总览与架构 对应总图 + 本体专题图）。

**已补齐的 8 张（2026-11-01 第二批）**：

- 04 对话与消息：渠道入站归一化、路由策略、builder/delivery 分工、SSE 只做增量帧；
- 07 外部 Agent(A2A)：Agent Card + JSON-RPC 四方法、协议映射与回调、30s 轮询兜底、联邦签名；
- 08 搜索：三路召回（FTS5 / 向量 / 图谱）+ 属性精确过滤独立通道 + 读宽写窄；
- 09 统计与监控：事件消费而非业务埋点、四类指标与看板；
- 11 前端应用：Dioxus WASM 骨架、页面与 HUD 组件、API 客户端、build.rs 产物；
- 12 基础设施与部署：启动顺序、路径与 JSONL 约定、单二进制 + dist 打包；
- 13 数据模型：**全套唯一一张 ER 图**，三族九表 + 字段来自现网 SQLite schema（`PRAGMA table_info` 直读，非手写推断）；
- 14 测试：五道门禁 + 测试分层 + 文档机检。

**仍然不做的**：仪表盘 / 实时监控面板（前端已有 HUD 组件，不属于架构图）；单文件静态图（用 Mermaid 更划算）。

**2026-11-09 新增一张（第 17 张）：工具调用上下文（身份）数据流转**

起因：身份注入改造落地后，原 `ai-orz-tools.html` 只画到「凭据需求从 registry 取 → `ToolExecutionRequest{resolved}` 进 domain → DAL」，**没有画「上下文（身份）怎么变成子进程 env」这一段**。新图只补这一段，不重造工具总图；两张图的分工是「调用编排全景」与「上下文数据流单链」。

**这张图不画的（避免误读为已实现）**：

- `pkg` 通用 TTL cache 模块、`DAO` 挂缓存与失效、访问选项 `CacheMode`（Bypass 默认 / Use）——**方向已定、未落码**，图内以卡片显式标注；
- 前端「Git 提交身份」预设一键 4 条与「恢复初始设置」按钮——同属未落码，不在图内；
- 工具输出的三分投影（运行时输出 / 最终结果 / 产物）只在卡片里给口径，不画成链路节点（它属于编排侧，见总图与 06 模块文档）。

**已知局限**：ER 图只覆盖三族九表（核心主链路），`ontology_*` 词表族与日志/统计明细表未画；如需全库 ER 图，建议先固定一份 schema 快照再生成。

**2026-11-13 增量同步（12 张图重渲染，revision 统一刷到 `ef25e17372b66079d7465a6f4e2889924dd14d39`）**

本基线为 **PR #7 分支 `feat/cerebellum-relative-strength-and-chain-thresholds` 的 HEAD，尚未进 main**；PR #7 合并后 `main^{tree}` 应与该 commit 的 tree 等值。docs 同步落在独立分支 `docs/diagrams-sync-20261113`，不进 PR #7（评审面保持干净）。

旧基线 `684ae17b` 那一版（commit `1fd3fcf1`）非废稿：12 张的判定与口径沿用，本次按新基线 `git rebase --onto` 改基后**重测声明值**并补第 8 条。

口径与行数全部当场实测（不是沿用 11-05 快照）：

- **architecture**：小脑行由「800ms / 0.85」改为「entry_gate + entry_pool 两个决策点，缺省超时 3000ms、两阈值各 0.9」；cortex 行 2,500 → 3,797 行；子图索引补第 4 张（tool-context）。
- **think-path**：新增三张卡（第 3 张为 PR #7 基线新增）：「小脑决策契约与场景矩阵」（四形态 / 单点 decide / 串行 batch 共享 timeout / chain_reply 六值含 close_default / 分档门槛 0.0·0.8·0.9 / degraded 与 decision 两列独立 / 熔断只判可用性）与「小脑输入预算：按占比分账 + Score 量纲」（骨架 12.5% / 待发 15% / 正文 72.5% + 片段 4% 块内派生、总和 ≤100% 唯一硬顶、`score_to_unit` 按等级数归一）；行数刷新 cerebellum 563、cortex 3,797、think_loop **866**、think_round **2,298**、think_trace **759**、think_trace_query **321**、inbox 368。
- **think-path · 第 8 条（PR #7 基线新增）**：卡片「思考强度：判相对偏移，先验与兜底分离」。口径以实落为准 = `strength_fallback`（不是早期书面稿的 `strength_anchor`）：`base` = Agent 覆盖 → Provider 基准 → DEFAULT(50)，既是配置权威也是判断与折算锚点；`fallback = base.offset(场景固定 + 意图动态)`，**仅降级（无结论/失败/超时/答案不可用）时下发**；`result = base.offset_with_headroom(delta, MAX_DELTA)`，`delta ∈ [-50,+50]` 按剩余空间折算、恒在 0–100。**偏移完全不进折算公式**，判断有结论时以 delta 为准、`fallback` 不参与也不二次叠加 —— 这就是「**同一偏移不得算两遍**」这条根因（旧链路把偏移在判断前硬并入锚点，基准顶格时 headroom=0，判「更深」兑现不了，8 次实测全落 98–100）。
- **memory**：新增卡片「短期记忆索引分层（预算驱动）」；`search` 副标补「属性精确 + ids 直取」；沉淀卡由「加 published」改为「置 `is_published`（与 `user_tags` 拆列双写）」。
- **ontology**：行数刷新 domain 1,478 / DAL 1,366 / 契约 962 / handler 26；**看板口径按现网接口重取**（域内 1,624 / 域外 18 / 0.9890，未触发 0.95 告警；有效性 94 组中 49 组标黄（`insufficient_sample` 并列标注、不抑制）；关系覆盖 1,230/23/1 = 0.9817，节点漂移 0），旧值 1,103/124/0.8989 与 91 组已在卡内显式标注作废。
- **search**：新增卡片「检索状态与受控出口」（`include_retired` / `retired_excluded_count` / `MemoryResult.status` / `include_history` / `ids` 直取 400 语义）。
- **frontend**：新增卡片「产物预览与 HTML 渲染出口」（HtmlPreview 唯一出口、sandbox 永不带 `allow-same-origin`、4 MiB 上限、小脑轨迹表）；页面/组件数刷新为 62 / 63。
- **data-model**：`long_term_knowledge_node` 补 `user_tags` 列（= 现网 PRAGMA 12 列），sources 增补 `migrations/20261105000001_split_node_tags.sql`。
- **messaging**：`route` 副标补「回复仲裁」，新增卡片「回复仲裁与协作压力」（Final 唯一权威 / 疲劳度回血 / 硬墙改知会收口 / 提示词写实际值）。
- **org-federation**：身份凭证由「5 类型」改为「7 凭据 + 2 身份注入（`UserInfo` / `AgentInfo` 不落库、不可绑定）」。
- **stats**：新增卡片「耗时统计口径（2026-11-10 新增）」（`avg_duration_ms` 只对带样本记录求均值、`duration_sample_count` 加权复算、`None ≠ Some(0.0)`；小脑决策观测走 trace 的另一条面）。
- **tools / infra**：tools 凭据行改为「工具级契约 · 注入四落点」并新增「凭据契约与身份注入」卡；infra `lance` 行补「三档重建 Auto/Missing/Force」并新增「向量重建三档与中间产物 GC」卡。

未做人眼视觉复核，仅通过 archify 四道门禁（validate / deliver / check / browser-check 全 pass、diagnostics 0）。

**2026-11-13 增量同步（第二轮：3 张重渲染，revision 刷到 `8b85679f077fde47650f2e5a81d48ea04b5a0114`）**

本基线为 **PR #8 分支 `feat/aman-local-changes-20261113` 的第 5 笔提交（`8b85679f`）**，尚未进 main；PR #8 合并后该 commit 将成为 `main` 上合并提交的祖先（其改动随 squash / merge 落进 main 的对应提交）。docs 改动单独一笔，推同一分支，不进新 PR。

判定方法：从每张图 HTML 的 `archify-source-evidence-data` 取 `nodes{}` 里的 `path` 清单，与本批 diff 文件集（`8b85679f` 的 67 个路径）求交得 touched 数；`touched=0` 且无新机制判「不需更」。

- **touched>0 且需更（3 张）**：messaging（2：`src/producer/message_channel.rs`、`src/service/dao/message/mod.rs`）、think-path（2：`dao/cerebellum/client.rs`、`dao/cortex/native/http.rs`）、architecture（2：`src/producer/mod.rs`、`dao/cortex/native/http.rs`）。
- **touched=1 但不需更（1 张）**：aop —— 唯一命中 `message_channel.rs` 的是 AOP 的 `MessageChannelProducer 5s` tick 组件；本批改的是该 producer 内部的入站过滤业务，属 messaging 图承载域，AOP 的抽象层（框架零业务 / 六 Trait / Registry / tick 生产者清单与兜底节奏）未变。
- **touched=0（其余 13 张）**：a2a / data-model / frontend / infra / memory / ontology / org-federation / search / stats / task-project / testing / tool-context / tools —— sources 与本批 diff 交集为空。

本次改动摘要：

- **messaging**：新增组件「入站过滤层（渠道无关）」（`src/producer/inbound_filter.rs`），连接改为 `route → inbound_filter → domain`（原 `route → domain` 删除），连线上 +1；`producer` 副标补「归一后进过滤层」、`dao` 副标补「MessageQuery 支持 chat_id」；新增卡片「入站过滤：渠道无关的单点门」（私聊恒放行 / `group_trigger_gate` 四策略含空前缀降级 mention / `relevance_decision` 区分 Unavailable（静默）与 Failed（疑惑） / `AdaptedMessage` 三信号 / `relevance_gate` 只声明口径、`judge_noul` 下沉并返回 Probability / Unavailable / Failed / `MessageQuery` 的 chat_id 下推 SQL）；`meta.subtitle` 同步插入「入站过滤（规则门 + 小脑相关性门）」。组件 17→18、连线 17→18、卡片 6→7。
- **think-path**：`cerebellum` 行数 563 → **519**、`cortex` 3,797 → **3,795**（当场实测）；`cerebellum_router` 副标补「judge_noul 轻量判定」；「小脑输入预算」卡新增第 6 条：入站子分配 36% / 14.4% / 14.4% / 7.2% = 72% ≤ 正文池 72.5%，编译期断言 `assert!(INBOUND_TOTAL_BP <= BODY_BP)` 钉死。组件 / 连线 / 卡片 / 视图数不变（16 / 16 / 10 / 4）。
- **architecture**：「认知架构：大脑不止一层」卡 `dao/cortex` 3,797 → **3,795**，并新增一条「Provider 端点不猜：`base_url` 全量必填（删内置厂商域名表与 `DEFAULT_SYSTEM_ONE_BASE_URL` 兜底，空即 `ConfigInvalid`；FastEmbed 是唯一免填例外）」。组件 / 连线 / 卡片 / 视图数不变（17 / 18 / 6 / 3）。

声明值当场实测（2026-11-13，与上一轮 r13 对照）：`dao/cerebellum` **519**（未变）、`dao/cortex` **3,795**（r13 的 3,797 → 本批减少 2 行，已同步修数）、`think_loop.rs` **866**（未变）、`think_trace.rs` **759**（未变）、`think_trace_query.rs` **321**（未变）、`inbox.rs` **368**（未变）、`think_round/` **2,298**（未变）、`pkg/think_path/` **2,499**（未变）、`cerebellum_router.rs` **519**（未变）、`cerebellum_context.rs` **601**（r13 的 606 → 本批删 `context_soft_target` / `context_hard_ceiling` 后为 601）、前端 pages **62** / components **63**（均未变）、覆盖率门槛 PR **38%** / main **45%**（未变）；新增源文件 `inbound_filter.rs` **185** / `inbound_filter_tests.rs` **154** / `relevance_gate.rs` **33**。预算常量实测：`FRAME_BP=1_250`、`PENDING_REPLY_BP=1_500`、`BODY_BP=7_250`、`SNIPPET_BP=400`、`INBOUND_HISTORY_BP=3_600`、`INBOUND_SEARCHED_MEMORY_BP=1_440`、`INBOUND_RECENT_MEMORY_BP=1_440`、`INBOUND_CURRENT_BP=720`、`INBOUND_TOTAL_BP=7_200`，编译期断言在 `cerebellum_context.rs` L111。

未做人眼视觉复核，仅通过 archify 四道门禁（validate / check / finalize --browser-check 全 pass、diagnostics 0）。

**2026-11-15 增量同步（四张重渲染，revision 刷到 `25cced8ff330f005a39ecb37a178e08732839a85`）**

本基线为 **PR #9 分支 `feat/aman-local-changes-20261114` 的第 4 笔提交（`25cced8f`）**，尚未进 main；PR #9 合并后该 commit 将成为 `main` 上合并提交的祖先。图纸改动单独一笔，推同一分支，不进新 PR。

判定方法：从每张图 HTML 的 `archify-source-evidence-data` 取 `nodes{}` 里的 `path` 清单，与 PR #9 全量文件集（`git diff --name-only origin/main...HEAD` 的 **125** 条路径）求交得 touched 数；`touched=0` 且无新机制判「不需更」。**求交基数必须是 PR 全量 125 路径**——用「本批四笔 103 路径」会漏掉前两笔的重叠路径，实测因此把 messaging 从 7 误算成 5、aop 从 2 误算成 1。

- **touched>0 且需更（4 张）**：task-project（**11**：`frontend/src/pages/workspace.rs`、`src/consumer/scheduler.rs`、`src/consumer/task_event_consumer.rs`、`src/consumer/wake_gate_policy.rs`、`src/models/events/task_status.rs`、`src/models/project.rs`、`src/router.rs`、`src/service/domain/project/{mod,service,task,task_graph}.rs`）、messaging（**7**：`frontend/src/pages/message/chat.rs`、`src/consumer/message_route_policy.rs`、`src/producer/inbound_filter.rs`、`src/producer/message_channel.rs`、`src/service/domain/message/{builder,delivery,mod}.rs`）、aop（**2**：`src/consumer/mod.rs`、`src/producer/message_channel.rs`）、architecture（**2**：`src/consumer/mod.rs`、`src/router.rs`）。
- **touched>0 但机制未变、判不刷新（7 张）**：a2a（2）、frontend（2）、org-federation（2）、stats（1）、think-path（1）、tool-context（1）、tools（1）——命中项分别是「调用签名补 `None` 参数」「页面计数未变」「链上下文不随 carrier 传播」这类机械适配，把偶然引用画进图等于把噪声当机制。
- **touched=0（6 张）**：data-model（只画存储形态，本批迁移只加列不改表形）、infra（进程/端口/容器，零涉）、memory（分层记忆与 JSONL 写入，零涉）、ontology（TBox 零变更）、search（FTS5 + 向量召回未动）、testing（门禁判据未改）。

本次改动摘要：

- **task-project**：`meta.subtitle` 由「TaskStatus 四态」改为**六态（含 PendingReview=5）+ 验收闭环**；组件 14→**22**、连线 17→**28**、卡片 4→**8**。新增 `reviewguard`（`review_guard fail-closed`）、`confirm`（`PUT /tasks/{id}/confirm`）、`reject`（`PUT /tasks/{id}/reject`）、`transfer`（`PUT /projects/{id}/owner`）、`scope`（`effective_root_user_id`）五个组件，以及三个 Sync 消费者 `TaskDispatchConsumer` / `TaskPendingReviewConsumer` / `TaskCompletionConsumer`。四张新卡：六态硬顺序（判别值只追加不插入）、验收闭环（提交 ≠ 完成：`submit_for_review` 不写 `progress=100` 与 `end_at`，100% 只在 `confirm_task` 生效）、验收身份守卫 fail-closed（允许集 = 发起者 ∪ 项目 Owner，空集即拒）、三个 Sync 消费者把「动作」与「通知」切开。
- **messaging**：新增 `identity` 组件（消息三身份：派发者 / 接收者 / 归属用户）与三条边；`db` 组件副标补 `root_user_id` 并加挂迁移 `20261115000002` 作为 source；`route` 副标改「三身份路由 · DeliverToRootUser」。两张新卡：消息三身份（`messages.root_user_id` 与 `from_id` / `to_id` 正交，旧「用户身份中继」已删）、第 6 态带来两条新消息类型（`TaskCompleted(14)` / `TaskPendingReview(15)`，`TaskDispatchNotification(10)` 已无生产发送点仅兜存量）。组件 18→**19**、连线 18→**21**、卡片 7→**9**。
- **aop**：`meta.subtitle` 由「7 类业务消费者」改为**12 类业务消费者（18 处注册 / 8 Sync + 10 Async）**；`consumer` 与 `channel` 副标同步。新增卡片「Sync 与 Async 的现行分布（实测）」：18 处 `register_consumer` 覆盖 12 个消费者文件，Sync 8 / Async 10，本轮新增的 dispatch / review / completion 三名全为 Sync。**组件与连线数不变（14 / 14）**——理由：任务三消费者属 `src/consumer/mod.rs` 的注册细节，AOP 图的抽象层是「框架零业务 + 六 Trait + Registry」，把具体业务消费者展开会把框架图变成业务图；口径数字与 Sync 清单进卡片即可承载。卡片 4→**5**。
- **architecture**：**只刷 revision 与卡片口径，不动组件与连线（17 / 18 / 6 全不变）**。既有「严格单向分层（红线）」卡补一条「验收闭环同层分工：Handler 层 `review_guard` 拦身份，Domain 状态机管流转」；「认知架构：大脑不止一层」卡补一条「任务链路补验收：六态含 `PendingReview=5`，`mark_done` = 提交验收，只有 `confirm_task` 进 Completed」。

声明值当场实测（2026-11-15，对照 r14）：`dao/cortex` **3,795** 行 / 9 文件（未变）、`dao/cerebellum` **519** 行（未变）、前端 pages **62** / components **63**（均未变）、覆盖率门槛 PR **38%** / main **45%**（`.github/workflows/rust.yml` L217，未变）、workspace members **5**（未变）、`src/handlers` **8** 项 / `src/service/domain` **7** 域（未变）、`register_consumer` **18 处**（本轮新增 3，此前 15）、`src/consumer/*.rs` **25** 个文件、其中 12 个含 `ConsumeMode` 声明（Sync 8 / Async 10）。新增源文件行数：`review_guard.rs` 104 / `confirm_task.rs` 83 / `reject_task.rs` 61 / `projects/transfer_owner.rs` 141 / `projects/scope.rs` 96 / `task_dispatched.rs` 82；迁移 `20261114000001` 11 行 / `20261115000001` 4 行 / `20261115000002` 27 行。

**已知差异（只记录，不改文档、不顺手修）**：`docs/modules/05-任务与项目/A-设计/task_acceptance_and_dual_channel_design.md`（AMan 自写，本轮同批入库）已在第 2 节六态、第 5 节 DoD、第 8 节负责人交接口径上覆盖本批设计；本批未发现文档与代码冲突（六态、`confirm_task` 唯一入口、`review_guard` fail-closed 三处口径与代码一致）。差集判定留待 AMan 定。

未做人眼视觉复核，仅通过 archify 四道门禁（validate / deliver / check / browser-check 全 pass、diagnostics 0）。

**2026-11-17 增量同步（四张重渲染，revision 刷到 `1664ec22c728fa8ba003938b92ed7ad7ff475906`）**

本基线 = **PR #11 分支 `feat/jsonl-sidecar-index-20261117` 的 tip 提交 `1664ec22`**（已推送 origin，非本地孤儿锚点），尚未进 main；PR #11 合并后该提交将成为 `main` 上合并提交的祖先。这一轮同时把上一批 PR #10（squash `3ee1bb31`，ID 生成收敛到 `common::id` / 记忆读取改优先级排序 / 工具准入判据更正）的影响一并评估——它当时没做架构图评估。

判定方法：从每张图 HTML 的 `archify-source-evidence-data` 取 **`nodes{}`**（不是 `sources[]`，后者会解析出全 0 的假象）里的 `path` 清单，与「本批全量 diff」＝`git diff --name-only origin/main...HEAD`（PR #11，28 路径）∪ `git diff --name-only 7e5c77b0..3ee1bb31`（PR #10，87 路径）＝**113 路径**求交。

逐图 touched（17 张，touched>0 共 12 张 / touched=0 共 5 张）：

- **真需刷（3 张）**：memory（**4**：`src/handlers/hr/agent/mod.rs`、`src/models/agent.rs`、`src/service/dao/memory/sqlite/knowledge.rs`、`src/service/dao/memory/sqlite/short_term.rs`）、think-path（**4**：`src/handlers/hr/agent/mod.rs`、`src/service/domain/runtime/think_trace.rs`、`src/service/domain/runtime/think_trace_tests.rs`、`src/service/domain/runtime/tool_execution.rs`）、architecture（**2**：`src/service/domain/runtime/think_trace.rs`、`src/router.rs`）。
- **另加 1 张非 touched 命中也要刷：infra**（batch touched **0**）——它的 `jsonl` 组件 source 指向 `src/pkg/daily_jsonl.rs`，而该文件本批被重命名为 `src/pkg/jsonl/writer.rs`（rename-aware 记 R059），组件 label 恰好就叫 `DailyJsonlWriter`。**「图里点名的那个东西现在不存在了」属于最硬的需更信号，与 touched 计数无关**。
- **touched>0 但判不刷（9 张）**：task-project（5）、tools（4）、org-federation（2）、a2a（1）、frontend（1）、messaging（1）、ontology（1）、tool-context（1）、search（3）。命中项集中在三类机械适配：①`uuid::Uuid::now_v7()` → `common::new_id!(X)` / `TimeId::generate()`（**同一实体同一前缀，ID 形态变了但图的组件与链路一条没变**；把 ID 生成点逐处画进图等于把实现细节当机制）；②`src/router.rs` 只改了 `update_agent_model_provider` 路由上方一行**注释**（准入判据措辞，路由与图层未动）；③`src/handlers/hr/agent/mod.rs` 只改了 `kr_{}` 的 ID 生成方式。
- **search 单独的判据**：3 处命中分别是 ID 生成、`dao/memory/mod.rs` 的 `ORDER BY … , rowid` 兜底、`dao/memory/sqlite/knowledge.rs` 的 writer 路径改名，都不改「三路召回 + 属性精确 + 受控出口」这一层；`rowid` 兜底由 memory 图的卡片承载（短期记忆同秒并列按 rowid），搜索图只是消费同一个 DAO。
- **touched=0（5 张）**：aop（`pkg/aop` 一行未动，框架层与消费者清单未变）、data-model（本批零迁移、零 `.sqlx` 变更，表形未变）、stats（`task_event_consumer` 未入本批全量 diff）、testing（门禁判据与 `Makefile` / `check.sh` 未改）、infra（路径命中 0，但按上面「源被删」规则仍刷）。

本次改动摘要：

- **think-path**：`trace` 组件副标 759 行 → **677 行**（当场实测，净缩 82 行：查询与索引逻辑外移到旁车索引）；连线 `trace → jsonl` 标签 `DailyJsonlWriter` → **`pkg/jsonl writer`**；连线 `jsonl → query` 标签 `按天分片读回` → **`索引过滤 + seek 命中行`**；新增第 11 张卡「轨迹查询走旁车索引（不再全量扫描）」6 条；`trace` 视图 note 补索引口径。组件 / 连线 / 视图数不变（16 / 16 / 4），卡片 10→**11**。
- **architecture**：`traces` 组件的 source `src/pkg/daily_jsonl.rs`（**该文件已不存在**）→ `src/pkg/jsonl/writer.rs`（+ 保留 `src/service/domain/runtime/think_trace.rs`）；副标补「两个 JSONL 产物名」。组件 / 连线 / 卡片 / 视图数全不变（17 / 18 / 6 / 3）。
- **infra**：`jsonl` 组件 label `DailyJsonlWriter` → **`pkg/jsonl · writer + sidecar`**，sources 由已删除的 `src/pkg/daily_jsonl.rs` 换成 `src/pkg/jsonl/writer.rs` + `src/pkg/jsonl/sidecar.rs`；「落盘都走同一套约定」卡第 2 条改写并 **+1 条**（索引只是加速层，删 `.jsonl.idx` 会按需重建，原文与物理行号零改动）。组件 / 连线不变（16 / 14），卡片 5 张、items 3→4。
- **memory**：「两条 JSONL 是两棵树」卡把 `共用 DailyJsonlWriter` 改为 `共用 pkg/jsonl（writer 按天 append + sidecar 旁车索引）`；「溯源链」卡 +1 条（加旁车索引后物理行号口径不变，`read_line` 仍按第 N 行回读）；「四层对齐人类认知」卡 +1 条（Short-term 同秒并列按 `rowid` 插入顺序兜底，历史哈希 ID 与 UUIDv7 都成立）。组件 / 连线不变（13 / 16），卡片 5 张、items 3→4 / 4→5。

声明值当场实测（2026-11-17 19:4x，HEAD `1664ec22`）：`src/service/domain/runtime/think_trace.rs` **677**（main 基线 759 → 本批 −82）；`src/pkg/tool_tracing/logger.rs` **185**（main 240）；`src/service/dao/memory/sqlite/trace.rs` **247**（main 359）；`src/pkg/jsonl/` **4 文件 / 891 行**（`writer.rs` 273 / `sidecar.rs` 355 / `mod.rs` 15 / `writer_test.rs` 248）；三个适配层 `think_trace_sidecar.rs` **191** / `tool_tracing/sidecar_index.rs` **161** / `dao/memory/sqlite/trace_sidecar.rs` **197**；`dao/cortex` **3,795 行**（未变）、`dao/cerebellum` **519 行**（未变）、`think_loop.rs` **866**（未变）、`think_round/` **2,298**（未变）、`pkg/think_path/` **2,499**（未变）、`think_trace_query.rs` **321**（未变）、`inbox.rs` **368**（未变）、前端 pages **62** / components **63**（均未变）、覆盖率门槛 PR **38%** / main **45%**（`.github/workflows/rust.yml` 的 `FAIL_UNDER`，未变）。`src/pkg/daily_jsonl.rs` 已重命名为 `src/pkg/jsonl/writer.rs`（rename-aware 记一条 R059），旧路径不再存在。

**只改卡片与源锚点、不改组件与连线**是本轮的总体取舍：这批改动改的是「同一组件内部的 IO 方式」与「ID 生成实现」，不是「有哪些组件、谁调谁」。把 `sidecar` 做成新组件会撞 `layout/component-overlap`，且会让图回答「JSONL 怎么写」这种索引层细节——按「门禁红就降粒度，不放宽门禁」处理。

未做人眼视觉复核，仅通过 archify 四道门禁（validate / deliver / check / browser-check 全 pass、diagnostics 0）。

## 增量同步记录

### 2026-11-19 增量同步（4 张重渲染，revision → `715ee557`）

**判定口径**：`archify-source-evidence-data` 的 `nodes{}` 引用路径清单 × **PR 全量 diff 路径**（`git diff --name-only origin/main...HEAD`，158 条）求交得 per-图 touched 数。旧判定非恒真，本表当场重算。

| 图 | touched | 判定 |
|---|---|---|
| think-path | **12** | **真需刷**（命中覆盖唤醒 / think_loop / think_round / trace / query / tool_execution 等本图主干组件）|
| messaging | **5** | **真需刷**（`resource_resolver` 不在 sources 里但 `delivery.rs` + `chat.rs` 命中；本批新增出站媒体分类属本图承载域）|
| architecture | **5** | **真需刷**（`traces` 组件 source 指向的 `src/pkg/think_trace.rs` **已不存在**，文件已迁到 `src/service/domain/runtime/`；「图里点名的东西现在没了」是最硬的需更信号）|
| stats | **4** | **真需刷**（`aop_stats_collector.rs` 与 `system/aop_stats.rs` 两个 source 文件**都已不存在**）|
| aop | 3 | 不刷（命中只有 `consumer/mod.rs` 删一行 mod 声明、`lib.rs` 换一行调用、`aop_stats_hook.rs` 内部字段；AOP 抽象层「六 Trait + Registry」一条没变，属机械适配）|
| tools | 4 | 不刷（命中是 `internal_intent` 附注与 readiness 缓存位置迁移，工具注册面与三层调用链未变）|
| task-project | 5 | 不刷（`transfer_owner` 只是把「目标 Agent 是否存在」从 domain 挪到 handler 校验，项目/任务状态机与 DAG 未变）|
| frontend | 3 | 不刷（命中 `chat.rs` 的产物 chip 预览与 `lib.rs`/`router.rs` 的路由注册，页面结构与组件树未变）|
| tool-context | 2 | 不刷（`tool_registry/mod.rs` 只多一行 `pub mod readiness_cache`、`tool_execution.rs` 改的是工具就绪缓存来源；身份注入四层链路一条没变）|
| infra | 1 | 不刷（只有 `lib.rs` 的 AOP 注入写法变化，配置/路径/存储初始化未变）|
| memory | 2 | 不刷（`models/prompt_builder.rs` 只把 `IntentAnalysis` 的 use 路径改到 models、`think_loop.rs` 是轨迹守卫；四层记忆与两条 JSONL 结构未变）|
| org-federation | 1 | 不刷（只有 `router.rs` 一行路由注册）|
| testing | 1 | 不刷（只有 `builtin_tests.rs` 加了 `internal_intent` 保留前缀护栏）|
| a2a / data-model / ontology / search | 0 | 不需更（sources 与本批 158 条 diff 路径交集为空）|

**四张改动摘要**：

- **think-path**：`trace` 组件 sources 从已不存在的 `src/pkg/think_trace.rs` 换成 `src/service/domain/runtime/think_trace.rs`，副标 `677 行` → **`926 行 · 两段式落盘 + 旁车索引 + 收尾守卫`**；`think_loop` 副标 `866 行` → **`889 行 · 三层预算 + 跨周期收集`**；`think_round` 副标 `2,298 行` → **`2,357 行`**；新增第 12 张卡「收尾守卫与跨周期收集（2026-11-19 新增）」5 条。组件 / 连线 / 视图数不变（16 / 16 / 4），卡片 11 → **12**。
- **messaging**：`attach` 组件 sources 的 `markdown_protocol/mod.rs` 换成 `src/models/outbound_media.rs`（出站媒体分类的落点），label 改「附件与媒体链路」、副标补「出站分类」；`delivery` 副标补「超限跳过 + 正文提示」；`meta.subtitle` 插入「出站媒体分类（MIME 单点判定 / 超限跳过）」；新增第 10 张卡「出站附件：MIME 单点判定 + 超限跳过（2026-11-19 新增）」5 条。组件 / 连线 / 视图数不变（19 / 21 / 4），卡片 9 → **10**。
- **stats**：`aopc` sources 从已不存在的 `src/consumer/aop_stats_collector.rs` 换成 `src/pkg/stats/runtime/aop.rs`；`dom` 组件改名为「AOP 统计聚合」，sources 从已不存在的 `src/service/domain/system/aop_stats.rs` 换成 `src/pkg/stats/runtime/mod.rs`，副标「全局单例」；`api` 副标补「直读单例」；`meta.subtitle` 插入「采集器落 pkg/stats/runtime（handler 直读全局单例）」；新增第 5 张卡「采集器归属（2026-11-19 下沉）」3 条。组件 / 连线 / 视图数不变（14 / 15 / 4），卡片 4 → **5**。
- **architecture**：`traces` 组件 sources 的 `src/pkg/think_trace.rs` 换成 `src/service/domain/runtime/think_trace.rs`；「严格单向分层（红线）」卡补第 5 条（2026-11-19 再归零四处反向依赖）。组件 / 连线 / 卡片 / 视图数全不变（17 / 18 / 6 / 3）。

**声明值当场实测（2026-11-19 18:0x，HEAD `715ee557`）**：

| 声明项 | 实测 | 对照 |
|---|---|---|
| `src/service/domain/runtime/think_trace.rs` | **926** | 上一轮 677（本批 +249）|
| `src/service/domain/runtime/think_trace_tests.rs` | **1,017** | — |
| `src/service/domain/runtime/think_trace_sidecar.rs` | **191** | 未变 |
| `src/service/domain/runtime/think_loop.rs` | **889** | 上一轮 866（+23）|
| `src/service/domain/runtime/think_round/` | **2,357** | 上一轮 2,298（+59）|
| `src/pkg/think_path/` | **2,499** | 未变 |
| `src/service/domain/runtime/think_trace_query.rs` | **321** | 未变 |
| `src/service/domain/runtime/inbox.rs` | **368** | 未变 |
| `src/service/domain/runtime/collected.rs` | **215** | 本批新增 |
| `src/service/domain/runtime/tool_execution.rs` | **560** | — |
| `src/service/domain/message/resource_resolver.rs` | **439** | — |
| `src/service/domain/message/resource_refs.rs` | **141** | 本批新增 |
| `src/models/outbound_media.rs` | **187** | 本批新增 |
| `src/service/dao/lark/http.rs` | **1,276** | — |
| `src/service/dao/wechat/ilink.rs` | **2,211** | — |
| `common/src/llm_schema.rs` | **708** | — |
| `dao/cortex` / `dao/cerebellum` | **3,795** / **519** | 实测未变 |
| 覆盖率门槛（`.github/workflows/rust.yml` 的 `FAIL_UNDER`） | PR **38%** / main **45%** | 实测未变 |

**三个旧路径已确认不存在**（`test -e` 实测 GONE）：`src/pkg/think_trace.rs`、`src/consumer/aop_stats_collector.rs`、`src/service/domain/system/aop_stats.rs`。这是「图里点名的组件已不存在」类漂移，属最硬需更信号，与 touched 计数无关也要刷。

**取舍说明**：本批的核心变化是四处**层迁移**与三处**新增能力**，但没有出现新的架构层或新的组件类别 —— `RoundCollected` / `TraceFinishGuard` / `OutboundMediaKind` 都是既有组件内部的能力，故只改 sources + 副标 + 卡片，**不加新组件、不改连线拓扑**。把 `sidecar` / `collected` 做成独立组件会撞 `layout/component-overlap`，按「门禁红就降粒度，不放宽门禁」处理。

**门禁（串行单进程）**：四张均 `validate` exit 0 → `finalize --browser-check` exit 0（gates 四道全 pass、diagnostics 0）→ `check` exit 0。产物字节数：think-path **874,529 B** / stats **857,263 B** / messaging **878,876 B** / architecture **867,872 B**。`docs/diagrams/*.json` 实测 **0**（回执未入仓）。

**未做人眼视觉复核**，仅通过 archify 四道门禁。

### 2026-11-20 增量同步（1 张重渲染：think-path，revision → `251c8462`）

**判定口径**：17 张图逐一取 `archify-source-evidence-data` 的 `nodes{}` 引用路径清单 × **PR #15 全量 diff 路径**（`git diff --name-status --no-renames origin/main...HEAD`，11 条，含 rename 两侧）求交。

**结果：17 张全部 touched = 0**。PR #15（feishu-md-tag-outbound）只动了 `common/src/markdown_render/`、`src/service/dao/lark/{http,mod,outbound_render}.rs`、`src/service/domain/message/resource_resolver.rs` 与 3 条 docs 索引文件——**没有任何一张图的 evidence 路径指向这些文件**，轨迹图 / 消息图 / 总图描述的结构与链路一条都没变。

**为什么仍然重渲染 think-path 这一张**：touched 计数只回答「路径有没有被改」，回答不了「图里写的那句话还对不对」。本图第 12 张卡（收尾守卫与跨周期收集）第 5 条写的是「工具调用附注 `_intent`」——而 PR #14（`188f0c0c`，已合入 main）已把该 wire 键改名为 `internal_intent`，`common/src/llm_schema.rs` 的 `CALL_INTENT_KEY` 只认新名。**这是内容层腐坏，不是路径层腐坏**：图引用路径全在、门禁全过、图照常渲染，但图上写的键名在现网已不存在。

**本张改动（只改这一处 + 血缘）**：

- `meta.repository.revision`：`715ee557…` → **`251c846200b64e66f0702f274078c3c6ab02683c`**（PR #15 head，父提交 `188f0c0c` = `origin/main`；合并后将成为该 PR 合并提交的祖先）。
- 第 12 张卡第 5 条：`_intent` → **`internal_intent`（2026-11-20 由 `_intent` 改名）**，并补一句改因——模型会把 `_` 前缀泛化成「业务字段也该加下划线」（`query` → `_query`），被 `additionalProperties: false` 拒绝。这条改因不是背景故事：它正是「为什么这个键名会变」的唯一解释，删掉它下次还会有人写回 `_intent`。
- **组件 / 连线 / 卡片 / 视图数全不变（16 / 16 / 12 / 4）**；组件副标里的行数声明当场复测，全部**未变**（`think_trace.rs` 926 / `think_loop.rs` 889 / `think_round/` 2,357 / `pkg/think_path/` 2,499 / `think_trace_query.rs` 321 / `inbox.rs` 368 / `dao/cerebellum` 519 / `dao/cortex` 3,795）——PR #15 没碰这些文件，与「未变」的预期一致。

**入口三处核对结果（本次只有 README 一处需要落记录）**：

- `docs/index.html`：think-path 的 desc 是「一轮思考 · 两阶段唤醒 · 轨迹链路（收尾守卫 + 跨周期收集）· 认知分层」——**不含键名**，无需改。
- `docs/ARCHITECTURE.md`：think-path 的图清单行讲的是「两阶段唤醒 → 快判断 → 输入注入 → 轮次 → 相位 → 两段式落盘 → 查询 → 前端」，**同样不含键名**，无需改。
- `docs/diagrams/README.md`：本节即本次唯一新增内容；「二、清单」里 think-path 那一行也不含键名，未动。

**门禁（串行单进程）**：`validate` exit 0（9 项 artifact 检查，composition showcase 0 error / 0 warning）→ `finalize --browser-check` exit 0（gates 四道全 pass、diagnostics 0、`browserCheck.ran=true`）→ `check` exit 0（diagnostics 0）。产物 **874,718 B**，sha256 前 16 位 `bb6585d34cf0479a`。`docs/diagrams/*.json` 实测 **0**（回执未入仓）。

**未做人眼视觉复核**，仅通过 archify 四道门禁；`visualReviewRecommendation` 仍为 advisory（resolvedCrossovers 3 / routesOverSuggestedBends 5 / routesOverSuggestedStretch 2），与上一版一致——本次没有动任何坐标。

### 2026-11-21 视觉可读性专项（线条交叉梳理 · 1 张重排：think-path，revision 不变 `251c8462`）

**起因**：用户审图时指出「线有点交叉」「有的线离得很近却要打弯」，要求以「整体通畅、消除打结」为目标重排。本次**只动几何与一处标签长度，不动任何语义**：16 组件 / 16 连线 / 12 卡片 / 4 视图全数保留，sources 与 revision 全部未改。

**根因（量化，不是观感）**：旧版 4 行布局里 `y=300` 一行挤了 6 个组件、行内间隙仅 30px（产线口径每行 ≤4、间隙 ≥110px），`think_loop` 一个节点同时挂 5 条出边，多条边只能钻进同一条 30px 窄走廊再折返——archify 自动 advisory 报 **3 处交叉 + 5 条 3 弯道**（`resolvedCrossovers=3 / routesOverSuggestedBends=5 / routesOverSuggestedStretch=2`）。

**改法（分区重排，正是用户提的「拆上下两段、中间连通」）**：改为 **5 列 × 5 行蛇形主干**——上段①`cerebellum` / ②`trigger → awakening → cerebellum_router → cortex`；中段③`inbox → think_loop → think_round → think_path`；下段④`consumer → trace → tools → frontend`；尾段⑤`jsonl → query → handlers`。每条主干边都落在相邻格（同排向右、同列向下），回边与分支走空列/空行，因此不再有交叉。

**可达性代价与取舍（如实披露）**：
- 画布由 1320×812 变 **1480×852**；`composition/desktop-readability` 走 `recognized-declared-wide`（1440 视口下投影 6.28px ≥ 硬底 6px，`hardFloorMet=true`）。**仍未达 requestedTarget 7.5px**，与旧版同属「投影够读但不宽裕」，未靠缩小字号换交叉。
- 一处连线标签由 `索引过滤 + seek 命中行` 缩为 **`索引过滤 + seek`**：该边两端是相邻格、水平间隙 110px，标签需 132px 才不压线。语义未变（`jsonl → query` 仍是「先按索引过滤、再按字节偏移 seek」）。README「二、清单」里那句链路描述讲的是行为而非标签原文，未改。
- 其余标签、卡片文案、视图 note 一字未动。

**门禁（串行单进程，`--out-dir` 逐轮换新）**：`finalize architecture … --repo-root . --quality showcase --json` → **exit 0**，四道全 pass（validate / deliver / check / browser-check，`browserCheck.ran=true`），`diagnostics` 0。composition 指标：**`properCrossings=0` / `resolvedCrossovers=0` / `ambiguousCorridors=0` / `arrowheadCollisions=0` / `maxBends=2`（建议上限 2）/ `routesOverSuggestedBends=0` / `maxStretch=1`（上限 1.35）/ `routesOverSuggestedStretch=0` / `labelRouteClearanceIssues=0`（最小余量 21.1px）**。产物 **872,619 B**，sha256 前 16 位 `367987dceebfa082`。`docs/diagrams/*.json` 实测 **0**（回执未入仓）。

**前后对比（同一把尺子，均为 archify 自动度量）**：`resolvedCrossovers` **3 → 0**；`routesOverSuggestedBends` **5 → 0**；`routesOverSuggestedStretch` **2 → 0**。

**未做人眼视觉复核**：以上全部是 archify 的自动几何度量，`visualReview` 仍为 `not-requested`。是否真的「好看」需人眼确认——工具只能保证「不交叉、不过弯、标签不压线、字号过底」。

### 2026-11-21 视觉可读性专项（线条交叉梳理 · 第 2 张重排：tools，revision 由孤儿锚点 `ef25e173` 改为已推送 `b5eac4ee`）

**起因**：用户在同专项里继续指图——「工具与技能子系统 · 三层调用与授权」同样线条挤在窄走廊里互相跨绕。本次**只动几何**，语义零变更：16 组件 / 16 连线 / 5 卡片 / 3 视图全数保留，sources 全部未改。

**根因（量化）**：旧版 3 行布局，`y=80` 与 `y=240` 两行各挤 **5 个组件**、行内间隙仅 **30px**（产线口径每行 ≤4、间隙 ≥110px），`domain` / `dal` / `policy` 三个枢纽的多条出边只能挤进同一走廊折返。旧版 advisory：`resolvedCrossovers=14 / routesOverSuggestedBends=11 / maxBends=5 / minLabelRouteClearance=5px`。

**改法**：改为 **4 列 × 4 行网格**（列 x=[20,366,712,1058]、行 y=[90,264,438,612]，卡片 180×64、行内间隙 166px）：授权面 `policy → fusion → gate → authdomain` 独占首行；主调用链 `think → domain → credentials → registry` 占第二行；`event → dal → load → skills` 第三行；三协议执行 `builtin → httptool → mcp → mcpserver` 尾行。每条边都落在相邻格，回边走空走廊。

**一并修正的声明值（如实披露）**：`meta.repository.revision` 原为 `ef25e173…`——实测该 commit **既不在 origin/main 也不在任何远端分支**（`git branch -r --contains` 零命中），属孤儿锚点；本次改为当前分支已推送 HEAD `b5eac4ee07688998ec9eabf198a3e85ec560e9fd`。组件 `sources` 路径逐条 `os.path.exists` 全部存在（`MISSING_PATHS: []`）。

**门禁（串行单进程，`--out-dir` 每轮换新）**：`finalize architecture … --repo-root . --quality showcase --json` → **exit 0**，四道全 pass（validate / deliver / check / browser-check，`browserCheck.ran=true`），`diagnostics` 0。composition 指标：**`properCrossings=0` / `resolvedCrossovers=0` / `ambiguousCorridors=0` / `arrowheadCollisions=0` / `maxBends=2` / `routesOverSuggestedBends=0` / `maxStretch=1` / `routesOverSuggestedStretch=0` / `labelRouteClearanceIssues=0`（最小余量 13.2px）/ `desktopReadabilityIssues=0`（投影字号 7.5px）**。产物 **862,813 B**，sha256 前 16 位 `f13d94d813f529e3`；画布 `1299×744 → 1278×744`。`docs/diagrams/*.json` 实测 **0**（回执未入仓）。

**前后对比（同一把尺子）**：`resolvedCrossovers` **14 → 0**；`routesOverSuggestedBends` **11 → 0**；`maxBends` **5 → 2**；`minLabelRouteClearance` **5px → 13.2px**。

**入口三处核对**：`docs/index.html` 与 `docs/ARCHITECTURE.md` 里 tools 的 desc 讲的是「三层调用 / 策略与授权 / 注册与装配」，**组件数 / 视图数 / 卡片数均未变**，故两处无需改动；「二、清单」那一行同样不含会漂移的数字，未动。本段是本次唯一新增的文档内容。

**未做人眼视觉复核**：以上全部是 archify 的自动几何度量，`visualReview` 仍为 `not-requested`。工具只能保证「不交叉、不过弯、标签不压线、字号过底」，是否真的好看需人眼确认。

### 2026-11-21 视觉可读性专项（线条交叉梳理 · 第 3 张重排：aop，revision 由 `25cced8f` 改为已推送 `b5eac4ee`）

**起因**：用户继续指图——「异步基建 · AOP 生产消费事件中心」同样线条交叉、近距离打弯。本次**只动几何**，语义零变更：14 组件 / 14 连线 / 5 卡片 / 3 视图全数保留，sources 全部未改（中文标签集合差集验证：`new_only=[]` / `old_only=[]` / `common=69`）。

**根因（与第 1、2 张不同，值得单独记一笔）**：旧版并**不是**「一行挤太多」——它的行内间隙合规（每行最多 4 个组件、间隙 110~120px）。真因是**摆位与连边不匹配**：`carrier` / `boot` 放在 y=400 最左，却要连到 y=80 的 `registry`（跨 3 行 + 横跨 2 格）；`cron` / `channel` 在 y=400 右侧、`a2a` 在 y=560 最左，却都要绕回 y=80 的 `publish`。这类「跨半张画布的长回边」只能在密集区域找缝隙穿过，于是出现折返与近距离打弯。旧版 advisory：`resolvedCrossovers=13 / routesOverSuggestedBends=7 / maxBends=4 / maxStretch=1.39 / minLabelRouteClearance=4.8px`。

**改法**：保持 4 列 × 4 行网格（列 x=[20,340,660,960]、行 y=[70,230,390,550]），把节点按**连边邻接关系**重排，使**每一条边最多只跨一个格距**（14 条边全部满足：8 条同格距正投影、6 条对角相邻）：
- 主链横排在 y=230 一行：`channel → publish → registry → queue`，`queue` 竖直下到 y=390 的 `scheduler`，再左回 `consumer`，下到 y=550 的 `domain`；
- 生产者在 `publish` 的正上 / 左上：`cron`（正上）、`a2a`（左上对角）、`channel`（正左）——三条 tick 边都只走一格；
- `events` / `boot` / `meta` / `carrier` / `stats` 各自贴住自己的目标：`events` 正上 `registry`、`boot` 对角邻 `registry`、`meta` 对角邻 `consumer`、`carrier` 横邻 `consumer` 且对角邻 `registry`、`stats` 正下 `consumer`。
- 唯一显式端口：`consumer → domain` 指定 `fromSide=bottom / toSide=top`（不加这一条时该边走 3 弯道 1.178 倍伸展；加上后降到 2 弯道、1.0 倍）。其余 13 条全部交自动布线。

**一并修正的声明值（如实披露）**：`meta.repository.revision` 原为 `25cced8f…`——实测它**不在 origin/main、也不在本分支**（只在 `origin/feat/aman-local-changes-20261114` 上，PR #9 的历史分支），属于对读者无意义的锚点；本次改为本分支已推送 HEAD `b5eac4ee07688998ec9eabf198a3e85ec560e9fd`，与「第 2 张」采用同一锚点。

**门禁（`--out-dir` 每轮换新）**：`finalize architecture … --repo-root . --quality showcase --json --browser-check` → **exit 0**，四道全 pass（validate / deliver / check / browser-check，`browserCheck.ran=true`），`diagnostics` 0。composition 指标：**`properCrossings=0` / `resolvedCrossovers=0` / `ambiguousCorridors=0` / `arrowheadCollisions=0` / `maxBends=2` / `routesOverSuggestedBends=0` / `maxStretch=1` / `routesOverSuggestedStretch=0` / `labelRouteClearanceIssues=0`（最小余量 17px）/ `desktopReadabilityIssues=0`（投影字号 7.5px）/ `microSegmentCount=0`**，且 `routeReview.crossings` 与 `routeReview.detours` **双双为空**。产物 **857,603 B**，sha256 `3c72432263b56437b8583e7d1f6e67643a5086805cd5a64d3e7c2487cbfc366e`；画布 `1148×692 → 1190×682`（高度反而少 10px）。`docs/diagrams/*.json` 实测 **0**（回执未入仓）。

**前后对比（同一把尺子）**：`resolvedCrossovers` **13 → 0**；`routesOverSuggestedBends` **7 → 0**；`maxBends` **4 → 2**；`maxStretch` **1.39 → 1.00**；`minLabelRouteClearance` **4.8px → 17px**。

**入口三处核对**：`docs/index.html` 与 `docs/ARCHITECTURE.md` 里 aop 的描述讲的是「框架零业务 / 六 Trait / Registry」与视图名（一次事件流转 / 启动顺序 / 生产者与兜底），**组件数 / 视图数 / 卡片数均未变**，两处无需改动；「二、清单」那一行也不含会漂移的数字，未动。本段是本次唯一新增的文档内容。

**未做人眼视觉复核**：以上全部是 archify 的自动几何度量，`visualReview` 仍为 `not-requested`。0 交叉 0 超限只说明「几何不违规」，是否真的好看需人眼确认。

## 五、维护约定

1. 新增图前先判断抽象层级；同层级才允许并列。
2. 图内文字用中文，代码锚点用仓库相对路径（不放行号）。
3. 改动后跑 `./scripts/check.sh docs-lint`（只扫 Markdown，不会校验 HTML，但会拦住绝对路径与旧式行号引用）。
4. 生成回执（`*.delivery.json`）不提交，只保留最终 HTML。
