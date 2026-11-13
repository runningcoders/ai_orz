---
kind: wiki_knowledge_card
name: 消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期
category: pkg/adapter（基础设施层，纯 trait+registry，零业务依赖）
scope:
  - "src/pkg/adapter/message.rs"
  - "src/pkg/adapter/mod.rs"
  - "src/producer/message_channel.rs"
  - "src/service/dal/lark.rs"（LarkMessageChannelDal 实现 trait）
  - "src/service/dal/message_channel.rs"
  - "src/service/domain/message/inbound.rs"（MessageDomain 入站门面：InboundSource 枚举 + MessageInboundAdapt trait，2026-09 新增）
  - "src/service/domain/message/mod.rs"（MessageDomainImpl 新增 lark_dal/wechat_dal 字段，inbound() 入口）
  - "src/consumer/lark_inbound.rs"（无状态 Consumer：`#[derive(Default)]`，统一经 domain 门面适配 + 中台回调投递）
  - "src/consumer/wechat_inbound.rs"（同上，Async 模式，不阻塞 DAO 读循环）
  - "src/consumer/mod.rs"（init 注册：Consumer::new() 零参数，不再注入 DAL 弱引用）
  - "src/service/dal/wechat/impl.rs"（WechatDalImpl 瘦身：移除自存 callback 字段，start 参数 _callback 不再持有）
  - "src/service/dao/message_channel/sqlite.rs"（push_query_filters 渠道列表默认软删除过滤，2026-09 新增）
  - "src/service/dao/message_channel_conversation/sqlite.rs"（会话绑定 DAO：app+tenant+chat → channel，2026-11 新增）
  - "src/service/dal/lark/impl.rs"（入站会话优先路由 + 会话绑定 upsert + 提及映射 + 群聊语义信号，2026-11 新增）
  - "src/service/dal/lark/impl_tests.rs"（提及匹配 / 群聊语义信号单测，2026-11 新增）
  - "src/models/message_channel.rs"（ChannelConfig 群触发字段 group_trigger / group_trigger_prefix，2026-11 新增）
  - "src/producer/inbound_filter.rs"（渠道无关入站规则门与判定上下文拼接，2026-11 新增）
  - "src/service/domain/runtime/relevance_gate.rs"（默认小脑 Noul 判定与全降级放行，2026-11 新增）
  - "common/src/api/message_channel.rs"（创建 / 展示 DTO 群触发字段，2026-11 新增）
  - "common/src/models/external_message.rs"（ExternalMessageContext：外部元数据聚合，2026-11 新增）
  - "migrations/20261113000002_add_message_channel_conversations.sql"（会话绑定表，2026-11 新增）
  - "src/pkg/lark_integration.rs"（lark-cli config init 文本输出契约：去 --json + App ID 文本行解析）
  - "frontend/src/pages/finance/message_channels.rs"（渠道创建弹窗「绑定 Agent」选框：所有渠道类型一致展示）
  - "frontend/src/pages/finance/message_channel_detail.rs"（入站监听复选框统一为 checkbox checkbox-sm checkbox-primary）
