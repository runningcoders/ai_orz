---
kind: wiki_knowledge_card
name: 微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程
category: dal wechat + dao wechat + consumer + adapter
scope:
  - "src/service/dal/wechat/**"
  - "src/service/dao/wechat/**"
  - "src/pkg/wechat_ilink.rs"
  - "src/consumer/wechat_inbound.rs"
  - "src/models/events/wechat.rs"
  - "common/src/api/wechat_integration.rs"
source_files:

  - src/service/dal/wechat/mod.rs#L30-L63（WechatDal 总 trait + init 注册到 MessageAdapterRegistry）

  - src/service/dal/wechat/impl.rs#L37-L68（WechatDalImpl 结构：message_channel_dal + wechat_dao + credential_dao + running 标记）
  - src/service/dal/wechat/impl.rs#L122-L250（adapt_wechat：iLink 入站消息 → AdaptedMessage 协议转换 + 事件过滤 + peer 校验 + 首次入站自动回填 wechat_peer_id）
  - src/service/dal/wechat/impl.rs#L369-L437（MessageInboundAdapter 实现：start 渠道数据驱动逐渠道建长轮询 + stop 全部释放）

  - src/service/dao/wechat/mod.rs#L24-L60（WechatDao trait：push / test_connection / start_polling / stop_polling / stop_all_polling / is_polling）

  - src/service/dao/wechat/ilink.rs#L37-L128（IlinkChannelCredentials + resolve_ilink_credentials：凭证解密 + kind=WechatIlink 校验 + base_url 空值回落默认域）
  - src/service/dao/wechat/ilink.rs#L133-L199（get_updates 长轮询：45s 超时 > 服务端 hold 35s；超时宽容视为无事件）
  - src/service/dao/wechat/ilink.rs#L249-L303（send_text 出站：context_token 必须回传，空值报错提示让对端先发消息）
  - src/service/dao/wechat/ilink.rs#L365-L461（poll_loop 循环体：收帧 publish AOP 事件 → 刷 context_token 会话 → 推进游标 → 一次写回 inbound_state）
  - src/service/dao/wechat/ilink.rs#L464-L551（PollLoopRegistry：channel_id 键控 + 凭证指纹 ensure 幂等 + 指纹变化自动重建）

  - src/pkg/wechat_ilink.rs#L88-L148（扫码登录协议：get_login_qrcode + poll_qrcode_status 长轮询）

  - src/consumer/wechat_inbound.rs#L20-L86（WechatInboundConsumer：ConsumeMode::Async + adapt_wechat 转换 + callback.on_message 投递上层）

  - src/models/events/wechat.rs#L18-L114（IlinkMessage + WechatInboundEvent：AOP 事件信封，order_key=bot_id 同 bot 串行）

  - common/src/api/wechat_integration.rs#L14-L79（DTO：WechatLoginQrcodeRequest + WechatLoginStatusRequest + WechatLoginStatusResponse + WechatCredentialSnapshot）

  - migrations/20260906000001_add_inbound_state_to_message_channels.sql

  - docs/wiki/zh/content/功能模块/消息系统/微信 iLink 专属渠道.md

  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/飞书集成系统.md

  - docs/wiki/knowledge/zh/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标.md

  - docs/wiki/knowledge/zh/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期/消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期.md

  - docs/wiki/knowledge/zh/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅.md

---

# 微信 iLink 专属渠道闭环（wechat_dal + ilink_dao + inbound_state + 授权流程）

## §1 整体方案

微信 iLink（ClawBot）是 AI Orz 第二个完成「双向私信」的消息渠道——用户通过微信 ClawBot 扫码授权绑定个人 bot → Agent 通过 iLink 协议长轮询收消息、HTTP API 推消息 → 形成 **微信 ↔ Agent 端到端对话** 的完整闭环。

