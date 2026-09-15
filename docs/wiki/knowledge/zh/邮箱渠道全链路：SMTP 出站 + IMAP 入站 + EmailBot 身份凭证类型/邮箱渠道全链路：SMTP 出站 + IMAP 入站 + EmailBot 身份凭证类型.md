---
kind: rag_card
name: 邮箱渠道全链路：SMTP 出站 + IMAP 入站 + EmailBot 身份凭证类型
category: 业务模块 / 消息渠道
scope:
- src/service/dao/email/**
- src/service/dal/email/**
- src/consumer/email_inbound.rs
- src/models/events/email.rs
- common/src/api/email_integration.rs
- common/src/models/identity_credentials.rs
- common/src/enums/credential_kind.rs
- common/src/enums/message_channel.rs
- common/src/models/message_channel.rs
- src/handlers/finance/email_integration/*.rs
- src/service/domain/finance/message_channel.rs
- src/service/domain/message/inbound.rs
- src/service/domain/message/mod.rs
- src/pkg/credential/mod.rs
source_files:
- src/service/dao/email/smtp.rs (EmailDaoImpl：SMTP 出站 push + EmailSmtpCredentials 运行凭证 + resolve_email_credentials 解析 + compose_subject 主题派生 + build_transport 隐式TLS/STARTTLS)
- src/service/dao/email/imap.rs (IMAP 入站轮询：ImapPollRegistry 凭证指纹幂等启停 + ImapSession SASL 登录 + UID SEARCH FETCH 消息拉取 + IN 列表 400 分块防 999 溢出 + 去重 seen_ids)
- src/service/dao/email/mod.rs (EmailDao trait：push / test_connection / start_polling / stop_polling / stop_all_polling / is_polling 六方法；DAO 单例 OnceLock + init/new 工厂)
- src/service/dal/email/mod.rs + impl.rs (EmailDal trait + impl：create_credential/update_credential/delete_credential/get_status/set_default_credential + EmailImapCredentials → EmailBot detail 解密 + 启停 IMAP 轮询)
- common/src/api/email_integration.rs (EmailIntegration Handler DTO：CreateCredentialRequest / UpdateCredentialRequest / CredentialStatusResponse / SetDefaultRequest + 路由常量 /email-integration/credentials/*)
- common/src/models/identity_credentials.rs#L1400-L1611 (CredentialDetail::EmailBot 变体：email_address/smtp_host/smtp_port/imap_host/imap_port/username/password + normalize/validate/encrypt/primary_id/primary_secret)
- src/models/events/email.rs (EmailInboundEvent + EmailPushEvent DomainEvent 枚举：FromEmail / EmailContent / EmailCredentials)
- src/consumer/email_inbound.rs (EmailInboundConsumer：订阅 EmailInboundEvent → DAL/Email 启停 IMAP 轮询 → 消息 DAL 持久化 → DomainEvent 投递触发 Agent 唤醒)
- src/service/domain/finance/message_channel.rs#L14-L30 (MessageChannelDomain：create 时 email_credential_id 关联 EmailBot 凭证 + 自动启停 IMAP 入站轮询)
- src/pkg/credential/mod.rs (credential 统一加密解密入口：encrypt_channel_secret / decrypt_channel_secret，DAO 层调用)
- docs/wiki/zh/content/功能模块/消息系统/消息渠道管理.md (Wiki 长文：消息渠道 CRUD + EmailBot 渠道配置)
- 【关联总卡】docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）/身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail 类型无关下沉）.md
- 【平行卡】docs/wiki/knowledge/zh/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程/微信 iLink 专属渠道闭环：wechat_dal + ilink_dao + inbound_state + 授权流程.md（同属消息渠道体系，结构可参考，微信入站按 channel 键控 vs 邮箱按凭证键控的差异）
---

## §1 概述

**本卡角色**：邮箱渠道（EmailBot）全链路知识卡——SMTP 出站推送 Agent 生成消息到指定邮箱 + IMAP 入站轮询接收外部邮件触发 Agent 处理 + EmailBot 身份凭证类型完整定义。是「身份凭证统一链路（总卡）」下的渠道细卡（Level 4 总分结构），与微信 iLink 卡平行覆盖消息渠道体系。

**定位**：新增邮箱渠道支持时读；排查 SMTP/IMAP 连接问题、理解 EmailBot 凭证字段、调试 IMAP 轮询启停、排查入站消息没到 Agent 时读。

**架构要点**：
- **出站链路**：Domain 调用 MessageChannelDomain.push 或 EmailDal.push → DAO 层 resolve_email_credentials 解析 EmailBot detail + 解密授权码 → build_transport（465 隐式 TLS relay / 其余 STARTTLS starttls_relay）→ lettre::AsyncTransport 发送；主题从正文首个非空行派生（MessagePo 无 title 字段，对齐飞书/微信全文推送语义）
- **入站链路**：消息渠道创建时自动启动 IMAP 轮询（按 email_credential_id 引用的凭证键控，一个 EmailBot 可被 N 个渠道共用）→ ImapPollRegistry 凭证指纹幂等启停（指纹变停旧建同）→ SASL PLAIN 登录 UID SEARCH UNSEEN → FETCH 拉取消息 → IN 列表 400 分块防 999 溢出 → seen_ids 去重 → EmailInboundEvent DomainEvent → EmailInboundConsumer 消费 → 消息持久化 → Agent 唤醒
- **凭证关联**：MessageChannel.config.email_credential_id → UserCredentialPo(kind=EmailBot) → CredentialDetail::EmailBot{email_address, smtp_host, smtp_port, imap_host, imap_port, username, password(加密)}；主凭证 id=邮箱地址（同 Lark app_id 地位），主 secret=密码/授权码

---

## §2 关键文件表

| 文件 | 角色 | 核心契约 / 红线 |
|------|------|-----------------|
| [dao/email/smtp.rs](src/service/dao/email/smtp.rs) | SMTP 出站 DAO | EmailDaoImpl：push(ctx, message, channel, credentials) → resolve_email_credentials 解析 + 主题派生 + build_transport → lettre 发送；test_connection 做参数完整性校验（阶段一无廉价探针）；Debug mask 密码为 *** |
| [dao/email/imap.rs](src/service/dao/email/imap.rs) | IMAP 入站轮询 DAO | ImapPollRegistry 凭证指纹 HashMap 键控 + async_trait start_polling/stop_polling；IMAP UID SEARCH UNSEEN + FETCH；IN 列表 400 分块防 999 溢出；seen_ids HashSet 去重 |
| [dao/email/mod.rs](src/service/dao/email/mod.rs) | EmailDao trait + 单例 | 六方法：push/test_connection/start_polling/stop_polling/stop_all_polling/is_polling；OnceLock EMAIL_DAO 全局单例；init() 在 service::init_all 内注入 |
| [dal/email/mod.rs + impl.rs](src/service/dal/email/mod.rs) | Email DAL 层 | trait + impl：create/update/delete/get_status/set_default_credential + to_email_imap_credentials 解密 EmailBot detail → EmailImapCredentials；启停 IMAP 轮询（channel create 触发 start） |
| [api/email_integration.rs](common/src/api/email_integration.rs) | Handler DTO | CreateCredentialRequest{email_address, smtp_host, smtp_port, imap_host, imap_port, username, password} / CredentialStatusResponse / SetDefaultRequest；路由 /finance/email-integration/credentials/* |
| [identity_credentials.rs](common/src/models/identity_credentials.rs#L1400-L1611) | CredentialDetail::EmailBot | 7 字段 + normalize trim + validate 邮箱@ + port≠0 + encrypt_sensitive 仅加密 password + primary_id 返回邮箱地址 + primary_secret 返回密码 |
| [models/events/email.rs](src/models/events/email.rs) | DomainEvent 枚举 | EmailInboundEvent{FromEmail, EmailContent, EmailCredentials} + EmailPushEvent |
| [consumer/email_inbound.rs](src/consumer/email_inbound.rs) | 入站事件消费者 | 订阅 EmailInboundEvent → 消息 DAL 持久化（MessagePo）→ publish message.received 触发 Agent 唤醒 |
| [domain/finance/message_channel.rs](src/service/domain/finance/message_channel.rs#L14-L30) | 渠道 Domain | create_message_channel 时 email_credential_id 绑定 → 调 EmailDal.start_polling 自动启动 IMAP 入站；update/delete 时同步启停 |

## §3 架构约定

### 出站流程

```
Agent 生成消息 → 消息 Domain / Handler 触发
  → MessageChannelDomain.push(ctx, message, channel)
    → 按 channel.email_credential_id 查 UserCredentialPo
    → EmailDal.resolve_credentials → CredentialDetail::EmailBot 解密
    → DAO push(ctx, message, channel, EmailSmtpCredentials)
      → require_to_address(channel.config.email_to_address)
      → compose_subject(message.content) // 正文首非空行派生
      → build_transport: 465→relay TLS / else→starttls_relay
      → lettre::AsyncTransport.send(email)
```

### 入站流程

```
系统启动 / 渠道创建 → EmailDal.start_polling(EmailImapCredentials)
  → ImapPollRegistry.ensure(credentials)
    → 计算 credential.fingerprint = hash(email_address+smtp_host+smtp_port+imap_host+imap_port+username)
    → 指纹变：停旧重建（stop_polling → start_polling_with_fresh_session）
  → ImapSession：SASL PLAIN 登录 UID SEARCH UNSEEN
    → FETCH (BODY.PEEK[])
    → IN 列表 400 分块防 999 溢出
    → seen_ids HashSet 去重
    → 每条 EmailInboundEvent
      → EmailInboundConsumer
        → MessageDomain 持久化 MessagePo（channel_type=email, inbound_source=email_bot）
        → AOP publish message.received（Agent 唤醒入口）
```

### 凭证指纹幂等机制

- key: credential_id（一个 EmailBot 可被 N 个渠道共用，按凭证而非按渠道键控轮询单元）
- fingerprint: `hash(email_address + smtp_host + smtp_port + imap_host + imap_port + username)` — 凭证内容没变就不重建轮询
- 好处：多渠道绑定同一 EmailBot 只开一个 IMAP 连接；凭证轮换自动停旧建新

---

## §4 硬约束

1. **EmailBot 凭证必须 kind=EmailBot 才能被邮件渠道引用**：DAO 层 resolve_email_credentials 做 kind 匹配校验 + 双重 match 解构 CredentialDetail::EmailBot，类型不匹配直接返回 InvalidRequest
2. **SMTP 出站必须有对端收件地址 email_to_address**：DAO push 和 test_connection 都调 require_to_address 做非空校验，缺少时返回引导性错误 message "邮件渠道缺少对端收件地址，请到渠道配置中填写"
3. **EmailSmtpCredentials Debug 必须 mask password 为 `***`**：禁止授权码明文出现在日志 / Debug 输出中；DAO transport.build 失败消息也不得打 credentials.password
4. **IMAP 入站按凭证键控不按渠道键控**：一个 EmailBot 凭证可能被 N 个 MessageChannel 共用，轮询单元粒度是 credential_id（而非 channel_id）；路由匹配由消费侧 DAL 完成（按 from_email / credential_id 路由到对应 Agent）
5. **凭证指纹变必须停旧重建**：EmailBot detail 字段更新 → fingerprint 变化 → ImapPollRegistry 必须停止旧 session 并建立新 session；禁止"热更新凭证"导致旧 session 持过期授权码
6. **IN 列表 SQL 必须 400 分块防 999 溢出**：imap.rs 中 `seen_ids` 转 IN 查询时必须按 400 分块批量查询，禁止直接构造 IN(1,2,...,999,1000) 列表
7. **IMAP 轮询失败只打 log_warn 不 panic**：网络抖动 / IMAP 服务器重启时 poll_loop 必须 catch + backoff 重试；单次 FETCH 失败 → 跳过当前 UID 继续下个；禁止整个轮询进程 panic 退出
8. **DAO 层 credential 解析不做 DB 查询**：resolve_email_credentials 接收已查好的 UserCredentialPo，DAO 层不调用任何 DAL/DAO 方法查凭证（保持 DAO 纯出站边界）
9. **test_connection 阶段一无廉价探针**：SMTP 没有 connect/auth/quit 三阶段廉价探针（lettre 不单独暴露），阶段一仅做凭证参数完整性校验；真实连通性由首次 push 运行态观察；禁止为探活真发信污染对端收件箱
10. **凭证创建/更新/删除必须自动启停 IMAP 轮询**：EmailDal.create_credential/delete_credential 必须调用 start_polling/stop_polling；忘记启停 → 入站消息丢失或轮询泄漏（IMAP 长连接资源消耗）
