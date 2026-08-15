# 消息渠道设计文档

> 🎯 **本文档定位**：消息渠道入站适配中台架构、渠道生命周期管理与多渠道推送分发设计
> 状态：定稿（2026-08-05 管理面已落地，运行面按渠道分化实现）
> 查阅场景：新增外部入站消息渠道类型、排查渠道生命周期 CRUD、理解渠道引用检查语义时打开；字段级 PO/DAO 定义直接看代码
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 整体分层架构
> - [message_interaction_design.md](./message_interaction_design.md) — 用户-Agent 消息交互链路与前台调度
> - [lark_cli_integration.md](./lark_cli_integration.md) — 飞书渠道多应用化与 lark-cli 工具集成
> - 【② Plan 落地】[身份凭证Domain统一CRUD重构.md](../plan/身份凭证Domain统一CRUD重构.md) — 凭证引用检查 + 加密的落地实现
> - 【② Plan 落地】[飞书集成二期.md](../plan/lark-cli_集成二期.md) — 飞书 WS 私信入站落地
> - 【③ Wiki 长文】[身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md](docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md) — §5 渠道引用检查 + §5 AES 加密
> - 【③ Wiki 长文】[消息渠道管理.md](docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md)
> - 【③ Wiki 长文】[消息渠道适配器.md](docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/消息渠道适配器.md)
> - 【③ Wiki 长文】[多渠道消息系统.md](docs/wiki/zh/content/项目概述/核心功能特性/多渠道消息系统/多渠道消息系统.md)
> - 【③ Wiki 长文】[消息通道生产者.md](docs/wiki/zh/content/基础设施/AOP%20事件系统/事件生产者/消息通道生产者.md)
> - 【④ RAG 卡】[身份凭证 Domain 统一 CRUD](docs/wiki/knowledge/zh/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用.md) — §3/§4 渠道引用拒删前置检查
> - 【④ RAG 卡】[AES-256-GCM 敏感字段加密](docs/wiki/knowledge/zh/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容.md) — §1 闭包注入设计动机
> - 【④ RAG 卡 2 张（Batch4 新增）】
>   - [消息渠道入站适配中台：MessageInboundAdapter trait + MessageAdapterRegistry 全局注册 + start_all stop_all 生命周期](docs/wiki/knowledge/zh/消息渠道入站适配中台：MessageInboundAdapter%20trait%20+%20MessageAdapterRegistry%20全局注册%20+%20start_all%20stop_all%20生命周期/消息渠道入站适配中台：MessageInboundAdapter%20trait%20+%20MessageAdapterRegistry%20全局注册%20+%20start_all%20stop_all%20生命周期.md) — pkg/adapter 纯基础设施分层 + start_all 非 fail-fast
>   - [Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics 健康指标](docs/wiki/knowledge/zh/Lark%20P2P%20WS%20私信入站：身份凭证引用解析%20+%20app_id%20聚合%20WS%20+%20open_id%20自动映射%20+%20LarkWsMetrics%20健康指标/Lark%20P2P%20WS%20私信入站：身份凭证引用解析%20+%20app_id%20聚合%20WS%20+%20open_id%20自动映射%20+%20LarkWsMetrics%20健康指标.md) — 飞书双向闭环

## 🚀 实现完成状态（2026-08-05 更新）

### 🟡 管理面完成，运行面渠道推送进度分化

