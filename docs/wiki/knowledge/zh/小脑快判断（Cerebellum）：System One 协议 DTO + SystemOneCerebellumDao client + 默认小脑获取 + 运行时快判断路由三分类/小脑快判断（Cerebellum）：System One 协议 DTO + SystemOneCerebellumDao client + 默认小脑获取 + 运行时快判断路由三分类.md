---
kind: wiki_knowledge_card
name: 小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类
category: 系统模型提供商 / Agent 思考运行时
scope:
  - "src/models/cerebellum_types.rs"
  - "src/service/dao/cerebellum/mod.rs"
  - "src/service/dao/cerebellum/client.rs"
  - "src/service/domain/runtime/cerebellum_router.rs"
  - "src/service/domain/runtime/think_loop.rs"
  - "src/service/dal/brain.rs"
  - "src/service/dao/model_provider/sqlite.rs"
  - "src/models/brain.rs"
  - "src/models/agent.rs"
  - "common/src/enums/provider.rs"
source_files:
  - src/models/cerebellum_types.rs#L23-L28 (协议边界常量：MAX_CHOICE_OPTIONS=255 / MIN_SCORE_LEVELS=2 / MAX_SCORE_LEVELS=10)
  - src/models/cerebellum_types.rs#L30-L110 (QuestionType noul/choice/score + QuestionCriteria untagged 双形态 + CerebellumQuestion::validate：类型×criteria 形态匹配 + 数量边界)
  - src/models/cerebellum_types.rs#L112-L202 (CerebellumRequest / CerebellumUsage / CerebellumAnswer 三型答案 + as_noul/as_choice/as_score + CerebellumResponse：answers 必填、usage 缺省容错零值 + ThinkFastResult)
  - src/service/dao/cerebellum/mod.rs#L24-L27 (DEFAULT_SYSTEM_ONE_BASE_URL="https://api.typesafe.ai" + DEFAULT_CEREBELLUM_TIMEOUT_MS=800)
  - src/service/dao/cerebellum/mod.rs#L29-L43 (CerebellumDao trait：think_fast(ctx, &ModelProviderPo, state, questions) -> ThinkFastResult；无状态、全配置随 PO 传入)
  - src/service/dao/cerebellum/mod.rs#L45-L87 (SystemOneCerebellumDao 单例 + OnceLock 管理 + dao()/init()；client 复用 pkg::http::presets::llm())
  - src/service/dao/cerebellum/client.rs#L23-L58 (resolve_timeout_ms：config.timeout_ms 优先、脏 config 兜底默认值；resolve_endpoint：空 api_key 硬错误 ConfigInvalid、base_url 缺省走默认端点、拼 /v1/systemone)
  - src/service/dao/cerebellum/client.rs#L60-L157 (think_fast：questions 非空 + 逐个 validate；响应缺 answers / answers 非对象 = 硬错误；单个答案解析失败容错跳过并告警；usage 缺省零值)
  - src/service/dao/cerebellum/client.rs#L159-L219 (transport_error timeout|connect|send 分类 + error_root_cause 沿 source 链取根因 + model_call_error 429/401|403/4xx/5xx 映射，与 cortex http.rs 同款口径)
  - src/service/domain/runtime/cerebellum_router.rs#L31-L46 (路由常量：CEREBELLUM_ROUTE_TIMEOUT_MS=800 / CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD=0.85 / ROUTE_QUESTION_ID="route" / trivial_direct 与 cortex_needed 两选项 / TRIVIAL_DIRECT_TEMPLATE / CORTEX_BOOST_TEMPLATE)
  - src/service/domain/runtime/cerebellum_router.rs#L50-L92 (DegradedReason 六值 + as_str() 观测口径 + RouteDecision 三分类 Direct/Inject/Skip)
  - src/service/domain/runtime/cerebellum_router.rs#L102-L234 (route 主流程：enabled=false → Skip{NoCerebellum} 零调用；无用户消息 → Skip{NoUserMessage}；state 只带轻量上下文；单 choice 问题 validate；tokio::time::timeout(800ms) 包裹 dal.think_fast；cerebellum_route 结构化日志三态)
  - src/service/domain/runtime/cerebellum_router.rs#L240-L293 (translate 纯函数三分类：缺答案/非 choice/未知选项 → Skip{UnsupportedAnswer}；confidence<0.85 → Skip{LowConfidence}；cortex_needed → Inject / trivial_direct → Direct；route_with_default_dao 生产入口 = dal::brain::dal())
  - src/service/domain/runtime/think_loop.rs#L288-L337 (run_think_loop 首轮前单点接线：brain.cerebellum=Some 时取 messages 中最后一条 User 文本 → route_with_default_dao；Direct+开关开 → 提前 return ThinkLoopResult::Final；Inject → messages.insert(0, ChatMessage::System)；Skip → 空操作)
  - src/service/dal/brain.rs#L211-L213 (wake_brain Local 分支经 dao::model_provider::get_default_cerebellum_provider 注入 brain.cerebellum；无启用记录 → None)
  - src/service/dal/brain.rs#L416-L426 (BrainDal::think_fast 透传 dao::cerebellum::dao() 单例)
  - src/service/dao/model_provider/sqlite.rs#L236-L275 (get_default_cerebellum_provider：capability=Decision + status=Normal + api_key 非空，同构 get_default_embedding_provider)
  - src/models/brain.rs#L1-L60 (Brain 新增 pub cerebellum: Option<ModelProviderPo>；new_local/new_external 显式 None，未唤醒即无小脑)
  - src/models/agent.rs#L82-L144 (AgentRuntimeConfig 新增 enable_cerebellum_route(serde default true) + cerebellum_trivial_direct(serde default false) 两开关)
  - common/src/enums/provider.rs#L32-L37 (ProviderType::Jev=8：尾部追加变体免 DB migration；Display "jev")
  - common/src/enums/provider.rs#L51-L55, L147-L150 (ModelCapability::Decision=2 + is_decision()；Display "decision")
  - docs/design/runtime_design.md (Agent 唤醒 + 工具二分整体设计 —— 小脑路由接线的宿主运行时设计)
  - docs/wiki/zh/content/功能模块/AI Agent 管理/小脑快判断（System One）.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/运行时领域.md
  - docs/wiki/zh/content/数据模型/数据模型.md
