---
kind: RAG 原子知识卡
name: 消息交互与SSE推送：MessageDomain delivery+management 双能力 + AgentLoopConsumer 循环驱动 + 多渠道出站（飞书/Slack/Email/Webhook/微信）
category: 业务模块 / 消息系统
scope:
  - "src/service/domain/message/**"
  - "src/service/dal/message*.rs"
  - "src/service/dao/message_push.rs"
  - "src/service/dao/lark/http.rs"
  - "src/service/dao/lark/ws.rs"
  - "src/service/dao/slack/http.rs"
  - "src/service/dao/email/smtp.rs"
  - "src/service/dao/webhook/http.rs"
  - "src/service/dao/wechat/http.rs"
  - "src/consumer/message.rs"
  - "src/consumer/agent_loop.rs"
  - "src/middleware/sse.rs"
  - "common/src/api/message*.rs"
source_files:
  - src/service/domain/message/mod.rs#L1-L60 (MessageDomain 双 trait：MessageDelivery 投递 + MessageManagement 管理；子模块 delivery.rs 管理.rs 分别 impl)
  - src/service/domain/message/delivery.rs#L1-L150 (MessageDelivery：send_message_to_user / send_message_to_agent → 先落 messages 表 → 再 AOP publish message.created → 同时 SSE 推当前在线用户)
  - src/service/domain/message/management.rs#L1-L100 (MessageManagement：query_messages（分页）/ list_threads（会话）/ mark_read（标记已读，HUD 未读橙光减 1） / delete_message（软删）)
  - src/service/dao/message_push.rs (MessagePushDao 出站分发中心：match kind = "lark_p2p" → LarkDao.push_card / "slack" → SlackDao / "email" → EmailDao / "webhook" → WebhookDao / "wechat" → WeChatDao；多渠道一入口，Ack 统一语义)
  - src/service/dao/lark/http.rs#L50-L120 (LarkDao.push_interactive_card：飞书卡片 Markdown→飞书卡片 JSON 映射 + user_access_token 缓存 2h)
  - src/service/dao/slack/http.rs (SlackDao.push_message：Block Kit 转换 + channel_id 查 SlackUser 映射)
  - src/dao/email/smtp.rs (EmailDao.send：lettre + tokio-rustls TLS；模板渲染 tera 注入 name/content)
  - src/service/dao/webhook/http.rs (WebhookDao.push：签名 HMAC-SHA256 X-Signature Header 防篡改；超时 10s；失败 3 次指数退避)
  - src/consumer/message.rs#L1-L80 (MessageConsumer Sync 消费：AOP message.created → 拉渠道订阅者 → 调 MessagePushDao.push 发多渠道；Ack/Nack 记录在 message_delivery_attempts 表)
  - src/consumer/agent_loop.rs#L1-L100 (AgentLoopConsumer：消息投递完成后若接收者是 Agent 且非 Busy → AOP publish agent.wake 事件 → 再走 Agent 唤醒链路；形成 用户消息→立即驱动Agent 的完整闭环)
  - src/middleware/sse.rs (Axum SSE 流式中间件：EventSource /subscribe；按 user_id 广播 channel；重连自动补发 last_event_id 之后的事件；心跳 15s)
  - docs/design/message_interaction_design.md（§前台/工作Agent 角色调度 §项目上下文路由隔离 §messages 表变更说明）
  - docs/design/agent_loop_engine_design.md（§Agent循环事件驱动 §两阶段唤醒挂点 §重复唤醒互斥 BusyGuard RAII）
  - docs/design/message_channel_design.md（§出站多渠道推送 §消息渠道注册表 §飞书/Slack/Email 三渠道参数）
  - docs/plan/agent_loop_engine_plan.md（§事件+定时双链路驱动 §consumer::agent_loop 注册顺序）
  - docs/plan/聊天MVP.md（§前台页面发送消息 §SSE 实时推送 §多渠道已读同步）
  - docs/plan/飞书P2P消息集成.md（§飞书私信入站 §飞书卡片出站）
  - docs/wiki/zh/content/功能模块/消息系统/消息系统.md（消息系统全景：入站→路由→存储→SSE 推送→多渠道出站 5 段链路总览）
  - docs/wiki/zh/content/功能模块/消息系统/消息管理.md（消息列表/已读/软删 + 会话 thread 聚合）
  - docs/wiki/zh/content/功能模块/消息系统/实时推送.md（SSE 协议说明：EventSource URL / 重连 last-event-id / 15s 心跳帧）
  - docs/wiki/zh/content/核心模块/服务层/领域层/消息领域.md（MessageDomain 双能力 trait：delivery + management；子模块结构）
  - docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md（多渠道全景：入站 Lark WS + 出站 5 渠道）
  - 【平行卡 1】docs/wiki/knowledge/zh/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标.md（飞书 WS 私信入站 → WS 消息转内部 Message → AOP publish → 本卡 MessageConsumer 消费；互为出入站）
  - 【平行卡 2】docs/wiki/knowledge/zh/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md（MessageConsumer + AgentLoopConsumer 是 AOP 8 类消费者中的 2 类；启动顺序 consumer::init 必须在 init_base_data 之后）
---

## §1 概述

**本卡角色**：用户↔Agent 消息交互、SSE 实时推送、多渠道出站的总知识卡。覆盖 MessageDomain 的 delivery（发送）/ management（查询管理）双能力、AgentLoopConsumer 完成投递后驱动 Agent 唤醒、MessagePushDao 作为出站统一入口分发到 5 类渠道（飞书卡片/Slack Block/Email SMTP/Webhook HMAC/微信客服）以及 SSE 中间件的广播机制。**定位：新增出站渠道、排查消息发了用户没收到、Agent 收到消息但没自动唤醒时读。**