| 模块 | 状态 | 测试 | 文件位置 |
|------|------|------|---------|
| ChannelType 枚举 | ✅ 完成 | - | `common/src/enums/message_channel.rs` |
| MessageChannel PO | ✅ 完成 | - | `src/models/message_channel.rs` |
| MessageChannelDao | ✅ 完成 | ✅ 测试通过 | `src/service/dao/message_channel.rs` |
| 各渠道 DAO | ✅ 完成 | ✅ 测试通过 | `src/service/dao/lark/`, `wechat/`, `slack/`, `email/`, `webhook/`, `a2a_callback/` |
| MessageChannelDal | ✅ CRUD + Channel 枚举分发完成 | ✅ 单元测试通过；**Lark/Webhook 等实际 HTTP 推送按渠道分化** | `src/service/dal/message_channel.rs` |
| Message Domain (send_to_agent/send_to_user/deliver_message) | ✅ 完成 | ✅ **7 个集成测试通过**（见下表） | `src/service/domain/message/` |
| Finance Domain 管理面 | ✅ CRUD/query/test 已具备 | ✅ 管理面 HTTP handler 通过 smoke test | `src/service/domain/finance/message_channel.rs` |
| API DTO | ✅ 管理面 DTO 已完成 | `cargo check` 通过 | `common/src/api/message_channel.rs` |
| Handler / Router | ✅ 管理面 action 已完成 | ✅ 管理面路由 smoke 可用 | `src/handlers/finance/message_channel/`, `src/router.rs` |
| 单元测试 | ✅ 完成 | **67/67 通过** | 各模块对应 `tests.rs` |
| 集成测试（消息投递 + 推送）| ✅ 完成 | **7/7 通过** | `tests/integration/message_delivery_test.rs` |

> 运行面消息发送接口已统一使用 Command 参数对象：`SendToAgentCommand`、`SendToUserCommand`、`SendToolCallRequestCommand`、`SendToolCallResultCommand`、`DeliverMessageCommand`，避免后续调用点参数继续膨胀。

### ⚠️ 运行面渠道推送实现状态（按渠道）

`MessageChannelDal::deliver_message` 负责把一条消息分发到用户所有已启用的渠道。各渠道实际 HTTP 推送的实现进度分化如下：

| 渠道 | ChannelType | HTTP 实际推送 | 说明 |
|------|-------------|---------------|------|
| 飞书 | Lark | ✅ 已实现 | 通过飞书机器人 Webhook 发送卡片消息 |
| 微信 | Wechat | 🟡 未实测（代码骨架已在） | 企业微信 Webhook 骨架 |
| Slack | Slack | 🟡 未实测（代码骨架已在） | Slack Webhook 骨架 |
| 邮件 | Email | 🟡 未实现（或骨架） | SMTP / API 邮件通道待定 |
| **通用 Webhook** | **Webhook** | ❌ **当前未实现**（显式返回 `unsupported_operation` 错误） | `src/service/dao/webhook/` 的 `deliver_message` 返回错误；DAL 捕获为 `failed += 1`，写入 `ChannelDeliveryDetail.error`，不影响其他渠道 / SSE |
| A2A 协议回调 | A2aCallback | ✅ 已实现 | A2A 事件回调推送 |

> **影响与后续工作**：用户在管理面创建 `通用 Webhook` 渠道后，消费端 deliver_message 不会真正发起 HTTP 请求，而是返回失败。通用 Webhook 的实现优先级待定（先完成飞书等高频场景）。集成测试 `test_webhook_channel_delivers_message_to_mock_server` 已对「unsupported → failed 聚合」做了明确断言，实现后把断言从 `failed=1` 切到 `success=1` + mock 收包校验即可。

### 📊 消息投递 + 推送集成测试矩阵（2026-08-05 新增）

文件：`tests/integration/message_delivery_test.rs`，共 7 个测试，全部在 CI 默认模式下可运行（无需真实 LLM / 真实 Embedding）。