---

# 小脑快判断（Cerebellum）与 System One 协议

## §1 概述

**本卡角色**：AI Orz「System One 快判断」能力的全链路原子卡——覆盖 **协议 DTO（`src/models/cerebellum_types.rs`）→ System One client（`src/service/dao/cerebellum/`）→ 默认小脑获取（`get_default_cerebellum_provider`）→ Brain 注入（`src/service/dal/brain.rs`）→ 运行时快判断路由（`cerebellum_router.rs`）→ think_loop 单点接线** 六段。核心语义：在进入大脑（cortex）主循环之前，让小脑（jev / TypeSafe AI System One 决策模型）用一次亚秒级调用给出「琐碎直回 / 需完整思考」的路由结论，从而为简单请求省掉主循环开销。

- **协议维度**：jev 不兼容 OpenAI chat/completions，走独立 `POST {base}/v1/systemone`（Bearer 鉴权）。请求携带 `model` + `state`（任意 JSON 上下文）+ `questions`（qid → 类型化问题）；响应 `answers`（qid → 类型化答案）+ `usage`。三类问题/答案一一对应：`noul`（无选项，输出 0~1 概率，无 confidence）/ `choice`（≤255 选项，输出 choice + confidence + probabilities）/ `score`（2~10 级有序等级，输出 score + probabilities + legend + confidence）。
- **用途维度**：新增 `ProviderType::Jev = 8` 与 `ModelCapability::Decision = 2`。二者均为 `#[repr(i32)]` 尾部追加变体，免 DB migration（仓库既有模式）；`Decision` 与 `Agent`（对话思考）、`Embedding`（向量化）在调度与展示上按用途隔离。
- **路由维度**：`cerebellum_router::route` 只发**一个** choice 问题（`route`），两选项 `trivial_direct` / `cortex_needed`；`confidence >= 0.85` 才采纳，否则保守降级。三层降级（无启用小脑 / 调用失败或超时 / 低置信）全部收敛为「静默跳过 = 现状」。
- **回滚维度**：管理面停用默认小脑（把 Decision provider 置 Disabled）= 一键全局回滚，**零代码零重启**（`brain.cerebellum` 变 None 即断链）。

