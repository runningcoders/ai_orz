# docs/diagrams · 架构图目录

本目录存放**自包含 HTML 架构图**：单个文件内含全部样式、脚本与数据，克隆仓库后本地双击即可交互，也可在 GitHub Pages 落地页 / 前端文档中心在线渲染。

> 更新方式：重新生成后**覆盖同名文件**即可；不要引用外部 CDN、不要把图拆成多文件。

---

## 一、一张图一个抽象层级

- **总图**只回答「系统里有哪些容器、请求怎么流、细节去哪看」（容器级）。
- **专题图**回答「这个子系统的能力线、链路过哪几层、口径是多少」（组件级 + 时序混合）。
- 不把子系统塞进总图：总图连 8 个业务域都只用一行字概括，单拆某一个会破坏层级一致性。

## 二、清单（16 张）

| 文件 | 层级 | 讲什么 | 视图 | 对应知识库模块 |
|---|---|---|---|---|
| [ai-orz-architecture.html](./ai-orz-architecture.html) | 容器级总图 | 三级 workspace、四层单向调用、存储与外部系统、认知架构分层 | 主请求路径 / 异步与 Agent 运行时 / 持久化 | 00-总览与架构 |
| [ai-orz-think-path.html](./ai-orz-think-path.html) | 组件级 | 两阶段唤醒 → 小脑快判断 → 思考中注入新输入 → 三层预算循环 → 七相位图引擎 → 两段式落盘（增量 + 收尾）→ 前端时间线 | 一轮思考 / 两阶段唤醒 / 轨迹链路 / 认知分层 | 02-Agent运行时 |
| [ai-orz-tools.html](./ai-orz-tools.html) | 组件级 | CoreTool 三协议路由（Builtin / HTTP / MCP）+ 凭据编排 + 策略拦截与六态授权 + 技能包装配 | 三层调用 / 策略与授权 / 注册与装配 | 06-工具与技能 |
| [ai-orz-aop.html](./ai-orz-aop.html) | 组件级 | pkg/aop 六 Trait + Registry 单例 + 7 类业务消费者 + 3 类 tick 生产者 + context_carrier 贯穿 | 一次事件流转 / 启动顺序 / 生产者与兜底 | 10-异步基建(AOP) |
| [ai-orz-memory.html](./ai-orz-memory.html) | 组件级 | 四层记忆（Core / Working / Short / Long）+ 两条 JSONL + 三路召回 + 睡眠沉淀 | 四层记忆 / 存储面 / 检索与沉淀 | 03-记忆系统 |
| [ai-orz-ontology.html](./ai-orz-ontology.html) | 组件级 | 受控词表 TBox + 属性定义与门闩 + 骨架三元组 + 漂移与有效性治理 | 读侧链路 / 写入门闩 / 治理面 | 00-总览与架构 |
| [ai-orz-org-federation.html](./ai-orz-org-federation.html) | 组件级 | 多级组织 + UserRole 并查集继承 + JWT 双模式 + 身份凭证 + 配对码建联与跨组织签名 | 组织与角色 / 认证与凭证 / 联邦组网 | 01-组织与权限 |
| [ai-orz-task-project.html](./ai-orz-task-project.html) | 组件级 | TaskStatus 四态 + progress 联动 + dependencies DAG + execution_plan / result 增量 + Project 五字段注入 | 任务状态机 / DAG 与唤醒 / 项目聚合 | 05-任务与项目 |
| [ai-orz-messaging.html](./ai-orz-messaging.html) | 组件级 | 渠道入站归一化 → 路由策略 → MessageDomain（builder / delivery）→ 附件字节流与正文引用 → DAL/DAO → SSE 与唤醒 | 入站与路由 / 编排与投递 / 存储与唤醒 | 04-对话与消息 |
| [ai-orz-a2a.html](./ai-orz-a2a.html) | 组件级 | Agent Card + JSON-RPC 四方法 + 协议映射与回调 + 30s 轮询兜底 + 联邦签名 | 协议入口 / 结果与兜底 / 联邦与长连接 | 07-外部Agent(A2A) |
| [ai-orz-search.html](./ai-orz-search.html) | 组件级 | FTS5 全文 + 向量检索 + 图谱遍历三路召回 + 属性精确过滤 | 工具入口 / 三路召回 / 存储面 | 08-搜索 |
| [ai-orz-stats.html](./ai-orz-stats.html) | 组件级 | 业务事件 → AOP 同步消费 → 四类指标落库 → 监控 API 与看板 | 采集 / 聚合 / 读取与看板 | 09-统计与监控 |
| [ai-orz-frontend.html](./ai-orz-frontend.html) | 组件级 | Dioxus WASM 入口 / 路由 / 页面 / HUD / API 客户端 + build.rs 产物 | 应用骨架 / 页面与组件 / 数据与产物 | 11-前端应用 |
| [ai-orz-infra.html](./ai-orz-infra.html) | 组件级 | 配置 → 连接池与迁移 → 路径与 JSONL → 后台任务 → 打包发布 | 启动与配置 / 运行期资源 / 构建与发布 | 12-基础设施与部署 |
| [ai-orz-data-model.html](./ai-orz-data-model.html) | ER 图（表级） | 三族九表：身份 / 工作 / 知识图谱，字段取自现网 SQLite schema | 身份与组织 / 任务与消息 / 知识图谱三件套 | 13-数据模型 |
| [ai-orz-testing.html](./ai-orz-testing.html) | 组件级 | fmt / clippy / test / docs_lint / inline-test-lint 五道门禁 + 护栏用例 | 门禁组合 / 测试分层 / 文档与索引 | 14-测试 |

## 三、阅读顺序

1. **总图**先建立全局认知（3 分钟）。
2. **运行时五张**：思维轨迹（怎么想）→ 工具与技能（怎么动手）→ 异步基建（怎么解耦）→ 记忆（怎么记住）→ 搜索（怎么找得到）。
3. **业务与通道四张**：组织与权限（谁能做什么）→ 任务与项目（活怎么流转）→ 对话与消息（话怎么传）→ 外部 Agent A2A（怎么接外面）。
4. **数据与运行面三张**：本体（图谱的 TBox，写记忆前看）→ 数据模型 ER 图（表与字段）→ 统计与监控（数字从哪来）。
5. **工程面三张**：前端应用（界面怎么搭）→ 基础设施与部署（怎么跑起来、发出去）→ 测试与门禁（怎么保证不坏）。

## 四、选型理由与缺口

**为什么是这 16 张**：优先选「跨层链路长 + 有状态机/多协议 + 看图能明显省读文档」的模块。15 个能力域现已**全部有图**（00-总览与架构 对应总图 + 本体专题图）。

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

**已知局限**：ER 图只覆盖三族九表（核心主链路），`ontology_*` 词表族与日志/统计明细表未画；如需全库 ER 图，建议先固定一份 schema 快照再生成。
## 五、维护约定

1. 新增图前先判断抽象层级；同层级才允许并列。
2. 图内文字用中文，代码锚点用仓库相对路径（不放行号）。
3. 改动后跑 `./scripts/check.sh docs-lint`（只扫 Markdown，不会校验 HTML，但会拦住绝对路径与旧式行号引用）。
4. 生成回执（`*.delivery.json`）不提交，只保留最终 HTML。