| # | 测试函数 | 覆盖场景 | 断言 |
|---|----------|----------|------|
| 1 | `test_send_message_persists_record` | 已有：Agent→Agent 消息 HTTP handler 入库 + 列表查询 | 写入 → 查询的闭环 + message_id 匹配 |
| 2 | `test_sse_endpoint_returns_event_stream` | 已有：SSE 订阅端点 200 OK | 连接成功（不读 body，避免阻塞） |
| 3 | `test_send_message_to_user_via_tool_persists_and_listable` | ✨ 新增：Agent→User 消息定向投递 + 角色校验 + 双向列表 + 工具注册检查 | `from_role=Agent(1)`、`to_role=User(0)`；`to_id=user` 与 `from_id=agent` 列表都能命中；`send_message` neural 工具已在 tool registry 中 |
| 4 | `test_sse_push_delivers_message_payload_to_subscriber` | ✨ 新增：端到端 SSE 推送内容（含 payload 结构） | 后台 spawn SSE subscriber → deliver_message → event JSON 中 `message_id` + `content` 正确 |
| 5 | `test_webhook_channel_delivers_message_to_mock_server` | ✨ 新增：Webhook 渠道投递 → 失败聚合不抛错（对应未实现 unsupported_operation 场景） | `total=1`、`failed=1`、`details.error` 含 `unsupported_operation`；**deliver_message 仍返回 Ok**；mock 服务器未收到虚假请求 |
| 6 | `test_deliver_message_no_channels_and_no_sse_still_returns_ok` | ✨ 新增：零渠道 + 零 SSE 订阅边界 | `total/success/failed/sse_delivered` 全 0；`deliver_message` 返回 Ok |
| 7 | `test_webhook_channel_invalid_url_reports_failed_without_panicking` | ✨ 新增：渠道配置 URL 不可达时，整体返回 Ok，错误入 details | 不可达 URL → 不向上抛错 → failed 聚合正确（unsupported 短路即 OK，真实实现后仍会通过 reqwest 错误路径） |

### Handler 管理面范围

当前管理面已暴露以下受保护路由：

```http
POST   /api/v1/finance/message-channels
GET    /api/v1/finance/message-channels
GET    /api/v1/finance/message-channels/{id}
PUT    /api/v1/finance/message-channels/{id}
DELETE /api/v1/finance/message-channels/{id}
PUT    /api/v1/finance/message-channels/{id}/status
POST   /api/v1/finance/message-channels/{id}/test
```

Handler 继续保持“一个用户 action 一个文件”的组织方式：`create/list/get/update/delete/status/test` 分别对应独立 handler 文件。Handler 只做 DTO 解析、`RequestContext` 补全、归属校验、Domain 编排和脱敏 Response DTO 组装，不直接调用 DAL/DAO，也不抽象通用 CRUD Handler。

状态更新统一走 `/status`，请求体携带目标 `ChannelStatus`，不拆启用/禁用等多条路由。简单状态流转规则内聚在 `MessageChannel::transition_status`：`Active` 与 `Disabled` 可互相切换；`Deleted` 为删除 action 的结果，不允许通过状态更新接口产生。测试连接走 `/test`，Handler 通过 Finance Domain 的 `test_message_channel` 入口调用，不越层访问 DAL。

响应 DTO 使用脱敏策略：查询与详情不返回 `access_token`、`secret`、`config_json` 内的 secret/password/token 明文，只返回 `has_access_token`、`has_secret`、`has_config_secret` 布尔值表达存在性。

---

## 📌 设计目标

消息渠道系统用于记录用户绑定的外部消息推送渠道，支持多渠道消息分发：

1. **多渠道支持**：支持飞书、微信、Slack、邮件等多种推送渠道
2. **用户绑定**：每个用户可以绑定多个渠道
3. **灵活配置**：每个渠道可以独立配置 webhook URL、token 等参数
4. **状态管理**：支持启用/禁用渠道，记录最后推送时间和错误信息
5. **分层架构**：严格遵循 DAO → DAL → Domain 分层设计

---

---

## 🏗️ 数据库设计

### 1. message_channels 表结构

