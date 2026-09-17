---
kind: RAG 原子知识卡
name: 消息交互与SSE推送：MessageDomain delivery+management 双能力 + AgentLoopConsumer 循环驱动 +
  多渠道出站（飞书/Slack/Email/Webhook/微信）
category: 业务模块 / 消息系统
scope:
- src/service/domain/message/**
- src/service/domain/runtime/awakening.rs
- src/service/dal/message*.rs
- src/service/dal/lark/**
- src/service/dao/message_push.rs
- src/service/dao/lark/http.rs
- src/service/dao/lark/ws.rs
- src/service/dao/slack/http.rs
- src/service/dao/email/smtp.rs
- src/service/dao/webhook/http.rs
- src/service/dao/wechat/http.rs
- src/models/message.rs
- src/consumer/message.rs
- src/consumer/agent_loop.rs
- src/middleware/sse.rs
- common/src/api/message*.rs
- common/src/enums/message.rs
- src/consumer/message_route_policy.rs
- src/pkg/policy/**
- migrations/*external_key*
source_files:
- 'src/service/domain/message/mod.rs#L1-L60 '
- 'src/service/domain/message/delivery.rs#L1-L150 '
- src/service/domain/message/management.rs#L1-L100 (MessageManagement：query_messages（分页）/
  list_threads（会话）/ mark_read（标记已读，HUD 未读橙光减 1） / delete_message（软删）)
- 'src/service/dao/message_push.rs '
- 'src/service/dao/lark/http.rs#L50-L120 '
- 'src/service/dao/slack/http.rs '
- 'src/dao/email/smtp.rs '
- 'src/service/dao/webhook/http.rs '
- 'src/consumer/message.rs#L1-L80 '
- src/consumer/message.rs#L337-L492
- 'src/consumer/agent_loop.rs#L1-L100 '
- 'src/middleware/sse.rs '
- docs/archive/design-archive/message_interaction_design.md
- docs/archive/design-archive/agent_loop_engine_design.md
- docs/archive/design-archive/message_channel_design.md
- docs/archive/plan-archive/agent_loop_engine_plan.md
- docs/archive/plan-archive/聊天MVP.md
- docs/archive/plan-archive/飞书P2P消息集成.md
- docs/wiki/zh/content/功能模块/消息系统/消息系统.md
- docs/wiki/zh/content/功能模块/消息系统/消息管理.md
- docs/wiki/zh/content/功能模块/消息系统/实时推送.md
- docs/wiki/zh/content/核心模块/服务层/领域层/消息领域.md
- docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md
- 【平行卡 1】docs/wiki/knowledge/zh/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id
  自动映射 + LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射
  + LarkWsMetrics 健康指标.md
- 【平行卡 2】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry
  全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8
  类业务消费者注册.md
- migrations/20260909000004_add_external_key_to_messages.sql (reply_to + external_key 字段 migration)
- src/models/message.rs (MessagePo reply_to + external_key + federation_contract)
- src/service/domain/runtime/awakening.rs (两阶段唤醒后注入 reply_to 上下文)
- src/service/dal/lark/impl.rs + src/service/dao/lark/http.rs (飞书 thread_id ↔ external_key 双向映射)
- docs/wiki/zh/content/功能模块/消息系统/消息系统.md
- docs/wiki/zh/content/功能模块/消息系统/消息管理.md
- common/src/mention.rs（2026-09-13 修复：放开 @ 前缀判定 + 光标 UTF-16/字节单位错配修复）
- frontend/src/components/mention_picker.rs（2026-09-13 修复：@ mention picker 触发逻辑对齐后端协议）
- frontend/src/components/chat/message_bubble.rs（2026-09-13 增量：气泡接收方 chip + 旁听消息弱化样式）
- frontend/src/pages/message/chat.rs（2026-09-13 增量：默认会话哨兵 project_id — 区分「不过滤」与「只要默认会话」）
- src/consumer/message.rs（2026-09-15 重构：routes_to_system_fallback + resolve_profile_user_id + handle_agent_message 三路分发重构）
- src/consumer/scheduler.rs（2026-09-15 增量：CronTrigger 身份分层中继）
- src/consumer/task_event_consumer.rs（2026-09-15 增量：TaskEvent 身份分层中继 + enrich_org_from_project_user）
- src/consumer/mod.rs（2026-09-15 新增：enrich_org_from_project_user helper）
- src/pkg/request_context.rs（2026-09-15 新增：message_sender_id / message_sender_role）
- src/consumer/message_route_policy.rs#L40-L60（2026-09-18 新增：MAX_AGENT_REPLY_CHAIN=5 + NO_REPLY_SENTINEL + AutoReplyRoute 三态枚举 Peer/Discard/EscalateToOwner）
- src/consumer/message_route_policy.rs#L148-L188（STATIC_ROUTE_DEFS 4 条静态规则 user_origin/system_origin/self_trigger/agent_notify，声明顺序即判定优先级）
- src/consumer/message_route_policy.rs#L260-L291（judge_static_reply_route 静态判定段零查库 + judge_chain_reply_route 链深度机械兜底段）
- src/pkg/policy/builtin.rs#L36-L67（2026-09-18 抽象化：impl_policy_delegate! 委托宏 + ThresholdPolicy 通用阈值策略 + FieldEqualsPolicy 字段全等策略）
- src/consumer/message.rs（2026-09-18 增量：handle_agent_message 接入两段式路由判定 + agent_reply_chain_depth 查库计链深度）
- common/src/enums/message.rs（2026-09-18 新增：MessageType::AgentNotify 知会类型，发送方声明无需回复）

---

## §1 概述

**本卡角色**：用户↔Agent 消息交互、SSE 实时推送、多渠道出站的总知识卡。覆盖 MessageDomain 的 delivery（发送）/ management（查询管理）双能力、AgentLoopConsumer 完成投递后驱动 Agent 唤醒、MessagePushDao 作为出站统一入口分发到 5 类渠道（飞书卡片/Slack Block/Email SMTP/Webhook HMAC/微信客服）以及 SSE 中间件的广播机制。**定位：新增出站渠道、排查消息发了用户没收到、Agent 收到消息但没自动唤醒时读。**

- **发送 4 段原子链路**（MessageDelivery::send_message_to_user/agent，内部按序，出错整体回滚）：① 先写 `messages` 表（带 status=Pending）→ ② SSE push 当前在线的目标 user_id 浏览器（通过 middleware/sse.rs 的 BroadcastChannel：Arc<RwLock<HashMap<user_id, Vec<mpsc::Sender>>>>）→ ③ AOP publish message.created 事件 → ④ 返回 Message ID。失败回滚：写 DB 后 SSE/AOP 任一步失败都不回滚 DB（消息已经落了就不能丢），但是会 return 500 给调用方附带"投递警告"标记让前端显示「发送成功但渠道推送部分失败，对方稍后能在站内收到」。
- **SSE 中间件 + HUD 未读计数橙光**（middleware/sse.rs）：EventSource `GET /api/v1/sse/subscribe?token=JWT`；JWT 解析 user_id 后加入广播映射。事件格式 3 类：`event: message.created data: {message_id, from_id, content, thread_id, unread_count}`（unread_count 让前端所有页面右上角角标同步更新，不用再单独拉未读接口）；`event: message.read`（对方已读后自己的消息自动勾选）；`event: heartbeat data: pong` 15s 一帧防 Nginx 超时。断线重连时前端自动带 `Last-Event-ID` 头，服务端从 `message_seen_logs` 表拿用户上次最后看到的 ID → SELECT id > last-id 的 200 条补推。
- **多渠道出站分发中心**（dao/message_push.rs + consumer/message.rs）：AOP message.created → MessageConsumer 读取目标 `channel_subscriptions` 表（用户配置：lark、slack、email、webhook、wechat 订阅勾选）→ 对每个订阅渠道调 `MessagePushDao.push(ctx, kind, channel_target, payload)`；匹配 kind 路由：lark_p2p 调 LarkDao.push_interactive_card（Markdown→飞书卡片，附回复按钮，回 A2A 回调地址）、slack 调 SlackDao.push_message（Block Kit）、email 调 EmailDao.send（tera 模板渲染 lettre SMTP）、webhook 调 WebhookDao.push（HMAC-SHA256 签名 X-Signature + 3 次指数退避 5s/20s/60s）、wechat 调 WeChatDao.push（客服消息 access_token 2h 缓存）。每次 push 结果写 `message_delivery_attempts` 表（含 status、http_status、err_msg、latency_ms），方便前端「消息投递详情」面板查看。

**95a0b1bf 修复：统一回复通道 + from_role 三路分发**：`MessageConsumer.handle_agent_message` 在 awaken() 返回 raw_output 非空时，按入口消息的 `from_role` 自动生成回复——User 入口 → `delivery.send_to_user(reply_to=原消息.id, to_user_id=原消息.from_id)`；Agent 入口 → `delivery.send_to_agent(from_role=Agent, to_agent_id=原消息.from_id)`；System 入口 → 跳过（系统消息无对话对象）。从此 Agent 不需要自己调用 send_message 工具回复当前对话用户，Framework 层兜底，彻底解决"必须猜 to_user_id 才能结束任务"的心理陷阱。

**消息链与话题讨论区（external_key + reply_to）**：messages 表新增 reply_to（回复链）与 external_key（话题讨论区关联键）字段（migration `20260909000004_add_external_key_to_messages.sql`）。出站推送时 dao/lark/http.rs 把 external_key 自动翻译为飞书 thread_id（双向映射：入站 thread_id → external_key 存入表，出站 external_key → thread_id 发给飞书）。src/service/domain/runtime/awakening.rs 在两阶段唤醒（IntentAnalyze → Awaken）完成后注入 reply_to 上下文到 Agent prompt，使 Agent 生成的回复自动挂在原消息下形成回复链。scheduler/consumer 三个生产端各加 1 行携带 reply_to 字段。

**默认会话哨兵 project_id + @ 修复 + 气泡 chip（2026-09-13 增量）**：
- **默认会话哨兵 project_id**：前端聊天页 `frontend/src/pages/message/chat.rs` 引入哨兵值区分两种查询语义——`project_id = None` 表示「不过滤，返回用户所有会话」；`project_id = Some(DEFAULT_SESSION_SENTINEL)` 表示「只返回默认会话的消息」。消除了旧代码中 `if project_id.is_none()` 二义性。
- **mention picker 修复**（`common/src/mention.rs` + `frontend/src/components/mention_picker.rs`）：① 放开 @ 触发的前缀判定——旧代码要求光标前紧接一个 `@` 才触发 picker，现改为允许 `@` 前有空格或行首；② 修正光标 UTF-16 vs 字节单位错配——Dioxus 前端 DOM selection range 用 UTF-16 code unit，Rust 字符串索引用字节，修复后 mention picker 光标定位不再偏移。
- **消息气泡 UI 增强**（`frontend/src/components/chat/message_bubble.rs`）：气泡头部新增接收方 chip（显示对话对象头像+名称）；旁听消息（非直接发给当前用户/Agent 的消息）应用弱化样式（opacity 0.6 + 灰色边框），视觉上区分"我是参与者"vs"我是旁听者"。

**2026-09-15 增量（commits 5ac0a99b + 440624b2）**：消息消费者重构 **三路分发规则**——原来的 User/Agent/System 三路 match 收敛为两个判定函数：`routes_to_system_fallback` 判定「是否无对等回复对象」（System 来源 + Agent 自触发 from==to 都走兜底）+ `resolve_profile_user_id` 推导用户画像（User 消息用发送者本人，后台唤醒回退到任务/项目的 root_user_id）。**Cron 触发器身份分层中继**——`CronTriggerConsumer` 和 `TaskEventConsumer` 原来统一 from_role=System，现在改为「按被触达事项的归属选身份」：项目归属用户非空时以 User 身份中继 Agent Final 自然回到用户，也不会自唤醒。新增 `consumer::enrich_org_from_project_user` 补齐组织上下文（系统触发 ctx 无组织绑定时从 root_user_id 查 UserPo.organization_id）。RequestContext 新增 `message_sender_id()` / `message_sender_role()` 专供消息发送使用（后台唤醒场景 caller_type=System 但执行者是被唤醒 Agent，必须把 agent_id 写进 from_id）。

**440624b2 修复：System 兜底 Final 按类型白名单投递 + 正文与投递对齐**：原来 `consumer/message.rs` System 分支无脑丢弃 Agent Final（后台唤醒没有来源方，里程碑/阻塞/项目收口会静默消失）。新增 `should_deliver_system_final` 白名单：TaskAssignment / TaskDispatchNotification 投递，排除 ProjectFollowupNotification（每小时巡检噪音）。同时 `domain/message/builder.rs` 补「正文与投递一致」断言——dispatch 消息说"系统自动送达"、followup 消息说"必须 send_message 主动上报"，两者语义不能反。

**防乒乓路由 + 策略引擎抽象化（2026-09-18 增量，commit bba278d6）**：A↔B Agent 互发自动回复可能形成无限乒乓——每条回复的 reply_to_id 都指向触发它的消息，链随往返单调递增。收敛到 `src/consumer/message_route_policy.rs` 的 **两段式回发路由判定**：① `judge_static_reply_route` 静态段（零查库）按 `STATIC_ROUTE_DEFS` 声明序判定 4 条规则——`user_origin` → Peer（用户身份中继后 Final 自然回到该用户）、`system_origin` → Discard（系统消息无对等回复对象）、`self_trigger` → Discard（from==to 自触发防自唤醒循环）、`agent_notify` → Discard（MessageType::AgentNotify 知会消息，发送方声明无需回复；仅 Agent 来源命中，用户来源已被声明在前的 user_origin 拦下）；外加通用 `FieldEqualsPolicy` 承载 NO_REPLY 哨兵判定——Agent 判断「后续工作与来源方无关」时让 Final 恰好输出 `NO_REPLY`，Framework trim 后全等匹配即 Discard。静态段返回 `None` 表示「跨 Agent 对等回复候选」。② `judge_chain_reply_route` 机械段——consumer 查库算链深度（沿 reply_to_id 向上连续 Agent 来源消息条数，`MessageConsumer::agent_reply_chain_depth`），达到 `MAX_AGENT_REPLY_CHAIN=5` 时不再信任模型侧协调约定（哨兵/知会声明已失效），机械终止回发并 `EscalateToOwner` 通知归属用户。三态决策 `AutoReplyRoute { Peer, Discard{reason}, EscalateToOwner{reason} }`。配套 **策略引擎抽象化**（`pkg/policy/builtin.rs`）：抽出通用 `ThresholdPolicy`（指标 ≥ 阈值即触发）与 `FieldEqualsPolicy`（字段全等）两个跨领域共性抽象，8 个内置策略中 5 个改为薄包装（TimeoutPolicy/ContextOverflowPolicy/TokenBudgetPolicy/ConsecutiveLlmErrorsPolicy/FinalAnswerPolicy），`impl_policy_delegate!` 宏转发 Policy trait、保留原类型名与 new 签名，调用点零改动；保留专用的仅 MaxRoundsPolicy（双键 Metrics）/ UserCancelPolicy（AtomicBool 状态源）/ NoProgressPolicy（多键聚合）。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| domain/message/mod.rs MessageDomain trait | Message 域总 trait | pub use 两个子 trait：MessageDelivery（出站写+SSE+AOP）+ MessageManagement（分页查询/thread聚合/标记已读/软删） | `:L1-L60` |
| domain/message/delivery.rs MessageDelivery impl | 发送核心 | send_message_to_user/agent：落库→SSE broadcast→AOP publish；整体 Result；已读标记和未读计数联动 | `:L1-L150` |
| domain/message/management.rs MessageManagement impl | 消息管理 | query_messages（Query 结构体：thread_id/sender_id/time_range/pagination）；list_threads（每个 thread 最新 1 条 + 未读计数）；mark_read；delete_message(status=0 软删) | `:L1-L100` |
| dao/message_push.rs MessagePushDao 出站分发 | 5 渠道统一入口 | match kind 字符串→对应外部 DAO 方法；统一返回 DeliveryAttempt；错误捕获转换，不 panic 影响 consumer | 见 trait 定义 |
| dao/lark/http.rs 飞书卡片出站 | LarkDao | push_interactive_card：user_id↔open_id 映射表查 → Markdown→飞书卡片 header+elements 转换 + 回复按钮 (open url 跳回本系统 /message/:id) | `:L50-L120` |
| dao/webhook/http.rs Webhook 出站 HMAC | WebhookDao | 签名 sign=HMAC_SHA256(timestamp + body, secret_hex).to_hex()；Header 带 X-Timestamp（毫秒）+ X-Signature；超时 10s；失败 3 次退避 5/20/60s | 见 webhook.rs |
| models/message.rs MessagePo | 消息实体 | reply_to（回复链，可空，指向同 messages 表） + external_key（话题讨论区关联键，用于跨渠道映射线程） + federation_contract 字段 | 见 src/models/message.rs |
| awakening.rs Runtime 两阶段唤醒 | 注入 reply_to 上下文 | IntentAnalyze → Awaken 完成后，把入口消息的 reply_to 注入 Agent prompt，使 Agent 回复自动挂链 | 见 src/service/domain/runtime/awakening.rs |
| dal/lark/impl.rs + dao/lark/http.rs 飞书双向映射 | external_key ↔ thread_id | 入站：飞书 thread_id → 存 messages.external_key；出站：external_key → 翻译为飞书 thread_id 发送（缺失映射降级为单条消息） | 见 src/service/dao/lark/http.rs |
| domain/message/mod.rs MessageDomain 扩展 | 回复链能力 | MessageDelivery send_* 新增 reply_to + external_key 参数；落库时带链；SSE 事件 payload 追加 reply_to | 见 src/service/domain/message/mod.rs |
| consumer/message.rs MessageConsumer | AOP 消费消息 + Agent 回复三路分发 | Sync ConsumeMode；message.created → 拉 channel_subscriptions → 循环 push；**handle_agent_message 三路分发**（2026-09-15 重构）：`routes_to_system_fallback` 判定是否无对等回复对象（System 来源 + Agent 自触发 from==to 兜底）+ `resolve_profile_user_id` 推导用户画像（User 消息用发送者本人，后台唤醒回退 root_user_id）；ack/nack 自动由 AOP Registry 调用 | `:L1-L80` |
| consumer/agent_loop.rs AgentLoopConsumer | AOP 消费消息 | MessageConsumer 之后的同级消费者（注册顺序在后）；message.to_id 是 agent_id → BusyGuard 查 state；Idle=AOP publish agent.wake 事件触发两阶段唤醒；Busy/Resting=把事件挂 agent.pending_message Vec，下次唤醒一次性消费 | `:L1-L100` |
| middleware/sse.rs SSE 广播中间件 | Axum 订阅 | BroadcastChannel: Arc<RwLock HashMap<user_id, Vec<mpsc::Sender<Event>>>>；new_user 注册 handler；heartbeat 15s tokio spawn 独立 loop；last_event_id 补发查询 | 见 sse.rs |
| consumer/scheduler.rs CronTriggerConsumer | Cron 定时触发器 | 身份分层中继（2026-09-15 增量）：原来统一 from_role=System，现在按被触达事项归属选身份——项目有 root_user_id 时 from_role=User / from_id=root_user_id（Agent Final 自然回到用户），A2A 项目无归属时才落 System | 见 src/consumer/scheduler.rs |
| consumer/task_event_consumer.rs TaskEventConsumer | 任务事件触发器 | 与 CronTrigger 同样的身份分层中继逻辑 + `consumer::enrich_org_from_project_user` 补齐系统触发 ctx 的组织上下文 | 见 src/consumer/task_event_consumer.rs |
| consumer/mod.rs | Consumer 公共 helper | `enrich_org_from_project_user(ctx, root_user_id)` 从项目归属用户补齐 ctx 组织上下文（系统触发链路 ctx 无组织绑定时，从 root_user_id 查 UserPo.organization_id 注入） | 见 src/consumer/mod.rs |
| pkg/request_context.rs RequestContext | 请求上下文扩展 | 新增 `message_sender_id()` / `message_sender_role()`（2026-09-15）专供消息发送——后台唤醒场景 caller_type=System 但执行者是被唤醒 Agent，必须把 agent_id 写进 from_id；原 `caller_id_or_system()` 改仅用于审计字段 | 见 src/pkg/request_context.rs |
| consumer/message_route_policy.rs 回发路由策略 | 防乒乓两段判定（2026-09-18） | `judge_static_reply_route`（STATIC_ROUTE_DEFS 4 条声明式规则 + FieldEqualsPolicy NO_REPLY 哨兵，零查库，返回 None 表示跨 Agent 对等候选）+ `judge_chain_reply_route`（ThresholdPolicy 链深度 ≥5 → EscalateToOwner）；`AutoReplyRoute { Peer, Discard{reason}, EscalateToOwner{reason} }` | `:L40-L60` `:L148-L188` `:L260-L291` |
| pkg/policy/builtin.rs 策略引擎通用抽象 | ThresholdPolicy / FieldEqualsPolicy（2026-09-18） | 通用阈值策略（metric ≥ threshold 即触发）+ 字段全等策略；8 内置策略中 5 个收敛为薄包装（Timeout/ContextOverflow/TokenBudget/ConsecutiveLlmErrors/FinalAnswer），`impl_policy_delegate!` 宏转发保留原类型名与 new 签名，调用点零改动；保留专用：MaxRounds（双键）/ UserCancel（AtomicBool 状态源）/ NoProgress（多键聚合） | `:L36-L67` |

**章节来源**
- [message/delivery.rs:L1-L150](src/service/domain/message/delivery.rs#L1-L150)
- [consumer/message.rs:L1-L80](src/consumer/message.rs#L1-L80)
- [middleware/sse.rs](src/middleware/sse.rs)

---

## §3 用户消息到 Agent 执行完整链路

```
用户在聊天页输入消息 → POST /api/v1/messages
  ↓ Handler: 校验权限(项目成员/直接好友) → 构造 SendMessageCommand
  ↓ MessageDomain.send_message_to_user / to_agent:
    [1] MessageDal.create(ctx, message) → 落 messages 表 status=Pending
    [2] SseBroadcast.send(user_id, Event::message_created(unread_count))
          → 当前浏览器开 EventSource 的所有标签页收到实时消息
          → 页面顶部 HUD 角标 +1（橙光光晕）
    [3] aop::publish(MessageCreatedEvent { message_id, from_id, to_kind })
  ↓ 返回 201 Created { message_id, sse_warn: bool }

AOP 事件被两个消费者按注册顺序依次消费：
[Consumer 1: MessageConsumer (Sync)]
  → channel_subscriptions 表查接收方用户勾选的渠道
  → 订阅 lark → LarkDao.push_interactive_card → 打开飞书就能看到卡片
  → 订阅 email → EmailDao.send → 收邮件通知
  → 订阅 webhook → WebhookDao.push → 第三方系统收到回调
  → 每条写 message_delivery_attempts (status + latency)
[Consumer 2: AgentLoopConsumer (Sync)]
  → 如果 message.to 是 Agent（agent_id）：
     → BusyGuard.try_acquire(state)
        Idle → AOP publish(AgentWakeEvent) → Runtime 两阶段唤醒
                → 唤醒时读取 agent.pending_messages（Busy 期间缓存的消息）
        Busy → 把此消息 push 进 agent.pending_messages Vec（下次唤醒处理）
        Resting → 不唤醒（resting 期间让 agent_rest 沉淀记忆完成再响应）

Runtime 唤醒 Agent → Phase1 IntentAnalyze 解析用户意图 → Phase2 Awaken 执行 → 工具调用 → 生成回复消息
  → 回复消息走同一条 MessageDelivery 链路（落库+SSE）→ 浏览器实时看到回复
```

---

## §4 硬约束与回归红线（16 条）

1. **MessageDelivery.send_message_* 永不 panic**：内部 DAO/SSE/AOP 任何一步出错都用 `?` 捕获并转换为 DomainError；对调用方返回 500 时 message_id 仍然是 Some（因为已经落库），前端不会出现"找不到消息"的 404。
2. **SSE 广播失败不回滚消息**：消息落库=用户最终会看到（刷新页面能查到），SSE 只是加速实时性；SSE 失败时返回 sse_warn=true 让前端弹 toast「实时推送失败，刷新查看」，绝不回滚 status=Pending 的消息行。
3. **AgentLoopConsumer Busy 时消息不丢（Vec 缓存）**：Agent 在 Busy 状态时新消息绝不丢，append 到 pending_messages；下次被唤醒时先 `std::mem::take(&mut agent.pending_messages)` 一次性全注入 Prompt，保证上下文完整；panic 时 Vec 在 Arc Mutex 中不丢数据。
4. **Webhook 签名校验 X-Timestamp 窗口 5 分钟**：Webhook 接收方验证 X-Timestamp 与本地时钟差 < 5min 才验签名；防止重放攻击；服务端 push 前生成时间戳 ms 精度。
5. **飞书 open_id 映射查不到直接跳过 + warn**：message_push 时若用户没绑定 Lark open_id，Lark 推送返回 SKIPPED 状态（不影响其他渠道推送）；message_delivery_attempts.status=Skipped 原因列 "no lark binding"。
6. **邮件正文不塞原始消息**：邮件只塞「预览摘要 200 字 + 查看完整消息 URL」，防止 Markdown 里有敏感信息被邮件服务商扫描；消息正文必须登录系统查看。
7. **消息软删 = status=0 且前端过滤**：delete 接口只改 status=0；所有 query/list 接口默认 WHERE status != 0（common pagination 规范 §软删除约定）；前端不展示已删消息，只有管理员专用 query_all（带 include_deleted）才可以看到。
8. **出站 external_key 存在则飞书/微信/Slack 自动映射线程 ID**：dao/lark/http.rs 等出站 DAO 必须先把 external_key 翻译为渠道线程 ID；缺失映射时降级为单条消息发送（不下沉到 thread 讨论区），同时打 log_warn 记录。
9. **唤醒注入 reply_to 必须同 project**：awakening.rs 注入 reply_to 上下文前，必须校验 reply_to 指向的消息与当前入口消息属于同一 project；跨 project 引用必须返回 400 拒绝，防止 Agent 在 A 项目回复中挂 B 项目的消息链。
10. **System 兜底分支判定必须收敛在 routes_to_system_fallback 单一扩展点**：新增"消息来源无对等回复对象"的场景（如 Agent 自触发 from==to、新的触发器类型），只改这个函数，不动 handle_agent_message 里的分发逻辑结构。禁止绕过 routes_to_system_fallback 直接在 handle_agent_message 里加新的 match 分支。
11. **触发器身份必须按归属中继，禁止统一 from_role=System**：CronTriggerConsumer 和 TaskEventConsumer 构造入口消息时，项目有 root_user_id 必须设 from_role=User / from_id=root_user_id（Agent Final 自然回到用户，也不会触发 Agent 自唤醒循环）；只有 A2A 项目无归属用户时才万不得已落 System（Final 自然丢弃）。违反此条会导致用户侧看到"来自 system 的消息"且渠道通知无人可投递。
12. **System 分支 Final 投递必须走 `should_deliver_system_final` 白名单**：后台唤醒（dispatch / followup / 巡检）没有来源方，Agent Final 曾被 System 分支整体丢弃——里程碑/阻塞/项目收口会静默消失。白名单：TaskAssignment / TaskDispatchNotification 投递到任务/项目归属用户；**禁止自动投递 ProjectFollowupNotification**（每小时定时，"无异常"收尾变周期性噪音，确有结论时由 Agent 按技能要求 send_message 主动上报）。新增投递类型必须在此函数加条件，**禁止绕开它直接在 match System 分支写投递**。
13. **MessageBuilder 正文与投递语义必须对齐**：系统 dispatch 消息正文声明"系统已自动送达"→ Agent 不能再调用 send_message（会重复）；followup 消息正文声明"需要 Agent send_message 主动上报"→ Agent 必须调用。`domain/message/builder.rs` 补断言钉住这条约束——若两者反了，投递行为与正文描述矛盾，用户体验炸。
14. **Agent Final 自动回发必须过 message_route_policy 两段判定，禁止绕过**（2026-09-18 新增）：`handle_agent_message` 唤醒完成后回发 Final 前，必须先 `judge_static_reply_route`（静态零查库），返回 None（跨 Agent 对等候选）再走 `judge_chain_reply_route`（链深度机械兜底）。禁止在 consumer 里凭 from_role 手写回发分支——静态规则表是单一扩展点，绕过它 A↔B 乒乓防线即失效。
15. **新增「无对等回复对象」场景只准在 STATIC_ROUTE_DEFS 加一条规则**（2026-09-18 新增）：声明顺序即判定优先级（user_origin 声明在前拦下用户来源，agent_notify 才不会误伤用户对话主干）；新增规则必须写明命中产出（Peer/Discard/Escalate 是路由语义，与引擎的 Deny/Confirm/Audit 是两套语义，规则表声明式携带 outcome，不走 PolicyAction），并同步补 `static_route_*` 测试。
16. **新阈值/等值类策略必须复用 ThresholdPolicy / FieldEqualsPolicy 通用抽象**（2026-09-18 新增）：禁止为每个阈值场景复制一份 Policy impl——8 个内置策略已收敛为 5 个薄包装（`impl_policy_delegate!` 转发，保留原类型名与 new 签名，调用点零改动）；只有判定形态确实不同（MaxRoundsPolicy 双键 / UserCancelPolicy AtomicBool 状态源 / NoProgressPolicy 多键聚合）才允许独立实现。NO_REPLY 哨兵仅 trim 后全等匹配，禁止改成 includes 子串匹配（会误伤恰好包含该词的正常正文）。
