# 微信渠道接入设计（iLink ClawBot / 企微智能机器人）

> 🎯 **定位**：微信侧两条入站信道（iLink ClawBot、企微智能机器人）的协议适配与分层落点设计，含阶段划分
> 状态：草稿（阶段一待实施）
> 触发场景：实现/修改微信渠道入站出站、新增 IM 渠道时先读本文；字段级细节以源码为准
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构与规范路由总入口
> - [CODE_STANDARDS.md](../CODE_STANDARDS.md) — 编码规范 SSOT
> - [消息渠道适配器](../wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md) — 现有渠道闭环与扩展指南
> - [用户身份凭证独立表落地](../plan/用户身份凭证独立表落地.md) — 凭证引用式存储的先例

---

## 1. 结论先行

1. **微信没有统一的 "Agent 接入协议"**。官方提供的是若干条彼此独立的 IM 信道，只有消息收发与卡片能力，Agent 编排全部由我方承担。
2. 选定两条纳入本设计：**iLink ClawBot**（个人微信，阶段一）与**企微智能机器人**（企业微信，阶段二）。
3. 两条传输形态完全不同（长轮询 vs WebSocket），但都能收敛到现有的 `MessageInboundAdapter` 抽象——该 trait 的 `start(callback)` 与传输方式无关，因此**入站链路零改动**。
4. 唯一要动基础设施的是**入站 Agent 路由**：现状硬编码飞书接待角色，需改造为「多条件加权 resolve」（§4.3）。这项改造与微信解耦，是通用能力升级。
5. 阶段一只实现 iLink。理由是它无需企业资质、无需 ICP 备案域名、无需公网 IP，能最快跑通"外部 IM → Agent → 回消息"的完整闭环。iLink 即**默认微信信道**，复用 `ChannelType::Wechat = 1`，不新增变体。

---

## 2. 起点：微信渠道曾是空壳占位（阶段一已改造完成）

改造前 `ChannelType::Wechat` 只在出站分支接好，链路两端都未实现：

| 环节 | 改造前 |
|------|--------|
| 入站 | 无 `WechatDal`，未向适配器注册中心注册任何适配器 |
| 出站 | `WechatDao::push` / `test_connection` 直接返回 `Err(UnsupportedOperation)` |
| 凭证 | 内联于 `ChannelConfig.wechat_app_id/app_secret/open_id`，未走 `CredentialKind` 抽象 |
| 模型 | `ChannelConfig` 无 iLink / 企微机器人所需字段 |

**已按本设计落地**：`wechat_app_id/app_secret/open_id` 三个占位字段已删除，
渠道只保留 `wechat_credential_id`（凭证引用）+ `wechat_peer_id`（对端）+ `wechat_listen_inbound`（轮询开关），
与飞书引用模式完全一致；长期凭证统一走 `CredentialKind::WechatIlink` 凭据表。

> 当前实现：[WechatDaoHttpImpl](src/service/dao/wechat/http.rs)、[ChannelConfig](src/models/message_channel.rs)

对比之下飞书是完整闭环，本设计所有落点均以飞书为模板。

---

## 3. 候选信道全景与选型

| 信道 | 传输 | 门槛 | 方向 | 结论 |
|------|------|------|------|------|
| **iLink（ClawBot）** | HTTP 长轮询（出站） | 扫码即可 | 用户 ↔ 我的 Agent | ✅ 阶段一 |
| **企微智能机器人** | WebSocket（出站） | 需企业微信 | 用户 ↔ 我的 Agent | ✅ 阶段二 |
| 微信客服（"客服号"） | HTTP 回调（入站） | 企业认证 + ICP 备案域名 + Nginx | 用户 ↔ 我的 Agent | ⏸ 暂缓 |
| 小程序 AI 开发模式 | 小程序 MCP | 内测未开放提审 | **方向相反**：能力被微信 AI 调用 | ❌ 不纳入 |

### 3.1 为什么阶段一选 iLink

- **零基础设施门槛**：出站长轮询，不需要公网 IP、域名、Nginx；企微机器人要企业微信，微信客服要备案域名且回调域名校验失败即卡死。
- **身份模型最轻**：扫码拿到 `bot_token` 即可用；微信客服需企业 ID + Secret + 客服账号三件套。
- **代价可接受**：插件能力元数据仅声明私聊（群聊未声明）；协议较新，腾讯可能调整。这两点对"先跑通闭环"不影响。

### 3.2 为什么暂缓微信客服

官方文档明确"接管后该账号下所有客服账号被机器人接管、不能再转人工"，且硬性要求已备案域名与企业认证。等确有 C 端正式客服场景再开，届时同样实现 `MessageInboundAdapter`（webhook 形态），改动面与阶段一基本重叠。

### 3.3 为什么不纳入小程序 AI

方向是反的——它把我们的能力暴露给微信 AI 调用，不是让我们的 Agent 接入微信。且原子接口必须跑在小程序客户端 JS 沙箱内，Rust 后端只能作为原子接口 fetch 的下游，与入站链路无关。

---

## 4. 统一入站抽象：两种传输如何收敛到一条链路

### 4.1 适配器 trait 与传输无关

```rust
pub trait MessageInboundAdapter: Send + Sync {
    fn channel_type(&self) -> ChannelType;
    async fn start(&self, callback: Arc<dyn MessageAdapterCallback>) -> Result<()>;
    async fn stop(&self) -> Result<()>;
    fn is_running(&self) -> bool;
}
```

