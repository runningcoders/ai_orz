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
| [ai-orz-think-path.html](./ai-orz-think-path.html) | 组件级 | 两阶段唤醒 → 小脑决策契约（entry_gate / entry_pool / chain_reply, close_default）→ 思考中注入新输入 → 三层预算循环 → 七相位图引擎 → 两段式落盘（增量 + 收尾）→ 前端时间线；含思考强度判相对偏移、先验与兜底分离 | 一轮思考 / 两阶段唤醒 / 轨迹链路 / 认知分层 | 02-Agent运行时 |
| [ai-orz-tools.html](./ai-orz-tools.html) | 组件级 | CoreTool 三协议路由（Builtin / HTTP / MCP）+ 工具级凭据契约与身份注入 + 策略拦截与六态授权 + 技能包装配 | 三层调用 / 策略与授权 / 注册与装配 | 06-工具与技能 |
| [ai-orz-tool-context.html](./ai-orz-tool-context.html) | 组件级 | 工具调用中的上下文（身份）数据流转：声明（`CredentialKind` 的 `UserInfo` / `AgentInfo` 两个非凭据变体 + `field` 白名单 + `binding` 四注入点）→ 解析（`is_identity` 二元分派 + 未命中按来源分叉）→ 注入（`check` 注入 + `base_env` 九键末位硬 insert）→ 使用（git 子进程 4 个 `GIT_*` / commit-msg hook） | 四层主链路 / 失败语义分叉 / 注入与优先级 / 落点实证 | 06-工具与技能 |
| [ai-orz-aop.html](./ai-orz-aop.html) | 组件级 | pkg/aop 六 Trait + Registry 单例 + 7 类业务消费者 + 3 类 tick 生产者 + context_carrier 贯穿 | 一次事件流转 / 启动顺序 / 生产者与兜底 | 10-异步基建(AOP) |
| [ai-orz-memory.html](./ai-orz-memory.html) | 组件级 | 四层记忆（Core / Working / Short / Long）+ 两条 JSONL + 三路召回 + 短期记忆索引分层 + 睡眠沉淀 | 四层记忆 / 存储面 / 检索与沉淀 | 03-记忆系统 |
| [ai-orz-ontology.html](./ai-orz-ontology.html) | 组件级 | 受控词表 TBox + 属性定义与门闩 + 骨架三元组 + 漂移与有效性治理（口径与现网 `get_ontology_drift_dashboard` / `get_property_effectiveness` 同源） | 读侧链路 / 写入门闩 / 治理面 | 00-总览与架构 |
| [ai-orz-org-federation.html](./ai-orz-org-federation.html) | 组件级 | 多级组织 + UserRole 并查集继承 + JWT 双模式 + 身份凭证 + 配对码建联与跨组织签名 | 组织与角色 / 认证与凭证 / 联邦组网 | 01-组织与权限 |
| [ai-orz-task-project.html](./ai-orz-task-project.html) | 组件级 | TaskStatus 四态 + progress 联动 + dependencies DAG + execution_plan / result 增量 + Project 五字段注入 | 任务状态机 / DAG 与唤醒 / 项目聚合 | 05-任务与项目 |
| [ai-orz-messaging.html](./ai-orz-messaging.html) | 组件级 | 渠道入站归一化 → 路由策略（含回复仲裁与协作压力）→ MessageDomain（builder / delivery）→ 附件字节流与正文引用 → DAL/DAO → SSE 与唤醒 | 入站与路由 / 编排与投递 / 存储与唤醒 | 04-对话与消息 |
| [ai-orz-a2a.html](./ai-orz-a2a.html) | 组件级 | Agent Card + JSON-RPC 四方法 + 协议映射与回调 + 30s 轮询兜底 + 联邦签名 | 协议入口 / 结果与兜底 / 联邦与长连接 | 07-外部Agent(A2A) |
| [ai-orz-search.html](./ai-orz-search.html) | 组件级 | FTS5 全文 + 向量检索 + 图谱遍历三路召回 + 属性精确过滤 + `ids` 直取与 `include_retired` / `include_history` 受控出口 | 工具入口 / 三路召回 / 存储面 | 08-搜索 |
| [ai-orz-stats.html](./ai-orz-stats.html) | 组件级 | 业务事件 → AOP 同步消费 → 四类指标落库（含按时序桶的耗时统计）→ 监控 API 与看板 | 采集 / 聚合 / 读取与看板 | 09-统计与监控 |
| [ai-orz-frontend.html](./ai-orz-frontend.html) | 组件级 | Dioxus WASM 入口 / 路由 / 页面 / HUD / API 客户端 + 产物预览（HtmlPreview 唯一渲染出口） + build.rs 产物 | 应用骨架 / 页面与组件 / 数据与产物 | 11-前端应用 |
| [ai-orz-infra.html](./ai-orz-infra.html) | 组件级 | 配置 → 连接池与迁移 → 路径与 JSONL → 向量重建三档 → 后台任务 → 打包发布 | 启动与配置 / 运行期资源 / 构建与发布 | 12-基础设施与部署 |
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

## 五、维护约定

1. 新增图前先判断抽象层级；同层级才允许并列。
2. 图内文字用中文，代码锚点用仓库相对路径（不放行号）。
3. 改动后跑 `./scripts/check.sh docs-lint`（只扫 Markdown，不会校验 HTML，但会拦住绝对路径与旧式行号引用）。
4. 生成回执（`*.delivery.json`）不提交，只保留最终 HTML。