---

## §2 关键文件与职责表

| 文件 | 角色 | 关键内容 | 锚点 |
|------|------|---------|------|
| [src/models/cerebellum_types.rs](src/models/cerebellum_types.rs) | System One 协议 DTO（唯一事实源） | `QuestionType`/`QuestionCriteria`/`CerebellumQuestion{validate}`；`CerebellumRequest`/`CerebellumAnswer`/`CerebellumResponse`/`ThinkFastResult`/`CerebellumUsage`；三边界常量 | `:L23-L202` |
| [src/service/dao/cerebellum/mod.rs](src/service/dao/cerebellum/mod.rs) | 小脑 DAO 门面 | `CerebellumDao` trait + `SystemOneCerebellumDao` 单例 + `dao()/init()`；默认端点与默认超时常量 | `:L24-L87` |
| [src/service/dao/cerebellum/client.rs](src/service/dao/cerebellum/client.rs) | System One HTTP client | `resolve_endpoint`/`resolve_timeout_ms` + `think_fast` 编解码与结构校验 + 错误分类（与 cortex 同口径） | `:L23-L219` |
| [src/service/domain/runtime/cerebellum_router.rs](src/service/domain/runtime/cerebellum_router.rs) | 运行时快判断路由（B3） | 常量集中 + `DegradedReason`/`RouteDecision` + `route()` 主流程 + `translate()` 纯函数 + `route_with_default_dao()` | `:L31-L293` |
| [src/service/domain/runtime/think_loop.rs](src/service/domain/runtime/think_loop.rs) | 接线点（单点覆盖四场景） | `run_think_loop` 首轮前 `brain.cerebellum` 判空 → 路由 → Direct 短路 / Inject 插 System 头 / Skip 空操作 | `:L288-L337` |
| [src/service/dal/brain.rs](src/service/dal/brain.rs) | Brain 装配与透传 | `wake_brain` Local 分支注入 `brain.cerebellum`；`BrainDal::think_fast` 透传 cerebellum dao | `:L211-L213`, `:L416-L426` |
| [src/service/dao/model_provider/sqlite.rs](src/service/dao/model_provider/sqlite.rs) | 默认小脑获取 | `get_default_cerebellum_provider`：`capability=Decision + status=Normal + api_key 非空` | `:L236-L275` |
| [src/models/brain.rs](src/models/brain.rs) | Brain 实体扩展 | 新增 `pub cerebellum: Option<ModelProviderPo>`；构造器显式 None | `:L1-L60` |
| [src/models/agent.rs](src/models/agent.rs) | 运行时开关 | `AgentRuntimeConfig` 新增 `enable_cerebellum_route`(默认 true) / `cerebellum_trivial_direct`(默认 false) | `:L82-L144` |
| [common/src/enums/provider.rs](common/src/enums/provider.rs) | 枚举扩展 | `ProviderType::Jev=8` + `ModelCapability::Decision=2`（含 `is_decision()`） | `:L32-L37`, `:L51-L55`, `:L147-L150` |

---

## §3 架构约定与数据流

### 3.1 协议链路（编解码）

```
CerebellumQuestion{type, instructions, criteria?}
        │  validate(): noul 不带 criteria / choice ∈ [1,255] 选项对象 / score ∈ [2,10] 等级数组
        ▼
CerebellumRequest{model = provider.model_name, state, questions: {qid → question}}
        │  POST {base}/v1/systemone  (Bearer api_key, timeout = config.timeout_ms ?? 800ms)
        ▼
CerebellumResponse{model, answers: {qid → CerebellumAnswer}, usage?}
        │  结构校验：顶层缺 answers / answers 非对象 → 硬错误；单答案解析失败 → 容错跳过
        ▼
ThinkFastResult{answers, usage}
```

