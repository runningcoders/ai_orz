---
kind: wiki_knowledge_card
name: Lark P2P WS 私信入站：身份凭证引用解析 + 按 app_id 聚合多渠道 WS + 用户身份自动映射 + LarkWsMetrics 健康指标
category: dal lark（LarkMessageChannelDal）+ dao lark WS token source
scope:
  - "src/service/dal/lark.rs"
  - "src/service/dao/lark/http.rs"
  - "src/service/dao/lark/ws.rs"（或 dao/lark/*ws*）
  - "src/models/message_channel.rs"
  - "src/pkg/adapter/message.rs"
source_files:
  - src/service/dal/lark.rs#L122-L160（LarkMessageChannelDal 结构体四字段：message_channel_dal 基础+ lark_dao + user_dao（凭证引用解析）+ running RwLock + callback RwLock；辅助 fn listens_inbound 缺省=true；fn identity_mode_of 缺省="auto"）
  - src/service/dal/lark.rs:LarkMessageChannelDal.start()（实现 MessageInboundAdapter.start：running 冲突检测 + set_callback → query_enabled_lark_channels 全部开启入站的飞书渠道 → filter listens_inbound=true → resolve_channel_credentials(sys_ctx, &channel) 查凭证引用 → 按 app_id HashMap 去重（同 app_id 共享 WS 连接）→ 遍历 by_app 调用 lark_dao.start_listener(app, credentials, LarkAdapterHandler { callback })）
  - src/service/dal/lark.rs:resolve_channel_credentials(ctx, &channel)（飞书凭证引用解析：channel.config.lark_credential_id → 先看 ChannelPushOptions.user 自带 identity_credentials → 无则 user_dal 查用户凭证库 kind=LarkApp → 找到后通过 AES-256-GCM 解密得到 app_id + app_secret → 返回 LarkAppCredentials。整个过程引用只读不落明文）
  - src/service/dal/lark.rs:query_enabled_lark_channels()（通过基础 MessageChannelDal 查所有 status=启用 + channel_type=Lark + config.lark_listen_inbound != Some(false) 的渠道）
  - src/service/dao/lark/http.rs#L236-L260（LarkWsTokenSource：WS 连接鉴权 token。因为 WS 会断线重连，重连时必须重新拉取 tenant_access_token；LarkWsTokenSource 用共享 per-app token_caches（HashMap<app_id, SharedTokenCache>）缓存避免重复请求；在重连时异步调用 `internal.tenant_access_token()`；返回的 token 用于 WS 建连握手）
  - src/service/dao/lark/ws.rs 或对应文件:Ln-Lm（LarkDao.start_listener(app, credentials, handler)：WebSocket 长连接建立 + 事件订阅（im.message.receive_v1）+ 心跳检测 + 自动断线重连（指数退避：1s→2s→4s→30s max）+ 每条 LarkEventPayload 通过 handler.on_event(payload) 交付给上层 LarkAdapterHandler）
  - src/service/dal/lark.rs 内定义的 LarkAdapterHandler 结构（桥接层：LarkEventHandler 接口把飞书 im.message.receive_v1 事件 → AdaptedMessage。关键映射：event.payload.event.sender.sender_id.open_id → AdaptedMessage.external_user_id；event.payload.event.message_id → external_message_id；event.payload.event.create_time → received_at；文本消息 content 字段 base64 decode → text → content）
  - src/service/dal/lark.rs:listener_stats() → LarkWsMetrics（调用 lark_dao.listener_stats()：按 app_id 维度聚合指标 current_connections/reconnect_count/last_ping_latency_ms/inbound_message_count_last_minute/disconnect_reason）
  - src/models/message_channel.rs#L206-L240（ChannelConfig lark 字段：lark_credential_id 引用 + lark_identity_mode auto/bot/user + lark_open_id 用户绑定 + lark_user_name 展示 + lark_listen_inbound 是否监听入站）
  - src/producer/message_channel.rs 收到 AdaptedMessage 后的用户身份自动映射：AdaptedMessage(ChannelType=Lark, external_user_id=open_xxx) → 查 MessageChannel 表中 lark_open_id = open_xxx 的记录 → 得到该渠道归属 user_id + agent_id → 如果消息接收方 agent_id = 渠道绑定的 agent，则 sender = user_id，receiver = agent_id → 新建 MessagePo → 发布 NewMessage AOP 事件唤醒 Agent
  - docs/archive/design-archive/lark_cli_integration.md
  - docs/archive/design-archive/message_channel_design.md
  - docs/archive/plan-archive/飞书P2P消息集成.md（Lark P2P 双向链路专项：三级凭证解析 + open_id 映射 + v4 入站中台落地）
  - docs/archive/plan-archive/lark-cli_集成二期.md（凭证用户级管理 + WS 指数退避重连 = 本卡依赖的底层能力）
  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/飞书集成系统.md
  - docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md
  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件生产者/消息通道生产者.md
  - docs/wiki/zh/content/API 参考/RESTful API/财务管理模块 API/消息渠道管理 API.md
  - docs/wiki/zh/content/项目概述/核心功能特性/系统管理功能/系统健康检查.md（Lark WS 指标面板展示）
  - 【平行卡1】消息渠道入站适配中台：MessageInboundAdapter trait + Registry + start_all stop_all（本 Batch 卡 3）
  - 【平行卡2】身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用（lark_credential_id 凭证引用的维护者）
  - 【平行卡3】AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容（resolve_channel_credentials 解密凭证时调用）
---

# Lark P2P WS 私信入站（凭证引用解析 + 多渠道按 app_id 聚合 + 用户身份自动映射 + 健康指标）

## §1 整体方案
飞书是 AI Orz 第一个完成「双向私信」的渠道：
- **出站**：Agent 想给用户发飞书消息 → 通过 ChannelConfig.lark_credential_id 引用查用户凭证库 → 解密得到 Lark App 凭证 → HTTP 调飞书 open-API 推消息
- **入站**：用户给机器人发私信 → 飞书服务器通过 WebSocket 长连接主动推送 im.message.receive_v1 事件 → ai_orz 收到事件 → 把外部 open_id 自动映射为系统内部 user_id/agent_id → 创建内部 MessagePo → 唤醒对应 Agent 处理
→ **双向闭环（用户 ↔ Agent 通过飞书端到端对话）** 达成。

**四大关键组件（从下到上分层）**：

**(a) Lark WebSocket 连接层（dao/lark/ws.rs + LarkWsTokenSource）**：
- 每个飞书 app（app_id 唯一）一条独立的 WS 连接。心跳/自动重连（指数退避：1s、2s、4s、8s、16s、30s 封顶）由 dao 层内部处理。
- **LarkWsTokenSource**：WS 握手时需要短期 tenant_access_token（2 小时过期）；因为重连会反复需要新 token，`LarkWsTokenSource` 把 http:reqwest::Client + token_caches（Arc<RwLock<HashMap<String, SharedTokenCache>>》 + app_id + app_secret 打包，实现 `WsTokenSource::ws_token()` 接口；重连时内部刷新 token，上层完全无感。
- 订阅事件：连接建立后调 `im.message.receive_v1`（私信接收）订阅；其他事件（加入群、卡片按钮等）暂不启用（按需后续增加）。
- 指标上报：每条连接内部维护计数 reconnect_count、last_ping_latency_ms、inbound_message_count。

**(b) LarkMessageChannelDal 入站管理（MessageInboundAdapter 实现）**：
- 系统内可能有 N（10~100）个 MessageChannel 记录（每个用户、每个 Agent 可能分别创建一条飞书渠道），但它们共享的 app_id 其实只有少数（一个公司通常只建 2-3 个飞书应用）。所以 start() 内的关键步骤 = **query_enabled_lark_channels → filter listens_inbound=true → resolve_channel_credentials → 按 app_id HashMap 聚合去重**：
  ```
  10 条渠道（channel_id）→ 按凭证的 app_id 聚合 → 只剩 2 个 app_id（A 应用 + B 应用） → 只开 2 条 WS 连接（共享！）
  ```
  共享后既节省飞书连接配额也节省本地资源，而事件分发后按 open_id → channel 反查正确路由到对应用户。
- **resolve_channel_credentials 引用解析（三级路径）**：
  1. 先看 PushOptions.user（producer 层预加载）→ 如果用户实体自带 identity_credentials（identity_kind=LarkApp）→ 直接拿来用（避免 DB IO）。
  2. 否则 user_dal.get_by_credential_id(ctx, lark_credential_id) 查用户凭证库 → 得到 UserIdentityCredentials { kind=LarkApp, detail: { app_id, app_secret_encrypted: <AES-256-GCM 密文> } }
  3. AES-256-GCM 调用 decrypt(&app_secret_encrypted, MASTER_KEY) 明文 app_secret（仅在内存中，不落盘、不 log）
  4. 包装为 LarkAppCredentials { app_id, app_secret } 返回；引用解析失败返回 None，该渠道跳过（log_warn + Conflict 不影响其他渠道）
- **listens_inbound 开关**：ChannelConfig.lark_listen_inbound=None → 默认 true；Some(false) → 管理员显式关闭入站监听，渠道只用于出站推送或 lark-cli 工具身份，不建 WS 连接。

**(c) LarkAdapterHandler 事件桥接（飞书事件 → AdaptedMessage）**：
- `LarkAdapterHandler` 实现 `LarkEventHandler`（dao 层定义的接口），收到 `im.message.receive_v1` 事件：
  - 文本消息：event.payload.event.content 是 base64 编码的 JSON → base64 decode → 取 text 字段 → AdaptedMessage.content
  - 图片/文件消息（todo vNext）：content type=image → 先解析 image_key，后续调 media API 下载，暂降级为 text 占位提示「用户发送了一张图片，暂不支持」
  - open_id / message_id / create_time → external_user_id / external_message_id / received_at
  - 调 callback.on_message(adapted_msg) 投递到中台回调 → producer 接管内部路由
- 未知消息类型 log_warn 丢弃不 panic，错误隔离。

**(d) Producer 用户身份自动映射 + 唤醒（消息通道生产者）**：
- 从 AdaptedMessage(Lark, open_id=ou_xxx) → 查 `MessageChannel WHERE channel_type=Lark AND config_json.lark_open_id = 'ou_xxx' AND status=启用`：
  - 找到 → 得到 channel.user_id（系统内部用户）+ channel.agent_id（该渠道绑定的 Agent）
  - 构造内部 MessagePo：sender=user_id, receiver=agent_id, channel_type=Lark, channel_ref_id=Some(channel.id)
  - publish AOP 事件 `NewMessageCreated` → 消息消费者 consumer/message.rs → try_set_busy → awaken → 正常进入 Agent 两阶段唤醒
  - 结果：用户在飞书端发一句「把昨天的会议纪要发给我」→ 几秒后 Agent 自动在飞书端回复（回复走 MessageDomain deliver 出站 → LarkDao.push → HTTP API 回飞书）
- 没找到渠道（open_id 没绑定）→ log_warn("收到未绑定的飞书入站消息") 丢弃；可选后续 vNext 把该消息投递到系统管理员渠道做人工审核。

**健康指标：LarkWsMetrics（系统健康面板展示）**：
- listener_stats() 从 lark_dao 聚合 → 维度：app_id（每个飞书应用）
- 字段：current_connections（当前几条 WS 连接）、reconnect_count（启动以来总重连次数）、last_ping_latency_ms（最近一次 ping-pong 延迟）、inbound_message_count_last_minute（近 1 分钟入站消息数，T+1 滑动窗口）、last_disconnect_reason（最近一次断线原因：网络/凭证过期/飞书维护）
- 系统健康页面 `/system/health` 指标卡片展示：Lark WS 总连接 / 重连次数 / 平均延迟。如果 last_ping_latency > 5000ms 或 last_disconnect_reason="凭证过期" → 红标告警，管理员需重新录入飞书凭证。

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [dal/lark.rs](/src/service/dal/lark.rs) | DAL：Lark 渠道入站总入口（MessageInboundAdapter 实现）| LarkMessageChannelDal struct ~L122；fn start() ~L650+（按 app_id 聚合 + resolve_channel_credentials）；resolve_channel_credentials 三级引用解析；LarkAdapterHandler 桥接事件；listener_stats 透传指标 |
| [dao/lark/http.rs](/src/service/dao/lark/http.rs) | DAO：Lark WS Token Source | LarkWsTokenSource ~L236；ws_token() 自动拉取短期 token（共享 per-app 缓存） |
| [dao/lark/ws.rs](/src/service/dao/lark)（对应 ws 文件）| DAO：WebSocket 连接 + 事件订阅 + 指数退避重连 | start_listener(app, credentials, handler)；心跳 + auto_reconnect；LarkEventHandler trait |
| [models/message_channel.rs](/src/models/message_channel.rs) | ChannelConfig 飞书相关 6 字段 | lark_credential_id 引用 + lark_identity_mode + lark_open_id 绑定 + lark_user_name + lark_listen_inbound 开关 ~L206 |
| 【① Design 1】lark_cli_integration.md（§飞书私信入站 + 指标）| 为什么 WS 而非飞书事件回调（回调需要公网 IP，WS 适合私有化）；按 app_id 聚合动机 | docs/archive/design-archive/lark_cli_integration.md |
| 【① Design 2】message_channel_design.md（§入站链路）| 身份引用路径、入站出站对称 | docs/archive/design-archive/message_channel_design.md |
| 【② Plan】飞书集成二期.md（真实路径）| 飞书私信入站落地计划与执行结果 | docs/archive/plan-archive/lark-cli_集成二期.md |
| 【③ Wiki 长文 1】飞书集成系统.md | 用户视角：飞书配置 + 渠道绑定 + 私信收发端到端说明 | docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/飞书集成系统.md |
| 【③ Wiki 长文 2】消息渠道管理.md | 管理员创建飞书渠道，填 lark_credential_id 引用、绑定 open_id 到用户，开关 lark_listen_inbound | docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md |
| 【③ Wiki 长文 3】消息通道生产者.md | AOP 层：AdaptedMessage → 内部 NewMessage → 唤醒链路 | docs/wiki/zh/content/基础设施/AOP%20事件系统/事件生产者/消息通道生产者.md |
| 【③ Wiki 长文 4】消息渠道管理 API.md | Finance 模块 create/update message_channel API，飞书字段全部暴露 | docs/wiki/zh/content/API%20参考/RESTful%20API/财务管理模块%20API/消息渠道管理%20API.md |
| 【③ Wiki 长文 5】系统健康检查.md | Lark WS Metrics 面板告警说明（延迟>5s / 凭证过期）| docs/wiki/zh/content/项目概述/核心功能特性/系统管理功能/系统健康检查.md |
| 【平行卡 1~3】入站中台 / 身份凭证 CRUD / AES 加密（source_files[] 尾路径）| 飞书渠道实现依赖的三张基础卡 | 见本卡 source_files[] 尾平行卡绝对路径 |

## §3 架构约定

1. **凭证永远只存引用，不存明文到渠道表**：`lark_credential_id` → 用户凭证库 → AES-256-GCM 解密 → 内存内 app_secret。渠道表绝不直接存 app_id/app_secret 明文（即使加密也不要，分散密钥点 = 风险面增加）。
2. **多个 MessageChannel 共享同一 app_id 的单条 WS**（共享连接配额、节省资源）。绝不能「一个渠道建一条 WebSocket 连接」= 100 渠道 = 100 WS = 直接被飞书平台限流封禁。
3. **指数退避封顶 30s**：无限退避（如用 1→2→4→8→… 不封顶）会导致飞书维护时断线后等 30 分钟才重连，用户体验差。封顶 30s 让用户感觉"重启飞书应用后 30s 内会自动恢复"。
4. **未知类型消息（图片/卡片）不 panic，降级文本提示**。当前只支持 text 是有意为之。未来 vNext 支持图片时，再把 LarkAdapterHandler 的 content 匹配扩展 image/rich_text。
5. **resolve_channel_credentials 返回 None 的渠道要跳过，不阻塞整组启动**（见上一张卡的 start_all 非 fail-fast 约束）。log_warn 一条含渠道 id 和 credential id，方便管理员定位「哪个渠道的凭证引用失效了」。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 MessageChannel 表 config_json 中硬编码 app_secret 明文或任何加密后的 secret**。凭证必须走用户凭证库引用路径。发现硬编码 = 安全红线（即使加密写两份也增加密钥泄漏攻击面）。
2. ❌ **禁止每条 MessageChannel 独立开一条 WS 连接**（造成连接膨胀）。必须先按 app_id 聚合再开连接，100 条渠道最多 = app_id 去重后数量（典型 2-5 条）。
3. ❌ **禁止重连失败后循环调用 `ws_token()`**（无限刷飞书 token 接口 → 被限流）。指数退避 + 共享 token_caches 双保险：如果 token 还在有效期（离过期 1h 以内）直接用缓存，不需要 HTTP 请求。
4. ✅ **Lark 事件处理必须同步记录 inbound_message_count_last_minute**。指标是 P0 故障的第一发现手段（「近 1 分钟入站 0 条」且系统里有用户发消息 = 大概率 WS 挂了）。
5. ✅ **open_id 映射不到内部用户必须 log_warn + 丢弃，不 panic**。外部用户不是系统用户的情况正常存在（有人搜错机器人 + 发了一条），panic = DoS 风险。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 5 篇 wiki 长文 + 2 Design + Plan（飞书集成二期）真实路径 + 3 张平行卡（入站中台 / 身份凭证 CRUD / AES 加密）；对应 Wiki 长文 cite 段回链本卡 + lark_cli_integration/message_channel_design 两份 Design + 集成二期 Plan。