与飞书 Lark WS 的核心差异：
- **协议模型**：Lark 是 App 级 WS（一个 app 管多个用户 open_id），iLink 是 **个人 bot 微信号** 级渠道（一个 bot 微信号 = 一个 MessageChannel 行，channel_id 键控长轮询，不做 app_id 聚合）
- **凭证获取路径**：Lark 需要开发者在飞书开放平台创建应用后填 AppID/AppSecret；iLink **用户自主扫码** → confirmed 时 bot_token/bot_id/base_url 一次性产出 → 自动落库 `wechat_ilink` 凭证行（整组轮换语义）
- **运行状态模型**：iLink 的 `context_token`（会话令牌滚动刷新）和 `get_updates_buf`（不透明游标）动态性比 Lark WS 更强，需要 `inbound_state` 列持久化运行时状态

**关键组件（从下到上分层）**：

**(a) 扫码登录配置面（pkg/wechat_ilink.rs）**：
- `get_login_qrcode()` → iLink 服务端 `GET /ilink/bot/get_bot_qrcode?bot_type=3` 返回 `qrcode`（轮询标识）+ `qrcode_img_content`（二维码渲染内容）
- `poll_qrcode_status(qrcode)` → `GET /ilink/bot/get_qrcode_status?qrcode=xxx` 长轮询（服务端 hold ~35s，客户端超时 45s）
- 状态机：wait → scaned → confirmed（或 expired）。confirmed 时返回 bot_token/bot_id/user_id/baseurl
- **Domain 层消费 confirmed**：encrypt bot_token → 写 `CredentialKind::WechatIlink` 行

**(b) DAO 层消息面协议客户端（dao/wechat/ilink.rs）**：
- **长轮询**：`get_updates()` → `POST /ilink/bot/getupdates`，body 带不透明游标 `get_updates_buf`（只能原样回传，不可比较）
- **出站推送**：`send_text()` → `POST /ilink/bot/sendmessage`，必须带 `to_user_id`（对端 peer_id）+ `context_token`（会话令牌，空值报错提示"让对端先发一条消息"）
- **PollLoopRegistry**：channel_id 键控 + **凭证指纹 ensure 幂等**（bot_id/bot_token/base_url 任一变化 → 指纹不同 → 自动停旧重建）
- **失败退避节奏**：前 5 次 2s 快速重试，超过后 30s 避限流（对齐 lark WS 指数退避封顶模式）
- **受管循环 poll_loop**：收帧即 publish AOP 事件 → Async consumer 消费业务 → 推进游标 + 刷 context_token → 一次写回 inbound_state（空轮询零写入）

**(c) DAL 层入站管理（dal/wechat/impl.rs）**：
- 实现 `MessageInboundAdapter` trait（注册到 pkg/adapter 中台）
- `start()`：查询全部启用且开启入站监听的微信渠道 → 按渠道 `wechat_credential_id` 引用解析凭证 → 逐渠道建长轮询
- `adapt_wechat()`：协议转换 + 四层过滤（非 FINISH/非 USER/peer 不匹配 → 跳过）
  - **首次入站自动回填 wechat_peer_id**：渠道 config 中 peer_id 未配置时，第一帧收到后自动回填（RMW 竞态窗口可接受，回填失败不阻断入站）
  - **不做 Agent 路由**：AdaptedMessage 的 to_agent_id 取渠道显式绑定，未绑定为 None，由 producer 层档位链路由
- WechatCredentialDal：凭证引用解析 + 渠道定位查询（供 finance domain 凭证删除联动、message_channel DAL 出站凭证解析）
- WechatListenerDal：`sync_listener_for_channel` / `release_listener_for_channel` / `rebuild_listeners_for_credential`（凭证轮换时自动重建关联渠道的轮询）

**(d) Consumer 异步消费（consumer/wechat_inbound.rs）**：
- `WechatInboundConsumer` 订阅 `wechat.inbound.message` 事件
- **ConsumeMode::Async**：DAO 读循环只 publish（入队即返回），协议转换 + 渠道查找 + 消息投递都在 AOP worker 线程执行
- 事件链路：poll_loop → publish WechatInboundEvent → consumer → adapt_wechat → callback.on_message → producer 路由 → MessageDomain 投递