- **硬校验 vs 容错边界**：请求侧「类型×criteria 形态 + 数量边界」是硬校验（本地 validate，不合规直接 `ConfigInvalid`）；响应侧只硬校验「顶层 answers 存在且为对象」，**单个答案**解析失败走容错跳过并 `log_warn!`——原因是协议版本演进可能新增答案类型，不能让一个新类型拖垮整批结果。
- **端点解析**：`provider.base_url` 非空即优先（支持自建 relay），缺省走 `DEFAULT_SYSTEM_ONE_BASE_URL`；`api_key` 为空**硬错误**（空 key 发出去只会被服务端拒绝，与 cortex `validate_provider_for_request` 口径一致）。
- **超时**：`config.timeout_ms`（`ModelProviderConfig` 可选字段）优先，缺省/脏 config 兜底 `DEFAULT_CEREBELLUM_TIMEOUT_MS = 800`；路由层再用 `tokio::time::timeout(800ms)` 包一层，形成「双保险」。

### 3.2 默认小脑获取与 Brain 注入

- `get_default_cerebellum_provider` 与 embedding 版**同构**：查询 `capability = Decision AND status = Normal AND api_key 非空`，取第一条。语义即「status 单启用即默认」——多小脑模型可配置、随时切换零额外操作。
- `wake_brain` 在 Local 分支调用该查询并写入 `brain.cerebellum`；无启用记录则为 `None`。因此「有没有小脑」完全由**数据状态**决定，运行时无额外开关需要同步。
- `BrainDal::think_fast` 把调用透传给 `dao::cerebellum::dao()` 单例（快判断无状态、全配置随 `&ModelProviderPo` 传入），domain 层 `cerebellum_router` 只依赖 `BrainDal` trait（可注入 mock，见 §4）。

### 3.3 运行时路由三分类（B3）

| 输入 | 条件 | 输出决策 | think_loop 动作 |
|------|------|---------|----------------|
| 总开关关 / `cerebellum=None` | `enabled=false` | `Skip{NoCerebellum}` | 无操作（且**零调用**，即全局回滚开关） |
| 无用户消息 | `last_user_message=None` | `Skip{NoUserMessage}` | 无操作 |
| 调用超时（>800ms） | timeout | `Skip{Timeout}` | 无操作 |
| 调用失败（网络/协议/服务端） | Err | `Skip{CallError}` | 无操作 |
| 答案非 choice / 未知选项 | — | `Skip{UnsupportedAnswer}` | 无操作 |
| confidence < 0.85 | — | `Skip{LowConfidence}` | 无操作 |
| `cortex_needed` 且置信达标 | — | `Inject{system_prompt=CORTEX_BOOST_TEMPLATE, confidence}` | `messages.insert(0, ChatMessage::System{...})` 后进主循环 |
| `trivial_direct` 且置信达标 | — | `Direct{content=TRIVIAL_DIRECT_TEMPLATE, confidence}` | 仅当 `cerebellum_trivial_direct=true` 才提前 `return ThinkLoopResult::Final`；开关关则**既不直回也不注入**（保守口径） |

- **轻量上下文铁律**：`state` 只携带 `agent_id` / `agent_name` / `last_user_message` 三项，**不取全量记忆**、不取完整会话历史——保证小脑调用是"亚秒快判断"而非"第二个大脑"。
- **结果只增强不替代**：`Inject` 只是在消息序列头部插一条 System 增强提示，cortex 主循环照常执行；只有显式打开 `cerebellum_trivial_direct` 才会短路主循环。
- **常量集中**：阈值（0.85）、超时（800ms）、选项 id、模板文案全部集中在 `cerebellum_router.rs` 顶部——Spike 后集中调参「只调常量不动结构」。

### 3.4 开关默认值语义

| 开关 | 默认 | 语义 |
|------|------|------|
| `enable_cerebellum_route` | `true` | 有启用小脑即自动参与路由（`Inject` 增强，纯增益、失败即现状） |
| `cerebellum_trivial_direct` | `false` | TRIVIAL 直回**默认关闭**，需 Spike 实测 + 灰度后另行拍板放开 |

---

## §4 硬约束与回归红线