```sql
CREATE TABLE IF NOT EXISTS message_channels (
    id TEXT PRIMARY KEY,
    org_id TEXT NOT NULL,
    user_id TEXT NOT NULL,                    -- 绑定的用户 ID
    channel_type TEXT NOT NULL,               -- 渠道类型：lark / wechat / slack / email / webhook
    channel_name TEXT NOT NULL,               -- 渠道名称（用户自定义）
    webhook_url TEXT,                         -- Webhook URL（飞书、Slack 等）
    access_token TEXT,                        -- 访问 Token（需要鉴权的渠道）
    secret TEXT,                               -- 签名密钥（可选）
    config_json TEXT,                          -- 扩展配置，JSON 格式
    is_enabled INTEGER NOT NULL DEFAULT 1,    -- 是否启用：0=禁用，1=启用
    last_pushed_at INTEGER,                    -- 最后成功推送时间
    last_error TEXT,                           -- 最后一次错误信息
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    created_by TEXT NOT NULL,
    modified_by TEXT NOT NULL,
    
    UNIQUE(user_id, channel_type)              -- 每个用户每种渠道只能绑定一个
);

-- 索引
CREATE INDEX idx_message_channels_org_id ON message_channels(org_id);
CREATE INDEX idx_message_channels_user_id ON message_channels(user_id);
CREATE INDEX idx_message_channels_channel_type ON message_channels(channel_type);
CREATE INDEX idx_message_channels_is_enabled ON message_channels(is_enabled);
```

### 2. 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `id` | TEXT | ✅ | 渠道 ID（UUID v7） |
| `org_id` | TEXT | ✅ | 组织 ID，多租户隔离 |
| `user_id` | TEXT | ✅ | 绑定的用户 ID |
| `agent_id` | TEXT | ❌ | 关联的 Agent ID（NULL 表示用户全局默认渠道） |
| `channel_type` | TEXT | ✅ | 渠道类型枚举值 |
| `channel_name` | TEXT | ✅ | 用户自定义的渠道名称 |
| `webhook_url` | TEXT | ❌ | Webhook 地址（飞书、Slack 等） |
| `access_token` | TEXT | ❌ | 访问 Token（需要鉴权的渠道） |
| `secret` | TEXT | ❌ | 签名密钥（可选） |
| `config_json` | TEXT | ❌ | 扩展配置，JSON 格式（对应 `ChannelConfig` 结构体） |
| `is_enabled` | INTEGER | ✅ | 是否启用：0=禁用，1=启用 |
| `last_pushed_at` | INTEGER | ❌ | 最后成功推送的时间戳 |
| `last_error` | TEXT | ❌ | 最后一次推送的错误信息 |

---

## ⚠️ 设计调整说明（已讨论确认）

### 关于唯一约束
- **不做数据库层面的唯一约束**：允许用户绑定多个同类型渠道
- 支持按 Agent 区分：同一个用户的不同 Agent 可以绑定不同渠道
- **业务层面去重**：通过「先读后写」避免完全重复的配置（webhook_url 完全相同）

### 关于敏感信息
- **初期明文存储**：加密密钥的存储也需要设计，后续统一处理加密层

### 关于推送失败处理
- **失败不重试**：只记录 `last_error`，不做自动重试
- **后续扩展**：需要时再新建 `message_push_logs` 表做详细记录和重试

### 关于消息格式
- **messages 表只存基础数据**：各渠道在自己的 DAL 层做格式转换（Markdown → 飞书卡片 等）

### 关于限流
- **不在此模块实现**：限流和监控作为独立通用模块实现，所有需要限流的地方统一使用

---

## 📐 枚举定义

### ChannelType 枚举

```rust
// common/src/enums/message_channel.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT", rename_all = "snake_case")]
pub enum ChannelType {
    Lark,       // 飞书
    Wechat,     // 微信
    Slack,      // Slack
    Email,      // 邮件
    Webhook,    // 通用 Webhook
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChannelType::Lark => "lark",
            ChannelType::Wechat => "wechat",
            ChannelType::Slack => "slack",
            ChannelType::Email => "email",
            ChannelType::Webhook => "webhook",
        }
    }
}
```

---

## 📦 ChannelConfig 结构体（config_json 对应）

