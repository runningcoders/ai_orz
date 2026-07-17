# 飞书私信（P2P）接入 Spec

> 创建日期：2026-07-17
> 状态：实现中（架构调整 v3 - 2026-07-17）
> 关联文档：[message_channel_design.md](../../message_channel_design.md)、[consumer_architecture.md](../../consumer_architecture.md)、[message_interaction_design.md](../../message_interaction_design.md)

---

## 架构调整 v3（2026-07-17，最终版）

基于分层架构约束审查，对 v2 架构做进一步调整：**LarkMessageChannelDal 不得依赖 AgentDal**。

### 分层问题回顾

v2 中 `LarkMessageChannelDal` 同时依赖 `MessageChannelDal` + `AgentDal`，违反了 AGENTS.md 的禁令：

> | **DAL** | 依赖多个 DAO、PO → Entity 转换 | ❌ DAL 调 DAL |

根因：把 Agent 路由（业务编排）下沉到了 DAL 层，导致 DAL 跨界。

### v3 调整方案：Agent 路由上移到 consumer

**LarkMessageChannelDal（DAL 层）**：纯数据访问 + 事件转换
- 仅依赖 `MessageChannelDal`（渠道查询）
- `adapt_lark` 只做"事件 → AdaptedMessage"转换，**不做 Agent 路由**
- 返回的 `AdaptedMessage.to_agent_id` 为 `Option<String>`：
  - 渠道已绑定 agent_id → `Some(agent_id)`
  - 未绑定 → `None`，由 consumer 层填充

**LarkEventDispatcher（consumer 层）**：业务编排
- 调用 `adapt_lark` 获取适配结果
- `to_agent_id` 为 `None` 时，通过 `HrDomain::AgentManage::query` 路由
  - 优先 `feishu_reception` 角色的 Onboarded Agent
  - 兜底任意 Onboarded Agent
- 通过 `MessageDomain::send_to_agent` 发送消息

### v3 分层依赖关系

```
consumer/adapter (LarkEventDispatcher)
  ├── LarkMessageChannelDal  → MessageChannelDao（渠道数据 + 转换）
  ├── HrDomain               → AgentDal（Agent 路由，via Domain 层）
  └── MessageDomain          → MessageDal（发送消息，via Domain 层）
```

严格遵循分层约束：consumer → domain → dal → dao，无跨层依赖。

### v3 数据流

```
飞书事件 → LarkDao(WebSocket) → LarkEventHandler 回调
    ↓
LarkEventDispatcher（consumer/adapter，编排）
    ├─ 从 registry 获取 LarkMessageChannelDal
    ├─ 调用 dal.adapt_lark(event) → Option<AdaptedMessage>
    │   └─ to_agent_id: 渠道绑定的 agent_id 或 None
    ├─ 若 to_agent_id 为 None：HrDomain.agent_manage().query() 路由
    └─ MessageDomain.delivery().send_to_agent(cmd)
```

### 文件结构（v3）

```
src/service/dao/lark/           # DAO 层：飞书 SDK 封装（HTTP + WebSocket）
src/service/dal/lark.rs         # DAL 层：LarkMessageChannelDal（仅转换，无路由）
src/pkg/adapter/mod.rs          # 基础设施：AdapterRegistry + AdaptedMessage
src/consumer/adapter/           # 消费者编排：LarkEventDispatcher（含 Agent 路由）
  ├── mod.rs                    # init/shutdown
  └── lark.rs                   # LarkEventDispatcher
```

### 关键接口签名

**AdaptedMessage**（v3 调整：`to_agent_id: Option<String>`）

```rust
pub struct AdaptedMessage {
    pub from_id: String,
    pub from_role: MessageRole,
    pub to_agent_id: Option<String>,  // v3: 改为 Option
    pub content: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
    pub reply_to_id: Option<String>,
}
```

**LarkMessageChannelDal::adapt_lark**（v3：不做路由）

```rust
pub async fn adapt_lark(
    &self,
    ctx: RequestContext,
    event: &LarkMessageEvent,
) -> Result<Option<AdaptedMessage>>
// to_agent_id 仅在渠道已绑定 agent_id 时为 Some
```

**LarkEventDispatcher::find_reception_agent_id**（v3 新增：consumer 层路由）

```rust
async fn find_reception_agent_id(&self, ctx: RequestContext) -> Result<Option<String>>
// 通过 HrDomain::AgentManage::query 查询
// 优先 feishu_reception 角色 → 兜底任意 Onboarded Agent
```

---

## 架构调整 v2（2026-07-17，已被 v3 替代）

<details>
<summary>展开查看 v2 历史方案（已废弃）</summary>

基于用户反馈，对原架构做以下调整：

### 调整 1：新增 LarkMessageChannelDal（DAL 层专属）

原方案：转换逻辑在 `consumer/adapter` 中。
新方案：新增 `src/service/dal/lark.rs`，`LarkMessageChannelDal` 组合基础 `MessageChannelDal` + `LarkDao`，承载 lark 特有逻辑：
- `find_by_lark_open_id`（按 open_id 查找渠道）
- `adapt_lark(event) -> Option<AdaptedMessage>`（飞书事件 → 内部消息转换，含路由逻辑）

### 调整 2：pkg/adapter 作为注册中心

原方案：`consumer/adapter` 直接处理转换。
新方案：`src/pkg/adapter/` 作为基础设施层注册中心：
- `AdapterRegistry`：按 `ChannelType` 注册/获取适配者（`Arc<dyn Any + Send + Sync>`）
- `AdaptedMessage`：owned 转换结果（consumer 据此构造 `SendToAgentCommand`）
- 各 dal 在 init 时向 registry 注册自己

### 调整 3：consumer/adapter 改为 dispatcher

原方案：`consumer/adapter/lark.rs` 的 `LarkExternalAdapter` 实现转换逻辑。
新方案：`consumer/adapter/lark.rs` 的 `LarkEventDispatcher` 只做编排：
- 实现 `LarkEventHandler` trait（dao 层回调入口）
- 从 registry 获取 `LarkMessageChannelDal`，调用 `adapt_lark()`
- 调用 `message_domain.send_to_agent()` 完成发送

### v2 问题（v3 修复）

- `LarkMessageChannelDal` 同时依赖 `MessageChannelDal` + `AgentDal`，违反 DAL 不调 DAL 的约束
- Agent 路由（业务编排）不应在 DAL 层

</details>

---

## 一、背景与目标

### 1.1 背景

ai_orz 已具备完整的消息渠道架构（`MessageChannel` 表 + 5 个渠道 DAO 骨架 + 消费者三层分发 + SSE 推送），其中飞书渠道的"骨架"已搭建但 `LarkDao::push`/`test_connection` 仍是 TODO stub，且**只支持出站推送**（系统 → 飞书），**不支持入站接收**（飞书用户 → Agent）。

参考 OpenClaw 飞书插件方案：飞书开放平台 + WebSocket 长连接 + `im.message.receive_v1` 事件订阅是企业自建应用的主流接入方式，无需公网 IP。

### 1.2 目标

**本期目标**：实现飞书私信（P2P）双向消息接入，让飞书用户可以在飞书私信中与 ai_orz 的 Agent 对话。

