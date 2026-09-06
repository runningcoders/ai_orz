---
kind: wiki_knowledge_card
name: IdentityCredentials 身份凭证扩展：WechatIlink 扫码授权 + 凭证整组轮换 + 与通用 CRUD 框架的集成点
category: common models credential + pkg wechat_ilink + domain finance identity_credential + handler wechat_integration
scope:
  - "common/src/models/identity_credentials.rs"（CredentialKind::WechatIlink + CredentialDetail::WechatIlink + CredentialDetailPatch::WechatIlink）
  - "src/pkg/wechat_ilink.rs"（扫码登录协议）
  - "src/handlers/finance/wechat_integration/**"（QR 登录 Handler）
  - "src/service/domain/finance/identity_credential.rs"（Domain 层扫码确认后自动落库凭证）
source_files:

  - common/src/models/identity_credentials.rs#L23-L58（CredentialKind::WechatIlink 新增变体 + requires_platform() 专用 kind 返回 false）
  - common/src/models/identity_credentials.rs#L145-L154（CredentialDetail::WechatIlink：bot_token/bot_id/user_id/base_url + encrypt_sensitive 加密 bot_token）
  - common/src/models/identity_credentials.rs#L205-L214（CredentialDetailPatch::WechatIlink：重新扫码整组覆盖）
  - common/src/models/identity_credentials.rs#L359-L374（WechatIlink validate：bot_token/bot_id/base_url 三要素必填 + base_url https 校验）

  - src/pkg/wechat_ilink.rs#L88-L121（get_login_qrcode：get_bot_qrcode → qrcode + qrcode_img_content）
  - src/pkg/wechat_ilink.rs#L124-L209（poll_qrcode_status：长轮询 wait→scaned→confirmed，confirmed 返回 bot_token/bot_id/base_url）

  - src/handlers/finance/wechat_integration/（get_login_qrcode.rs + get_status.rs + login_status.rs：扫码登录 API 三端点）

  - src/service/domain/finance/identity_credential.rs（Domain 层：confirmed 时 create WechatIlink 凭证行；bot_token 加密落库；已存在同类型凭证 → 整组轮换）

  - common/src/api/wechat_integration.rs#L14-L79（DTO：WechatLoginQrcodeRequest + WechatLoginStatusResponse + WechatCredentialSnapshot）

  - src/service/dao/wechat/ilink.rs#L74-L128（resolve_ilink_credentials：从 user_credential 行解析 IlinkChannelCredentials；校验 kind=WechatIlink + 解密 bot_token）

  - docs/wiki/zh/content/核心模块/凭证与安全/身份凭证与授权流程.md

  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一%20Domain%20CRUD%20加密存储与生命周期联动）.md

  - docs/wiki/knowledge/zh/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）/身份凭证统一链路（总卡：模型层%20+%20Domain%20层%20CRUD%20+%20Handler%20层%20API%20+%20外部集成联动%20+%20CredentialDetail%20类型无关下沉）.md

  - docs/wiki/knowledge/zh/微信%20iLink%20专属渠道闭环：wechat_dal%20+%20ilink_dao%20+%20inbound_state%20+%20授权流程/微信%20iLink%20专属渠道闭环：wechat_dal%20+%20ilink_dao%20+%20inbound_state%20+%20授权流程.md

---

# IdentityCredentials 身份凭证扩展（WechatIlink 扫码授权 + 整组轮换）

## §1 整体方案

本卡描述「**微信 ClawBot 扫码授权**」这一全新凭证获取路径如何与 AI Orz 已有的「身份凭证统一 CRUD 框架」集成——扫码授权 = 用户打开微信扫前端展示的二维码 → 手机端确认登录 → 后端长轮询 poll_qrcode_status 返回 confirmed → **自动加密落库 CredentialKind::WechatIlink 凭证行** → 渠道通过 `wechat_credential_id` 引用。

