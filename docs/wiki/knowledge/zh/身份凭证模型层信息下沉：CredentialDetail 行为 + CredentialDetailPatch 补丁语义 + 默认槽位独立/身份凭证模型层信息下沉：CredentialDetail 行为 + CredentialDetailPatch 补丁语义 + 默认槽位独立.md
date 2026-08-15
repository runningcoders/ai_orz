---
kind: wiki_knowledge_card
name: 身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立
category: common模型层
scope:
  - "common/src/models/identity_credentials.rs"
  - "common/src/enums/credential*.rs"
source_files:
  - common/src/models/identity_credentials.rs:Ln-Lm
  - common/src/enums/credential_kind.rs:Ln-Lm
  - src/models/user.rs:Ln-Lm
  - src/service/dao/user/sqlite.rs:Ln-Lm
  - src/service/dal/user.rs:Ln-Lm
  - docs/design/message_channel_design.md
  - docs/design/lark_cli_integration.md
  - docs/plan/身份凭证Domain统一CRUD重构.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md
  - docs/wiki/knowledge/zh/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用.md
  - docs/wiki/knowledge/zh/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容/AES-256-GCM 敏感字段加密：encrypt_channel_secret 闭包注入 + 加密原语位置 + 版本兼容.md
---

# 身份凭证模型层信息下沉

## §1 整体方案
身份凭证重构前，凭证 detail 的字段校验/trim 规范化/敏感字段加密/默认槽位设置逻辑散落在 8 个 lark/github CRUD Handler + Domain 层，每新增凭证类型（Slack/WeChat/Webhook...）需要复制 1 套骨架。重构方案严格遵循信息专家原则（同模式 Vectorizable trait），把三类类型差异知识**下沉到 common 模型内部行为**：

(a) CredentialDetail 枚举（LarkApp / GithubToken 2 变体）新增 6 个统一行为方法：`kind()` → CredentialKind / `primary_id()` → 凭证唯一标识（app_id / gh_login）/ `normalized()` → trim 规范化 + 必填默认值填充 / `validate()` → 字段级必填校验 + 格式校验（长度/前缀/uuid 格式等）/ `encrypt_sensitive(encrypt_fn: Fn(&str) -> Result<String>)` → 闭包注入加密原语，逐个变体敏感字段加密（common 不依赖 pkg::crypto 实现零耦）/ `apply_patch(patch: CredentialDetailPatch, encrypt_fn) -> Result<CredentialUpdateImpact>` → 补丁语义：`Some("")` 清除、`None` 保持、非空覆盖；返回 impact.secret_changed 布尔（哪些敏感字段被实际写入）用于下游 WS 移交判断。

(b) 新增 `CredentialDetailPatch` 枚举（与 CredentialDetail 变体一一对应，每个字段 Option 化）：空 patch = 完全不改动；嵌套字段级 patch 语义通过 `update` Handler 参数构造。新增 `CredentialUpdateImpact` 结构体（secret_changed: bool，是否有 app_id/encrypt_key 类字段被真的写入）。

(c) UserIdentityCredentials 顶层结构（users.identity_credentials JSON 列）新增 3 个默认槽位操作方法：`set_default_for(kind, Option<id>)`（None/空白清除、非空检查存在+类型匹配、各类型默认槽位独立）/ `clear_default_for(kind, id)`（删除凭证时联动清除对应默认槽位）/ `resolve_lark_credential_ref(Option<id>) -> Result<&Credential>` + `resolve_github_credential() -> Option<&Credential>`（消息渠道创建/gh_cli 工具身份的统一单一校验入口，避免 Handler 层重复写「存在+类型匹配」校验）。

新增类型时，**common 层扩 2 个变体（CredentialDetail + CredentialDetailPatch）+ 6 个方法对应 arm**，Domain/Handler 零字段级改动。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [common/src/models/identity_credentials.rs](common/src/models/identity_credentials.rs) | 模型层核心（UserIdentityCredentials 顶层 + UserIdentityCredential 单条 + CredentialDetail 枚举 + Patch 枚举 + Impact）| 6 行为方法（kind/primary_id/normalized/validate/encrypt_sensitive/apply_patch）、3 默认槽位方法（set_default_for/clear_default_for/default_slot_mut）、2 凭证解析方法（resolve_lark_credential_ref/resolve_github_credential）、parse/to_column_value JSON 序列化 |
| [common/src/enums/credential_kind.rs](common/src/enums/credential_kind.rs)（若存在；否则定义在 identity_credentials.rs 同文件顶部）| CredentialKind 枚举（LarkApp/GithubToken）| default_slot_mut match arm 对应；新增凭证类型时需新增一个枚举值（与 CredentialDetail 变体数一致）|
| [src/models/user.rs](src/models/user.rs) | UserPo：identity_credentials 列定义（TEXT）| users.sql DDL 中 identity_credentials TEXT 默认空串（空串=无凭证），与 UserIdentityCredentials.to_column_value 输出一致；字段级读取走 UserDal.get_identity_credentials |
| [src/service/dao/user/sqlite.rs](src/service/dao/user/sqlite.rs) | DAO 层：get_identity_credentials 读取原始列值 + save_identity_credentials 写回 | 纯列读写 + 反序列化封装（**禁止在 DAO 层做字段级校验**）|
| [src/service/dal/user.rs](src/service/dal/user.rs) | DAL 层：save_identity_credentials 的事务/行级更新（并发安全） | 并发写凭证库时乐观锁：UPDATE users SET identity_credentials = $1, version = version+1 WHERE id = $2 AND version = $3；失败重试最多 3 次 |
| 【对应 Wiki 长文】身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md | 系统化上下文（必读 §5 模型层信息下沉小节）| /Users/aman/Technology/rust/ai_orz/docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md |
| 【② Plan 定稿】身份凭证Domain统一CRUD重构.md §三 涉及文件 §四 扩展模板 | 改动清单 + 新增凭证类型 4 步模板 | docs/plan/身份凭证Domain统一CRUD重构.md |
| 【① Design 1】message_channel_design.md §3 凭证引用检查 | resolve_lark_credential_ref 设计动机 | docs/design/message_channel_design.md |
| 【① Design 2】lark_cli_integration.md §二 凭证变更 WS 移交 | secret_changed 字段来源（CredentialUpdateImpact）| docs/design/lark_cli_integration.md |
| 【平行卡 1】Domain 统一 CRUD（5 方法 + 2 Cmd + match kind）| 上层 Domain 调用方式 | docs/wiki/knowledge/zh/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用.md |
| 【平行卡 2】AES-256-GCM 敏感字段加密（encrypt_sensitive 闭包注入） | encrypt_sensitive 闭包参数来源与原语位置 | docs/wiki/knowledge/zh/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容/AES-256-GCM%20敏感字段加密：encrypt_channel_secret%20闭包注入%20+%20加密原语位置%20+%20版本兼容.md |