```rust
// src/models/message_channel.rs
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChannelConfig {
    // 飞书配置
    pub lark_app_id: Option<String>,
    pub lark_app_secret: Option<String>,
    pub lark_encrypt_key: Option<String>,
    pub lark_verification_token: Option<String>,
    
    // 微信配置
    pub wechat_app_id: Option<String>,
    pub wechat_app_secret: Option<String>,
    pub wechat_open_id: Option<String>,
    
    // 邮件配置
    pub email_smtp_host: Option<String>,
    pub email_smtp_port: Option<u16>,
    pub email_username: Option<String>,
    pub email_password: Option<String>,
    pub email_from_address: Option<String>,
    pub email_to_address: Option<String>,
    
    // Slack 配置
    pub slack_bot_token: Option<String>,
    pub slack_channel_id: Option<String>,
    
    // 通用 Webhook 配置
    pub webhook_method: Option<String>,      // GET / POST / PUT
    pub webhook_headers: Option<HashMap<String, String>>,
    pub webhook_body_template: Option<String>, // 消息体模板
    
    // 其他扩展字段
    pub extra: Option<HashMap<String, String>>,
}
```

---

## 🧩 分层设计

### 1. DAO 层（数据访问）

```rust
// src/dao/message_channel/mod.rs
#[async_trait]
pub trait MessageChannelDao {
    // 创建渠道
    async fn create(&self, ctx: RequestContext, channel: &MessageChannelPo) -> Result<MessageChannelPo, AppError>;
    
    // 更新渠道
    async fn update(&self, ctx: RequestContext, channel: &MessageChannelPo) -> Result<MessageChannelPo, AppError>;
    
    // 删除渠道
    async fn delete(&self, ctx: RequestContext, channel_id: &str) -> Result<(), AppError>;
    
    // 根据 ID 查询
    async fn get_by_id(&self, ctx: RequestContext, channel_id: &str) -> Result<Option<MessageChannelPo>, AppError>;
    
    // 查询用户的所有渠道
    async fn list_by_user(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<MessageChannelPo>, AppError>;
    
    // 查询用户指定类型的渠道
    async fn get_by_user_and_type(&self, ctx: RequestContext, user_id: &str, channel_type: ChannelType) -> Result<Option<MessageChannelPo>, AppError>;
    
    // 更新最后推送时间
    async fn update_last_pushed(&self, ctx: RequestContext, channel_id: &str, pushed_at: i64) -> Result<(), AppError>;
    
    // 记录推送错误
    async fn update_last_error(&self, ctx: RequestContext, channel_id: &str, error: &str) -> Result<(), AppError>;
    
    // 启用/禁用渠道
    async fn set_enabled(&self, ctx: RequestContext, channel_id: &str, is_enabled: bool) -> Result<(), AppError>;
}
```

### 2. DAL 层（业务组合）

```rust
// src/dal/message_channel.rs
#[async_trait]
pub trait MessageChannelDal {
    // 绑定渠道
    async fn bind_channel(&self, ctx: RequestContext, user_id: &str, channel_type: ChannelType, config: ChannelConfig) -> Result<MessageChannelPo, AppError>;
    
    // 解绑渠道
    async fn unbind_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<(), AppError>;
    
    // 获取用户可用的渠道（已启用）
    async fn get_enabled_channels(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<MessageChannelPo>, AppError>;
    
    // 获取用户指定类型的渠道（仅已启用）
    async fn get_enabled_channel(&self, ctx: RequestContext, user_id: &str, channel_type: ChannelType) -> Result<Option<MessageChannelPo>, AppError>;
    
    // 记录推送成功
    async fn mark_push_success(&self, ctx: RequestContext, channel_id: &str) -> Result<(), AppError>;
    
    // 记录推送失败
    async fn mark_push_failed(&self, ctx: RequestContext, channel_id: &str, error: &str) -> Result<(), AppError>;
}
```

### 3. Domain 层（核心业务）

```rust
// src/domain/message_channel/mod.rs
pub struct MessageChannelDomain {
    dal: Arc<dyn MessageChannelDal>,
}

impl MessageChannelDomain {
    // 绑定渠道（验证配置有效性）
    pub async fn bind_channel(&self, ctx: RequestContext, channel: BindChannelRequest) -> Result<MessageChannelDto, AppError>;
    
    // 解绑渠道
    pub async fn unbind_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<(), AppError>;
    
    // 列出用户所有渠道
    pub async fn list_user_channels(&self, ctx: RequestContext, user_id: &str) -> Result<Vec<MessageChannelDto>, AppError>;
    
    // 启用/禁用渠道
    pub async fn toggle_channel(&self, ctx: RequestContext, channel_id: &str, enabled: bool) -> Result<(), AppError>;
}
```

