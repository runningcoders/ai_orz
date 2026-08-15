---
kind: wiki_knowledge_card
name: AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容
category: pkg层加密基础设施
scope:
  - "src/pkg/crypto.rs"
  - "src/pkg/config.rs"
  - "common/src/models/identity_credentials.rs"
  - "src/service/dal/user.rs"
source_files:
  - src/pkg/crypto.rs:Ln-Lm（encrypt_channel_secret + decrypt_channel_secret AES-256-GCM 实现）
  - src/pkg/config.rs:Ln-Lm（MASTER_KEY 加载与校验）
  - common/src/models/identity_credentials.rs:Ln-Lm（encrypt_sensitive/apply_patch 闭包参数）
  - src/service/domain/finance/identity_credential.rs:Ln-Lm（Domain 层闭包传入位置 + 调用点）
  - src/service/dal/user.rs:Ln-Lm（decrypt 场景——读取凭证后解密敏感字段用于渠道建连）
  - docs/design/message_channel_design.md
  - docs/plan/身份凭证Domain统一CRUD重构.md
  - docs/wiki/zh/content/架构设计/数据存储架构.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md
  - docs/wiki/knowledge/zh/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立.md
---

# AES-256-GCM 敏感字段加密（闭包注入模式）

## §1 整体方案
身份凭证的敏感字段（Lark 的 app_secret / encrypt_key，Github 的 personal_access_token，未来 Slack 的 bot_token / signing_secret 等）统一使用 **AES-256-GCM 认证加密**（提供机密性 + 完整性校验：篡改密文立即解密失败不静默）。加密主密钥 MASTER_KEY 从环境变量 `AIORZ_MASTER_KEY` 加载（32 字节 base64；启动时校验为空则用固定开发密钥 + sys_warn! 告警，严禁生产用默认密钥）。

**闭包注入解耦模式（关键设计）**：common 层模型（CredentialDetail.encrypt_sensitive / apply_patch）需要对敏感字段加密，但 common crate 不能直接依赖后端 pkg::crypto（否则 common 被污染前端 WASM 构建、common 单元测试无法独立跑）。解决方式：**加密/解密原语以闭包形式从 Domain 层传入模型层**——`encrypt_sensitive(encrypt_fn: impl Fn(&str) -> Result<String>)` 和 `apply_patch(patch, encrypt_fn: impl Fn(&str) -> Result<String>)` 的 encrypt_fn 参数就是 Domain 传入的 `|s| pkg::crypto::encrypt_channel_secret(s)`；common 本身零 crypto 依赖。

**调用链概览**：
- 创建流程（加密点 1）：Handler 明文 CreateCredentialCmd → Domain.create_credential → `detail = detail.encrypt_sensitive(|s| pkg::crypto::encrypt_channel_secret(s))?`（仅敏感字段加密）→ 用户凭证库 JSON 序列化 → 落库 users.identity_credentials 列。
- 更新流程（加密点 2）：Domain.update_credential → `impact = credential.detail.apply_patch(patch, |s| pkg::crypto::encrypt_channel_secret(s))?` → **仅被补丁覆盖成新明文的敏感字段**进入 encrypt_fn 调用（未改动字段不重复加密，避免二次加密解密失败）。
- 渠道建连流程（解密点 唯一）：MessageChannelDal.create/connect → 查用户凭证库 → `pkg::crypto::decrypt_channel_secret(encrypted_app_secret)?` → 得到明文 secret 用于飞书 WS 建联 / API 请求。**明文绝不落地**（仅内存中短暂持有，建联完成立即 drop）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键实现/入口 |
|------|------|------------|
| [src/pkg/crypto.rs](src/pkg/crypto.rs) | 加密原语单文件实现（pkg 基础设施层，无业务）| `encrypt_channel_secret(plaintext: &str) -> Result<String>`（AES-256-GCM + base64 输出；格式：base64(nonce \|\| ciphertext \|\| tag)，nonce 12 字节随机）；`decrypt_channel_secret(ciphertext_b64: &str) -> Result<String>`（验证完整性：tag 不匹配直接返回 Err，不返回坏明文）；`generate_master_key_base64()` 工具函数；单元测试：roundtrip（encrypt→decrypt=original）/tamper（改一字节解密失败）。 |
| [src/pkg/config.rs](src/pkg/config.rs) | MASTER_KEY 加载与启动校验 | config.get().master_key 读 env AIORZ_MASTER_KEY（base64）；启动时 decode 校验长度 = 32 字节；**生产模式** env 未设置 → 启动失败（bail_err!）；**dev 模式** 未设置 → sys_warn! 打印告警 + 使用 `DEFAULT_DEV_MASTER_KEY`（仅本地测试用，禁止提交自己的密钥）；|
| [common/src/models/identity_credentials.rs](common/src/models/identity_credentials.rs) | encrypt_sensitive + apply_patch 的闭包参数声明 | `fn encrypt_sensitive<F: Fn(&str) -> Result<String>>(self, encrypt_fn: F) -> Result<Self>`（self 消费，返回加密后的新 detail，避免 in-place 改字段）；`fn apply_patch<F: Fn(&str) -> Result<String>>(self, patch, encrypt_fn: F) -> Result<(Self, CredentialUpdateImpact)>`；对每个变体的敏感字段枚举（Lark：app_secret、encrypt_key、verification_token；Github：token），**非敏感字段（app_id、name、login、avatar_url 等）绝不加密**（否则渠道引用检查无法读 app_id）。|
| [src/service/domain/finance/identity_credential.rs](src/service/domain/finance/identity_credential.rs) | Domain 层闭包传入点（2 处）| create 流程：`detail.encrypt_sensitive(|s| crate::pkg::crypto::encrypt_channel_secret(s))?`；update 流程：`detail.apply_patch(patch, |s| crate::pkg::crypto::encrypt_channel_secret(s))?` |
| [src/service/dal/message_channel.rs](src/service/dal/message_channel.rs) 或 lark.rs 或 Dao 层 | 唯一解密点（渠道建连/发消息）| `let app_secret = pkg::crypto::decrypt_channel_secret(&cred.detail.app_secret)?` → 明文传入 lark_cli / reqwest 客户端；明文变量作用域仅限函数内部，绝不写入日志/响应/事件 payload |
| 【对应 Wiki 长文 1】身份凭证管理.md | 系统化上下文 §5 AES-256-GCM 加密小节 | /Users/aman/Technology/rust/ai_orz/docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md |
| 【对应 Wiki 长文 2】数据存储架构.md | users.identity_credentials 列加密说明 + 主密钥管理 | /Users/aman/Technology/rust/ai_orz/docs/wiki/zh/content/架构设计/数据存储架构.md |
| 【① Design】message_channel_design.md §2 凭证加密 | 为什么选 AES-256-GCM / 为什么闭包注入 | docs/design/message_channel_design.md |
| 【② Plan 定稿】身份凭证Domain统一CRUD重构.md §二 架构思路 (b) 敏感字段加密下沉 | 闭包注入设计决策 + §六 执行结果加密分支测试通过 | docs/plan/身份凭证Domain统一CRUD重构.md |
| 【平行卡】模型层信息下沉（encrypt_sensitive 6 行为方法之一）| 模型层 encrypt_sensitive/apply_patch 定义 | docs/wiki/knowledge/zh/身份凭证模型层信息下沉：CredentialDetail%20行为%20+%20CredentialDetailPatch%20补丁语义%20+%20默认槽位独立/身份凭证模型层信息下沉：CredentialDetail%20行为%20+%20CredentialDetailPatch%20补丁语义%20+%20默认槽位独立.md |