- **发送 4 段原子链路**（MessageDelivery::send_message_to_user/agent，内部按序，出错整体回滚）：① 先写 `messages` 表（带 status=Pending）→ ② SSE push 当前在线的目标 user_id 浏览器（通过 middleware/sse.rs 的 BroadcastChannel：Arc<RwLock<HashMap<user_id, Vec<mpsc::Sender>>>>）→ ③ AOP publish message.created 事件 → ④ 返回 Message ID。失败回滚：写 DB 后 SSE/AOP 任一步失败都不回滚 DB（消息已经落了就不能丢），但是会 return 500 给调用方附带"投递警告"标记让前端显示「发送成功但渠道推送部分失败，对方稍后能在站内收到」。
- **SSE 中间件 + HUD 未读计数橙光**（middleware/sse.rs）：EventSource `GET /api/v1/sse/subscribe?token=JWT`；JWT 解析 user_id 后加入广播映射。事件格式 3 类：`event: message.created data: {message_id, from_id, content, thread_id, unread_count}`（unread_count 让前端所有页面右上角角标同步更新，不用再单独拉未读接口）；`event: message.read`（对方已读后自己的消息自动勾选）；`event: heartbeat data: pong` 15s 一帧防 Nginx 超时。断线重连时前端自动带 `Last-Event-ID` 头，服务端从 `message_seen_logs` 表拿用户上次最后看到的 ID → SELECT id > last-id 的 200 条补推。
- **多渠道出站分发中心**（dao/message_push.rs + consumer/message.rs）：AOP message.created → MessageConsumer 读取目标 `channel_subscriptions` 表（用户配置：lark、slack、email、webhook、wechat 订阅勾选）→ 对每个订阅渠道调 `MessagePushDao.push(ctx, kind, channel_target, payload)`；匹配 kind 路由：lark_p2p 调 LarkDao.push_interactive_card（Markdown→飞书卡片，附回复按钮，回 A2A 回调地址）、slack 调 SlackDao.push_message（Block Kit）、email 调 EmailDao.send（tera 模板渲染 lettre SMTP）、webhook 调 WebhookDao.push（HMAC-SHA256 签名 X-Signature + 3 次指数退避 5s/20s/60s）、wechat 调 WeChatDao.push（客服消息 access_token 2h 缓存）。每次 push 结果写 `message_delivery_attempts` 表（含 status、http_status、err_msg、latency_ms），方便前端「消息投递详情」面板查看。

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
| consumer/message.rs MessageConsumer | AOP 消费消息 | Sync ConsumeMode；message.created → 拉 channel_subscriptions → 循环 push；ack/nack 自动由 AOP Registry 调用 | `:L1-L80` |
| consumer/agent_loop.rs AgentLoopConsumer | AOP 消费消息 | MessageConsumer 之后的同级消费者（注册顺序在后）；message.to_id 是 agent_id → BusyGuard 查 state；Idle=AOP publish agent.wake 事件触发两阶段唤醒；Busy/Resting=把事件挂 agent.pending_message Vec，下次唤醒一次性消费 | `:L1-L100` |
| middleware/sse.rs SSE 广播中间件 | Axum 订阅 | BroadcastChannel: Arc<RwLock HashMap<user_id, Vec<mpsc::Sender<Event>>>>；new_user 注册 handler；heartbeat 15s tokio spawn 独立 loop；last_event_id 补发查询 | 见 sse.rs |

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

## §4 硬约束与回归红线（7 条）

1. **MessageDelivery.send_message_* 永不 panic**：内部 DAO/SSE/AOP 任何一步出错都用 `?` 捕获并转换为 DomainError；对调用方返回 500 时 message_id 仍然是 Some（因为已经落库），前端不会出现"找不到消息"的 404。
2. **SSE 广播失败不回滚消息**：消息落库=用户最终会看到（刷新页面能查到），SSE 只是加速实时性；SSE 失败时返回 sse_warn=true 让前端弹 toast「实时推送失败，刷新查看」，绝不回滚 status=Pending 的消息行。
3. **AgentLoopConsumer Busy 时消息不丢（Vec 缓存）**：Agent 在 Busy 状态时新消息绝不丢，append 到 pending_messages；下次被唤醒时先 `std::mem::take(&mut agent.pending_messages)` 一次性全注入 Prompt，保证上下文完整；panic 时 Vec 在 Arc Mutex 中不丢数据。
4. **Webhook 签名校验 X-Timestamp 窗口 5 分钟**：Webhook 接收方验证 X-Timestamp 与本地时钟差 < 5min 才验签名；防止重放攻击；服务端 push 前生成时间戳 ms 精度。
5. **飞书 open_id 映射查不到直接跳过 + warn**：message_push 时若用户没绑定 Lark open_id，Lark 推送返回 SKIPPED 状态（不影响其他渠道推送）；message_delivery_attempts.status=Skipped 原因列 "no lark binding"。
6. **邮件正文不塞原始消息**：邮件只塞「预览摘要 200 字 + 查看完整消息 URL」，防止 Markdown 里有敏感信息被邮件服务商扫描；消息正文必须登录系统查看。
7. **消息软删 = status=0 且前端过滤**：delete 接口只改 status=0；所有 query/list 接口默认 WHERE status != 0（common pagination 规范 §软删除约定）；前端不展示已删消息，只有管理员专用 query_all（带 include_deleted）才可以看到。
