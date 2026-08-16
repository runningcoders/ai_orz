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
source_files:
  - src/pkg/adapter/message.rs#L46-L78（MessageInboundAdapter trait：4 方法 channel_type()/start(callback)/stop()/is_running()；async_trait 标注；start 接收 Arc<dyn MessageAdapterCallback> 投递回调；重复启动 Conflict 错误）
  - src/pkg/adapter/message.rs#L36-L44（MessageAdapterCallback：on_message(AdaptedMessage) 回调接口。入站适配器收到外部消息后统一为 AdaptedMessage，调用此回调投递；生产者层注册此回调将 AdaptedMessage 转换为 Agent 消息入站事件）
  - src/pkg/adapter/message.rs#L82-L100（MessageAdapterRegistry 注册中台：内部 adapters: RwLock<Vec<Arc<dyn MessageInboundAdapter>>>；register() / start_all() / stop_all() / list_running() 四个方法；start_all 内部遍历调用 adapter.start(callback) 遇到错误继续跑下一个不整组 failfast，单渠道挂不影响其他渠道）
  - src/pkg/adapter/mod.rs#L47-L70（AdapterRegistry 父层注册中心：按 ChannelType 存储 Arc<dyn Any + Send + Sync>，支持 downcast。未来支持 Webhook Adapter/Slack Adapter 等多种适配器形态，供 producer 按渠道类型 downcast 取具体实现）
  - src/pkg/adapter/mod.rs#L21-L45（AdaptedMessage 基础设施层统一结构：channel_type + 外部消息原始负载 + 渠道侧用户标识 + 原始时间戳，owned 结构，producer 据此构造 SendToAgentCommand 投递内部消息系统）
  - src/producer/message_channel.rs:Ln-Lm（消息通道生产者：启动时调用 registry.start_all(inject_message_cb) → 收到 AdaptedMessage → 按渠道+open_id 找 MessageChannel → 反查用户/Agent → publish AOP 事件 NewMessage 触发 consumer；优雅关闭调用 stop_all）
  - src/service/dal/lark.rs#L632-L710（LarkMessageChannelDal 实现 MessageInboundAdapter：channel_type = Lark；start = running RwLock 检查+置位+回调注入+启用的飞书渠道按 app_id 聚合+resolve_channel_credentials+调用 lark_dao 开启 WS；stop = running=false + 断开全部 WS；listener_stats() 透传 metrics 供系统健康面板）
  - src/service/dal/message_channel.rs#L329-L367（MessageChannelDalImpl push_to_channel：match ChannelType 分发出站调用（纯分发无 trait，漏加编译直接报错）。入站链路由 LarkMessageChannelDal 独立实现 MessageInboundAdapter，两者对称但独立）
  - common/src/enums/channel_type.rs:Ln-Lm（ChannelType enum：Lark/Wechat/Slack/Email/Webhook/A2aCallback 六渠道，与出站一致；新增入站渠道时扩展此枚举，MessageInboundAdapter 匹配）
  - docs/archive/design-archive/message_channel_design.md
  - docs/archive/plan-archive/飞书P2P消息集成.md（v4 AOP 入站中台方案落地：MessageInboundAdapter trait + Registry + LarkEventDispatcher）
  - docs/archive/plan-archive/lark-cli_集成二期.md（二期 WS 指数退避重连 + 凭证用户级管理 = 入站中台依赖的基础设施）
  - docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md
  - docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/消息领域.md
  - docs/wiki/zh/content/基础设施/AOP 事件系统/事件生产者/消息通道生产者.md
  - docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md
  - docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Message 领域编排.md
  - 【平行卡1】身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用 docs/wiki/knowledge/zh/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用.md
  - 【平行卡2】AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容 docs/wiki/knowledge/zh/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容.md
  - 【平行卡3】Lark P2P WS 私信入站：resolve_channel_credentials + 用户身份自动映射 + 监听健康指标（本 Batch 卡 4）
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
```
- **pkg/adapter（中台）** = 纯基础设施，没有任何业务类型引用。它不知道什么是"Agent"、"用户"、"MessagePo"，只知道"渠道类型 + AdaptedMessage + 回调"。所有类型在 pkg/adapter 内定义。
- **DAL 层（渠道实现）** = 把飞书 WS 事件 / Slack SocketMode 事件 / 微信 回调 转成统一 AdaptedMessage，调用 callback.on_message()。DAL 依赖 pkg/adapter（接口层），不反向依赖。
- **producer（消息通道生产者）** = 启动时注入回调（`on_message(adapted_msg) → 找渠道 → 找用户/Agent → 发布 NewMessage AOP 事件`），调用 MessageAdapterRegistry.start_all 让所有渠道一起启动；优雅关闭调用 stop_all。

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

**AdaptedMessage 统一入站消息（中台统一转换结果，owned）**：
```rust
pub struct AdaptedMessage {
    pub channel_type: ChannelType,     // 来自哪个渠道
    pub external_user_id: String,      // 渠道侧用户标识（飞书 open_id、微信 open_id 等）
    pub external_message_id: String,   // 渠道侧消息 ID（用于去重与回复引用）
    pub content: String,               // 纯文本内容（富文本/图片附件后续扩展 content_type + attachments）
    pub received_at: i64,              // 渠道侧接收时间戳
    pub channel_binding_id: Option<String>, // 绑定到哪个 MessageChannel 记录（若已解析可提前注入）
}
```
producer 侧拿到 AdaptedMessage 后：按 `(channel_type, external_user_id)` → 从消息渠道表查该外部 ID 绑定到哪个 `MessageChannel.po.lark_open_id` → 反查渠道归属 user_id / agent_id → 构造内部 `MessagePo`（sender = 外部用户，receiver = 对应 Agent）→ publish AOP 事件 `NewMessageCreated` → 正常 consumer 唤醒流程接管。

**回调解耦：MessageAdapterCallback（中台不依赖 producer，用 trait 注入）**：
- 中台定义 trait `MessageAdapterCallback { async fn on_message(&self, msg: AdaptedMessage) -> Result<()> }`；producer 实现它，把 AdaptedMessage 转成内部 NewMessage 事件。
- 这样未来写单元测试时可以给 start() 注入 MockCallback，无需真实启动 producer 链路。

**系统健康面板指标对接**：
- MessageAdapterRegistry 提供 list_running() → 返回各渠道状态；producer 聚合后通过 `/api/v1/system/health/aop` 对外暴露（包括 Lark Ws Metrics：当前连接数/重连次数/入站消息计数/last_ping 延迟等）。
- LarkMessageChannelDal 有 `listener_stats() -> LarkWsMetrics` 透传 lark_dao 的连接监控。

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [pkg/adapter/message.rs](/src/pkg/adapter/message.rs) | 中台：trait + 注册中心 | MessageInboundAdapter ~L46；MessageAdapterCallback ~L36；MessageAdapterRegistry（start_all/stop_all/register）~L82 |
| [pkg/adapter/mod.rs](/src/pkg/adapter/mod.rs) | 中台：父层 AdaptedMessage + AdapterRegistry 通用注册表 | AdaptedMessage ~L21；AdapterRegistry HashMap<ChannelType, Arc<dyn Any>> ~L47 |
| [dal/lark.rs](/src/service/dal/lark.rs) | Lark 渠道：MessageInboundAdapter 实现 + start 多应用聚合 | impl MessageInboundAdapter for LarkMessageChannelDal ~L632；start 内：query_enabled_lark_channels + 按 app_id 聚合去重 + resolve_channel_credentials + lark_dao 开 WS |
| [dal/message_channel.rs](/src/service/dal/message_channel.rs) | 消息渠道 DAL：出站分发（对照参考）| push_to_channel ChannelType match 纯分发（入站走独立 trait）~L329 |
| [producer/message_channel.rs](/src/producer/message_channel.rs) | AOP 消息通道生产者：注入回调 + start_all/stop_all + AdaptedMessage → NewMessageEvent 映射 | init 阶段注册回调 |
| 【① Design】message_channel_design.md（入站+出站全链路架构）| 入站适配器中台 vs 出站 push_to_channel 双路径对称设计 | docs/archive/design-archive/message_channel_design.md |
| 【③ Wiki 长文 1】消息渠道适配器.md | 新增入站渠道三步流程（用户视角） | docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md |
| 【③ Wiki 长文 2】多渠道消息系统.md | 入站+出站全链路图 | docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md |
| 【③ Wiki 长文 3】消息通道生产者.md | AOP 事件生产者如何调用 start_all 并把 AdaptedMessage 映射为内部事件 | docs/wiki/zh/content/基础设施/AOP%20事件系统/事件生产者/消息通道生产者.md |
| 【③ Wiki 长文 4】消息渠道管理.md | 前端管理员创建/绑定渠道到 open_id，lark_listen_inbound 开关 | docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md |
| 【平行卡 1】身份凭证 Domain 统一 CRUD（飞书渠道凭证引用解析依赖）| 渠道只存 lark_credential_id 引用，不存明文凭证 | source_files[] 尾平行卡1 路径 |
| 【平行卡 2】AES-256-GCM 敏感字段加密（凭证引用解析后拿到的 secret 要解密使用）| encrypt_channel_secret 闭包注入 | source_files[] 尾平行卡2 路径 |

## §3 架构约定

1. **pkg/adapter 必须纯基础设施层（零业务依赖）**：禁止在 pkg/adapter 目录中引入任何 MessagePo、Agent、User、MessageChannelDal 等业务类型。这些类型一律在 producer 层或 DAL 层处理。如果未来你需要新增 AdaptedMessage 字段，请仔细确认它是否真的不包含业务语义。
2. **start_all 的非 fail-fast 是硬要求**：一个渠道挂了不能拖垮其他渠道。内部用 match adapter.start() 捕获 Err 后 log_error!，然后 continue 到下一个。绝不能 `?` 把错误冒泡出去。
3. **ChannelType 枚举与 MessageChannel 出站共用同一个**：新增渠道（如 DingTalk）时，先扩展 common::ChannelType，然后出站写 push_to_channel match，入站写 MessageInboundAdapter 实现，两个路径都写齐全。
4. **一个 ChannelType 只允许注册一个 MessageInboundAdapter**（但该适配器内部可以管理 N 个子连接、N 个 app_id）。如果 register 重复注册同类型，Registry 内部返回错误（保护扩展者，防止 init 两次注册）。
5. **AdaptedMessage 永远是 owned 结构**，不借用外部消息对象池（外部消息可能是 WS 事件中的借用字段，生命周期很短）。转换时全部 clone 成 owned 字符串。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 MessageInboundAdapter::start 内部 panic**。外部渠道消息格式异常、凭证解析失败，都必须 wrap 成 Err 返回（或 log_warn 跳过单条消息），绝不允许直接 unwrap/panic 导致整个进程崩溃。
2. ❌ **禁止 MessageAdapterRegistry.start_all 对单渠道失败用 ? 冒泡**。冒泡会导致：飞书凭证过期 → 启动失败 → 微信/Slack 也起不来 → 全部渠道挂了 → P0 级故障。必须：match + log_error + continue。
3. ❌ **禁止 pkg/adapter 目录中引入 common::api::** 或 service/domain 层业务类型（MessagePo/Agent 等）。业务类型污染中台 = 分层被破坏，未来想加 Slack 的 AdapterRegistry 单元测试就必须把整个业务服务拉起来，测试隔离做不出来。
4. ✅ **AdaptedMessage.external_user_id + channel_type 组合必须全局唯一映射内部用户**：producer 侧查找逻辑依赖 (ChannelType + external_id) → MessageChannel → user_id；没找到时 log_warn 丢弃不 panic，外部用户如果没在系统内绑定渠道就只是无法收到响应，不应该抛错。
5. ✅ **Lark 渠道实现中 resolve_channel_credentials 返回 None 时要 warn 并跳过该渠道**：管理员渠道创建时引用的凭证被删除了 → 不能 panic、不能让启动失败；记录一条"channel id={} lark_credential_id={} 找不到凭证"跳过即可。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 5 篇 wiki 长文 + 1 Design + Plan 占位 + 3 张平行卡（身份凭证 CRUD / AES 加密 / Lark WS P2P 入站）；对应 Wiki 长文 cite 段回链本卡 + message_channel_design Design + 3 张平行卡。