source_files:

  - src/pkg/adapter/message.rs#L46-L78（MessageInboundAdapter trait：4 方法 channel_type()/start(callback)/stop()/is_running()；async_trait 标注；start 接收 Arc<dyn MessageAdapterCallback> 投递回调；重复启动 Conflict 错误）
  - src/service/dal/lark/impl.rs#L325-L405（adapt_lark 入站路由：find_channel_by_lark_identity 映射 from_id → find_channel_by_conversation 会话优先 → 未命中则 bind_conversation upsert；chat_id 私聊/群聊统一落库）
  - src/service/dal/lark/impl.rs#L600-L730（collect_bot_agent_map 唯一映射 + merge_bot_agent_binding 歧义标记 + find_channel_by_conversation/bind_conversation）
  - src/pkg/adapter/mod.rs#L80-L93（AdaptedMessage 渠道无关信号：channel_id / is_group / addressed_to_agent）
  - src/producer/message_channel.rs#L160-L410（producer 统一入站过滤：should_process_inbound + 群聊历史 + 短期记忆检索 + 渠道超时 + 小脑判定）
  - src/producer/inbound_filter.rs#L11-L185（纯规则 group_trigger_gate + 阈值放行 + 按模型容量 36/14.4/14.4/7.2 分配）
  - src/service/domain/runtime/relevance_gate.rs#L1-L26（场景口径，委托 DAL）
  - src/service/dal/brain.rs#L66-L190（judge_noul：默认小脑 Noul 判定、渠道超时配置、全降级返回 None、统计事件）
  - src/models/model_provider.rs#L131-L143（context_capacity_tokens：min(推荐上下文, 最大上下文) 唯一实现）
  - src/service/domain/runtime/cerebellum_context.rs#L90-L111（INBOUND_* 份额：36/14.4/14.4/7.2，子分配 ≤ BODY_BP）
  - src/service/dao/message_channel_conversation/sqlite.rs#L1-L90（会话绑定 DAO：upsert ON CONFLICT(channel_type, app_id, tenant, chat_id) + find_channel_id）
  - common/src/models/external_message.rs#L1-L45（ExternalMessageContext：external_key/thread_id/chat_id/external_mentions 聚合）

  - src/pkg/adapter/message.rs#L36-L44（MessageAdapterCallback：on_message(AdaptedMessage) 回调接口。入站适配器收到外部消息后统一为 AdaptedMessage，调用此回调投递；生产者层注册此回调将 AdaptedMessage 转换为 Agent 消息入站事件）
  - 'src/pkg/adapter/message.rs#L82-L100（MessageAdapterRegistry 注册中台：内部 adapters: RwLock<Vec<Arc<dyn MessageInboundAdapter>>>；register() / start_all() / stop_all() / list_running() 四个方法；start_all 内部遍历调用 adapter.start(callback) 遇到错误继续跑下一个不整组 failfast，单渠道挂不影响其他渠道）'

  - src/pkg/adapter/mod.rs#L47-L70（AdapterRegistry 父层注册中心：按 ChannelType 存储 Arc<dyn Any + Send + Sync>，支持 downcast。未来支持 Webhook Adapter/Slack Adapter 等多种适配器形态，供 producer 按渠道类型 downcast 取具体实现）

  - src/pkg/adapter/mod.rs#L21-L45（AdaptedMessage 基础设施层统一结构：channel_type + 外部消息原始负载 + 渠道侧用户标识 + 原始时间戳，owned 结构，producer 据此构造 SendToAgentCommand 投递内部消息系统）
  - 'src/producer/message_channel.rs:Ln-Lm（消息通道生产者：启动时调用 registry.start_all(inject_message_cb) → 收到 AdaptedMessage → 按渠道+open_id 找 MessageChannel → 反查用户/Agent → publish AOP 事件 NewMessage 触发 consumer；优雅关闭调用 stop_all）'

  - src/service/dal/lark.rs#L632-L710（LarkMessageChannelDal 实现 MessageInboundAdapter：channel_type = Lark；start = running RwLock 检查+置位+回调注入+启用的飞书渠道按 app_id 聚合+resolve_channel_credentials+调用 lark_dao 开启 WS；stop = running=false + 断开全部 WS；listener_stats() 透传 metrics 供系统健康面板）

  - src/service/dal/message_channel.rs#L329-L367（MessageChannelDalImpl push_to_channel：match ChannelType 分发出站调用（纯分发无 trait，漏加编译直接报错）。入站链路由 LarkMessageChannelDal 独立实现 MessageInboundAdapter，两者对称但独立）
  - src/service/dal/message_channel.rs (2026-09 增量：新增 CLEAR_* sentinel 通用清除标记 + scope_project + channel 双写落库修复断链)
  - src/service/domain/finance/message_channel.rs (2026-09 增量：domain 层清除逻辑，走通用清除哨兵而非手动置空 agent_id)
  - src/models/message_channel.rs (2026-09 增量：模型层新增 CLEAR_* sentinel 常量)
  - src/service/dal/wechat/impl.rs#L37-L46（WechatDalImpl struct：message_channel_dal / wechat_dao / credential_dao / running —— 已移除自存的 `callback: RwLock<Option<Arc<dyn MessageAdapterCallback>>>` 字段，投递回调统一由中台登记持有）
  - src/service/dal/wechat/impl.rs#L361（start(&self, _callback)：callback 参数用 `_` 前缀标记不再自存，入站投递回调统一从 `pkg::adapter::message::registry().current_callback()` 取用）
  - src/service/dal/wechat/mod.rs#L48-L63（WechatDalImpl.init 注册到 MessageAdapterRegistry：无条件注册，微信启停由渠道数据驱动）

  - src/service/domain/message/inbound.rs#L1-L66（【全新 2026-09 新增】入站消息适配门面：`InboundSource` 枚举收敛 Lark/Wechat 两种外部事件 + `MessageInboundAdapt` trait 定义 adapt_inbound 入口；MessageDomainImpl 通过该 trait 统一经 match source 分发到各渠道 DAL 的 adapt_lark / adapt_wechat 方法，返回 `Option<AdaptedMessage>` —— 过滤/未绑定时返回 None）
  - src/service/domain/message/mod.rs#L31（pub use inbound::{InboundSource, MessageInboundAdapt}）+ mod.rs#L45-L62（`new()` 构造函数新增 lark_dal / wechat_dal 两个 Arc 注入）+ mod.rs#L82-L112（MessageDomainImpl struct 新增 lark_dal / wechat_dal 字段）+ mod.rs#L123-L125（MessageDomain::inbound() → &dyn MessageInboundAdapt 入口）

  - src/consumer/lark_inbound.rs#L1-L73（【简化后】无状态 Consumer：`#[derive(Default)] LarkInboundConsumer` 零字段，无 DAL 引用。on_event 三步：① deserialize LarkInboundEvent → ② `message_domain::domain().inbound().adapt_inbound(ctx, InboundSource::Lark(...))` 适配 → ③ `crate::pkg::adapter::message::registry().current_callback()` 取回调投递。Async 模式不阻塞 WS 读循环。转换失败仅 log_error 不 nack 重试）
  - src/consumer/wechat_inbound.rs#L1-L80（【简化后】同上，`WechatInboundConsumer` 无状态，事件类型 WechatInboundEvent，Async 模式，转换失败记录 channel_id + message_key 便于排查）
  - src/consumer/mod.rs#L21-L58（init 注册所有 consumer：`Arc::new(xxx::XxxConsumer::new())` 零参数构造，不再向 consumer 注入 DAL 弱引用；lark_inbound / wechat_inbound 行注释明确标注"适配走 message domain 门面，投递回调经中台取用"）
  - 'common/src/enums/channel_type.rs:Ln-Lm（ChannelType enum：Lark/Wechat/Slack/Email/Webhook/A2aCallback 六渠道，与出站一致；新增入站渠道时扩展此枚举，MessageInboundAdapter 匹配）'

  - src/service/dao/message_channel/sqlite.rs#L338-L355（【2026-09 新增】push_query_filters 默认软删除过滤：未显式传 status_in 时追加 `AND status != 0` 排除 Deleted，已删除渠道不再泄漏到列表；显式 status_in 覆盖默认过滤，保留查历史/恢复的 query 逃生通道，与 message DAO 同范式）
  - src/pkg/lark_integration.rs#L480-L492（extract_app_id：lark-cli v1.0.94 建应用成功输出为文本行 `App configured! App ID: cli_xxx`（非 JSON），逐行定位 `App ID:` 取首个 token + `cli_` 前缀形制校验，避免误匹配帮助文本）
  - src/pkg/lark_integration.rs#L561-L567（start_bind_session：`config init --new` 去掉 `--json`——v1.0.94 传该 flag 以 unknown flag 立即退出，验证 URL 与 appId 双双丢失）
  - frontend/src/pages/finance/message_channels.rs#L498-L513（创建弹窗「绑定 Agent」选框：去掉 is_lark_type 门控，所有渠道类型一致展示——入站路由优先取渠道 agent_id，缺选框会让用户无法指定飞书渠道接单 Agent）
  - frontend/src/pages/finance/message_channel_detail.rs#L853-L868（入站监听复选框统一预置技能同步弹窗写法：`checkbox checkbox-sm checkbox-primary` + onchange 读 e.checked()，label 整行可点，修不可见/不可选）

  - docs/modules/04-对话与消息/A-设计/message_channel_design.md

  - docs/modules/04-对话与消息/B-落地方案/飞书P2P消息集成.md（v4 AOP 入站中台方案落地：MessageInboundAdapter trait + Registry + LarkEventDispatcher）

  - docs/modules/04-对话与消息/B-落地方案/lark-cli_集成二期.md（二期 WS 指数退避重连 + 凭证用户级管理 = 入站中台依赖的基础设施）

  - docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md

  - docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md

  - docs/wiki/zh/content/核心模块/服务层/领域层/消息领域.md

  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件生产者/消息通道生产者.md

  - docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md

  - docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Message 领域编排.md

  - 【平行卡1】身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用 docs/wiki/knowledge/zh/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用.md

  - 【平行卡2】AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容 docs/wiki/knowledge/zh/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容.md

  - 【平行卡3】Lark P2P WS 私信入站：resolve_channel_credentials + 用户身份自动映射 + 监听健康指标 docs/wiki/knowledge/zh/Lark%20P2P%20WS%20私信入站：身份凭证引用解析%20+%20app_id%20聚合%20WS%20+%20open_id%20自动映射%20+%20LarkWsMetrics%20健康指标/Lark%20P2P%20WS%20私信入站：身份凭证引用解析%20+%20app_id%20聚合%20WS%20+%20open_id%20自动映射%20+%20LarkWsMetrics%20健康指标.md

  - 【Level 4 细卡】InboundState 入站运行状态：动态游标 + 会话滚动刷新（本 Batch 2026-09-07 新增细卡，微信 iLink 等增量拉取型渠道的运行时状态持久化模型）docs/wiki/knowledge/zh/InboundState%20入站运行状态：动态游标%20+%20会话滚动刷新/InboundState%20入站运行状态：动态游标%20+%20会话滚动刷新.md

  - docs/wiki/zh/content/功能模块/消息系统/微信%20iLink%20专属渠道.md