与现存"手动表单输入凭证"路径（飞书 AppID/AppSecret、GitHub PAT、GenericToken 等）的关键差异：
- **凭证获取路径是扫码而非表单**：confirmed 时 bot_token/bot_id/base_url 一次性产出，用户不接触明文凭证
- **整组轮换语义**：同一用户已有 WechatIlink 凭证时，重新扫码 = 旧凭证软删 + 新凭证创建（`rotated: true` 标志）。bot_token 不可在原地更新——iLink 协议的 bot_token 是会话绑定的，整组轮换
- **加密边界不变**：bot_token 在 Domain 层 encrypt_sensitive → `enc:v1:xxx` 密文落库；DAO 层 resolve_ilink_credentials 调 `pkg::crypto::decrypt_channel_secret` 解密

**CredentialKind::WechatIlink 新增凭证类型**：
```rust
CredentialKind::WechatIlink // 专用 kind，不需要 platform
// CredentialDetail::WechatIlink { bot_token, bot_id, user_id: Option, base_url }
// encrypt_sensitive 只加密 bot_token（其他字段非敏感）
// validate：bot_token/bot_id/base_url 三要素必填 + base_url https 校验
```

**扫码授权三端点**（路由：`/api/v1/finance/identity/wechat/`）：
1. **get_login_qrcode** → 调 `pkg::wechat_ilink::get_login_qrcode()` → 返回 `{ qrcode, qrcode_img_content }`
2. **get_status** → 调 `pkg::wechat_ilink::poll_qrcode_status(qrcode)` → 返回 `{ status: wait|scaned|expired|confirmed }`（长轮询，hold ~35s 属正常）
3. **login_status**（Domain 层 confirmed 处理）→ 调 `IdentityCredentialDomain.create` 创建/轮换 WechatIlink 凭证行 → 返回 `{ credential_id, bot_id, rotated }`