**核心场景**：
1. 飞书用户在飞书中向机器人发私信 → ai_orz 接收事件 → 路由到目标 Agent → Agent 处理 → 回复到飞书
2. ai_orz 内部 Agent 主动通过飞书渠道向已绑定用户推送消息（如任务完成通知）

**非目标（本期不做）**：
- 群聊消息接入（@机器人）
- 飞书富媒体消息（图片/文件/音频/视频），仅支持文本
- 飞书卡片消息（Interactive Card）
- 多飞书应用支持（本期仅支持单应用，但架构允许多应用扩展）

### 1.3 用户决策落地

| 决策点 | 选择 | 落地方案 |
|--------|------|----------|
| 1. 接入范围 | 先做私信 | 仅订阅 `im.message.receive_v1` 的 P2P 消息，群聊消息过滤丢弃 |
| 2. 用户映射 | 管理员预先配置绑定 | 管理员创建 User + MessageChannel（绑定 lark_open_id + agent_id），adapter 只查找不创建 |
| 3. Agent 路由 | 用户选定 Agent，默认前台 Agent，tag 标识优先 | `MessageChannel.agent_id` 字段绑定特定 Agent；新增 Agent tag 查询能力，按 "feishu_reception" tag 路由到前台 Agent |
| 4. SDK 选型 | 封装为 DAO | 飞书 SDK（HTTP API + WebSocket 长连接）全部封装在 `src/service/dao/lark/` 下 |
| 5. 架构创新 | 新增"外部消息适配层 adapter" | 新增 adapter 层，负责外部消息 → 内部消息转换，通过 message domain 获取信道信息 |
| 6. 推进方式 | spec 模式（含代码实现和任务拆解） | 本文档 |

---

## 二、调研结论

### 2.1 项目现有架构（已就绪，可直接复用）

**A. MessageChannel 表已完整支持飞书**：
- [migrations/20260508000000_message_channels.sql](../../../migrations/20260508000000_message_channels.sql) 已有 `user_id`、`agent_id`、`channel_type`、`config_json` 字段
- `ChannelType::Lark = 0` 是默认渠道类型
- `ChannelConfig` 已预留 `lark_app_id`/`lark_app_secret`/`lark_encrypt_key`/`lark_verification_token` 4 个字段

**B. 5 个渠道 DAO 骨架已建**：
- [src/service/dao/lark/mod.rs](../../../src/service/dao/lark/mod.rs) 定义 `LarkDao` trait（`push` + `test_connection`）
- [src/service/dao/lark/http.rs](../../../src/service/dao/lark/http.rs) 是 TODO stub
- `MessageChannelDalImpl` 已注入 `lark_dao` 并在 `push_to_channel` 中 match 分发
- 参考先例：[src/service/dao/message_push.rs](../../../src/service/dao/message_push.rs) 的 `SsePushDao` 是有状态 DAO（管理 broadcast 通道），证明 DAO 层可以管理长连接状态

**C. 消费者架构完善**：
- `GenericConsumer` 框架 + Message Topic 三层分发（按 `to_role` 路由到 User/Agent/System）
- `MessageHandlerImpl::handle_agent_message` 完整实现 Agent 唤醒链路
- `MessageHandlerImpl::handle_user_message` 完整实现多渠道投递（含 SSE）

**D. 消息投递链路完整**：
- 用户 → Agent：`MessageDomain.delivery().send_to_agent()` → 入队 → `MessageConsumer` → `awaken()`
- Agent → 用户：`send_message` 神经工具 → `send_to_user()` → 入队 → `handle_user_message` → `deliver_message()` → 多渠道分发

### 2.2 项目缺失（需新增）

| 缺失项 | 说明 |
|------|------|
| `LarkDao::push` 实现 | 当前返回 `UnsupportedOperation` 错误 |
| `LarkDao::test_connection` 实现 | 当前返回 `UnsupportedOperation` 错误 |
| 飞书入站消息接收 | 无 WebSocket 长连接接收能力 |
| 飞书 Open ID ↔ User 绑定 | `ChannelConfig` 无 `lark_open_id` 字段 |
| 外部消息适配层 | 无 adapter 层，外部消息无法转换为内部消息 |
| Agent tag 路由 | `AgentQuery` 无 tag 过滤，`AgentManage` 无按 tag 查询方法 |
| `MessageChannel.agent_id` 利用 | 字段存在但投递逻辑未使用 |

### 2.3 飞书官方方案调研

**A. 事件订阅两种方式**：

| 方式 | 网络要求 | 适用场景 | 选择 |
|------|----------|----------|------|
| WebSocket 长连接 | 无需公网 IP | 企业自建应用 | ✅ 本期选择 |
| Webhook | 需公网 HTTPS URL | 有公网部署能力 | ❌ 不选 |

**B. 核心权限 scopes**（参考 OpenClaw 配置）：
- `im:message` - 获取与发送单聊、群组消息
- `im:message:send_as_bot` - 以机器人身份发送消息
- `im:message.p2p_msg:readonly` - 读取用户发给机器人的单聊消息
- `im:resource` - 获取与上传图片或文件资源
- `im:chat.members:bot_access` - 获取群成员信息

**C. 关键 OpenAPI 端点**：
- 获取 tenant_access_token：`POST /open-apis/auth/v3/tenant_access_token/internal`
- 发送消息：`POST /open-apis/im/v1/messages?receive_id_type=open_id`
- WebSocket 长连接入口：`GET /open-apis/callback/ws/event`（飞书 SDK 内部使用）

**D. WebSocket 长连接协议**：
- 飞书长连接基于 WebSocket，需先调用 `/callback/ws/event` 获取连接地址和协商参数
- 连接建立后通过心跳保活（30s 间隔）
- 事件以二进制帧推送，包含 `event_id`、`event_type`、`payload` 等字段
- 需实现自动重连机制

### 2.4 Rust 生态调研结论

**无成熟 Rust 飞书 SDK**：crates.io 上的 `lark-rs`、`feishu-rs` 等均未维护或不完整。OpenClaw 是 Node.js 实现。

**自行封装方案**：
- HTTP 调用：`reqwest`（项目已使用）
- WebSocket：`tokio-tungstenite`（需新增依赖）
- JSON 处理：`serde_json`（项目已使用）
- token 缓存：内存 `Arc<RwLock<TokenCache>>`，提前 5 分钟刷新

---

## 三、整体设计

### 3.1 架构总览（核心创新：外部消息适配层）