**(e) inbound_state 运行时持久化（common::models::inbound_state）**：
- 消息渠道表 `message_channels` 新增 TEXT 列 `inbound_state`（通用 JSON，运行时循环独占写）
- `InboundState { cursor: InboundCursor, sessions: InboundSessions }`
- 游标 kind=Opaque（iLink 的 get_updates_buf 不透明），sessions 按 peer 组织 + context_token 滚动刷新 + 100 上限裁剪

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [dal/wechat/mod.rs](src/service/dal/wechat/mod.rs) | DAL 总 trait + 单例管理 | WechatDal trait L30；init() L48（注册到 MessageAdapterRegistry）；new_with_credential_dao() L65 |
| [dal/wechat/impl.rs](src/service/dal/wechat/impl.rs) | DAL 实现：凭证面 + 监听面 + 适配面 | WechatDalImpl struct L37；adapt_wechat L122；MessageInboundAdapter impl L369 |
| [dao/wechat/mod.rs](src/service/dao/wechat/mod.rs) | DAO trait | WechatDao trait L24：push / start_polling / stop_polling / stop_all_polling |
| [dao/wechat/ilink.rs](src/service/dao/wechat/ilink.rs) | DAO：iLink 协议客户端 + 受管长轮询 | IlinkChannelCredentials L37；get_updates L177；send_text L249；PollLoopRegistry L464；poll_loop L365 |
| [pkg/wechat_ilink.rs](src/pkg/wechat_ilink.rs) | pkg：扫码登录协议客户端 | get_login_qrcode L89；poll_qrcode_status L124；ILINK_DEFAULT_BASE_URL L20 |
| [consumer/wechat_inbound.rs](src/consumer/wechat_inbound.rs) | Consumer：微信入站消息消费 | WechatInboundConsumer struct L20；on_event L46（adapt_wechat → callback.on_message） |
| [models/events/wechat.rs](src/models/events/wechat.rs) | AOP 事件类型 | IlinkMessage L18；WechatInboundEvent L105；Event impl（kind=wechat.inbound.message, order_key=bot_id） |
| [common/src/api/wechat_integration.rs](common/src/api/wechat_integration.rs) | 前后端共享 DTO | WechatLoginQrcodeRequest L16；WechatLoginStatusRequest L29；WechatLoginStatusResponse L37 |
| common/src/models/inbound_state.rs | 运行状态通用模型 | InboundState L16；InboundCursor L60（CursorKind::Opaque）；InboundSessions L127 |
| migrations/20260906000001_add_inbound_state_to_message_channels.sql | DDL：inbound_state 列 + 唯一约束 | 消息渠道表新增 TEXT 列 inbound_state |
| 【① Wiki 长文 1】微信 iLink 专属渠道.md | 端到端完整说明：扫码授权 → 入站收帧 → 消息投递 → 出站回复 | docs/wiki/zh/content/功能模块/消息系统/微信%20iLink%20专属渠道.md |
| 【③ Wiki 长文 2】消息渠道管理.md | 管理员创建微信渠道，填 wechat_credential_id 引用 | docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md |
| 【③ Wiki 长文 3】消息通道生产者.md | AOP 层：WechatInboundEvent → AdaptedMessage → NewMessage → 唤醒 | docs/wiki/zh/content/基础设施/AOP%20事件系统/事件生产者/消息通道生产者.md |
| 【平行卡 1】Lark P2P WS 私信入站（对称渠道实现）| LarkMessageChannelDal + LarkWsTokenSource + open_id 自动映射 + LarkWsMetrics | 本卡 source_files[] 尾 Lark P2P WS 卡绝对路径 |
| 【平行卡 2】消息渠道入站适配中台（基础设施层 trait+registry）| MessageInboundAdapter trait + MessageAdapterRegistry + start_all/stop_all | 本卡 source_files[] 尾入站适配中台卡绝对路径 |
| 【平行卡 3】Domain 内部事件与消费者全链路 | 8 类 Consumer 注册 + Async 模式 + AOP 投递 | 本卡 source_files[] 尾 Domain 事件卡绝对路径 |

## §3 架构约定