## §2 关键文件路径表格

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [common/src/models/identity_credentials.rs](common/src/models/identity_credentials.rs) | 凭证类型契约（前后端共享）| CredentialKind::WechatIlink L38；CredentialDetail::WechatIlink L145；CredentialDetailPatch::WechatIlink L205；WechatIlink validate L359 |
| [pkg/wechat_ilink.rs](src/pkg/wechat_ilink.rs) | 扫码登录协议客户端 | get_login_qrcode L89；poll_qrcode_status L124；IlinkQrStatusKind 四态 L41 |
| [handlers/finance/wechat_integration/](src/handlers/finance/wechat_integration/) | REST API 三端点 | get_login_qrcode.rs + get_status.rs + login_status.rs |
| [domain/finance/identity_credential.rs](src/service/domain/finance/identity_credential.rs) | Domain 层：扫码确认后落库 | confirmed 分支：encrypt bot_token → create WechatIlink 凭证 → 已存在同类型凭证 → 整组轮换（软删旧 + 创建新）|
| [common/src/api/wechat_integration.rs](common/src/api/wechat_integration.rs) | 前后端共享 DTO | WechatLoginQrcodeRequest L16；WechatLoginStatusResponse L37；WechatCredentialSnapshot L69 |
| [dao/wechat/ilink.rs](src/service/dao/wechat/ilink.rs#L74-L128) | DAO 消费凭证 | resolve_ilink_credentials：校验 kind=WechatIlink + 解密 bot_token + base_url 空值回落默认域 |
| 【总卡】身份凭证统一链路 | 本卡描述 WechatIlink 类型如何接入通用框架（新增 kind + new 获取路径）；总卡 source_files[] 尾追加本卡 | 见本卡 source_files[] 尾总卡绝对路径 |
| 【① Wiki 长文】身份凭证与授权流程.md | 完整扫码授权说明 | docs/wiki/zh/content/核心模块/凭证与安全/身份凭证与授权流程.md |
| 【② Wiki 长文】微信 iLink 专属渠道.md | 扫码授权 → 创建渠道 → 入站收帧端到端 | docs/wiki/zh/content/功能模块/消息系统/微信%20iLink%20专属渠道.md |
| 【平行卡】微信 iLink 专属渠道闭环 | 本卡提供凭证层底座，消费方是微信 DAO/DAL | 见本卡 source_files[] 尾微信 iLink 卡绝对路径 |

## §3 架构约定

1. **扫码授权获取的凭证必须走 encrypt_sensitive 加密 bot_token**：Domain 层 confirmed 分支内调 `detail.encrypt_sensitive(|s| Ok(format!("enc:v1:{s}")))`——加密发生在 Domain 编排层，不泄漏到 Handler 层。禁止 Handler 层直接写 UserCredentialDao。
2. **WechatIlink 是专用 kind，不需要 platform 二元匹配**：`CredentialKind::WechatIlink.requires_platform() == false`——generic 类 kind（GenericToken/OAuth/UserPassword）需要 platform，专用 kind（LarkApp/WechatIlink）不需要。CredentialRequirement 校验矩阵正确处理这个分支。
3. **整组轮换语义而非原地 patch**：重新扫码 confirmed 时，旧凭证软删 + 新凭证创建。bot_token 与 iLink 会话绑定，旧 token 失效后无法继续使用，原地 update_detail 语义上不是"轮换"。Domain 层 detect_duplicate + hash 幂等判断后决定 create 还是 rotate。
4. **Domain 层禁止直接调用 wechat_ilink DAO**：Handler 层调 pkg/wechat_ilink 拿 confirmed 凭据 → 调 IdentityCredentialDomain.create → Domain 内部走 credential_detail.encrypt_sensitive → UserCredentialDao SQLite 实现。分层严格：Handler（适配层）→ Domain（业务编排）→ DAL/DAO（数据访问）。
5. **resolve_ilink_credentials 是唯一的 WechatIlink 凭证解析入口**：DAO 层出站 push 和 start_polling 都调它，避免在多个 DAO 实现里重复解密/校验逻辑。校验 kind + decrypt bot_token + validate base_url → 返回 IlinkChannelCredentials。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 Handler 层直接落库凭证**：扫码 confirmed 后 Handler 必须调 Domain 层（IdentityCredentialDomain.create）。绕过 Domain 层 = 绕过 encrypt_sensitive + hash 幂等校验 + 整组轮换逻辑。
2. ❌ **禁止在 common::models 里写后端特定加密原语**：CredentialDetail::encrypt_sensitive 参数是闭包 `F: Fn(&str) -> Result<String>`，common 不依赖 `pkg::crypto`。加密原语在 Domain 层传入（`pkg::crypto::encrypt_channel_secret` 闭包注入）。
3. ✅ **base_url https 校验**：WechatIlink::validate 必须 `if !base_url.starts_with("https://")` 报错——iLink 协议走 HTTPS，不允许 http 明文传输 bot_token。
4. ✅ **poll_qrcode_status 长轮询超时宽容**：客户端 45s 超时 ≈ 服务端 35s hold → 客户端先到超时被视为"本轮无事件"（返回 Wait），不是网络错误。调用方必须把超时视为正常状态而不是异常。
5. ✅ **整组轮换后旧渠道的轮询必须停掉**：Domain 层凭证轮换触发 `credential_changed` 事件 → WechatListenerDal.rebuild_listeners_for_credential → 停旧 bot_id 轮询 + 建新 bot_id 轮询。禁止两个轮询同时跑（旧 token 已失效）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 1 篇 Wiki 长文（身份凭证与授权流程）+ 1 张总卡（身份凭证统一链路）+ 1 张平行卡（微信 iLink 专属渠道）；总卡 source_files[] 尾追加本卡相对路径；Wiki 长文 cite 段回链本卡。

---

# 本卡 Level 5 声明（AGENTS §2.1.3.2）

- scope 与现存「身份凭证统一链路」总卡交集约 25%（总卡覆盖通用 CRUD 框架，本卡覆盖 WechatIlink 新类型 + 扫码获取路径——路径是全新的，不是"手动表单输入"的增量）
- 不触发 Level 1-4 合并/拆分判定，直接新建
- 但作为**身份凭证统一链路总卡的下游集成点**，总卡 source_files[] 尾需追加本卡路径（Step 4 更新）