---

## 🚀 消息分发集成

### 在 Message Domain 中集成

```rust
// src/domain/message/process_message.rs
impl MessageProcessDomain {
    async fn handle_user_message(&self, ctx: RequestContext, msg: &MessagePo) -> Result<()> {
        // 1. 基础投递：更新状态
        self.message_dal
            .update_status(ctx.clone(), &msg.id, MessageStatus::Completed)
            .await?;
        
        // 2. 获取用户所有已启用的渠道
        let channels = self.message_channel_dal
            .get_enabled_channels(ctx.clone(), msg.to_id())
            .await?;
        
        // 3. 并发推送到所有渠道（不阻塞主流程，失败不影响）
        for channel in channels {
            let self_clone = self.clone();
            let msg_clone = msg.clone();
            let ctx_clone = ctx.clone();
            
            tokio::spawn(async move {
                let result = self_clone
                    .push_to_channel(&ctx_clone, &msg_clone, &channel)
                    .await;
                
                if let Err(e) = result {
                    tracing::error!("failed to push to channel {}: {}", channel.id, e);
                }
            });
        }
        
        Ok(())
    }
    
    // 推送到具体渠道
    async fn push_to_channel(&self, ctx: RequestContext, msg: &MessagePo, channel: &MessageChannelPo) -> Result<(), AppError> {
        match channel.channel_type {
            ChannelType::Lark => {
                self.lark_dal.push_message(ctx, channel, msg).await
            }
            ChannelType::Wechat => {
                self.wechat_dal.push_message(ctx, channel, msg).await
            }
            // ... 其他渠道
            _ => Ok(()),
        }
    }
}
```

---

## 📋 实现任务清单

- [ ] 创建 `common/src/enums/message_channel.rs` 枚举
- [ ] 创建 `src/models/message_channel.rs` PO 结构体
- [ ] 创建数据库迁移脚本
- [ ] 实现 `MessageChannelDao` SQLite 实现
- [ ] 实现 `MessageChannelDal`
- [ ] 实现 `MessageChannelDomain`
- [ ] 编写单元测试
- [ ] 在 Message Domain 中集成渠道推送

---

## 💡 设计思考

### 为什么用独立表而不是用户表扩展字段？

1. **扩展性**：新增渠道类型不需要修改表结构
2. **多对一**：一个用户可以绑定多个同类型渠道（如多个飞书群）
3. **状态独立**：每个渠道的启用状态、推送记录独立管理
4. **配置灵活**：不同渠道有不同的配置字段，用 `config_json` 扩展

### 为什么推送使用 tokio::spawn 异步执行？

1. **不阻塞主流程**：消息状态更新后立即返回，推送是附加功能
2. **失败隔离**：某个渠道推送失败不影响其他渠道和主流程
3. **性能优化**：多个渠道可以并发推送，提高响应速度
4. **重试机制**：后续可以独立实现推送重试逻辑，不影响消息处理

---

## 🏗️ 最终消息分发架构设计（2026-05-08 确认）

### 核心设计原则（经过 5 轮讨论最终确认）

| 原则 | 说明 |
|------|------|
| **无 trait，纯 match** | 不使用任何 trait 约束，最简单直接 |
| **DAL 统一整合** | 渠道配置管理 + 消息分发统一在 `MessageChannelDal` |
| **严格分层封装** | 所有 DAO 都是 DAL 私有字段，Domain 层完全看不到 DAO |
| **无循环依赖** | DAL 依赖 DAO，DAO 不依赖 DAL，单向依赖 |
| **错误统一** | 不创建独立错误类型，统一到 `AppError` |

---

### 最终架构图