---

# 消息渠道入站适配中台（MessageInboundAdapter + Registry + 生命周期）

## §1 整体方案
消息系统支持 6 种渠道（Lark/Wechat/Slack/Email/Webhook/A2aCallback）出站推送，但只有飞书（Lark）支持**私信入站**（其他渠道的入站要么是 Webhook HTTP 回调，要么还没做）。入站适配中台 = 把"多渠道监听生命周期管理"这件事从业务代码中抽出来，让所有未来新增入站渠道都按同一套流程接入，producer 层零改动。

**分层解耦（严格依赖方向，不允许 pkg/adapter 依赖业务层）**：
```
 producer（AOP 消息通道生产者，启停所有渠道，将 AdaptedMessage 转为入站事件）
     │  只依赖 pkg/adapter（基础设施层接口）
     ▼
 ┌─────────────────────────────────────────────────────────┐
 │ pkg/adapter（中台，纯基础设施，零业务）                    │
 │   - AdaptedMessage 统一消息转换结果                         │
 │   - MessageAdapterCallback 回调接口（投递 AdaptedMessage）  │
 │   - MessageInboundAdapter trait（渠道实现此接口）           │
 │   - MessageAdapterRegistry 全局中台（注册/启动/停止）       │
 │   - AdapterRegistry 父层注册中心（按 ChannelType 存 Any）   │
 └─────────────────────────────────────────────────────────┘
     ▲  ▲
     │  │  各渠道 DAL 实现 MessageInboundAdapter，init 时注册
 DAL 层：LarkMessageChannelDal / 未来 SlackWsDal / WechatWxDal ...

 ┌─────────────────────────────────────────────────────────┐
 │ 【2026-09 新增】MessageDomain 入站门面层（domain 层）       │
 │   - InboundSource 枚举：Lark(Box<LarkInboundEvent>)        │
 │     + Wechat(Box<WechatInboundEvent>) —— 收敛外部事件类型   │
 │   - MessageInboundAdapt trait：adapt_inbound(ctx, source)  │
 │     → Result<Option<AdaptedMessage>>                      │
 │   - MessageDomainImpl 统一持有 lark_dal + wechat_dal，      │
 │     通过 inbound() 方法暴露 trait 入口                      │
 └─────────────────────────────────────────────────────────┘
     ▲
     │  Consumer → domain 门面 → 渠道 DAL（统一经枚举分发）
 ┌─────────────────────────────────────────────────────────┐
 │ 【2026-09 简化后】Consumer 层：无状态化                    │
 │   - LarkInboundConsumer / WechatInboundConsumer             │
 │     均为 #[derive(Default)] 零字段，不再持有 DAL 引用        │
 │   - on_event 两步走：                                       │
 │     ① message_domain::domain().inbound().adapt_inbound()   │
 │     ② registry().current_callback().on_message()           │
 │   - consumer init 注册零参数：Consumer::new()，             │
 │     不再注入 DAL 弱引用                                     │
 └─────────────────────────────────────────────────────────┘
```
- **pkg/adapter（中台）** = 纯基础设施，没有任何业务类型引用。它不知道什么是"Agent"、"用户"、"MessagePo"，只知道"渠道类型 + AdaptedMessage + 回调"。所有类型在 pkg/adapter 内定义。
- **DAL 层（渠道实现）** = 把飞书 WS 事件 / Slack SocketMode 事件 / 微信 回调 转成统一 AdaptedMessage，调用 callback.on_message()。DAL 依赖 pkg/adapter（接口层），不反向依赖。
- **producer（消息通道生产者）** = 启动时注入回调（`on_message(adapted_msg) → 找渠道 → 找用户/Agent → 发布 NewMessage AOP 事件`），调用 MessageAdapterRegistry.start_all 让所有渠道一起启动；优雅关闭调用 stop_all。

