# 消息渠道设计文档

## 📌 设计目标

消息渠道系统用于记录用户绑定的外部消息推送渠道，支持多渠道消息分发：

1. **多渠道支持**：支持飞书、微信、Slack、邮件等多种推送渠道
2. **用户绑定**：每个用户可以绑定多个渠道
3. **灵活配置**：每个渠道可以独立配置 webhook URL、token 等参数
4. **状态管理**：支持启用/禁用渠道，记录最后推送时间和错误信息
5. **分层架构**：严格遵循 DAO → DAL → Domain 分层设计

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

DAO 层（6 个完全独立的 DAO）
    ├── MessageChannelDao  ✅ 渠道配置 CRUD
    ├── LarkDao           ✅ 飞书推送
    ├── WechatDao         ✅ 微信推送
    ├── SlackDao          ✅ Slack 推送
    ├── EmailDao          ✅ 邮件推送
    └── WebhookDao        ✅ Webhook 推送
```

---

### 目录结构（最简）

```
src/service/
├── dao/
│   ├── mod.rs
│   ├── message_channel.rs      # 渠道配置 CRUD
│   ├── lark_dao.rs             # 飞书 DAO
│   ├── wechat_dao.rs           # 微信 DAO
│   ├── slack_dao.rs            # Slack DAO
│   ├── email_dao.rs            # 邮件 DAO
│   └── webhook_dao.rs          # Webhook DAO
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