```
Domain 层
    │ 只能调用 DAL 暴露的 8 个公共方法
    ▼
MessageChannelDal (统一整合)
    ├── 渠道配置管理
    │   ├── create_channel()
    │   ├── update_channel()
    │   ├── delete_channel()
    │   ├── get_channel()
    │   ├── list_user_channels()
    │   └── test_channel()  ✅ 测试渠道连接
    │
    └── 消息分发
        └── deliver_message()  ✅ 分发消息到所有渠道
        └── 内部私有方法（不对外暴露）
            ├── push_to_channel()  纯 match 分发
            └── update_channel_push_status()

DAO 层（7 个完全独立的 DAO）
    ├── MessageChannelDao  ✅ 渠道配置 CRUD
    ├── LarkDao           ✅ 飞书推送
    ├── WechatDao         ✅ 微信推送
    ├── SlackDao          ✅ Slack 推送
    ├── EmailDao          ✅ 邮件推送
    ├── WebhookDao        ✅ Webhook 推送
    └── A2aCallbackDao    ✅ A2A Callback 推送
```

---

### 目录结构（最简）

```
src/service/
├── dao/
│   ├── mod.rs
│   ├── message_channel.rs      # 渠道配置 CRUD
│   ├── lark/                   # 飞书 DAO（mod.rs + http.rs）
│   ├── wechat/                 # 微信 DAO（mod.rs + http.rs）
│   ├── slack/                  # Slack DAO（mod.rs + http.rs）
│   ├── email/                  # 邮件 DAO（mod.rs + http.rs）
│   ├── webhook/                # Webhook DAO（mod.rs + http.rs）
│   └── a2a_callback/           # A2A Callback DAO（mod.rs + http.rs）
│
└── dal/
    ├── mod.rs
    └── message_channel_dal.rs   # 统一整合：配置管理 + 消息分发
```

---

### 各渠道 DAO 设计（完全独立，无 trait）

以 `lark_dao.rs` 为例：

```rust
#[derive(Clone, Default)]
pub struct LarkDao;

impl LarkDao {
    /// 推送消息（约定方法名）
    pub async fn push(
        &self,
        _ctx: RequestContext,
        _message: &Message,
        _channel: &MessageChannel,
    ) -> Result<(), String> {
        // TODO: 实现飞书推送逻辑
        Err("飞书推送未实现".to_string())
    }
    
    /// 测试连接（约定方法名）
    pub async fn test_connection(
        &self,
        _ctx: RequestContext,
        _channel: &MessageChannel,
    ) -> Result<(), String> {
        // TODO: 实现飞书连接测试逻辑
        Err("飞书测试未实现".to_string())
    }
}
```

✅ 关键点：
- 完全独立，不实现任何 trait
- `push()` 和 `test_connection()` 只是约定的方法名
- 可以自由添加其他渠道特有方法

---

### MessageChannelDal 核心分发逻辑