- **Consumer（组装层，2026-09 简化后）** = 原来 lark_inbound / wechat_inbound consumer 都持有渠道 DAL 弱引用，consumer.init() 时注入。现在变成无状态 `#[derive(Default)]`，consumer 不再持有任何 DAL 引用。统一经 `message_domain::domain().inbound().adapt_inbound(ctx, InboundSource::Lark/Wechat(...))` 走 domain 门面做协议转换 + 渠道定位 + 用户映射；投递回调统一经 `crate::pkg::adapter::message::registry().current_callback()` 从中台登记取。consumer init 时零参数构造：`LarkInboundConsumer::new()` / `WechatInboundConsumer::new()`。Async 模式（慢业务不阻塞 DAO 读循环）。

**Consumer 无状态化 + domain 门面消费模式**：
```rust
// 简化前：consumer 持有 DAL 弱引用
// struct LarkInboundConsumer { lark_dal: Weak<LarkDalImpl> }
// async fn on_event(&self, ...) { let dal = self.lark_dal.upgrade().await; dal.adapt_lark(...); }

// 简化后：consumer 零字段
#[derive(Default)]
pub struct LarkInboundConsumer;

async fn on_event(&self, ctx: RequestContext, event: LarkInboundEvent) -> Result<()> {
    // ① 入站适配：domain 门面收敛
    let adapted = message_domain::domain()
        .inbound()
        .adapt_inbound(ctx, InboundSource::Lark(Box::new(event)))
        .await?;

    // ② 投递：中台回调统一入口
    if let Some(msg) = adapted {
        crate::pkg::adapter::message::registry().current_callback().unwrap().on_message(msg).await?;
    }
    Ok(())
}
```
- **InboundSource 枚举收敛**：新入站渠道（如 Slack）只需在 `InboundSource` 加 variant + 在 `MessageInboundAdapt::adapt_inbound` 的 match 加分支，consumer 层零改动。比之前 consumer 各持有各 DAL 引用的方式更易扩展。
- **WechatDalImpl 瘦身**：移除了自存的 `callback: RwLock<Option<Arc<dyn MessageAdapterCallback>>>` 字段——投递回调统一由中台 registry 登记持有，消费侧经中台取用。MessageInboundAdapter::start 的 callback 参数仍保留（trait 契约），但 WechatDalImpl 用 `_callback` 标记不再自存。

**新增入站渠道的 3 步流程（中台设计的核心目标 = 降低扩展成本）**：
1. DAL 层实现 `MessageInboundAdapter` trait 的 4 方法（channel_type/start/stop/is_running）
2. DAL init 时调用 `MessageAdapterRegistry::global().register(Arc<MyNewAdapter>)` 注册
3. producer 零改动！下一次服务启动 start_all 会自动包含新渠道，回调按 ChannelType 分发

**MessageInboundAdapter trait 4 方法与契约**：
```rust
#[async_trait]
pub trait MessageInboundAdapter: Send + Sync {
    fn channel_type(&self) -> ChannelType;                      // Lark/WeChat/Slack
    async fn start(&self, callback: Arc<dyn MessageAdapterCallback>) -> Result<()>; // 启动监听（重复启动=Conflict）
    async fn stop(&self) -> Result<()>;                          // 停止监听（未启动返回 Ok）
    fn is_running(&self) -> bool;                                  // 健康面板用
}
```
- **start 幂等冲突检测**：先 `running: RwLock<bool>` 写锁检查，若已有 running=true → 返回 `err!(Conflict, "<channel> adapter already running")`。防止管理员在前端点两次"启动渠道"。
- **start 内部多子渠道启动（Lark 典型）**：一个 MessageInboundAdapter = 一种渠道类型（Lark），但系统内可能建了 10 个不同飞书应用/用户的 MessageChannel。start 内部自己聚合去重（Lark 按 app_id），同一 app_id 共享同一条 WS 连接，多个渠道共用。
- **start_all 非 fail-fast**：一个渠道 start 失败（例如飞书凭证过期 → 启动报错），MessageAdapterRegistry.start_all 内部 catch 错误并记录日志，然后继续下一个渠道。不让单个渠道故障导致所有渠道都起不来。

**AdaptedMessage 统一入站消息（中台统一转换结果，owned，2026-11 收敛）**：
```rust
pub struct AdaptedMessage {
    pub from_id: String,                 // 外部用户映射后的内部用户 ID（from_role 恒 User）
    pub to_agent_id: Option<String>,     // 渠道绑定 agent；None 时 consumer 走角色路由
    pub channel_type: ChannelType,       // 来自哪个渠道
    pub content: String,                 // 纯文本内容（富文本已摊平）
    pub reply_to_id: Option<String>,     // 消息链父消息
    pub external: ExternalMessageContext, // 渠道无关外部元数据聚合（见下）
    pub attachments: Vec<AdaptedAttachment>,
}

pub struct ExternalMessageContext {
    pub external_key: Option<String>,    // 平台消息 ID（`lark:om_xxx`，渠道前缀防撞键）
    pub thread_id: Option<String>,       // 平台话题 ID（飞书 `omt_xxx`）
    pub chat_id: Option<String>,         // 平台会话 ID（飞书私聊/群聊均有 `oc_xxx`）
    pub external_mentions: Vec<ExternalMention>, // 平台 @ 提及原始信息
}
```
producer 侧拿到 AdaptedMessage 后：渠道 DAL（如 `adapt_lark`）已完成「发送者 → 内部用户」映射，并按 **`(channel_type, app_id, tenant_key, chat_id)` 会话绑定** 优先路由到渠道 / Agent；未绑定时回退 `(app_id, sender_open_id)` 定位渠道，成功后把会话绑定 upsert 到 `message_channel_conversations`。producer 只负责把 `external` 原样搬进 `SendToAgentCommand`。