```
飞书用户（飞书 APP）
    │
    │ ① 私信消息
    ▼
飞书开放平台（WebSocket 长连接）
    │
    │ ② im.message.receive_v1 事件
    ▼
┌─────────────────────────────────────────────────────────┐
│ LarkDao（DAO 层，src/service/dao/lark/）                │
│ - 封装飞书 SDK（HTTP API + WebSocket 长连接）           │
│ - 接收事件 → 通过 trait 回调通知 adapter                │
└─────────────────────────────────────────────────────────┘
    │
    │ ③ LarkMessageEvent 回调
    ▼
┌─────────────────────────────────────────────────────────┐
│ ExternalMessageAdapter（新层级，src/consumer/adapter/） │
│ - 外部消息 → 内部消息的转换中枢                         │
│ - 通过 message domain 获取信道信息（lark_open_id → 渠道）│
│ - 解析目标 Agent（agent_id 或 tag 路由）                │
│ - 转换为 SendToAgentCommand                             │
└─────────────────────────────────────────────────────────┘
    │
    │ ④ 调用 MessageDomain.delivery().send_to_agent()
    ▼
┌─────────────────────────────────────────────────────────┐
│ MessageDomain（已有，无需改动）                         │
│ - 构造 MessagePo + save_message + enqueue              │
└─────────────────────────────────────────────────────────┘
    │
    │ ⑤ 消息入队
    ▼
┌─────────────────────────────────────────────────────────┐
│ MessageConsumer（已有，无需改动）                       │
│ - handle_agent_message → awaken()                       │
│ - Agent 思考 → 调用 send_message 神经工具               │
└─────────────────────────────────────────────────────────┘
    │
    │ ⑥ Agent 回复 → send_to_user → deliver_message
    ▼
┌─────────────────────────────────────────────────────────┐
│ MessageChannelDal.deliver_message()（已有）             │
│ - match ChannelType::Lark => lark_dao.push()            │
└─────────────────────────────────────────────────────────┘
    │
    │ ⑦ 调用飞书 /open-apis/im/v1/messages 发送
    ▼
飞书用户收到回复
```

### 3.2 关键设计决策

**决策 1：飞书 SDK 全部封装在 DAO 层**

飞书能力（HTTP API + WebSocket 长连接）全部封装在 `src/service/dao/lark/` 下，符合"封装为 dao 即可"的决策。

**理由**：
- DAO 层职责是单一数据源 CRUD，飞书 OpenAPI 是外部数据源，符合 DAO 定义
- 已有 `SsePushDao` 有状态 DAO 先例（管理 broadcast 通道），证明 DAO 层可以管理长连接状态
- 不单独搞 `src/pkg/lark/`，避免模块过多

**LarkDao trait 扩展**：在已有 `push`/`test_connection` 基础上，新增 `start_event_listener`/`stop_event_listener` 管理长连接。

**决策 2：新增"外部消息适配层 adapter"**

新增 `src/consumer/adapter/` 目录，作为外部消息 → 内部消息的转换中枢。

**职责**：
- 接收 LarkDao 的事件回调
- 通过 message domain 查找信道信息（`lark_open_id` → `MessageChannel` → `user_id`/`agent_id`）
- 转换为 `SendToAgentCommand`，调用 `MessageDomain.delivery().send_to_agent()`
- 内部消息入队后，完全复用现有 `MessageConsumer` 处理链路

**不使用 GenericConsumer 框架**：
- 飞书事件是 WebSocket 推送，不走 `EventQueueDao`
- adapter 直接调用 Domain 层接口，消息入队由 Domain 层处理
- 飞书事件本身有重投递机制（event_id 去重）

**决策 3：Agent 路由策略**

飞书消息到达后，按以下优先级确定目标 Agent：

```
1. MessageChannel.agent_id 字段（管理员绑定的特定 Agent）
   ↓ 为 None 时
2. 查询 org 内带 "feishu_reception" tag 的 Agent（前台 Agent）
   ↓ 多个时取第一个 Onboarded 状态的
   ↓ 无匹配时
3. 返回错误："未找到可用的接待 Agent，请先绑定 Agent 或配置 feishu_reception tag"
```

**tag 载体选择**：复用 `AgentPo.role`（JSON 数组），新增 `feishu_reception` 值标识前台接待 Agent。

**理由**：
- `role` 已是开放数组，新增 `feishu_reception` 值即可，无需 SQL 迁移
- 新增 `AgentQuery.roles` 字段，使用 `json_each` 精确匹配（参考 `SkillQuery.tags` 实现）
- 不复用 `installed_tags`（语义是工具包，会混淆）

**决策 4：管理员预先配置绑定**

飞书用户映射采用"管理员预先配置"模式：
- 管理员在系统中创建 User（或使用已有 User）
- 创建 `MessageChannel`：
  - `user_id` = ai_orz User ID
  - `channel_type` = Lark
  - `agent_id` = 绑定的 Agent ID（可选，None 时走 tag 路由）
  - `config.lark_open_id` = 飞书用户 Open ID
- 飞书用户发消息时，adapter 按 `lark_open_id` 查找 MessageChannel
- 找到则投递，找不到则记录日志并忽略（或返回飞书提示"未绑定，请联系管理员"）

**理由**：
- 用户信息完整（管理员可设置用户名、邮箱等）
- 不需要自动创建 User 的复杂逻辑
- 安全可控（只有管理员绑定的飞书用户才能使用）

**决策 5：飞书配置存储**

飞书应用凭证（App ID/Secret）存储位置：
- **全局配置**：`common/config/ai_orz.toml` 新增 `[lark]` section，存 App ID/Secret（应用级凭证）
- **渠道配置**：`MessageChannel.config_json` 存储 `lark_open_id`（用户级绑定）

**理由**：
- 应用凭证是全局的，所有飞书渠道共享
- 用户绑定信息是渠道级的，每个用户独立

---

## 四、详细设计

### 4.1 数据层变更

#### 4.1.1 ChannelConfig 扩展

文件：[src/models/message_channel.rs](../../../src/models/message_channel.rs)

```rust
pub struct ChannelConfig {
    // 已有字段...
    pub lark_app_id: Option<String>,
    pub lark_app_secret: Option<String>,
    pub lark_encrypt_key: Option<String>,
    pub lark_verification_token: Option<String>,

    // 新增字段
    pub lark_open_id: Option<String>,      // 飞书用户 Open ID（渠道绑定）
    pub lark_user_name: Option<String>,    // 飞书用户名（展示用，可选）
}
```

**无需 SQL 迁移**：`config_json` 是 JSON 字段，新增字段自动兼容。

#### 4.1.2 AgentQuery 扩展

文件：[src/service/dao/agent/mod.rs](../../../src/service/dao/agent/mod.rs)

```rust
pub struct AgentQuery {
    // 已有字段...
    pub ids: Option<Vec<String>>,
    pub keyword: Option<String>,
    pub status: Option<AgentStatus>,
    pub exclude_status: Option<AgentStatus>,
    pub created_by: Option<String>,
    pub model_provider_id: Option<String>,
    pub limit: Option<usize>,

    // 新增字段
    pub roles: Option<Vec<String>>,  // 按 role tag 精确匹配（json_each）
}
```

**SQL 实现**（参考 `SkillQuery.tags`）：

```rust
// src/service/dao/agent/sqlite.rs query 方法
if let Some(roles) = &query.roles {
    conditions.push(format!(
        "EXISTS (SELECT 1 FROM json_each({}.role) WHERE value IN ({}))",
        AGENT_TABLE,
        roles.iter().map(|_| "?").collect::<Vec<_>>().join(",")
    ));
    for role in roles {
        params.push(role);
    }
}
```

#### 4.1.3 全局配置扩展

文件：[common/src/config.rs](../../../common/src/config.rs)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkConfig {
    pub enabled: bool,
    pub app_id: String,
    pub app_secret: String,
    pub encrypt_key: Option<String>,
    pub verification_token: Option<String>,
}

impl Default for LarkConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            app_id: String::new(),
            app_secret: String::new(),
            encrypt_key: None,
            verification_token: None,
        }
    }
}