1. **DAO 层不做凭证解析**：出站 push 和轮询启动均由 DAL 层按渠道 `wechat_credential_id` 引用解析出 `IlinkChannelCredentials` 后传入（`resolve_ilink_credentials` 解密 bot_token + 校验 kind=WechatIlink）。DAO 不依赖 UserCredentialDao。
2. **轮询 registry 按 channel_id 键控，一渠道一轮询**（阶段一：一个 bot 微信号 = 一个 MessageChannel）。与 Lark WS 按 app_id 聚合不同，不做共享连接——个人 bot 微信号天然隔离。
3. **PollLoopRegistry.ensure 幂等三态**：未运行→启动；运行中且凭证指纹相同→no-op；运行中但指纹不同（bot_id/bot_token/base_url 任一变化）→stop 旧 → start 新。凭证轮换自动重建。
4. **InboundStateWriter 窄接口**：DAO 不依赖 MessageChannelDao 完整类型，只注入 `InboundStateWriter { async fn save(channel_id, state) }` 薄接口。生产实现委托 `MessageChannelDao::set_inbound_state`；测试可注入内存实现。
5. **DAO 轮询循环内 publish AOP 事件（Async 模式）**：读循环里只 publish → 返回，业务由 consumer 异步消费。`ConsumeMode::Async` 的动机：协议转换 + 渠道查找 + callback 投递可能在 DB/HTTP 上耗时，阻塞长轮询读循环 = 消息积压 + 长轮询超时。
6. **context_token 滚动刷新必须随消息返回**：出站 sendmessage 的 context_token 参数来自 `inbound_state.sessions` 的最新值；对端如果从未入站过（peer 未建立会话），出站报错"让对端先发一条消息再回复"。这是 iLink 协议要求。
7. **首次入站自动回填 wechat_peer_id（一次性，不回滚）**：回填失败仅 warn 不阻断入站，下次入站重试。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 MessageChannel.config_json 硬编码 bot_token 明文或任何加密后的 secret**。凭证必须走 `wechat_credential_id` → user_credentials 表引用路径。bot_token 的加解密边界固定在 DAO 层 `resolve_ilink_credentials`（调 `pkg::crypto::decrypt_channel_secret`），与 Lark 同构。
2. ❌ **禁止 poll_loop 循环内做 DB IO / HTTP 调用**。读循环里只有 publish AOP 事件 + push InboundStateWriter::save。业务逻辑（adapt_wechat + 渠道查找 + callback.on_message）必须在 Consumer 层。禁止把 ConsumeMode 改成 Sync。
3. ❌ **禁止 sendmessage 不传 context_token 或 to_user_id 即尝试出站**。这两个参数空值协议层直接报错——调用方必须确保 inbound_state.sessions 中有对应 peer_id 的最新 context_token。
4. ✅ **45s 长轮询超时 > 服务端 35s hold**。客户端超时提前返回是长轮询常态（服务端在客户端超时后才返回 Wait），被视为本轮无事件（直接返回空 IlinkUpdates）。禁止把超时降到 35s 以下。
5. ✅ **IlinkChannelCredentials.fingerprint 三要素 hash（bot_id + bot_token + base_url）**。用 DefaultHasher 而非明文拼接：指纹常驻内存 registry，避免 bot_token 以可读形式留存。禁止 `format!("{bot_id}:{bot_token}:{base_url}")` 明文拼接。
6. ✅ **失败退避节奏**：前 5 次 2s 快速重试，超过后退避 30s（避免触发 iLink 限流）。禁止无限快速重试或不封顶指数退避。
7. ✅ **InboundCursor.kind=Opaque 必须原样回传**：`get_updates_buf` 是不透明字符串，禁止解析、禁止比较大小、禁止回退到更早游标（只能原样回传服务端给的新值）。服务端返回空值则保持旧游标不变。
8. ✅ **四类互引闭环**：本卡 source_files[] 含 1 篇 Wiki 长文（微信 iLink 专属渠道）+ 3 张平行卡（Lark WS 私信入站 / 入站适配中台 / Domain 事件消费者）；Wiki 长文 cite 段回链本卡 + 平行卡。

---

# T2 附记：本卡为 Level 5 纯新主题（AGENTS §2.1.3.2）

- scope 与现存 `Lark P2P WS` 卡交集 < 20%（独立第三方渠道：协议模型、凭证获取、连接管理全不同）
- scope 与 `消息渠道入站适配中台` 卡交集约 15%（WechatDalImpl 是中台 trait 的新实现，但中台卡 scope 聚焦 pkg/adapter 基础设施层本身，不覆盖具体渠道）
- 不触发 Level 1-4 合并/拆分判定，直接新建