## §3 架构约定

1. **凭证模型层本身不依赖 pkg::crypto**：加密原语以闭包参数 `encrypt_fn: impl Fn(&str) -> Result<String>` 形式注入（Domain 层调用 encrypt_sensitive/apply_patch 时传 `pkg::crypto::encrypt_channel_secret`）。目的：保持 common crate 纯（不依赖 tokio/aes-gcm 后端实现），未来换加密库/或在 WASM 用纯 Rust aes-gcm 时，common 0 改动。
2. **补丁语义统一（三态）**：所有 CredentialDetailPatch 字段必须是 Option<Option<String>>（嵌套 Option）：外层 None = 保持原值；外层 Some(None) 或外层 Some(Some("")) = 清除该字段；外层 Some(Some(non_empty)) = 覆盖。任何「更新语义」使用三态补丁，禁止用单独 bool 标志位表示「是否清除某字段」。
3. **`impact.secret_changed` 单一事实源**：只有当 secret 类字段（app_secret/encrypt_key/personal_access_token 等）**真的被覆盖成新明文值** 时才返回 true；如果 patch 是 None 保持或 Some("") 清除（非覆盖）→ false。下游 WS 移交、清 HOME 配置、清 gh 登录态等副作用全部以 impact.secret_changed 为唯一判断依据。
4. **默认槽位独立**：每种 CredentialKind 的默认槽位字段在 UserIdentityCredentials 中完全独立（lark = default_credential_id；github = default_github_credential_id），新增凭证类型时只需新增一个独立的 default_XXX_credential_id 字段 + default_slot_mut() 对应 arm；set_default_for 对不同类型的操作绝对不会互相干扰。
5. **落库列值一致性**：空凭证库 → UserIdentityCredentials::default().to_column_value() 返回空串（与 users DDL 默认值空串完全对齐）；非空 → 序列化 JSON。parse() 中空串同样解析成空库。DAO 读写必须同时对齐 parse/to_column_value（禁止 DAO 读空串返回 Err）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止在 Handler 或 Domain 层手工写「存在+类型匹配」校验**：飞书渠道凭证选择统一调用 `library.resolve_lark_credential_ref(id_opt)`（报 InvalidRequest 文案一致）；gh_cli 工具身份统一调用 `library.resolve_github_credential()`（默认→第一条回退逻辑一致）。任何自定义手写都会造成「错误文案漂移 + 一处修另外一处漏掉」。
2. ❌ **禁止 CredentialDetail 变体新增方法时漏掉对应 patch arm**：每个 CredentialDetail 变体的 6 个行为方法 + CredentialDetailPatch 对应变体必须**数量/字段名完全一一对应**（可通过 clippy `non_exhaustive` / 自定义测试断言保证：variants!(CredentialDetail).len() == variants!(CredentialDetailPatch).len()）。
3. ❌ **禁止 encrypt_sensitive 加密已经加密过的字段**：`encrypt_sensitive()` 只在「第一次 create + apply_patch 覆盖明文」时调用；**不得在每次落库前重复调用**（会造成二次加密，解密失败）。模型层的职责之一就是保证 apply_patch 只对"被补丁覆盖成新明文"的敏感字段调用 encrypt_fn，**未改动字段绝不进入 encrypt 流程**（在 apply_patch 内部判断）。
4. ✅ **新增凭证类型三步强约束（common 层必须全部完成再动 domain/handler）**：① CredentialKind 枚举加新值 → ② CredentialDetail 加变体 + 6 行为方法 arm（含 normalize/validate/encrypt_sensitive 明确哪些字段算敏感 + 哪些 primary_id）→ ③ CredentialDetailPatch 加变体 + default_slot_mut() 新增独立字段 + 对应 `set_default_for` 新增 arm。**3 步全完成前禁止创建 Domain/Handler**（防止模型未定义就 Handler 先写死字段导致编译通过但运行时错判）。
5. ✅ **版本兼容强约束**：如果未来 CredentialDetail 新增字段（比如 lark 新增 bot_verify_token），**必须把新字段定义成 `#[serde(default)]` Option<String>**；同时 validate() 中不得要求新字段必填（旧 JSON 列中不存在时允许 None）。禁止破坏旧 JSON 反序列化（反序列化失败 = 整个凭证库丢失，线上事故级）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 wiki 长文绝对路径 1 条（新建）+ Design 2 + Plan 1（**真实路径非占位**）+ 平行卡 2；对应 Wiki 长文 cite 段必须回链本卡 + Design + Plan。