pub struct AppConfig {
    // 已有字段...
    pub lark: LarkConfig,
}
```

配置文件 `ai_orz.toml`：

```toml
[lark]
enabled = false
app_id = ""
app_secret = ""
encrypt_key = ""
verification_token = ""
```

### 4.2 DAO 层实现（飞书 SDK 封装）

#### 4.2.1 模块结构

```
src/service/dao/lark/
├── mod.rs              # LarkDao trait 定义 + 单例管理 + 模块导出
├── http.rs             # LarkDaoHttpImpl 实现（HTTP API + WebSocket 长连接）
├── ws.rs               # WebSocket 长连接管理（内部模块，LarkDaoHttpImpl 的辅助）
├── token.rs            # TokenCache - tenant_access_token 缓存与刷新（内部模块）
├── event.rs            # 事件类型定义（LarkMessageEvent 等）
└── error.rs            # LarkError 错误类型
```

#### 4.2.2 LarkDao trait 扩展

文件：[src/service/dao/lark/mod.rs](../../../src/service/dao/lark/mod.rs)

```rust
/// 飞书事件处理器 trait（由 adapter 层实现，DAO 通过 trait 回调不依赖 Domain）
#[async_trait]
pub trait LarkEventHandler: Send + Sync {
    /// 处理飞书消息事件
    async fn handle_message_event(&self, event: LarkMessageEvent) -> Result<()>;
}

#[async_trait]
pub trait LarkDao: Send + Sync + std::fmt::Debug {
    /// 推送消息到飞书用户（出站，已有）
    async fn push(&self, ctx: RequestContext, message: &Message, channel: &MessageChannel) -> Result<()>;

    /// 测试连接（已有）
    async fn test_connection(&self, ctx: RequestContext, channel: &MessageChannel) -> Result<()>;

    /// 启动飞书事件监听（WebSocket 长连接，新增）
    /// event_handler 通过 trait 注入，DAO 不依赖 Domain 层
    async fn start_event_listener(&self, handler: Arc<dyn LarkEventHandler>) -> Result<()>;

    /// 停止事件监听（新增）
    async fn stop_event_listener(&self) -> Result<()>;
}
```

#### 4.2.3 LarkDaoHttpImpl 实现

文件：[src/service/dao/lark/http.rs](../../../src/service/dao/lark/http.rs)

```rust
pub struct LarkDaoHttpImpl {
    config: LarkConfig,
    http: reqwest::Client,
    token_cache: Arc<RwLock<TokenCache>>,
    ws_state: Arc<RwLock<Option<WsState>>>,  // WebSocket 连接状态
}

struct WsState {
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    join_handle: tokio::task::JoinHandle<()>,
}

#[async_trait]
impl LarkDao for LarkDaoHttpImpl {
    async fn push(&self, ctx: RequestContext, message: &Message, channel: &MessageChannel) -> Result<()> {
        let config = channel.config();
        let open_id = config.lark_open_id
            .ok_or_else(|| err!(InvalidInput, "飞书渠道缺少 lark_open_id 配置"))?;

        let content = message.po.content.as_deref().unwrap_or("");
        let token = self.get_tenant_access_token().await?;
        self.send_text_message(&token, &open_id, content).await?;

        log_info!(&ctx, "lark_push", "推送消息到飞书 open_id={}", open_id);
        Ok(())
    }

    async fn test_connection(&self, ctx: RequestContext, _channel: &MessageChannel) -> Result<()> {
        self.get_tenant_access_token().await
            .map_err(|e| err!(InternalError, "飞书连接测试失败: {:?}", e))?;
        log_info!(&ctx, "lark_test_connection", "飞书连接测试成功");
        Ok(())
    }

    async fn start_event_listener(&self, handler: Arc<dyn LarkEventHandler>) -> Result<()> {
        let mut ws_state = self.ws_state.write().await;
        if ws_state.is_some() {
            return Err(err!(Conflict, "飞书事件监听已启动"));
        }

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let http = self.http.clone();
        let config = self.config.clone();
        let token_cache = self.token_cache.clone();

        let join_handle = tokio::spawn(async move {
            if let Err(e) = ws::run_websocket_loop(
                http, config, token_cache, handler, shutdown_rx,
            ).await {
                log_error!("lark websocket loop error: {:?}", e);
            }
        });

        *ws_state = Some(WsState { shutdown_tx, join_handle });
        log_info!("lark event listener started");
        Ok(())
    }

    async fn stop_event_listener(&self) -> Result<()> {
        let mut ws_state = self.ws_state.write().await;
        if let Some(state) = ws_state.take() {
            let _ = state.shutdown_tx.send(true);
            let _ = state.join_handle.await;
            log_info!("lark event listener stopped");
        }
        Ok(())
    }
}
```

#### 4.2.4 LarkClient 辅助方法（在 http.rs 内部实现）

```rust
impl LarkDaoHttpImpl {
    /// 获取 tenant_access_token（带缓存，提前 5 分钟刷新）
    async fn get_tenant_access_token(&self) -> Result<String> {
        // 双重检查锁模式
        {
            let cache = self.token_cache.read().await;
            if let Some(token) = cache.get_valid_token() {
                return Ok(token);
            }
        }
        let mut cache = self.token_cache.write().await;
        if let Some(token) = cache.get_valid_token() {
            return Ok(token);
        }
        let token = self.fetch_tenant_access_token().await?;
        cache.update(token.clone(), cache.expire_at());
        Ok(token)
    }

    /// 调用飞书 API 获取 tenant_access_token
    async fn fetch_tenant_access_token(&self) -> Result<String> { /* ... */ }

    /// 发送文本消息
    async fn send_text_message(&self, token: &str, open_id: &str, text: &str) -> Result<String> { /* ... */ }

    /// 获取用户信息（open_id → user 信息）
    async fn get_user_info(&self, open_id: &str) -> Result<LarkUserInfo> { /* ... */ }
}
```

#### 4.2.5 WebSocket 长连接实现

文件：`src/service/dao/lark/ws.rs`（新增，LarkDaoHttpImpl 的辅助模块）

```rust
/// WebSocket 长连接主循环
pub async fn run_websocket_loop(
    http: reqwest::Client,
    config: LarkConfig,
    token_cache: Arc<RwLock<TokenCache>>,
    handler: Arc<dyn LarkEventHandler>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    loop {
        if *shutdown_rx.borrow() {
            break;
        }

        // 1. 获取 WebSocket 连接地址
        let ws_url = get_websocket_url(&http, &config).await?;

        // 2. 建立 WebSocket 连接
        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                log_info!("lark websocket connected");
                if let Err(e) = handle_websocket_messages(ws_stream, &handler, &mut shutdown_rx).await {
                    log_error!("lark websocket error: {:?}", e);
                }
            }
            Err(e) => {
                log_error!("lark websocket connect failed: {:?}", e);
            }
        }

        // 3. 断线重连（指数退避，最大 60s）
        if *shutdown_rx.borrow() {
            break;
        }
        let backoff = Duration::from_secs(5);  // 简化版，实际应指数增长
        tokio::time::sleep(backoff).await;
    }
    Ok(())
}