> 当前实现：[MessageInboundAdapter](src/pkg/adapter/message.rs#L54-L71)

`start` 的契约只要求"spawn 后台任务，收到消息时调 `callback.on_message`"，**不关心底层是 WS、长轮询还是 webhook**。因此：

- 企微机器人：复用 `pkg/ws` 的 `WsClientAdapter` + supervisor + 指数退避重连；
- iLink：一条自建的长轮询循环，**不需要 `pkg/ws`**（比企微更简单）。

### 4.2 完整链路

```text
微信用户
   │
   ▼
DAO adapter（长轮询 / WS / webhook）—— 只收帧，不做业务
   │ publish AOP 事件（信封 = 协议原始数据）
   ▼
consumer（ConsumeMode::Async）—— 协议转换 + 渠道定位 + 用户映射
   │ AdaptedMessage
   ▼
producer（MessageChannelProducer::on_message）—— Agent 路由
   │ SendToAgentCommand
   ▼
MessageDomain::delivery().send_to_agent —— 唤醒 Agent
   │ （Agent 产出回复）
   ▼
MessageChannelDal::push_to_channel → WechatDao::push —— 出站
```

关键点：**DAO 读循环里不做业务**。入站一律 publish AOP 事件，由 `ConsumeMode::Async` 的 consumer 消费——否则长轮询/WS 收帧会被慢业务阻塞。这是飞书已验证的模式，iLink 必须照做。

> 当前实现：[LarkInboundConsumer](src/consumer/lark_inbound.rs)、[MessageChannelProducer](src/producer/message_channel.rs)

### 4.3 唯一需要动基础设施的地方：入站 Agent 路由

producer 当前硬编码 `ROLE_FEISHU_RECEPTION` 做兜底路由：

> 当前实现：[on_message 路由](src/producer/message_channel.rs#L42-L64)

新增渠道后必须按渠道分派接待角色，否则微信消息会误路由到飞书前台。

#### 4.3.1 `AdaptedMessage` 增 `channel_type`

producer 需要知道消息来自哪个渠道才能构造路由条件，因此基础设施加 1 个字段（不采用 `reception_role: Option<String>` 方案——那是让 `pkg/` 承载渠道语义，且角色字符串会散落到每个 DAL）。

> 当前实现：[AdaptedMessage](src/pkg/adapter/mod.rs)、[on_message 路由](src/producer/message_channel.rs#L42-L64)

#### 4.3.2 路由能力改造：多条件加权 resolve

**这是独立于微信的通用能力改造，微信渠道是第一个受益方。**

现状：

> 当前实现：[resolve_agent trait](src/service/domain/hr/mod.rs#L265-L269)、[实现与打分](src/service/domain/hr/mod.rs#L105-L237)、[AgentMatchCriteria](common/src/api/agent.rs#L483-L534)、[match_scores](common/src/api/agent.rs#L540-L560)

- `resolve_agent(ctx, criteria)` 只接受**单个** criteria，内部无回落链
- 单个 criteria 内部是**多维加权求和**（角色 / capability 关键词 / 工具包 tag）
- 没有任何维度能表达"指定了这个 Agent 就必须用它"——**没有 id 维度**

改造目标（用户拍板）：先拓展单条件的能力，再提供接受多条件的方法；不同条件权重完全不同（指定 id 最高，其次角色、能力匹配度）；单条件退化为多条件的特例。

**第一步：条件内加 id 维度（决定性权重）**

`AgentMatchCriteria` 新增字段与快捷构造：

```rust
/// 指定 Agent ID 集合：命中即决定性胜出（权重远高于其他维度）
pub any_id: Option<Vec<String>>,

impl AgentMatchCriteria {
    pub fn by_id(id: impl Into<String>) -> Self;
    pub fn by_ids(ids: Vec<String>) -> Self;
}
```

`match_scores` 新增 `ID_EXACT_MATCH: i32 = 1_000_000`。**权重量级隔离**是这套设计的地基：

| 维度 | 权重 | 说明 |
|------|------|------|
| **id 精确命中** | **1_000_000**（新增） | 决定性。需要 100 次 role 全匹配或 10 万次 capability 命中才能被追平 |
| role 全匹配 | 10_000 | 现有 tier1 |
| role 部分精确 | 5_000 (+100/条) | 现有 tier2 |
| role 子串层级 | 1_000 (+50/条) | 现有 tier3 |
| 语义兜底 | 200 | 现有 tier4 |
| capability 关键词 | 10/条 | 现有 |
| installed_tag | 3/条 | 现有 |

> ⚠️ **实现约束**：现有实现先 `query(limit=10)` 拉候选集再打分（`src/service/domain/hr/mod.rs#L119-L131`）。指定 id 时**必须直查**——limit 10 的候选集很可能不含目标 Agent。因此 id 维度走短路：任一 criteria 含 `any_id` 时，先逐个 `get_by_id` 直查，命中且 Onboarded 即返回。

**第二步：多条件方法 `resolve_agent_multi`**

```rust
/// 多条件有序匹配：按 criteria 顺序分档，首个「有命中」的档位胜出。
/// 单条件是它的特例：resolve_agent(c) == resolve_agent_multi(vec![c])
async fn resolve_agent_multi(
    &self,
    ctx: RequestContext,
    criteria: Vec<AgentMatchCriteria>,
) -> Result<Option<Agent>>;
```

算法：

1. 空 `vec` → 等同 `AgentMatchCriteria::any()`
2. 依次对每个 criteria 走现有打分流程（含第 1 步的 id 短路）
3. 该 criteria 下存在 `score > 0` 的 Agent → 立即返回其中最高分者
4. 全部 criteria 均无命中 → 退化回最早 Onboarded（保持现有语义，兜底永远成立）

#### 4.3.3 为什么是「有序档位」而不是「跨条件求和」

直觉上"多条件加权求和"更自然，但它会**倒挂**。现有 role 打分有 tier3 子串层级，且是**双向**包含（`ar.contains(r) || r.contains(ar)`），于是：

```text
criteria = [ by_role("wechat_reception"), by_role("reception") ]
Agent X roles = ["wechat_reception"]  →  条件1 精确命中 10000 + 条件2 子串命中 1000 = 11000
Agent Y roles = ["reception"]         →  条件1 子串命中 1000 + 条件2 精确命中 10000 = 11000
                                         打平 → 落到 created_at 比较，结果不确定 ❌
```

取 `max` 同样打平（双方都 10000）。**根因是弱维度（子串）跨条件累加后能抵消强维度（精确）的优势**。

有序档位天然规避：档位 1 里 X 命中 10000、Y 命中 1000 → X 胜出，不再看档位 2。

所以最终语义是**两层加权**：

- **条件间**：有序，档位优先（体现"指定 > 渠道专属 > 通用兜底"的意图）
- **条件内**：多维加权求和（体现"角色匹配度 > 能力匹配度"的匹配质量）

新增维度只需动 `AgentMatchCriteria` 字段 + `match_scores` 权重，档位链由调用方按业务意图组装——可无限拓展。

#### 4.3.4 producer 侧的档位链

```rust
let mut chain = Vec::new();
// 档位 1：渠道显式绑定的 Agent（决定性）
if let Some(agent_id) = channel.agent_id() {
    chain.push(AgentMatchCriteria::by_id(agent_id));
}
// 档位 2：渠道专属接待角色
chain.push(AgentMatchCriteria::by_role(reception_role_of(channel_type)));
// 档位 3：通用接待角色（兜底，恒存在）
chain.push(AgentMatchCriteria::by_role(ROLE_RECEPTION));
```

`reception_role_of`：`ChannelType::Wechat → "wechat_reception"`、`ChannelType::Lark → "feishu_reception"`。删掉 producer 里的硬编码，飞书链路一并走同一条链（行为不变：飞书渠道若无显式绑定，仍命中 `feishu_reception`）。

**兼容性**：现有 6 个 `resolve_agent` 调用点委托为单元素版本，行为完全不变；`resolve_agent_multi` 是纯新增。

---

## 5. iLink（ClawBot）设计 — 阶段一

### 5.1 协议要点

接入域：`https://ilinkai.weixin.qq.com`（登录响应中的 `baseurl` 为准，不硬编码）。

| 接口 | 方法 / 路径 | 用途 |
|------|------------|------|
| 二维码 | `GET /ilink/bot/get_bot_qrcode?bot_type=` | 取登录二维码 |
| 二维码状态 | `GET /ilink/bot/get_qrcode_status?qrcode=` | 轮询 `wait → scaned → confirmed → expired` |
| 拉消息 | `POST /ilink/bot/getupdates` | 长轮询，游标 `get_updates_buf`，服务端 hold ~35s |
| 发消息 | `POST /ilink/bot/sendmessage` | **必须回传收到的 `context_token`** |
| 上传 | `POST /ilink/bot/getuploadurl` | 媒体上传地址 |
| 配置 | `POST /ilink/bot/getconfig` | 账号配置（含 `typing_ticket`） |
| 输入态 | `POST /ilink/bot/sendtyping` | "正在输入"提示 |

请求头固定三件套：

```text
AuthorizationType: ilink_bot_token
Authorization:     Bearer <bot_token>
X-WECHAT-UIN:      <随机 uint32 的 base64，每次请求重新生成>
```

消息结构关键字段：`from_user_id` / `to_user_id` / `client_id` / `message_type`（`USER` | `BOT`）/ `message_state`（`FINISH`）/ `item_list[]` / `context_token`。支持文本、图片、视频、文件、语音；阶段一只处理文本。

发消息时 `from_user_id` 留空、`to_user_id` 填对端标识、`context_token` 回传收到的值：

```jsonc
{ "msg": {
    "from_user_id": "",          // 留空
    "to_user_id": "<对端 peer>",  // 取自入站消息的 from_user_id
    "client_id": "<本地生成>",
    "message_type": "BOT", "message_state": "FINISH",
    "item_list": [], "context_token": "<入站消息携带>"
} }
```

#### 5.1.1 三种标识别混淆

| 标识 | 谁 | 稳定性 | 用途 |
|------|----|--------|------|
| `ilink_bot_id` / `ilink_user_id` | **bot 自己** | 长期不变（登录时返回，随 `bot_token` 一起持久化） | 日志、多 bot 路由。**不是对端** |
| `from_user_id`（peer） | 对端微信用户 | **稳定**，等同 openid 语义 | 会话隔离、身份映射、白名单 |
| `context_token` | 会话上下文 | **会变**，每条入站消息刷新 | 回消息时必须带上最新的 |

**peer 稳定的三条依据**（决定了它可以落库长期复用）：

1. OpenClaw 提供 `session.dmScope per-account-channel-peer`，按 **peer** 隔离私聊会话——若 peer 会变，这个隔离维度就没有意义；
2. 访问控制走 **pairing + allowlist**（`openclaw pairing approve <CODE>` 按发送者审批）——若 peer 会变，白名单审批立即失效；
3. 参考实现按 `userId` 维护多轮对话 `Map`——若 peer 会变，多轮上下文无法维持。

**`context_token` 则相反**：参考实现的主循环每收到一条消息就重新缓存一次，说明它是滚动刷新的会话态。因此落地要求是——入站即覆盖写、出站取最新值；**不保证多久之后仍有效**，主动推送（用户长时间未发言后推送）存在失效风险，需有明确失败提示引导用户先发一条消息。

#### 5.1.2 专属渠道语义：一个 bot 只能对话扫码者（1:1）

iLink 的授权模型决定了它不是"群发通道"，而是**专属 1:1 渠道**：

- 二维码必须由**某个微信用户主动扫码**才完成授权（`wait → scaned → confirmed`）；
- 该 bot 的消息面只对扫码者生效——bot 只能给这个人发消息，也只有这个人的消息会被 bot 收到；
- 因此「一个 bot 微信号 = 一个 channel = 一个对端」不是简化假设，而是协议事实。

由此确定的设计口径：

| 结论 | 影响 |
|------|------|
| 对端唯一且稳定 | `wechat_peer_id` 单值字段够用，无需设计多 peer 解析规则 |
| 对端可在首条入站时确定 | 首次入站自动回填 `wechat_peer_id`，用户无需手填（已实现） |
| `inbound_state.sessions` 实际最多一项 | Vec 结构保留（不为此改结构），但不会出现多 peer 竞争 |
| 主动推送上限明确 | 推送目标恒为扫码者，"推给谁"不存在歧义，仅受 `context_token` 有效期制约 |

> 对比：飞书是「一个应用对多个用户」（`list_by_user_id` 返回多条渠道 = 多个对端各占一条）；iLink 是「一个 bot 对一个用户」，渠道天然单一对端，反而更简单。

> ⚠️ 待实测：`context_token` 的确切有效期，以及失效时服务端的错误形态。这决定主动推送能否可靠实现，阶段一不做主动推送，联调时顺带观察即可。

外部参考：
- 微信开放社区 [clawbot 相关接口](https://developers.weixin.qq.com/doc/aispeech/knowledge/openapi/Clawbotrelated.html)
- OpenClaw 渠道文档 [WeChat](https://docs.openclaw.ai/channels/weixin)（插件 `@tencent-weixin/openclaw-weixin` 由腾讯微信团队维护）
- 腾讯云社区 [基于微信 ilink API 的自定义机器人](https://cloud.tencent.com/developer/article/2651968)（协议整理，最接近可照抄）

### 5.2 扫码登录流程

iLink 的凭证不是静态 `app_id + secret`，而是**扫码换 token**，因此需要新增登录交互：

```text
前端                     后端                       iLink
 │ 1. 请求登录二维码        │                          │
 │───────────────────────▶│ get_bot_qrcode ─────────▶│
 │◀─── qrcode_url ────────│◀─────────────────────────│
 │ 2. 展示二维码，轮询状态  │                          │
 │───────────────────────▶│ get_qrcode_status ──────▶│
 │   wait / scaned        │◀─────────────────────────│
 │                        │  （confirmed）            │
 │◀─── 登录成功 ──────────│ bot_token / ilink_bot_id │
```

状态机 `wait → scaned → confirmed → expired`；`expired` 自动刷新（上限 3 次）。`confirmed` 返回 `bot_token` + `ilink_bot_id` + `ilink_user_id` + `baseurl`。

落地要求：

- 新增受保护的登录接口（生成二维码 / 查询状态），**不暴露在任何匿名路由**；
- 登录成功后自动 upsert 一条 `wechat_ilink` 凭据到 `user_credentials`（见 §5.2.1），渠道只存 `credential_id` 引用；
- 二维码是临时凭证，接口响应需带短期有效期，前端不做本地持久化。

#### 5.2.1 凭据类型：新增 `WechatIlink`，不能复用现有类型

这是本次需求的前提基础。现有 5 种凭据（[CredentialDetail](common/src/models/identity_credentials.rs#L95-L140)）：

| 现有类型 | 字段形态 | 为什么不适合 iLink |
|---------|---------|------------------|
| `LarkApp` | `{ app_id, app_secret, encrypt_key?, verification_token? }` | 是"手填的双凭证对"形态，无 base_url；硬套（如拿 verification_token 存 base_url）属语义污染 |
| `GithubToken` / `GenericToken` | `{ token }` 单值 | **token 之外的 `bot_id` / `user_id` / `base_url` 无处安放**；`primary_id()` 返回 None，丢掉"渠道移交对比"能力 |
| `OAuth` | refresh 流四件套 | 形态完全不符 |
| `UserPassword` | 用户名密码对 | 不符 |

**决定性论据**：iLink 的四个字段（`bot_token` / `bot_id` / `user_id` / `base_url`）是**扫码 confirmed 时一次性原子产出**的，且**重新扫码 = 整组轮换**（token、base_url 都可能变）。拆开存（比如 token 进凭据、base_url 进 channel config）会制造"重新扫码后 token 更新了但 base_url 没同步"的分裂状态——它们必须同属一个 detail 变体。

新增变体（`common/src/models/identity_credentials.rs`）：

```rust
// CredentialKind
/// 微信 iLink（ClawBot）扫码凭据
WechatIlink,
// as_str() -> "wechat_ilink"

// CredentialDetail
/// 微信 iLink 扫码凭据（confirmed 一次性产出，整组轮换）
WechatIlink {
    /// bot 令牌（落库前经 encrypt_channel_secret 加密）
    bot_token: String,
    /// iLink bot 标识
    bot_id: String,
    /// bot 侧用户标识（可选，部分登录响应未返回）
    user_id: Option<String>,
    /// 接入域（以登录响应为准，不硬编码）
    base_url: String,
},
```

六个 match 分支的取值约定：`primary_id()` → `bot_id`（有外部主标识，同 `LarkApp.app_id` 的地位）；`primary_secret()` → `bot_token`；`encrypt_sensitive()` / `validate()` → 只碰 `bot_token`；`CredentialDetailPatch` 加同构变体（重新扫码走 `apply_patch` 整组覆盖）。

**与手填凭证的关键差异**：这张凭据**不是用户填出来的，是扫码流程产生的**。前端凭据页需要的不是字段表单，而是一个「微信扫码授权」按钮 + 二维码展示；confirmed 后由后端自动创建/更新凭据并回显 `bot_id`。凭据 CRUD、加密、patch 机制全部复用现有体系，零表结构变更。

改动清单：

- `common/src/models/identity_credentials.rs`：`CredentialKind` + `as_str` + `CredentialDetail` + `CredentialDetailPatch` + `kind()` / `primary_id()` / `primary_secret()` / `normalized()` / `validate()` / `encrypt_sensitive()` / `apply_patch()` 七处 match
- 后端：凭据 CRUD 通用（不动），新增扫码登录接口（二维码 / 状态查询）负责凭据的创建与轮换
- 前端：`frontend/src/components/credential_form.rs` 的 `all_credential_kinds()` 加选项 + 凭据页加「扫码授权」入口（二维码交互）

**已落地**（凭据闭环第一 slice）：`WechatIlink` 变体全量 match + 单测；`src/pkg/wechat_ilink.rs` 登录协议客户端（含状态解析单测）；domain `wechat_login_qrcode` / `wechat_login_poll`（confirmed 自动 upsert：默认凭据存在则整组轮换，否则创建并设默认）+ `wechat_integration_status` 聚合；handlers `wechat_integration/{get_status, get_login_qrcode, login_status}`；路由 `/api/v1/finance/identity/wechat/{status,qrcode,qrcode/status}`。待办：`delete_credential` 的 WechatIlink 渠道引用前置检查（渠道功能落地时补，见实现内注释）。

**前端已落地**（凭据页微信卡片）：`frontend/src/api/wechat_integration.rs`（status / qrcode / login_status 客户端 + query 参数 percent-encode）；`frontend/src/pages/finance/identity_wechat.rs` 的 `IdentityWechatSection` 区块——「微信」区块内「iLink 机器人」子卡（凭证列表：名称 / bot_id / 默认徽标），「扫码授权」按钮弹 Modal 展示二维码（`qr_img_src` 兼容 data URI / URL / 裸 base64 三种形态）+ 长轮询状态机（Wait 立即重发 / Scaned 提示 / Expired 可重新生成 / Confirmed 刷新列表），轮询循环带 `Arc<AtomicBool>` 卸载守卫（同 lark 绑定轮询模式）。区块设计为多子卡容器，未来企微等其他微信凭据类型在同一区块追加子卡。

### 5.3 数据模型变更

#### 5.3.1 ChannelType：复用 `Wechat = 1`，不新增变体

iLink 就是默认的微信信道（用户拍板）。未来的企微 bot / 公众号 / 微信客服届时再按需新增变体——现在不为假想需求预留。

因此 `common/src/enums/message_channel.rs` **本次不改**，省掉 `From<i32>` / `as_str()` / 前端映射 / DB 存量数据这一整串破坏性变更风险。

> 当前实现：[ChannelType](common/src/enums/message_channel.rs#L14-L28)、[From<i32>](common/src/enums/message_channel.rs#L30-L42)、[as_str](common/src/enums/message_channel.rs#L62-L73)

#### 5.3.2 ChannelConfig 新增字段

| 字段 | 类型 | 说明 |
|------|------|------|
| `wechat_credential_id` | `Option<String>` | iLink 凭证引用（指向 `user_credentials`），扫码登录成功后回填 |
| `wechat_bot_id` | `Option<String>` | `ilink_bot_id`，日志与多 bot 路由用 |
| `wechat_listen_inbound` | `Option<bool>` | 是否建立长轮询（缺省 true，对齐 `lark_listen_inbound`） |
| `wechat_peer_id` | `Option<String>` | 对端微信用户标识（= 入站消息 `from_user_id`，稳定不常变，见 §5.1.1；首次入站自动回填，无需手填） |

长期凭证（`bot_token`）**不进 config_json**，走 `CredentialKind` 独立凭据表（见 §5.2）；会话态（`context_token`）与游标**不进 config_json**，走 §5.3.4 的新列。config_json 只保留用户可编辑的静态配置——这条边界划清后，轮询循环写运行状态不会覆盖管理后台的配置改动（该表 `update` 是整体覆盖且无乐观锁）。

> 当前实现：[MessageChannelDao::update](src/service/dao/message_channel/sqlite.rs#L57-L90)

#### 5.3.3 入站运行状态模型：动态游标 + 动态令牌

游标、会话这类**运行时信息**是跨实体可复用的通用能力（未来飞书、邮件、任意增量拉取表都能用），因此下沉到 `common`，而不是塞进 `ChannelConfig` 或 `config_json`。

落点：`common/src/models/inbound_state.rs`（新增，在 `common/src/models/mod.rs` 导出）。

顶层是一个 `InboundState`——**一个实体的全部入站运行时信息，一列存取**，内分两部分：

```rust
/// 入站运行状态：动态、由运行时循环读写，与用户静态配置（config_json）物理隔离。
/// 未来新增动态信息直接加字段，零 DDL。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct InboundState {
    /// 动态游标：增量拉取进度
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<InboundCursor>,
    /// 动态会话态：按对端组织，内含滚动刷新的动态令牌
    #[serde(default)]
    pub sessions: InboundSessions,
}
```

| 部分 | 装什么 | 变化频率 |
|------|--------|---------|
| `cursor` | 增量拉取进度（iLink 的 `get_updates_buf`） | 每轮轮询 |
| `sessions` | 按 peer 的会话态；**动态令牌**（`context_token`）是其中一项 | 每条入站消息 |

「动态令牌」以**会话为单位**组织而不是独立一块，因为这类令牌都是 per-peer 的（iLink 的 `context_token`、企微的会话票据同理）。若未来出现 bot 级（与 peer 无关）的动态凭证（如短期 access_token 的缓存），再在顶层加 `auth` 字段——结构体演化不需要 DDL。

游标与会话都由**同一个轮询循环**产生和更新（收到一批消息 → 刷会话 → 推进游标 → 一次写回），合成一列后每轮只做一次 UPDATE，读取侧也是一次 `SELECT` 整体加载进内存。

**动态游标**：

```rust
/// 游标语义：决定 value 如何解释、能否比较、能否安全回退
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CursorKind {
    /// 不透明字符串：只能原样回传，不可比较大小、不可回退。
    /// iLink 的 get_updates_buf 属此类。
    #[default]
    Opaque,
    /// 单调递增序号：可比较，回退安全
    Sequence,
    /// 毫秒时间戳：可比较
    Timestamp,
    /// 数值偏移：可比较
    Offset,
}

/// 通用增量拉取游标（与具体协议解耦）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct InboundCursor {
    /// 游标内容（序列化为 UTF-8 文本存储；不透明类型由协议自解释）
    pub value: String,
    /// 游标语义
    #[serde(default)]
    pub kind: CursorKind,
    /// 产生游标的来源标识（如 `ilink`、`wecom_bot`），排障与迁移用
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 最近更新时间（毫秒时间戳），用于判定陈旧 / 决定是否重置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at_ms: Option<i64>,
}
```

方法：`InboundCursor::opaque(value, source)` / `::sequence(n, source)` / `is_empty()` / `age_ms()` / `to_json()` / `from_json()`（`from_json` 解析失败返回 `None`，等价于"无游标，从头开始"——fail-open 而非 panic）。

**为什么游标是单值 JSON 而不是拆成多列**：不同协议的游标形态差异极大（iLink 是不透明 base64 串，飞书可能是序号），拆列会随协议增长不断加列；单列 + `kind` 自解释，新增协议零 DDL。任何实体需要增量拉取时，加同名同类型的 `inbound_state TEXT` 列即可复用。

**动态会话**：

```rust
/// 入站会话上下文（以对端为单位）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct InboundSession {
    /// 对端标识（协议侧原值：iLink 为 from_user_id，企微为 userid）
    pub peer_id: String,
    /// 会话令牌：滚动刷新的会话态，回消息时须回传（iLink 的 context_token）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    /// 最近一次入站的消息 ID，排障用，也可与 AOP 幂等键对照
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_id: Option<String>,
    /// 最近更新时间（毫秒）：挑"最新会话"与判断陈旧都靠它
    pub updated_at_ms: i64,
}

/// 入站会话集合：一个渠道/实体一份，整体 JSON 落列
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct InboundSessions {
    #[serde(default)]
    pub sessions: Vec<InboundSession>,
}
```

方法：`get(peer_id)` / `upsert(peer_id, context_token, last_message_id)` / `latest()`（peer 未配置时的兜底，取 `updated_at_ms` 最大者）/ `retain_latest(max)`（超过上限丢弃最旧的，防长期运行无限膨胀，默认上限 100）。

**用 `Vec` 而不是 `HashMap`**：map 的 key 会与 `peer_id` 字段冗余；peer 数量是个位数，线性查找的开销可忽略；Vec 顺序稳定，日志与排障更直观。`InboundSessions` 同样与协议无关——企微的 `userid`、`chat_id` 也能塞进 `peer_id`，未来任何"需要对端会话态"的渠道直接复用，零 DDL。

#### 5.3.4 新增一列：`inbound_state`

```sql
-- 入站运行状态（InboundState 的 JSON 序列化：动态游标 + 动态会话，NULL = 从头开始）
ALTER TABLE message_channels ADD COLUMN inbound_state TEXT NULL;
```

| 项 | 存储位置 | 写入频率 | 说明 |
|----|---------|---------|------|
| `bot_token` | 凭据表（`CredentialKind::WechatIlink`） | 扫码时一次 | 长期凭证，不进本列 |
| `context_token` | `inbound_state.sessions` | 每条入站消息 | 动态令牌，回消息时必须回传 |
| `get_updates_buf` | `inbound_state.cursor` | 每轮轮询（~35s） | 动态游标 |

**为什么 context_token 也落库**：此前评估为"动态下发、存储意义不大、存内存即可"。落库成本确实极低（跟游标同列同一次写），但换来两个实在好处——进程重启后仍能回复历史会话，以及未来做主动推送时无需重新等用户发消息。用户拍板落库。

**竞态分析**：`config_json` 的整体覆盖 RMW 竞态（轮询写 vs 管理后台改）依然存在，但新列与 config_json 物理隔离——轮询循环只写 `inbound_state`，管理后台只写 `config_json`，互不覆盖。`inbound_state` 本身由**单个轮询循环**独占更新，无并发写者，整列覆盖写安全。

**读写模式**：轮询循环启动时 `SELECT` 一次加载整列 JSON 进内存，此后内存持有 `InboundState`，每轮结束时整体写回。两次写（游标 + 会话）合并为一次 UPDATE。

DAO 新增（对齐现有 `set_status` / `mark_push_success` 的局部更新模式）：

```rust
/// 仅更新入站运行状态（inbound_state 列整体覆盖），不动其他列
async fn set_inbound_state(&self, ctx: RequestContext, id: &str, state: &InboundState) -> Result<()>;
```

> 当前实现：[局部更新先例](src/service/dao/message_channel/mod.rs#L79-L88)、[message_channels DDL](migrations/20260420000000_initial.sql#L326-L345)

#### 5.3.5 Agent 接待角色

`common/src/constants/agent_roles.rs` 新增 `ROLE_WECHAT_RECEPTION = "wechat_reception"`（中文名"微信前台"），与 `ROLE_FEISHU_RECEPTION` 并列，供 §4.3.4 档位链的第 2 档使用。

### 5.4 分层落点

严格单向：`Adapter → Domain → DAL → DAO → Models`。

| 层 | 文件 | 职责 |
|----|------|------|
| Models | `src/models/events/wechat.rs` | `WechatInboundEvent` 事件信封 + iLink 消息 DTO，kind = `wechat.inbound.message` |
| Models | `src/models/message_channel.rs` | `ChannelConfig` 字段扩展 |
| pkg | `src/pkg/wechat_ilink.rs` | **登录协议客户端**（二维码 + 状态长轮询，配置面出站；对齐 lark_integration 先例，已落地） |
| DAO | `src/service/dao/wechat/ilink.rs` | iLink 消息面 HTTP 客户端（getupdates / sendmessage 等封装）+ 长轮询循环 |
| DAO | `src/service/dao/wechat/http.rs` | 出站 `push` 实现（补齐 `UnsupportedOperation`） |
| DAL | `src/service/dal/wechat/mod.rs` `impl.rs` `credentials.rs` `listener.rs` | `adapt_wechat`、身份定位、凭证解析、实现 `MessageInboundAdapter` 并注册 |
| Consumer | `src/consumer/wechat_inbound.rs` | 订阅 `wechat.inbound.message`，`ConsumeMode::Async` |
| Producer | `src/producer/message_channel.rs` | 按 `channel_type` 分派接待角色（方案 A） |
| Adapter | 渠道管理接口 | 二维码登录、启停监听 |

**登录为什么在 pkg 不在 DAO**：扫码登录是配置面协议流（归属"集成辅助"，与 `pkg/lark_integration` 的 device flow 同构，domain 薄委托 pkg）；消息收发是数据面出站（DAO 职责）。两面条路并存，各归各位。

事件与事件模型照抄飞书：

> 当前实现：[LarkInboundEvent](src/models/events/lark.rs#L114-L139)

要点：
- 信封只带协议原始数据，**不带内部身份**；
- `id()` 取 iLink 消息 ID 作幂等键；
- `order_key()` 取 `bot_id`，同 bot 内串行、不同 bot 并行。

### 5.5 出站

`WechatDao::push` 由 `UnsupportedOperation` 改为真实实现，调用 `ilink/bot/sendmessage`。

**`context_token` 的取用是本节唯一需要设计的点**：出站是独立链路（Agent 产出 → `delivery` → `push_to_channel`），不会把入站时的 `context_token` 带回来。

先澄清一个容易想岔的地方：**出站不需要任何额外扩参**，`channel` 本身就够。

现有出站链路是"按用户查渠道"而非"透传对端"：

> 当前实现：[delivery_to_user 查渠道](src/service/dal/message_channel.rs#L240-L295)、[push_to_channel 分发](src/service/dal/message_channel.rs#L346-L367)、[飞书取接收者](src/service/dao/lark/http.rs#L266-L301)

```rust
// delivery_to_user：先按 user_id 查出该用户的全部活跃渠道
let channels = self.message_channel_dao.list_by_user_id(ctx, user_id, true).await?;
// 再逐个推送，channel 对象带着该渠道的全部配置
self.push_to_channel(ctx, message, &channel).await
```

而飞书 DAO 取接收者也是**从 `channel.config()` 读**，不是从 message 里推：

```rust
let open_id = config.lark_open_id.as_ref().ok_or_else(|| err!(...))?;  // 渠道配置写死接收者
```

即现有模型就是 **「一个 channel = 一个对端」**（`list_by_user_id` 返回多条 = 多个对端各占一条渠道记录）。`MessagePo` 也确实没有存外部发送者标识的字段（`from_id` / `to_id` 都是内部 ID）。

因此 iLink 照抄即可，**`push_to_channel` 签名零改动**：

| 出站要素 | 取值来源 |
|---------|---------|
| 发给谁（peer） | `channel.config().wechat_peer_id`；未配置时取 `inbound_state.sessions.latest()`（首次入站自动回填，用户无需手填）（首次入站自动回填，用户无需手填） |
| `context_token` | `inbound_state.sessions` 按 peer 取（见 §5.3.4） |
| `bot_token` | 凭据表，按 `config.wechat_credential_id` 查（DAL 解析后传给 DAO，同飞书 `resolve_lark_credentials`） |

设计决策：

- **`context_token` 落库到 `inbound_state.sessions`**（见 §5.3.4），入站时按 `peer` 覆盖写，出站时按上表规则查取；
- 内存态只做**读缓存**（加载该 channel 的 sessions JSON），写路径始终落库，避免进程重启后会话上下文丢失；
- 阶段一不做主动推送，但数据已就位——需要时直接取 `inbound_state.sessions` 里的 `context_token`，无需再改造。

出站约束（红线，照项目既有约定）：

- 必须走 `src/pkg/http/` 的 preset（长轮询用 `with_timeout_ms` 显式放宽到 >35s；普通调用用 `outbound()`），**禁止裸 `reqwest::Client::new()`**；
- 构建失败 fail-fast，**绝不回退裸客户端**。

> 当前实现：[presets](src/pkg/http/presets.rs)

### 5.6 幂等、游标与容错

| 项 | 方案 |
|----|------|
| 消息去重 | 依赖 AOP 事件的 `id()`（iLink 消息 ID）做幂等键，无需额外存储 |
| 轮询游标 | `get_updates_buf` 持久化到 `inbound_state.cursor`（`InboundCursor { kind: Opaque, source: "ilink" }`），随每轮状态整体写回，每轮轮询成功后写一次；进程重启从上次游标续拉，不再重复消费历史消息 |
| 轮询超时 | ~35s 是正常现象，`AbortError` 当作空响应继续下一轮，不记错误 |
| 连续失败 | 前 5 次间隔 2s 重试，超过后退避 30s，避免触发限流 |
| 慢业务隔离 | consumer 必须 `ConsumeMode::Async`，否则阻塞整条轮询循环 |

### 5.7 凭证 ↔ 渠道联动

**引用模式（对齐 lark，结构性消灭同步问题）**：

- 渠道 `config_json` 只存 `wechat_credential_id` 引用，**不存** `bot_token` / `base_url` 的任何副本；
- 运行时按引用解析：`wechat_credential_id` → `user_credentials` 行 → `WechatIlink { bot_token, bot_id, base_url }`（对齐 [resolve_channel_credentials](src/service/dal/lark/impl.rs#L149-L155)，批量场景带 per-batch cache 防 N+1）；
- iLink 的 `baseurl` = 登录 confirmed 响应返回的**后续全部 bot 协议请求接入域**（getupdates / sendmessage / getconfig 等）。登录流程本身走默认域 `ilinkai.weixin.qq.com`，登录响应可带回不同 baseurl，DAO 必须以 `credential.base_url` 为准，禁硬编码默认域；
- 不把 base_url 复制进渠道 config 的理由：重扫 = 整组轮换，复制会造成"token 更新了、base_url 没跟上"的分裂状态——这正是四字段绑成一个凭证变体的理由（§5.2.1）。

**凭证变更 → 监听移交**（对齐 [handover_listeners_after_credential_change](src/service/dal/lark/impl.rs#L530-L551)）：

- `update_credential` 的类型分发目前只有 `LarkApp` 分支，微信渠道落地时补 `WechatIlink` 分支；
- 移交判据（与 lark 的差异：**多一个 base_url 维度**，因飞书接入域固定）：
  - `bot_id` 变化 → 停旧长轮询 → ensure 新 bot；
  - `bot_id` 不变但 `bot_token` 或 `base_url` 变化 → **强制重建轮询循环**（旧循环持有旧 token/base_url，不重建会静默拉空或失败）；
  - 其余（如仅改名）no-op；
- 前提：长轮询循环必须做成**可 stop / ensure 的受管形态**（对齐 lark WS 的 registry 管理模式），不能是 spawn 后不管的死循环；
- 失败仅告警，不阻断凭证更新（同 lark）。

> **实现口径（已落地）**：移交判据统一收敛为**凭证指纹**（bot_id / bot_token / base_url 三要素哈希）。`start_polling` 的 ensure 语义 = 指纹相同幂等 no-op、指纹不同停旧重建，三种变化维度（bot_id / token / base_url）无需上层区分；`update_credential` 的 `WechatIlink` 分支一律对引用渠道重新 ensure，重命名等无实质变化的更新指纹相同自动 no-op。入站运行状态写回经 `InboundStateWriter` 窄接口（`dao/wechat/ilink.rs`）注入，DAO 不依赖 MessageChannelDao 完整类型（守住"DAO 不依赖其他 DAO"）。

---

## 6. 企微智能机器人设计 — 阶段二

### 6.1 协议要点

- 端点：`wss://openws.work.weixin.qq.com`
- 帧协议：`aibot_subscribe`（`bot_id` + `secret` 鉴权）→ `aibot_msg_callback`（收消息）→ `aibot_respond_msg`（流式回复）/ `aibot_send_msg`（主动推送）/ `ping`（30s 心跳）
- 单聊 + 群聊 @；支持 markdown 与 template_card
- 官方限制：单个机器人同时只能保持**一条**有效长连接；回复窗口约 3 分钟，超时截断

> 官方文档：[智能机器人长连接](https://developer.work.weixin.qq.com/document/60904)

### 6.2 与飞书的差异（决定实现形状）

| 维度 | 飞书 | 企微机器人 |
|------|------|-----------|
| 鉴权时机 | 连前拿预鉴权 URL | **连后发 `aibot_subscribe` 帧** |
| 心跳 | 应用层 `{"type":"ping"}` | `ping` 帧，30s |
| 连接数 | 多应用多连接 | 单机器人单连接（需主备而非多连） |
| 回复窗口 | 无硬性限制 | ~3 分钟，需流式占位 |

### 6.3 流式占位（必须实现）

Agent 执行可能远超 3 分钟，直接等到结束再回会被截断。必须：

1. 收到消息立即回 `finish=false` 的流式消息："正在处理，请稍候…"（同时满足 5 秒内响应要求）；
2. 处理完成后用**同一个 `stream.id`** 发 `finish=true` 的最终内容，客户端全量替换占位；
3. 超过 5 分钟仍未完成，回退到 `aibot_send_msg` 异步推送。

### 6.4 落点

与 §5.4 同构，差异仅在 DAO 层：

- `src/service/dao/wecom_bot/ws.rs` 实现 `WsClientAdapter`，复用 `pkg/ws` 的 supervisor 与退避重连；
- `src/service/dao/wecom_bot/http.rs` 出站（`aibot_send_msg` / `aibot_respond_msg`）；
- 复用 §5.3 的 `ChannelType::WeComBot` 与 `ROLE_WECOM_RECEPTION`。

---

## 7. 阶段划分

### 阶段一：iLink（本次实施）

**A. 通用能力（与微信解耦，独立可测）**

- [x] `common/src/models/inbound_state.rs`：新增 `InboundState` / `InboundCursor` / `CursorKind` / `InboundSessions`（含 serde 兼容与 `latest()` / `retain_latest()` 单测）
- [x] `common/src/api/agent.rs`：`AgentMatchCriteria` 加 `any_id` + `by_id()` / `by_ids()`；`match_scores` 加 `ID_EXACT_MATCH`
- [x] `src/service/domain/hr/mod.rs`：`resolve_agent` 加 id 维度（含直查短路）；新增 `resolve_agent_multi`；`resolve_agent` 委托为单元素版本
- [x] `src/pkg/adapter/mod.rs`：`AdaptedMessage` 加 `channel_type`
- [x] `src/producer/message_channel.rs`：删掉 `ROLE_FEISHU_RECEPTION` 硬编码，改按 §4.3.4 组装档位链
- [x] 迁移 `20260906000001_add_inbound_state_to_message_channels.sql`：加 `inbound_state` 一列
- [x] `MessageChannelDao` 加 `set_inbound_state`（整列覆盖写）

**B. 微信渠道**

- [x] `ChannelConfig` 加 iLink 字段；`agent_roles` 加 `ROLE_WECHAT_RECEPTION`
- [x] `CredentialKind::WechatIlink` 新增变体（7 处 match + Patch，见 §5.2.1）——✅ **已落地**
- [x] `src/models/events/wechat.rs`：`WechatInboundEvent` + iLink 消息 DTO
- [x] `src/service/dao/wechat/ilink.rs`：7 接口封装 + 长轮询循环（游标 / 会话写入 `inbound_state`；受管 stop/ensure 形态）
- [x] `src/service/dao/wechat/http.rs`：出站 `push` 落地（`context_token` 取自 `inbound_state.sessions`；接入域用 `credential.base_url`）
- [x] `src/service/dal/wechat/*`：`adapt_wechat`、身份定位、凭证解析（引用模式，见 §5.7）、注册适配器
- [x] 凭证变更 → 轮询移交：`update_credential` 补 `WechatIlink` 分支（bot_id / bot_token / base_url 任一变化即重建，见 §5.7）
- [x] `src/consumer/wechat_inbound.rs` + `consumer/mod.rs` 注册（`ConsumeMode::Async`）
- [x] 扫码登录接口（受保护）+ 前端凭据页授权入口，凭证存凭据表——✅ **已落地**
- [x] 单测（游标序列化 / id 维度打分 / 档位链 / 事件解析 / 幂等）；集成测试待真实扫码联调后补

**C. 渠道 CRUD 打通 iLink（真实逻辑取代占位字段）**

- [x] 删占位字段：`ChannelConfig.wechat_app_id` / `wechat_app_secret` / `wechat_open_id`（凭证已走凭据表，渠道里保留即双份真相）
- [x] `CreateWechatChannelConfig` 改为 `{ credential_id, peer_id, listen_inbound }`；响应 `WechatChannelConfig` 回显 `{ credential_id, credential_name, bot_id, peer_id, listen_inbound }`
- [x] create / update handler 映射新字段；凭证校验泛化为 `validate_channel_credential_ref`（飞书 LarkApp 与微信 WechatIlink 共用，kind 不匹配即拒）
- [x] `has_config_secret` 去掉 `wechat_app_secret`（微信渠道 config_json 已无敏感字段）
- [x] 前端渠道列表页创建表单 + 详情页编辑/展示：iLink 凭证下拉、对端 ID（留空自动回填）、入站监听开关、凭证未授权引导条
- [x] 单测：微信凭证必填 / kind 不匹配拒绝 / wechat 字段抽取 / 前端表单校验（共 +5）

**验收**：真实微信扫码后能收到 Agent 回复；连续对话上下文正确；重启后从游标续拉、不重复消费；渠道显式绑定 Agent 时必定命中它；`cargo fmt` + `clippy -D warnings` + `cargo test --lib` 全绿。

### 阶段二：企微智能机器人

- [ ] `ChannelType` 新增 `WeComBot` 变体（此时才动 `From<i32>` / `as_str` / 前端映射）
- [ ] `src/service/dao/wecom_bot/ws.rs`（`WsClientAdapter` + 订阅帧鉴权）
- [ ] 流式占位（占位 → 同 `stream.id` 全量替换 → 超时回退异步推送）
- [ ] `src/service/dal/wecom_bot/*` + consumer
- [ ] 手动联调（含群聊 @ 场景）

---

## 8. 风险与未决问题

| 风险 | 说明 | 应对 |
|------|------|------|
| 协议稳定性 | iLink 为 2026 年新协议，官方可能调整接口 | DAO 层集中封装，协议变更只影响 `dao/wechat/ilink.rs` |
| 账号身份 | iLink 以**个人微信号**作为 bot 身份，需独立微信号承载 | 文档明确"一个 bot 微信号 = 一个 channel"；不复用个人主号 |
| 专属渠道 | bot 只对扫码者生效，**无法群发、无法对话其他人** | 明确定位为专属 1:1 渠道（§5.1.2）；不做批量触达类功能 |
| Token 失效 | `bot_token` 可能过期或被踢 | 登录状态检测 + 重新扫码入口；失效时只影响该 channel |
| 群聊能力 | 插件能力元数据仅声明私聊 | 阶段一只做私聊，群聊待官方明确 |
| 主动推送 | 依赖 `context_token`，而它滚动刷新、有效期未知 | 数据已就位（`inbound_state.sessions`），阶段一不实现；真要做须先实测有效期与失效错误码，并准备"引导用户先发一条消息"的降级提示 |
| 合规边界 | 个人号通道的使用范围需遵守腾讯协议 | 仅用于自有 Agent 接入，不做批量营销与消息群发 |

**未决**：

1. iLink 是否有独立于 OpenClaw 插件的正式开发者文档（当前最完整信息来自插件源码与社区整理）——实施前需再确认一次官方口径。
2. `getupdates` 空游标的服务端语义（从最早还是从最新开始）——**影响面已缩小**：游标持久化后只有首次登录（无游标）会遇到，且此时几乎没有历史消息。联调时顺带实测即可。
3. 多 bot 账号（一个 ai_orz 实例挂多个微信）时的会话隔离粒度——阶段一按 `channel_id` 隔离，够用。
4. ~~一个 iLink bot 是否需要同时与多个微信用户对话~~——**已明确（不需要）**：二维码必须由某个微信用户主动扫码才完成授权，bot 的消息面只对扫码者生效，即专属 1:1 渠道。因此「一个 bot = 一个 channel = 一个对端」是协议事实而非简化假设，见 §5.1.2。无需设计多 peer 解析规则，`inbound_state.sessions` 实际最多一项（Vec 结构保留，不为此改结构）。
5. ~~`resolve_agent_multi` 是 trait 新增方法，需同步 Domain 单测的 Mock stub~~——**已完成**：Mock stub 已同步，档位链有实证单测。