**回调解耦：MessageAdapterCallback（中台不依赖 producer，用 trait 注入）**：
- 中台定义 trait `MessageAdapterCallback { async fn on_message(&self, msg: AdaptedMessage) -> Result<()> }`；producer 实现它，把 AdaptedMessage 转成内部 NewMessage 事件。
- 这样未来写单元测试时可以给 start() 注入 MockCallback，无需真实启动 producer 链路。

**系统健康面板指标对接**：
- MessageAdapterRegistry 提供 list_running() → 返回各渠道状态；producer 聚合后通过 `/api/v1/system/health/aop` 对外暴露（包括 Lark Ws Metrics：当前连接数/重连次数/入站消息计数/last_ping 延迟等）。
- LarkMessageChannelDal 有 `listener_stats() -> LarkWsMetrics` 透传 lark_dao 的连接监控。

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [pkg/adapter/message.rs](src/pkg/adapter/message.rs) | 中台：trait + 注册中心 | MessageInboundAdapter ~L46；MessageAdapterCallback ~L36；MessageAdapterRegistry（start_all/stop_all/register）~L82 |
| [pkg/adapter/mod.rs](src/pkg/adapter/mod.rs) | 中台：父层 AdaptedMessage + AdapterRegistry 通用注册表 | AdaptedMessage ~L21；AdapterRegistry HashMap<ChannelType, Arc<dyn Any>> ~L47 |
| [service/domain/message/inbound.rs](src/service/domain/message/inbound.rs) | 【2026-09 新增】domain 层入站适配门面 | InboundSource 枚举（Lark/Wechat）L24；MessageInboundAdapt trait L36；impl 分发到 lark_dal.adapt_lark / wechat_dal.adapt_wechat L51 |
| [service/domain/message/mod.rs](src/service/domain/message/mod.rs) | domain 层：MessageDomainImpl 聚合含 lark_dal/wechat_dal | MessageDomainImpl struct 新增字段 L82；new() 构造注入 L45；inbound() trait 入口 L123 |
| [dal/lark.rs](src/service/dal/lark/mod.rs) | Lark 渠道：MessageInboundAdapter 实现 + start 多应用聚合 | impl MessageInboundAdapter for LarkMessageChannelDal ~L632；start 内：query_enabled_lark_channels + 按 app_id 聚合去重 + resolve_channel_credentials + lark_dao 开 WS |
| [dal/wechat/impl.rs](src/service/dal/wechat/impl.rs) | 微信渠道：MessageInboundAdapter 实现（**瘦身**）| WechatDalImpl struct L37：已无自存 callback 字段；start(&self, _callback) L361：callback 不再持有，统一经 registry().current_callback() 取用 |
| [dal/message_channel.rs](src/service/dal/message_channel.rs) | 消息渠道 DAL：出站分发（对照参考）| push_to_channel ChannelType match 纯分发（入站走独立 trait）~L329 |
| [consumer/lark_inbound.rs](src/consumer/lark_inbound.rs) | 【简化后】无状态 Lark 入站 Consumer | `#[derive(Default)] LarkInboundConsumer` L19；on_event 两步：domain 门面适配 → registry 回调投递 L43-L72 |
| [consumer/wechat_inbound.rs](src/consumer/wechat_inbound.rs) | 【简化后】无状态微信入站 Consumer | 同上 L18-L79 |
| [consumer/mod.rs](src/consumer/mod.rs) | Consumer 注册中心：零参数构造 | init() 注册 LarkInboundConsumer::new() / WechatInboundConsumer::new() L21-L58；不再注入 DAL 弱引用 |
| [producer/message_channel.rs](src/producer/message_channel.rs) | AOP 消息通道生产者：注入回调 + start_all/stop_all + AdaptedMessage → NewMessageEvent 映射 | init 阶段注册回调（回调被中台 registry 登记，consumer 经 current_callback() 取用）|
| 【① Design】message_channel_design.md（入站+出站全链路架构）| 入站适配器中台 vs 出站 push_to_channel 双路径对称设计 | docs/modules/04-对话与消息/A-设计/message_channel_design.md |
| 【③ Wiki 长文 1】消息渠道适配器.md | 新增入站渠道三步流程（用户视角） | docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md |
| 【③ Wiki 长文 2】多渠道消息系统.md | 入站+出站全链路图 | docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md |
| 【③ Wiki 长文 3】消息通道生产者.md | AOP 事件生产者如何调用 start_all 并把 AdaptedMessage 映射为内部事件 | docs/wiki/zh/content/基础设施/AOP%20事件系统/事件生产者/消息通道生产者.md |
| 【③ Wiki 长文 4】消息渠道管理.md | 前端管理员创建/绑定渠道到 open_id，lark_listen_inbound 开关 | docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md |
| 【平行卡 1】身份凭证 Domain 统一 CRUD（飞书渠道凭证引用解析依赖）| 渠道只存 lark_credential_id 引用，不存明文凭证 | source_files[] 尾平行卡1 路径 |
| 【平行卡 2】AES-256-GCM 敏感字段加密（凭证引用解析后拿到的 secret 要解密使用）| encrypt_channel_secret 闭包注入 | source_files[] 尾平行卡2 路径 |
| [dal/message_channel.rs](src/service/dal/message_channel.rs) (2026-09 增量：scope_project 双写落库断链修复 + CLEAR 哨兵) | MessageChannel DAO 清除绑定 | DAO 落库必须同时更新 scope_project + channel 双表的关联元数据（agent_id / user_id / credential_id 等）；新增 CLEAR_* sentinel 通用清除标记（禁止 handler 手动置空）|
| [domain/finance/message_channel.rs](src/service/domain/finance/message_channel.rs) (2026-09 增量) | domain 层清除逻辑 | 解除 Agent 绑定必须走通用清除哨兵而非手动 UPDATE 置空 agent_id，防止悬空引用残留 |
| [models/message_channel.rs](src/models/message_channel.rs) (2026-09 增量) | 模型层 CLEAR sentinel | 新增 CLEAR_AGENT_ID / CLEAR_USER_ID / CLEAR_CREDENTIAL_ID 等通用清除哨兵常量，DAO 层据此识别"清除"语义 vs 正常 NULL |