## §3 架构约定

1. **敏感字段清单统一在模型层声明（单一事实源）**：哪些字段需要加密只由 CredentialDetail 每个变体的 encrypt_sensitive/apply_patch arm 决定（禁止在 DAO/DAL/Handler 层手工写"字段 X 需要加密"）。新增凭证类型时只需在对应变体 arm 中列出敏感字段。
2. **绝不二次加密**：create 时全量加密；update 时只加密"本次补丁改动的明文敏感字段"——apply_patch 内部逻辑保证：`if let Some(new_plain) = patch.field { encrypted_field = encrypt_fn(new_plain)? }`；其余字段原样保留（原来的密文不改动），因此不会重复 encrypt。
3. **明文绝不进入日志宏 / SSE / AOP 事件 / HTTP 响应**：任何 log_info!/log_warn! 输出凭证相关内容时，敏感字段必须 hash/脱敏（`app_secret_sha256_short`）；禁止把 detail 整体 debug 打印到日志（Debug 输出会含明文或密文都泄漏结构）。
4. **MASTER_KEY 与代码彻底分离**：MASTER_KEY 只能从 env 或外部 KMS 加载；严禁提交任何真实密钥到仓库；dev 默认密钥 sys_warn! 只在本地起作用，CI/生产启动脚本设置 AIORZ_MASTER_KEY 会覆盖。
5. **解密失败立即硬失败，不降级**：decrypt_channel_secret 返回 Err 时，渠道建联流程直接失败返回 Err（不静默使用空 secret 重试）；原因：tag 不匹配 = 密文被篡改或主密钥错误 → 用错误明文连接会造成更隐蔽 bug（API 报 401 难以定位）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 common 层 `use crate::pkg::crypto` / 任何直接依赖加密实现**：common 永远通过 encrypt_fn/decrypt_fn 闭包参数间接调用加密原语；直接依赖会破坏 common 的纯属性（前端 WASM 编译失败、common 单测无法隔离跑、未来换加密库需同时改 common）。
2. ❌ **禁止 DAO 层保存前统一整个 JSON 列加密**：必须字段级加密（只加密敏感字段），非敏感字段（app_id/name/kind/id/时间戳）必须保留明文 JSON——因为 DAL/Handler 层经常需要读 app_id 做渠道引用检查、读 created_at 做排序、读 kind 做类型判断；整列加密会让每次读都要解密全 JSON，性能下降 10-50 倍 + 无法对 identity_credentials 做 JSON 路径查询。
3. ❌ **禁止在测试中使用真实 MASTER_KEY 断言密文固定**：AES-GCM nonce 是 12 字节随机，每次 encrypt 密文都不同（即使相同明文+相同 key）→ 测试必须用 roundtrip（encrypt→decrypt == 原明文）断言；禁止断言密文字符串固定（会非确定性失败）。
4. ✅ **MASTER_KEY 轮换强约束（未来需要时）**：当需要更换主密钥时，必须走 **双密钥期兼容方案**：decrypt_channel_secret 支持尝试「新 key 解密失败 → 旧 key 解密 → 重新加密用新 key → 更新回 DB」异步批量迁移脚本；禁止一刀切换 key 直接解密失败（线上所有凭证失效 = P0 事故）。
5. ✅ **版本兼容强约束**：密文格式必须带版本前缀（`v1:` + base64(nonce||ct||tag)），未来升级算法（如 ChaCha20-Poly1305/AES-256-GCM-SIV）时 decrypt_channel_secret 按前缀切换实现，旧 `v1:` 密文照常解密；禁止使用无前缀的裸 base64（未来无法识别格式版本）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 1 新建 wiki 长文（身份凭证管理.md）+ 数据存储架构长文 + Design 1 + Plan（真实定稿）+ 平行卡 1；对应 Wiki 长文 cite 段回链本卡 + Design + Plan。