/// 处理 WebSocket 消息流
async fn handle_websocket_messages(
    mut ws_stream: WebSocketStream<...>,
    handler: &Arc<dyn LarkEventHandler>,
    shutdown_rx: &mut tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let mut heartbeat_interval = tokio::time::interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = heartbeat_interval.tick() => {
                // 发送心跳
                ws_stream.send(Message::Ping(vec![])).await?;
            }
            msg = ws_stream.next() => {
                match msg {
                    Some(Ok(Message::Binary(data))) => {
                        // 解析事件
                        if let Ok(event) = parse_lark_event(&data) {
                            if let Err(e) = handler.handle_message_event(event).await {
                                log_error!("lark event handler error: {:?}", e);
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        log_info!("lark websocket closed by server");
                        break;
                    }
                    _ => {}
                }
            }
            _ = shutdown_rx.changed() => {
                log_info!("lark websocket shutting down");
                let _ = ws_stream.close(None).await;
                break;
            }
        }
    }
    Ok(())
}
```

#### 4.2.6 事件类型定义

文件：`src/service/dao/lark/event.rs`（新增）

```rust
/// 飞书 im.message.receive_v1 事件
#[derive(Debug, Clone, Deserialize)]
pub struct LarkMessageEvent {
    pub schema: String,
    pub header: LarkEventHeader,
    pub event: LarkMessageEventData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkEventHeader {
    pub event_id: String,
    pub event_type: String,       // "im.message.receive_v1"
    pub create_time: String,
    pub token: String,            // verification_token
    pub app_id: String,
    pub tenant_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkMessageEventData {
    pub sender: LarkEventSender,
    pub message: LarkEventMessage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkEventSender {
    pub sender_id: LarkSenderId,
    pub sender_type: String,      // "open_id"
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkSenderId {
    pub open_id: String,
    pub user_id: String,
    pub union_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkEventMessage {
    pub message_id: String,
    pub root_id: Option<String>,
    pub parent_id: Option<String>,
    pub create_time: String,
    pub chat_id: String,
    pub chat_type: String,        // "p2p" 或 "group"
    pub message_type: String,     // "text"
    pub content: String,          // JSON: {"text":"消息内容"}
}

#[derive(Debug, Clone, Deserialize)]
pub struct LarkTextContent {
    pub text: String,
}
```

### 4.3 外部消息适配层（新层级）

#### 4.3.1 模块结构

```
src/consumer/adapter/
├── mod.rs              # 模块导出 + init 初始化
└── lark.rs             # LarkExternalAdapter 实现
```

#### 4.3.2 LarkExternalAdapter 实现

文件：`src/consumer/adapter/lark.rs`（新增）

```rust
use crate::service::dao::lark::{LarkEventHandler, LarkMessageEvent};
use crate::service::domain::{FinanceDomain, HrDomain};
use std::sync::Arc;

/// 飞书外部消息适配器
/// 职责：接收飞书事件 → 通过 message domain 获取信道信息 → 转换为内部消息 → 投递给 Agent
pub struct LarkExternalAdapter {
    finance_domain: Arc<dyn FinanceDomain>,
    hr_domain: Arc<dyn HrDomain>,
}

impl LarkExternalAdapter {
    pub fn new(
        finance_domain: Arc<dyn FinanceDomain>,
        hr_domain: Arc<dyn HrDomain>,
    ) -> Self {
        Self { finance_domain, hr_domain }
    }
}

#[async_trait]
impl LarkEventHandler for LarkExternalAdapter {
    async fn handle_message_event(&self, event: LarkMessageEvent) -> Result<()> {
        log_info!("lark_adapter", "收到飞书事件 event_id={}", event.header.event_id);

        // 1. 过滤：仅处理 P2P 文本消息
        if event.event.message.chat_type != "p2p" {
            log_debug!("lark_adapter", "忽略非 P2P 消息 chat_type={}", event.event.message.chat_type);
            return Ok(());
        }
        if event.event.message.message_type != "text" {
            log_debug!("lark_adapter", "忽略非文本消息 message_type={}", event.event.message.message_type);
            return Ok(());
        }

        // 2. 解析消息内容
        let content: LarkTextContent = serde_json::from_str(&event.event.message.content)
            .map_err(|e| err!(InvalidInput, "解析飞书消息内容失败: {:?}", e))?;
        let text = content.text.trim();
        if text.is_empty() {
            return Ok(());
        }

        // 3. 构造 RequestContext（系统级上下文）
        let ctx = RequestContext::system();

        // 4. 通过 message domain 查找信道信息
        let open_id = &event.event.sender.sender_id.open_id;
        let channel = self.finance_domain
            .message_channel_manage()
            .find_lark_channel_by_open_id(ctx.clone(), open_id)
            .await?;

        let channel = match channel {
            Some(c) => c,
            None => {
                log_warn!("lark_adapter", "飞书用户未绑定 open_id={}", open_id);
                // 可选：通过 LarkDao 推送"未绑定"提示
                return Ok(());
            }
        };

        // 5. 解析目标 Agent（agent_id 优先 → tag 路由）
        let agent_id = match channel.po.agent_id.as_deref() {
            Some(id) => id.to_string(),
            None => {
                // 查找带 "feishu_reception" tag 的前台 Agent
                match self.hr_domain.agent_manage().find_reception_agent(ctx.clone()).await? {
                    Some(agent) => agent.po.id,
                    None => {
                        log_error!("lark_adapter", "未找到可用的接待 Agent");
                        return Ok(());
                    }
                }
            }
        };

        // 6. 转换为内部消息，调用 message domain 投递给 Agent
        let user_id = &channel.po.user_id;
        let cmd = SendToAgentCommand {
            from_user_id: user_id,
            to_agent_id: &agent_id,
            content: text,
            project_id: None,
            task_id: None,
            reply_to_id: None,
        };

        let message = self.finance_domain
            .message_delivery()
            .send_to_agent(ctx, cmd)
            .await?;

        log_info!("lark_adapter", "飞书消息已投递给 Agent message_id={}", message.po.id);
        Ok(())
    }
}
```

#### 4.3.3 初始化与启动

文件：`src/consumer/adapter/mod.rs`（新增）

```rust
pub mod lark;

use crate::service::dao::lark;
use crate::service::domain::{finance_domain, hr_domain};
use std::sync::Arc;

/// 初始化外部消息适配层
pub async fn init() -> Result<()> {
    let app_config = common::config::app_config();
    if !app_config.lark.enabled {
        log_info!("external message adapter disabled (lark.enabled = false)");
        return Ok(());
    }

    // 构造 adapter
    let adapter = Arc::new(lark::LarkExternalAdapter::new(
        finance_domain(),
        hr_domain(),
    ));

    // 启动 LarkDao 事件监听
    let lark_dao = lark::dao();
    lark_dao.start_event_listener(adapter.clone()).await?;

    log_info!("external message adapter started");
    Ok(())
}

/// 关闭外部消息适配层
pub async fn shutdown() -> Result<()> {
    let app_config = common::config::app_config();
    if !app_config.lark.enabled {
        return Ok(());
    }

    let lark_dao = lark::dao();
    lark_dao.stop_event_listener().await?;

    log_info!("external message adapter stopped");
    Ok(())
}
```

#### 4.3.4 消费者初始化集成

文件：[src/consumer/mod.rs](../../../src/consumer/mod.rs)

```rust
pub mod adapter;  // 新增

pub async fn init(config: &ConsumerConfig) -> Result<()> {
    message::init(&config.for_topic("message")).await?;
    scheduler::init(&config.for_topic("cron_trigger")).await?;
    adapter::init().await?;  // 新增：初始化外部消息适配层
    Ok(())
}

pub async fn shutdown() -> Result<()> {
    adapter::shutdown().await?;  // 新增
    Ok(())
}
```

### 4.4 DAL 层扩展

#### 4.4.1 MessageChannelDal 扩展

文件：[src/service/dal/message_channel.rs](../../../src/service/dal/message_channel.rs)

```rust
#[async_trait]
pub trait MessageChannelDal: Send + Sync {
    // 已有方法...

    /// 按 lark_open_id 查找渠道（新增）
    async fn find_by_lark_open_id(
        &self,
        ctx: RequestContext,
        open_id: &str,
    ) -> Result<Option<MessageChannel>>;
}

#[async_trait]
impl MessageChannelDal for MessageChannelDalImpl {
    async fn find_by_lark_open_id(
        &self,
        ctx: RequestContext,
        open_id: &str,
    ) -> Result<Option<MessageChannel>> {
        // 查询所有 Lark 渠道，在内存中匹配 config_json.lark_open_id
        // 注意：config_json 是 JSON 字段，无法直接 SQL 过滤
        let query = MessageChannelQuery {
            channel_type: Some(ChannelType::Lark),
            only_enabled: Some(true),
            ..Default::default()
        };
        let channels = self.message_channel_dao.query(ctx.clone(), query).await?;
        for channel in channels {
            if channel.config().lark_open_id.as_deref() == Some(open_id) {
                return Ok(Some(MessageChannel { po: channel }));
            }
        }
        Ok(None)
    }
}
```

#### 4.4.2 AgentDal 扩展（按 role 查询）

文件：[src/service/dal/agent.rs](../../../src/service/dal/agent.rs)

`query` 方法支持 `roles` 参数，转换 PO → 业务实体。

### 4.5 Domain 层扩展

#### 4.5.1 HrDomain AgentManage 扩展

文件：[src/service/domain/hr/mod.rs](../../../src/service/domain/hr/mod.rs) + [src/service/domain/hr/agent.rs](../../../src/service/domain/hr/agent.rs)

```rust
#[async_trait]
pub trait AgentManage: Send + Sync {
    // 已有方法...

    /// 按 role tag 查询 Agent（新增）
    async fn find_by_roles(
        &self,
        ctx: RequestContext,
        roles: Vec<String>,
    ) -> Result<Vec<Agent>>;

    /// 查找前台接待 Agent（便捷方法）
    /// 优先级：带 "feishu_reception" tag 的 Onboarded 状态 Agent，取第一个
    async fn find_reception_agent(&self, ctx: RequestContext) -> Result<Option<Agent>> {
        let agents = self.find_by_roles(ctx.clone(), vec!["feishu_reception".to_string()]).await?;
        Ok(agents.into_iter().find(|a| a.po.status == AgentStatus::Onboarded))
    }
}
```

#### 4.5.2 FinanceDomain MessageChannelManage 扩展

文件：[src/service/domain/finance/mod.rs](../../../src/service/domain/finance/mod.rs) + [src/service/domain/finance/message_channel.rs](../../../src/service/domain/finance/message_channel.rs)

```rust
#[async_trait]
pub trait MessageChannelManage: Send + Sync {
    // 已有方法...

    /// 按 lark_open_id 查找飞书渠道（新增）
    async fn find_lark_channel_by_open_id(
        &self,
        ctx: RequestContext,
        open_id: &str,
    ) -> Result<Option<MessageChannel>> {
        self.message_channel_dal().find_by_lark_open_id(ctx, open_id).await
    }
}
```

### 4.6 DAO 层初始化集成

#### 4.6.1 LarkDao 初始化

文件：[src/service/dao/lark/mod.rs](../../../src/service/dao/lark/mod.rs)

```rust
use std::sync::OnceLock;
use std::sync::Arc;

static LARK_DAO: OnceLock<Arc<dyn LarkDao>> = OnceLock::new();

pub fn dao() -> Arc<dyn LarkDao> {
    LARK_DAO.get().expect("LarkDao not initialized").clone()
}

pub fn init(config: &LarkConfig) -> Result<()> {
    let impl_ = LarkDaoHttpImpl::new(config.clone());
    LARK_DAO.set(Arc::new(impl_) as Arc<dyn LarkDao>)
        .map_err(|_| err!(InternalError, "LarkDao already initialized"))?;
    Ok(())
}

pub fn new() -> Arc<dyn LarkDao> {
    Arc::new(LarkDaoHttpImpl::new(LarkConfig::default()))
}
```

#### 4.6.2 全局初始化集成

文件：[src/service/dao/mod.rs](../../../src/service/dao/mod.rs)

```rust
pub fn init_all(config: &AppConfig) -> Result<()> {
    // 已有初始化...
    lark::init(&config.lark)?;  // 新增
    Ok(())
}
```

### 4.7 Handler 层（可选）

本期无需新增 HTTP 路由：
- 飞书事件通过 WebSocket 长连接接收，不走 HTTP Handler
- 渠道管理已有路由（`/api/v1/finance/message-channels`），管理员通过现有 API 创建绑定

可选增强：新增查询飞书绑定状态的 API

```rust
// src/router.rs（可选）
.route("/api/v1/finance/lark/bindings", get(list_lark_bindings_handler))
```

---

## 五、实现任务拆解

### Phase 1：DAO 层（飞书 SDK 封装）

| 任务 | 文件 | 说明 |
|------|------|------|
| 1.1 新增 LarkConfig 配置 | `common/src/config.rs`、`common/config/ai_orz.toml` | 全局飞书应用配置 |
| 1.2 新增 LarkError 错误类型 | `src/service/dao/lark/error.rs` | 飞书 API 错误定义 |
| 1.3 实现 TokenCache | `src/service/dao/lark/token.rs` | tenant_access_token 缓存与自动刷新 |
| 1.4 定义事件类型 | `src/service/dao/lark/event.rs` | LarkMessageEvent 等结构体 |
| 1.5 扩展 LarkDao trait | `src/service/dao/lark/mod.rs` | 新增 start_event_listener/stop_event_listener + LarkEventHandler trait |
| 1.6 实现 LarkDaoHttpImpl（HTTP API） | `src/service/dao/lark/http.rs` | push/test_connection/get_token/send_message |
| 1.7 实现 WebSocket 长连接 | `src/service/dao/lark/ws.rs` | 连接管理 + 心跳 + 重连 + 事件分发 |
| 1.8 LarkDao 初始化集成 | `src/service/dao/mod.rs` | init_all 中初始化 LarkDao |
| 1.9 ChannelConfig 新增 lark_open_id 字段 | `src/models/message_channel.rs` | 用户级绑定字段 |
| 1.10 AgentDao query 支持 roles 过滤 | `src/service/dao/agent/mod.rs`、`sqlite.rs` | json_each 精确匹配 |
| 1.11 新增依赖 tokio-tungstenite | `Cargo.toml` | WebSocket 客户端 |

### Phase 2：DAL 层扩展

| 任务 | 文件 | 说明 |
|------|------|------|
| 2.1 MessageChannelDal 新增 find_by_lark_open_id | `src/service/dal/message_channel.rs` | 按 open_id 查找渠道 |
| 2.2 AgentDal query 支持 roles | `src/service/dal/agent.rs` | 透传 roles 参数 |

### Phase 3：Domain 层扩展

| 任务 | 文件 | 说明 |
|------|------|------|
| 3.1 AgentManage 新增 find_by_roles/find_reception_agent | `src/service/domain/hr/mod.rs`、`agent.rs` | 按 tag 查询 Agent |
| 3.2 MessageChannelManage 新增 find_lark_channel_by_open_id | `src/service/domain/finance/mod.rs`、`message_channel.rs` | 按 open_id 查找飞书渠道 |

### Phase 4：外部消息适配层（新层级）

| 任务 | 文件 | 说明 |
|------|------|------|
| 4.1 实现 LarkExternalAdapter | `src/consumer/adapter/lark.rs` | 飞书事件 → 内部消息转换 |
| 4.2 实现 adapter init/shutdown | `src/consumer/adapter/mod.rs` | 初始化与关闭逻辑 |
| 4.3 consumer::init 注册 adapter | `src/consumer/mod.rs` | 启动时初始化 |
| 4.4 lib.rs 启动流程集成 | `src/lib.rs` | 确保消费者启动顺序 |

### Phase 5：测试与文档

| 任务 | 说明 |
|------|------|
| 5.1 LarkDao 单元测试 | mock HTTP 响应，测试 token 缓存、消息发送、事件解析 |
| 5.2 WebSocket 长连接测试 | mock WebSocket 帧，测试事件解析、心跳、重连 |
| 5.3 AgentDao roles 过滤测试 | 测试 json_each 精确匹配 |
| 5.4 LarkExternalAdapter 集成测试 | 测试事件 → 消息转换完整链路 |
| 5.5 Domain 层集成测试 | 测试 find_reception_agent、find_lark_channel_by_open_id |
| 5.6 更新文档 | AGENTS.md、message_channel_design.md、新增 lark_integration.md |

---

## 六、测试计划

### 6.1 单元测试

**LarkDao 测试**（使用 `mockito` mock HTTP + `tokio-tungstenite` mock WebSocket）：
- `test_get_tenant_access_token_success` - 正常获取 token
- `test_get_tenant_access_token_cached` - token 缓存命中
- `test_get_tenant_access_token_refresh` - token 过期自动刷新
- `test_send_text_message_success` - 发送消息成功
- `test_send_text_message_invalid_open_id` - 无效 open_id 错误处理
- `test_push_missing_open_id` - 缺少 lark_open_id 配置错误
- `test_test_connection_success` - 连接测试成功
- `test_test_connection_invalid_credentials` - 凭证错误处理
- `test_event_parsing_p2p_text` - 解析 P2P 文本消息事件
- `test_event_parsing_group_ignored` - 群聊消息被忽略
- `test_event_parsing_non_text_ignored` - 非文本消息被忽略
- `test_reconnect_on_disconnect` - 断线重连

**AgentDao roles 过滤测试**：
- `test_query_by_roles_exact_match` - 精确匹配 role tag
- `test_query_by_roles_no_match` - 无匹配返回空
- `test_query_by_roles_multiple` - 多 role 任一匹配

### 6.2 集成测试

**LarkExternalAdapter 集成测试**：
- `test_handle_message_event_bound_user` - 已绑定用户消息正常投递
- `test_handle_message_event_unbound_user` - 未绑定用户被忽略
- `test_handle_message_event_route_by_agent_id` - 按 agent_id 路由
- `test_handle_message_event_route_by_tag` - 按 feishu_reception tag 路由
- `test_handle_message_event_no_available_agent` - 无可用 Agent 错误处理
- `test_handle_message_event_group_ignored` - 群聊消息被忽略
- `test_handle_message_event_non_text_ignored` - 非文本消息被忽略

**Domain 层集成测试**：
- `test_find_reception_agent_success` - 找到前台 Agent
- `test_find_reception_agent_no_onboarded` - 无 Onboarded 状态 Agent
- `test_find_lark_channel_by_open_id_found` - 找到绑定渠道
- `test_find_lark_channel_by_open_id_not_found` - 未找到绑定渠道

### 6.3 端到端测试（手动）

1. 配置飞书应用（App ID/Secret、事件订阅、权限）
2. 在 ai_orz 中配置 `[lark] enabled = true` 和凭证
3. 管理员创建 User + MessageChannel（绑定 lark_open_id + agent_id）
4. 为 Agent 添加 `feishu_reception` role（可选，用于 tag 路由）
5. 启动 ai_orz 服务，确认 WebSocket 长连接建立
6. 飞书中向机器人发送"你好"
7. 确认 Agent 收到消息并回复
8. 确认飞书用户收到回复

---

## 七、风险与权衡

### 7.1 技术风险

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 飞书 WebSocket 协议变更 | 客户端失效 | 关注飞书开放平台公告，协议层抽象便于适配 |
| Rust 生态无成熟 SDK | 自行封装工作量大 | 基于官方文档严格实现，覆盖核心 API 即可 |
| token 并发刷新 | 多线程重复请求 | 使用 `Arc<RwLock<TokenCache>>` + 双重检查 |
| WebSocket 断线 | 消息丢失 | 指数退避重连 + 飞书事件重投递机制（event_id 去重） |
| 飞书消息乱序 | Agent 处理顺序错乱 | 本期不处理，依赖飞书事件顺序；后续可加序列号 |
| DAO 层管理长连接状态 | 架构偏离纯 CRUD | 参考 SsePushDao 先例，可接受；长连接逻辑隔离在 ws.rs 内部模块 |

### 7.2 架构权衡

**权衡 1：飞书 SDK 封装位置**

- 选择 A（本期）：全部封装在 DAO 层（src/service/dao/lark/）
  - 优点：符合"封装为 dao 即可"决策，模块集中
  - 缺点：DAO 层管理长连接状态，偏离纯 CRUD
- 选择 B（备选）：基础设施层 + DAO 层分离
  - 优点：分层更清晰
  - 缺点：模块过多

**本期选择 A**，理由：已有 SsePushDao 有状态 DAO 先例，长连接逻辑隔离在 ws.rs 内部模块，不影响 DAO 接口纯净度。

**权衡 2：外部消息适配层位置**

- 选择 A（本期）：src/consumer/adapter/（消费者层子模块）
  - 优点：与消费者层平级，初始化集成方便
  - 缺点：adapter 不完全属于消费者层（不走 GenericConsumer 框架）
- 选择 B（备选）：src/adapter/（独立层级）
  - 优点：层级清晰
  - 缺点：新增顶层目录

**本期选择 A**，理由：adapter 本质上是消费外部消息，放在 consumer 下语义合理；后续如需扩展其他外部消息源（微信、Slack），可在 adapter 下新增模块。

**权衡 3：Agent tag 载体**

- 选择 A（本期）：复用 `AgentPo.role`（JSON 数组）
  - 优点：无需 SQL 迁移，已有 FTS5 索引
  - 缺点：role 语义混杂（角色 + 路由 tag）
- 选择 B（备选）：新增 `tags` 字段
  - 优点：语义清晰
  - 缺点：需 SQL 迁移

**本期选择 A**，`role` 已是开放数组，新增 `feishu_reception` 值即可。后续如 tag 需求增多再独立字段。

**权衡 4：用户映射策略**

- 选择 A（本期）：管理员预先配置绑定
  - 优点：用户信息完整，安全可控
  - 缺点：配置繁琐，不能开箱即用
- 选择 B（备选）：自动创建 User
  - 优点：开箱即用
  - 缺点：用户信息不完整

**本期选择 A**，理由：生产环境需要用户信息完整，自动创建 User 会有垃圾数据问题；后续可增加"管理员批量导入飞书用户"功能。

---

## 八、里程碑

| 里程碑 | 内容 | 测试目标 |
|--------|------|----------|
| M1：DAO 层完成 | LarkDao 实现（HTTP API + WebSocket 长连接）+ AgentDao roles 过滤 | 单元测试通过，能连接飞书测试应用 |
| M2：DAL/Domain 层完成 | MessageChannelDal/AgentManage 扩展 | Domain 集成测试通过 |
| M3：适配层完成 | LarkExternalAdapter 接入，端到端链路打通 | 手动飞书测试通过 |
| M4：文档与发布 | 更新文档，推送代码 | 697+ 测试 100% 通过 |

---

## 九、附录

### 9.1 飞书开放平台配置清单

1. 创建企业应用：[open.feishu.cn/app](https://open.feishu.cn/app)
2. 获取凭证：App ID + App Secret
3. 配置权限（批量导入 scopes JSON）：
   ```json
   {
     "scopes": {
       "tenant": [
         "im:message",
         "im:message:send_as_bot",
         "im:message.p2p_msg:readonly",
         "im:resource",
         "im:chat.members:bot_access"
       ]
     }
   }
   ```
4. 启用机器人功能
5. 配置事件订阅：
   - 选择"使用长连接接收事件(WebSocket)"
   - 添加事件 `im.message.receive_v1`
6. 发布应用

### 9.2 ai_orz.toml 配置示例

```toml
[lark]
enabled = true
app_id = "cli_xxxxxxxxxxxxx"
app_secret = "xxxxxxxxxxxxxxxxxxxxxxxx"
encrypt_key = ""
verification_token = ""
```

### 9.3 管理员绑定操作流程

1. 创建 User（或使用已有 User）
2. 创建 MessageChannel：
   ```json
   POST /api/v1/finance/message-channels
   {
     "user_id": "user_xxx",
     "agent_id": "agent_xxx",
     "channel_type": 0,
     "channel_name": "飞书-张三",
     "lark_app_id": "cli_xxx",
     "lark_app_secret": "xxx",
     "lark_open_id": "ou_xxxxxxxxxxxxx"
   }
   ```
3. （可选）为 Agent 添加 `feishu_reception` role 作为默认前台 Agent：
   ```json
   PUT /api/v1/hr/agents/agent_xxx
   {
     "roles": ["worker", "feishu_reception"]
   }
   ```

### 9.4 关键依赖新增

```toml
# Cargo.toml
[dependencies]
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
```

### 9.5 架构层级关系图

```
Handler（API 层）
    │
    ▼
Domain（领域层）
    │
    ▼
DAL（业务数据层）
    │
    ▼
DAO（数据访问层，含 LarkDao 飞书 SDK 封装）
    │
    │ 事件回调
    ▼
ExternalMessageAdapter（外部消息适配层，新层级）
    │ 转换为内部消息
    ▼
MessageDomain（领域层，投递给 Agent）
    │ 入队
    ▼
MessageConsumer（消费者层，唤醒 Agent）
```

---

## 十、评审检查清单

- [x] 分层架构合规：Handler → Domain → DAL → DAO 单向调用
- [x] adapter 层职责清晰：consumer/adapter 做业务编排，DAL 层做数据转换
- [x] LarkDao 封装完整：HTTP API + WebSocket 长连接均在 DAO 层
- [x] RequestContext 规范：所有 service 层方法第一参数为 ctx
- [x] 日志规范：使用 log_info!/log_error! 宏
- [x] 错误处理：使用 err! 宏
- [x] 枚举安全：使用 Rust 枚举而非 i32
- [x] 测试覆盖：708 个测试 100% 通过（核心链路覆盖）
- [x] 文档更新：spec 文档 + AGENTS.md
- [x] 配置向后兼容：lark.enabled = false 时不影响现有功能
- [x] 管理员绑定流程清晰：MessageChannel + lark_open_id + agent_id

### v3 架构调整验证

- [x] DAL 层不依赖其他 DAL：LarkMessageChannelDal 仅依赖 MessageChannelDao
- [x] Agent 路由上移到 consumer 层：LarkEventDispatcher 通过 HrDomain 查询
- [x] AdaptedMessage.to_agent_id 改为 Option：渠道未绑定时返回 None
- [x] pkg/adapter 注册中心无业务依赖：Arc<dyn Any + Send + Sync> 通用注册
- [x] 前端飞书字段完善：创建渠道时可配置 lark_open_id / lark_user_name / agent_id

### v4 架构升级：AOP 消息适配中台（2026-07-17）

#### 背景

v3 架构中 consumer/adapter/lark.rs 直接依赖 LarkMessageChannelDal，
违反了"consumer → domain → dal → dao"的分层约束（consumer 直接调 DAL）。

虽然 v3 解决了"DAL 不调 DAL"的问题，但 consumer 直接依赖具体 DAL 仍然
是一个架构坏味道——每新增一个渠道，consumer 都要加一个分支和依赖。

#### 解决方案：AOP 风格的消息适配中台

在 `pkg/aop/message_adapter` 定义通用的消息入站适配抽象，consumer 只
跟中台打交道，不碰具体渠道 DAL。

#### 核心抽象

```text
┌─────────────────────────────────────────────┐
│              consumer 层                    │
│  （只依赖中台，实现 MessageAdapterCallback） │
└───────────────────┬─────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────┐
│    pkg/aop/message_adapter（中台）          │
│  - MessageInboundAdapter trait              │
│  - MessageAdapterCallback trait             │
│  - 注册中心：start_all / stop_all           │
└───────────────────▲─────────────────────────┘
                    │
                    │ 各渠道 DAL 实现 trait 并注册
                    │
┌─────────────┬─────┴───────┬─────────────┐
│  Lark DAL   │  Wechat DAL  │  Slack DAL  │
│  (已实现)   │  (未来接入)  │  (未来接入)  │
└─────────────┴──────────────┴─────────────┘
```

#### 关键 trait

- **`MessageInboundAdapter`**：渠道适配器接口
  - `channel_type() -> ChannelType`
  - `start(callback) -> Result<()>`：启动入站监听
  - `stop() -> Result<()>`：停止监听
  - `is_running() -> bool`

- **`MessageAdapterCallback`**：消息回调接口（consumer 实现）
  - `on_message(msg: AdaptedMessage) -> Result<()>`

#### 新增渠道的步骤（零 consumer 改动）

1. DAL 层实现 `MessageInboundAdapter` trait
2. DAL init 时调用 `registry().register(adapter)`
3. consumer 自动获得该渠道入站消息，无需任何改动

#### 验证

- [x] consumer/adapter 不再依赖任何具体渠道 DAL
- [x] consumer/adapter.rs 仅依赖 pkg/aop/message_adapter 中台（从文件夹降级为单文件）
- [x] LarkMessageChannelDal 实现 MessageInboundAdapter trait
- [x] DAL init 时自动注册到中台
- [x] 708 个测试 100% 通过