## §3 架构约定

1. **pkg/adapter 必须纯基础设施层（零业务依赖）**：禁止在 pkg/adapter 目录中引入任何 MessagePo、Agent、User、MessageChannelDal 等业务类型。这些类型一律在 producer 层或 DAL 层处理。如果未来你需要新增 AdaptedMessage 字段，请仔细确认它是否真的不包含业务语义。
2. **start_all 的非 fail-fast 是硬要求**：一个渠道挂了不能拖垮其他渠道。内部用 match adapter.start() 捕获 Err 后 log_error!，然后 continue 到下一个。绝不能 `?` 把错误冒泡出去。
3. **ChannelType 枚举与 MessageChannel 出站共用同一个**：新增渠道（如 DingTalk）时，先扩展 common::ChannelType，然后出站写 push_to_channel match，入站写 MessageInboundAdapter 实现，两个路径都写齐全。
4. **一个 ChannelType 只允许注册一个 MessageInboundAdapter**（但该适配器内部可以管理 N 个子连接、N 个 app_id）。如果 register 重复注册同类型，Registry 内部返回错误（保护扩展者，防止 init 两次注册）。
5. **AdaptedMessage 永远是 owned 结构**，不借用外部消息对象池（外部消息可能是 WS 事件中的借用字段，生命周期很短）。转换时全部 clone 成 owned 字符串。
6. **Consumer 无状态化是架构硬要求（2026-09 引入）**：所有入站 consumer（LarkInboundConsumer / WechatInboundConsumer / 未来 SlackInboundConsumer）必须 `#[derive(Default)]` 零字段，**禁止** consumer 持有任何渠道 DAL 引用（弱引用或强引用均禁止）。渠道 DAL 的获取统一经 `message_domain::domain().inbound()` 门面间接路由，consumer init 时零参数注册 `Consumer::new()`。
7. **Domain 门面统一消费模式**：新增入站渠道时，扩展者必须：① 在 `InboundSource` 加 variant ② 在 `MessageInboundAdapt::adapt_inbound` match 加分支 ③ MessageDomainImpl 构造函数加新 DAL 字段。**禁止** consumer 层绕过 domain 门面直连渠道 DAL。这条约定的目的是把渠道分发逻辑集中在 domain 层，不分散在各 consumer 中。
8. **投递回调单一来源（2026-09 引入）**：consumer 投递 AdaptedMessage 统一经 `pkg::adapter::message::registry().current_callback()` 取用，**禁止** consumer 自行持有或构造 MessageAdapterCallback。WechatDalImpl 已移除自存 callback 字段，lark_dao / 未来 slack_dao 等也应遵循"只发布事件、不持有回调"的模式。
9. **（2026-09 scope_project 断链修复）MessageChannel DAO 落库必须 scope_project + channel 双写**：agent_id / user_id / credential_id 等关联元数据同时更新 scope_project 关联表和 message_channel 主表，禁止只改其中一表导致"scope_project 残留旧值、channel 已更新"的悬空断链。
10. **（2026-09 CLEAR 哨兵）渠道解除绑定必须走模型层 CLEAR_* sentinel**：`CLEAR_AGENT_ID` / `CLEAR_USER_ID` / `CLEAR_CREDENTIAL_ID` 等哨兵值表示"显式清除"语义，DAO 层据此走双表清除逻辑；handler 层禁止手动 UPDATE SET agent_id = NULL（与"本就无绑定"的 NULL 语义混淆，且可能漏改 scope_project 表）。
11. **（2026-09 渠道列表软删除过滤）MessageChannelQuery DAO 侧必须默认排除软删除渠道**：`push_query_filters` 在未显式传 `status_in` 时追加 `AND status != 0`（Deleted），COUNT 与 LIST 复用同一函数；只有显式 `status_in` 才绕过该默认过滤（查历史/恢复用 query 逃生通道）。禁止让已删除渠道泄漏到管理列表。
12. **（2026-09 绑定 Agent 统一入口）渠道创建/编辑弹窗必须对所有渠道类型展示「绑定 Agent」选框**：入站路由本就优先取渠道 `agent_id`（见下游 Lark 卡的 open_id → channel.agent_id 映射），若飞书渠道缺该选框，用户无法指定接单 Agent。前端禁止用 `is_lark_type` 等门控把该选框对飞书隐藏。
13. **（2026-11 会话路由）入站路由主键是「应用 + 租户 + 会话」，不是发送者**：渠道 DAL 必须先查 `message_channel_conversations`（`channel_type + external_app_id + external_tenant_key + external_chat_id` → `channel_id`），命中且渠道 Active 即用它；未命中才回退发送者身份并 upsert 绑定。发送者只负责 `from_id` 映射。新增渠道禁止再写「只按发送者查渠道」的路由。
14. **（2026-11 外部元数据聚合）新增外部字段只扩展 `ExternalMessageContext`**：`AdaptedMessage` / `SendToAgentCommand` 不再逐字段平铺平台元数据；新增 chat/thread/mention 类字段一律加进聚合结构，避免所有命令构造点被迫同步修改。
15. **（2026-11 提及映射确定性）机器人 open_id → Agent 映射必须无歧义**：优先用当前渠道的 `lark_bot_open_id → channel.agent_id`；跨渠道兜底时，同一 bot 映射到多个 Agent 视为歧义，保留 `@name` 展示，禁止按渠道遍历顺序覆盖。
16. **（2026-11 平台前缀统一）外部键前缀一律取 `ChannelType::as_str()`**：如 `lark:om_xxx`、`lark:oc_xxx`，禁止在适配器里手写平台字符串，避免不同渠道出现 `feishu:` / `lark:` 混用。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 MessageInboundAdapter::start 内部 panic**。外部渠道消息格式异常、凭证解析失败，都必须 wrap 成 Err 返回（或 log_warn 跳过单条消息），绝不允许直接 unwrap/panic 导致整个进程崩溃。
2. ❌ **禁止 MessageAdapterRegistry.start_all 对单渠道失败用 ? 冒泡**。冒泡会导致：飞书凭证过期 → 启动失败 → 微信/Slack 也起不来 → 全部渠道挂了 → P0 级故障。必须：match + log_error + continue。
3. ❌ **禁止 pkg/adapter 目录中引入 common::api::** 或 service/domain 层业务类型（MessagePo/Agent 等）。业务类型污染中台 = 分层被破坏，未来想加 Slack 的 AdapterRegistry 单元测试就必须把整个业务服务拉起来，测试隔离做不出来。
4. ✅ **发送者必须先映射到内部用户才能入站**：渠道 DAL 按 `(app_id, sender_open_id)` 定位绑定渠道取其 `user_id`；未绑定发送者 log 丢弃不 panic（外部用户没在系统内绑定渠道只是无法触发接待，不应抛错）。会话绑定失败也不阻断入站，仅 log_warn。
5. ✅ **会话绑定必须按应用 + 租户隔离**：`message_channel_conversations` 唯一键含 `channel_type + external_app_id + external_tenant_key + external_chat_id`；禁止只用 chat_id 做键，否则多应用 / 多租户会串会话。
5. ✅ **Lark 渠道实现中 resolve_channel_credentials 返回 None 时要 warn 并跳过该渠道**：管理员渠道创建时引用的凭证被删除了 → 不能 panic、不能让启动失败；记录一条"channel id={} lark_credential_id={} 找不到凭证"跳过即可。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 5 篇 wiki 长文 + 1 Design + Plan 占位 + 3 张平行卡（身份凭证 CRUD / AES 加密 / Lark WS P2P 入站）；对应 Wiki 长文 cite 段回链本卡 + message_channel_design Design + 3 张平行卡。
7. ✅ **（2026-09-XX 新增）MessageChannel DAO 落库必须 scope_project + channel 双表统一更新关联元数据**（agent_id / user_id / credential_id），禁止只改 channel 主表导致 scope_project 残留旧值悬空引用。
8. ✅ **（2026-09-XX 新增）渠道解除 Agent 绑定必须走模型层 CLEAR_* sentinel**（`CLEAR_AGENT_ID` / `CLEAR_USER_ID` / `CLEAR_CREDENTIAL_ID`），禁止 handler 手动 UPDATE SET agent_id = NULL——避免与"本就无绑定"的 NULL 语义混淆，且保证 DAO 层双表清除逻辑被正确触发。
9. ✅ **（2026-09-29 新增）渠道列表读侧必须默认软删除过滤**：`push_query_filters` 未传 `status_in` 时 `AND status != 0`，禁止返回 Deleted 渠道；显式 `status_in` 保留逃生通道（查历史/恢复）。DAO 单测覆盖「默认排除 Deleted」与「显式 status_in 生效」两分支。
10. ✅ **（2026-09-29 新增）入站监听开关必须对所有渠道类型可见可选**：前端复选框统一预置技能同步弹窗写法（`checkbox checkbox-sm checkbox-primary` + `onchange` 读 `e.checked()`，label 整行可点），禁止用 `toggle` + 信号盲切导致不可见/双触发；飞书 `config init` 必须不带 `--json`（v1.0.94 会 unknown flag 退出），appId 从文本行 `App ID: cli_xxx` 解析。