```rust
pub struct MessageChannelDal {
    // ✅ 所有 DAO 都是私有，不对外暴露！
    message_channel_dao: Arc<dyn MessageChannelDao>,
    lark_dao: Arc<LarkDao>,
    wechat_dao: Arc<WechatDao>,
    slack_dao: Arc<SlackDao>,
    email_dao: Arc<EmailDao>,
    webhook_dao: Arc<WebhookDao>,
    a2a_callback_dao: Arc<A2aCallbackDao>,
}

impl MessageChannelDal {
    // ... 配置管理的公共方法 ...
    
    /// ✅ 测试渠道连接（公共方法）
    pub async fn test_channel(&self, ctx: RequestContext, channel_id: &str) -> Result<()> {
        let channel = self.get_channel(ctx.clone(), channel_id).await?;
        
        // 🎯 核心：纯 match 分发！无 trait！
        match channel.channel_type() {
            ChannelType::Lark => self.lark_dao.test_connection(ctx, &channel).await,
            ChannelType::Wechat => self.wechat_dao.test_connection(ctx, &channel).await,
            ChannelType::Slack => self.slack_dao.test_connection(ctx, &channel).await,
            ChannelType::Email => self.email_dao.test_connection(ctx, &channel).await,
            ChannelType::Webhook => self.webhook_dao.test_connection(ctx, &channel).await,
            ChannelType::A2aCallback => self.a2a_callback_dao.test_connection(ctx, &channel).await,
        }.map_err(|e| AppError::ChannelPushError(e))
    }
    
    /// ✅ 分发消息到用户所有可用渠道（公共方法）
    pub async fn deliver_message(
        &self,
        ctx: RequestContext,
        message: &Message,
        user_id: &str,
    ) -> Result<DeliveryResult> {
        // 1. 查询用户可用渠道
        let channels = self.message_channel_dao
            .find_active_by_user(&ctx, user_id)
            .await?;
        
        if channels.is_empty() {
            return Ok(DeliveryResult::empty());
        }
        
        // 2. 逐个渠道推送
        let mut details = Vec::with_capacity(channels.len());
        
        for channel in channels {
            let result = self.push_to_channel(ctx.clone(), message, &channel).await;
            
            // 3. 更新渠道状态
            let _ = self.update_channel_push_status(&ctx, &channel, &result).await;
            
            details.push(ChannelDeliveryDetail {
                channel_id: channel.id().to_string(),
                channel_type: channel.channel_type(),
                channel_name: channel.po.channel_name.clone(),
                success: result.is_ok(),
                error: result.err(),
            });
        }
        
        Ok(DeliveryResult::from_details(details))
    }
    
    /// 🎯 核心分发逻辑（内部私有，不对外暴露！
    async fn push_to_channel(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
    ) -> Result<(), String> {
        match channel.channel_type() {
            ChannelType::Lark => 
                self.lark_dao.push(ctx, message, channel).await,
            
            ChannelType::Wechat => 
                self.wechat_dao.push(ctx, message, channel).await,
            
            ChannelType::Slack => 
                self.slack_dao.push(ctx, message, channel).await,
            
            ChannelType::Email => 
                self.email_dao.push(ctx, message, channel).await,
            
            ChannelType::Webhook => 
                self.webhook_dao.push(ctx, message, channel).await,
            
            ChannelType::A2aCallback => 
                self.a2a_callback_dao.push(ctx, message, channel).await,
        }
    }
}
```

---

### 设计优势总结

| 优势 | 说明 |
|------|------|
| ✅ **无循环依赖** | DAL 依赖 DAO，DAO 不依赖 DAL，单向依赖 |
| ✅ **0 层抽象** | 没有 trait，没有工厂，没有注册表，纯 match |
| ✅ **严格分层** | DAO 完全私有，Domain 层只能看到 DAL 公共方法 |
| ✅ **渠道自由扩展** | 各渠道 DAO 可以自由添加特有方法 |
| ✅ **编译安全** | 新增渠道漏加 match arm，编译直接报错 |

---

### 反模式提醒（绝对不要做）

❌ **不要**：创建 PushChannel trait
❌ **不要**：创建工厂模式动态创建渠道实例
❌ **不要**：创建注册表模式
❌ **不要**：在 DAL 暴露 DAO getter 方法
❌ **不要**：创建独立的错误类型
❌ **不要**：拆分 MessageChannelDal + MessageDeliveryDal（合并才是正确的）

✅ **要**：纯 `match` 分发！
✅ **要**：DAL 统一整合！
✅ **要**：严格分层封装！

---

*此设计方案经过 5 轮讨论迭代，于 2026-05-08 最终确认，符合严格分层架构理念，无过度设计。*

---

> ⚠️ **快照过期说明（2026-08-13）**：本文「ChannelConfig 结构体」一节中的 `lark_app_id` / `lark_app_secret` 内联凭证字段已于飞书集成二期删除——应用凭证改存 users 表 `identity_credentials` JSON 列（finance domain 凭证中枢），渠道仅存 `lark_credential_id` 引用 + `lark_identity_mode`；出站凭证解析由 `MessageChannelDal` 完成（DAO 只接收已解析凭证）。现状以 wiki「财务领域/飞书集成系统」卡片为准，决策脉络见 [lark_cli_integration.md](./lark_cli_integration.md)。