1. ❌ **禁止把 jev 走 OpenAI 兼容通道**：jev 不兼容 chat/completions，必须走 `dao/cerebellum` 的 `/v1/systemone`；违反 = 调用必然失败。
2. ❌ **禁止把「无启用小脑」当成错误抛出**：`brain.cerebellum=None` / 总开关关必须静默 `Skip{NoCerebellum}` 且**零网络调用**；违反 = 未配置小脑的环境整体不可用，且破坏「停用即回滚」的一键回滚语义。
3. ❌ **禁止路由失败/超时阻塞主循环**：三层降级（无小脑 / 调用失败或超时 / 低置信）必须全部收敛为「静默跳过 = 现状」；违反 = 小脑抖动导致主流程不可用。
4. ❌ **禁止在 state 里塞全量记忆或完整会话历史**：state 只允许轻量上下文（消息文本 + Agent 标识）；违反 = 小脑调用退化为第二个大脑，亚秒预算破裂。
5. ❌ **禁止小脑结论直接替代主循环**（除显式开关）：`Inject` 只能追加一条 System 增强提示；`Direct` 短路必须受 `cerebellum_trivial_direct` 开关约束，且该开关默认 false。
6. ✅ **协议边界校验必须前置本地执行**：`CerebellumQuestion::validate` 必须在发出请求前跑（choice ∈ [1,255] / score ∈ [2,10] / noul 不带 criteria）；违反 = 无效请求打到服务端。
7. ✅ **响应容错边界必须"顶层硬、逐项软"**：顶层缺 `answers` / `answers` 非对象 → 硬错误；单答案解析失败 → 容错跳过 + 告警。禁止反过来（顶层缺失静默通过 = 决策结果凭空消失）。
8. ✅ **错误码映射必须与 cortex 同口径**：429 → `ModelRateLimited`、401|403 → `ModelAuth`、4xx → `ModelBadRequest`（含内容过滤 → `ModelContentFiltered`）、5xx → `ModelServerError`。
9. ✅ **`ProviderType::Jev` / `ModelCapability::Decision` 只能尾部追加**：`#[repr(i32)]` 变体位置必须追加在末尾，禁止插队；违反 = 存量 DB 判别值错位。
10. ✅ **默认小脑必须"单启用即默认"**：`get_default_cerebellum_provider` 只认 `capability=Decision + status=Normal + api_key 非空`；禁止引入额外"是否默认"标记列。
11. ✅ **路由问题 id 与选项 id 必须集中常量**：`ROUTE_QUESTION_ID` / `CHOICE_TRIVIAL_DIRECT` / `CHOICE_CORTEX_NEEDED` 不允许散落字面量；违反 = 翻译层与请求层漂移导致永远 `UnsupportedAnswer`。
12. ✅ **`confidence` 阈值必须双向生效**：低于 `CEREBELLUM_ROUTE_CONFIDENCE_THRESHOLD` 时既不 Direct 也不 Inject；禁止只挡一边。

---

## §5 历史演进

| 版本 | 变更 | 影响范围 |
|------|------|---------|
| v1.0（一期落地） | 新增 System One 协议 DTO（`cerebellum_types.rs`）+ `SystemOneCerebellumDao` client（`dao/cerebellum/`）+ `ProviderType::Jev=8` / `ModelCapability::Decision=2` + `get_default_cerebellum_provider`；Brain 新增 `cerebellum` 字段并在 `wake_brain` 注入 | `cerebellum_types.rs` + `dao/cerebellum/*` + `common/enums/provider.rs` + `models/brain.rs` + `dal/brain.rs` + `dao/model_provider/sqlite.rs` |
| v1.1（运行时接线） | `cerebellum_router.rs` 落地（常量集中 + 三分类 + 纯函数 translate + 三层降级）；`run_think_loop` 首轮前单点接线（Direct 短路 / Inject 插 System / Skip 空操作）；`AgentRuntimeConfig` 新增 `enable_cerebellum_route` / `cerebellum_trivial_direct` 双开关 | `cerebellum_router.rs` + `think_loop.rs` + `models/agent.rs` |

**引入原因**：在唤醒 → 思考主循环链路上增加一次「几乎零成本」的意图快判断，让寒暄/简单确认类请求不必付出完整多轮 cortex 主循环的 token 与延迟成本；同时保持绝对安全——任何异常都退化为现状，且可通过停用默认小脑一键全局回滚。协议口径当前依据官方公开文档多来源交叉实证，**未经真实端点实测**，连通性 Spike 顺延至凭据到位后二期启动前集中纠偏（届时应同步更新本卡 §1/§3 的实测结论）。