## §5 历史演进

| 日期 | 事件 | 变更内容 | 关联文件 |
|------|------|---------|---------|
| 2026-09 之前 | 初版 | MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all/stop_all 生命周期。consumer（lark_inbound / wechat_inbound）各自持有渠道 DAL 弱引用，on_event 内 upgrade 后直连 DAL 做 adapt，投递回调也自存一份 | src/pkg/adapter/message.rs；src/consumer/lark_inbound.rs（旧版含 Weak<LarkDalImpl> 字段）；src/consumer/wechat_inbound.rs（旧版含 Weak<WechatDalImpl> 字段）；src/service/dal/wechat/impl.rs（旧版含 callback: RwLock<Option<Arc<dyn MessageAdapterCallback>>> 自存字段） |
| 5db51263 + 97a482b1 | 2026-09 | 渠道解除 Agent 绑定 + scope_project DAO 落库断链修复：models/message_channel.rs 新增 CLEAR_* sentinel 通用清除标记；dal/message_channel.rs 双表统一更新 scope_project + channel 关联元数据（agent_id / user_id / credential_id）；domain/finance/message_channel.rs 清除逻辑走哨兵而非手动置空 | src/models/message_channel.rs；src/service/dal/message_channel.rs；src/service/domain/finance/message_channel.rs |
| **2026-09（本次增量）** | **Consumer 架构简化 + Domain 门面层引入** | 三件事同时落地：① 新增 `src/service/domain/message/inbound.rs`（66 行）：InboundSource 枚举收敛 Lark/Wechat + MessageInboundAdapt trait adapt_inbound 入口 ② MessageDomainImpl 构造新增 lark_dal / wechat_dal 字段，统一经 domain 门面消费 ③ consumer 无状态化：`#[derive(Default)]` 零字段，consumer init 零参数构造；适配走 domain 门面、投递回调统一经 registry().current_callback() 取用；WechatDalImpl 移除自存 callback 字段 | src/service/domain/message/inbound.rs（全新）；src/service/domain/message/mod.rs（新增字段 + new() 构造 + inbound() trait 入口）；src/consumer/lark_inbound.rs / wechat_inbound.rs（零字段重写）；src/consumer/mod.rs（init 注册零参数）；src/service/dal/wechat/impl.rs（移除 callback 字段） |
| **405ba0bd / e1e65baf** | **2026-09-29 渠道管理三处修复** | ① `push_query_filters` 补默认软删除过滤（未显式 status_in 时 `AND status != 0`），已删除渠道不再泄漏 ② 渠道创建弹窗「绑定 Agent」选框去掉 `is_lark_type` 门控，所有渠道类型一致展示 ③ 入站监听复选框统一为 `checkbox checkbox-sm checkbox-primary` + `e.checked()`；`lark-cli config init` 去掉 `--json`（v1.0.94 unknown flag 退出）改文本行解析 appId | src/service/dao/message_channel/sqlite.rs；frontend/src/pages/finance/message_channels.rs；frontend/src/pages/finance/message_channel_detail.rs；src/pkg/lark_integration.rs |

**本次简化的核心收益**：
- Consumer 测试成本大幅降低：无状态 struct 无需构造依赖、无需处理 Weak upgrade 错误分支，单元测试直接 `LarkInboundConsumer::new().on_event(...)` 即可。
- 新渠道扩展更收敛：之前要"加 DAL 字段 → 改 consumer init → 改 consumer 持有 → 改 on_event 分发"四步；现在只需"InboundSource 加 variant → adapt_inbound 加 match 分支 → MessageDomainImpl 加字段"三步，**consumer 层零改动**。
- 回调所有权更清晰：之前 WechatDalImpl 自存 callback、consumer 也可能持有一份（不同持有方导致"谁负责释放"不明确）；现在统一由中台 registry 登记持有，consumer / DAL 均不直接持有，生命周期单一可控。

## §6 渠道会话列表接口（2026-11）

入站路由的「应用 + 租户 + 会话 → 渠道」绑定记录通过渠道子资源分页暴露给前端，用于核对某渠道当前服务的私聊 / 群会话。

| 项 | 约定 |
|----|------|
| 路由 | `GET /api/v1/finance/message-channels/{id}/conversations?limit=&offset=` |
| 响应 | `PagedResult<MessageChannelConversationItem>`（items + total） |
| 归属 | 渠道必须属于当前组织 + 当前用户，否则统一 NotFound（不泄漏存在性） |
| 分页 | 默认 20，上限 100，按 `created_at DESC`；limit/offset 由 `PaginationParams` 承载 |
| 前端 | 渠道详情页「绑定的会话」卡片，上一页 / 下一页 + 「第 X-Y 条 / 共 N 条」 |
| 分层 | Handler → `MessageChannelManage::list_channel_conversations` → `MessageChannelDal::list_channel_conversations` → `MessageChannelConversationDao` |

**红线**：列表只返回 `channel_id` 命中的绑定；禁止按 `app_id` / `chat_id` 反查其他渠道的会话（跨租户 / 跨渠道泄漏）。会话绑定依附渠道，渠道软删除后列表自然不再包含其记录（查询按 channel_id，不做物理清理）。

## §7 群消息统一入站过滤与小脑相关性门（2026-11）

过滤不在各渠道适配层，而在 producer 的统一入站汇聚点：适配层只做协议转换，输出渠道无关信号 `channel_id` / `is_group` / `addressed_to_agent`，后续规则对所有渠道一致生效。

| 策略 `group_trigger` | 规则 | 备注 |
|---|---|---|
| `mention`（缺省） | `addressed_to_agent` 为真才放行（适配器翻译平台语义，如飞书 @ 机器人） | 平台无法判定归属时适配器填 `true`，按放行降级 |
| `prefix` | 正文 `trim_start` 后以 `group_trigger_prefix` 开头才放行 | 前缀为空时降级为 `mention` |
| `all` | 规则层全部放行，再交小脑判断 | 飞书需 `im:message.group_msg` 权限 |

判定分两段，纯规则与副作用分离：

- `producer::inbound_filter::group_trigger_gate`（纯函数）：私聊恒 `Process`；群聊按上表返回 `Process / Skip / Judge`，可直接单测。
- `MessageChannelProducer::should_process_inbound`：读取渠道配置执行规则，仅 `Judge` 时调用 `judge_inbound_relevance`。

小脑判断（可选增强，任何降级都放行）：

- 域层 `judge_relevance` 声明场景口径，委托 `dal::brain::judge_noul` 取 `cerebellum::try_dao()` + `model_provider::try_dao()` 的默认小脑 provider；未初始化 / 无 provider / `api_key` 为空 → 放行。
- 问题 id `inbound_relevance.primary`，`QuestionType::Noul`；state 携带组装好的判定上下文。
- 上下文由 `build_group_context` 拼成四段：按 `chat_id` 的最近 10 条消息（有话题时叠加 `thread_id` 兼容过滤）、正文检索到的 3 条相关短期记忆、Agent 最近 5 条短期记忆、本次消息与发送者。
- 上下文预算：容量取默认小脑 provider 的 `min(推荐上下文, 最大上下文)`，按万分比 `群聊 36% / 相关记忆 14.4% / 最近记忆 14.4% / 本次消息 7.2%`（合计 72%，不挤占判据骨架）；单条摘要片段 4%；容量未配置则不裁。
- 作用域：消息按 `chat_id + organization_id` 查询（不按 agent 过滤，保留群内其他成员发言）；短期记忆按 `agent_id` 查询。
- 超时取渠道配置 `group_trigger_timeout_ms`（缺省 1000ms，收敛 200~5000ms），经 `tokio::time::timeout` 控制；概率阈值 0.5，超时 / 调用失败 / 答案缺失 → 放行。
- 设计取舍：宁可多回一条，不可静默吞消息；小脑不是主流程依赖，故障不得阻断入站。

配置链路：`ChannelConfig.group_trigger` / `group_trigger_prefix` / `group_trigger_timeout_ms` 为渠道无关顶层配置，创建 / 更新 / 详情响应 DTO 双向透传；前端创建与编辑弹窗提供策略下拉（`mention` / `all` / `prefix`）、条件前缀与 `all` 模式超时输入。

**红线**：过滤逻辑不得下沉到单个渠道适配层；适配层只输出渠道无关信号。`all` 模式的小脑调用必须有超时与降级；`prefix` 为空不得当成「匹配一切」，必须回落到 `mention` 规则。
