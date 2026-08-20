---
kind: wiki_knowledge_card
name: 身份凭证统一链路（总卡：模型层 + Domain 层 CRUD + Handler 层 API + 外部集成联动 + CredentialDetail
  类型无关下沉）
category: 身份凭证
scope:
- src/service/domain/identity_credential/**
- src/service/dal/identity_credential.rs
- src/service/dao/identity_credential/**
- src/handlers/identity_credential/**
- src/models/identity_credential.rs
- common/src/api/identity_credential.rs
source_files:
- migrations/20260420000000_initial.sql#L405-L419
- migrations/20260420000000_initial.sql#L751-L764
- src/models/user_credential.rs#L1-L140
- src/service/dao/user_credential/mod.rs#L1-L102
- src/service/dao/user_credential/sqlite.rs#L1-L358
- src/service/domain/finance/identity_credential.rs#L1-L454
- src/service/dal/user.rs#L86-L150
- src/service/dao/lark/mod.rs#L54-L96
- src/service/dal/lark.rs#L1-L50
- src/service/dal/message_channel.rs#L1-L50
- docs/plan/用户身份凭证独立表落地.md
- docs/wiki/knowledge/zh/身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用/身份凭证
  Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用.md
- docs/wiki/knowledge/zh/身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式/身份凭证
  Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式.md
- docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md
- docs/wiki/knowledge/zh/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义
  + 默认槽位独立/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立.md
- docs/wiki/knowledge/zh/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 +
  LarkWsMetrics 健康指标/Lark P2P WS 私信入站：身份凭证引用解析 + app_id 聚合 WS + open_id 自动映射 + LarkWsMetrics
  健康指标.md

---

# 身份凭证统一链路（总卡）

## §1 整体方案

b4f9a560 变更把身份凭证从「分散在各外部集成 Domain 自己造轮子存凭证（飞书 LarkAppCredential / GitHub GitHubPAT / 微信 WeChatAppSecret …）」统一为**类型无关 CRUD**：一张 `identity_credentials` 表 + 一张 `CredentialDetail` JSON 字段存任意类型凭证明细，Domain 层提供 `create/get/update/delete/list/query` 类型无关统一 API；**加密下沉**：明文凭证 ↔ AES-256-GCM 加密/解密统一在 DAL 层 close 到 DAO 写入前/读取后；所有外部集成（飞书/微信/GitHub/Slack…）**只消费**身份凭证 Domain 的 getter，**不直接**碰 DAO/DB。

CredentialDetail 字段下沉（b4f9a560 的关键重构）：之前各集成定义独立 credential_type → enum 匹配 → 硬解字段（如 `if type==LARK { read app_id field from json }`）现在改为 **Domain 层不感知 Detail 结构**，CredentialDetail 是 `serde_json::Value` 类型，各外部集成自己拿 getter 后自行 into() → 自己的强类型 Detail（LarkCredentialDetail / GitHubPATCredentialDetail …），Domain CRUD 代码完全类型无关，新增一种凭证类型无需改 Domain 层一行代码。

本卡是 Level 4 总卡，下辖 3 张细粒度拆卡：模型层细节卡、Domain 层细节卡、Handler 层细节卡。阅读路径：总卡（理解全链路）→ 选对应细卡（跳对应层精确代码锚点）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/宏/入口 |
|------|------|----------------|
| [src/models/identity_credential.rs](src/models/identity_credential.rs) | PO + Entity 定义（模型层） | `IdentityCredentialPo`（id/user_id/org_id/credential_type: IdentityCredentialType/credential_detail_hash/encrypted_detail/encrypted_dek_encryption_key/version/status/created_at/updated_at）；`CredentialDetail = serde_json::Value`（类型无关下沉核心）；`IdentityCredentialEntity { po: IdentityCredentialPo, plaintext_detail: CredentialDetail }`（对外只暴露解密后的 plaintext_detail） |
| [src/service/dao/identity_credential/sqlite.rs](src/service/dao/identity_credential/sqlite.rs) | DAO：SQLite 单一表 CRUD（加密态读写） | 表 `identity_credentials STRICT`；查询条件：user_id, org_id, credential_type IN (), credential_detail_hash（去重比对用） |
| [src/service/dal/identity_credential.rs](src/service/dal/identity_credential.rs) | DAL：PO↔Entity 转换 + **加解密边界**（核心！）| `decrypt_detail(encrypted_blob, dek, kek) -> Result<CredentialDetail>`：AES-256-GCM 解密（DEK 被 KEK 加密后存储；KEK 来自环境变量 IDENTITY_KEK）；`encrypt_detail(plaintext) -> (encrypted_blob, encrypted_dek, hash)`：加密 + 计算 content_hash（用于幂等去重判断）；DAOOps 只写加密态，不碰明文（符合「密文 DAO、明文 Entity」PO 与 Entity 分层边界 §3.5） |
| [src/service/domain/identity_credential/crud.rs](src/service/domain/identity_credential/crud.rs) | Domain：类型无关 CRUD（单一事实源） | `create_credential(ctx, CreateCmd { credential_type, detail: Value, scope })`：detail 是 serde_json::Value（**不感知结构**）→ DAL 加密 → DAO 入库；`get_credential_by_id(ctx, id) -> Option<Entity>`（含解密 detail）；`update_detail(ctx, id, new_detail: Value)`：版本号递增 + 重加密 + 新 hash；`delete_credential(ctx, id)`：软删 status=0；`list/query`：按 user_id/org_id/credential_type 过滤 + 分页 |
| [src/handlers/identity_credential/*](src/handlers/identity_credential/) | Handler：REST API（按业务方法拆分文件）| create / update / list / delete 各 1 独立 handler.rs 文件（§4.5 Handler 拆分规范）；所有 DTO 从 common/src/api/identity_credential.rs 导入；权限：普通用户只 CRUD 自己 user_id 的，Admin 可看 org 下所有，SuperAdmin 跨 org |
| [src/pkg/encryption.rs](src/pkg/encryption.rs) | pkg 层通用加密工具（AES-256-GCM） | `aes_256_gcm_encrypt(plaintext, key, nonce?) -> CipherText { blob, tag, nonce }` / `decrypt()`；DEK 是每次加密随机生成的 32 bytes Data Encryption Key，加密后和 blob 一起存；KEK 是 Key Encryption Key 来自环境变量，永不进 DB |
| [common/src/api/identity_credential.rs](common/src/api/identity_credential.rs) | common DTO 单一事实源（前后端共用） | `CreateIdentityCredentialRequest { credential_type: IdentityCredentialType, credential_detail: serde_json::Value, scope: CredentialScope }`；`IdentityCredentialResponse { id, credential_type, scope, detail_hash, created_at }`（**响应里不返回明文 detail**，防止 HTTP 日志泄露；明文只在 Server 内部 Entity 层存在） |
| 细粒度拆解卡：模型层 | CredentialDetail 6 行为方法 + CredentialDetailPatch 三态补丁 + 默认槽位独立 | [身份凭证模型层信息下沉卡](docs/wiki/knowledge/zh/身份凭证模型层信息下沉：CredentialDetail%20行为%20+%20CredentialDetailPatch%20补丁语义%20+%20默认槽位独立/身份凭证模型层信息下沉：CredentialDetail%20行为%20+%20CredentialDetailPatch%20补丁语义%20+%20默认槽位独立.md) |
| 细粒度拆解卡：Domain 层 | 5 类型无关方法 + 2 Command + match kind 分发生命周期副作用 | [身份凭证 Domain 统一 CRUD 卡](docs/wiki/knowledge/zh/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用/身份凭证%20Domain%20统一%20CRUD：5%20类型无关方法%20+%202%20Command%20+%20match%20kind%20分发生命周期副作用.md) |
| 细粒度拆解卡：Handler 层 | 八文件迁移 + DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式 | [身份凭证 Handler 八文件迁移卡](docs/wiki/knowledge/zh/身份凭证%20Handler%20八文件迁移：DTO%20零改动%20+%20CreateCredentialCmd%20构造器%20+%20统一调用方式/身份凭证%20Handler%20八文件迁移：DTO%20零改动%20+%20CreateCredentialCmd%20构造器%20+%20统一调用方式.md) |
| 【Wiki 长文】身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md | 系统化上下文（§5 详细分析 + §8 Troubleshooting）| [身份凭证管理长文](docs/wiki/zh/content/功能模块/系统管理/身份凭证管理（统一%20Domain%20CRUD%20加密存储与生命周期联动）.md) |
| 【② Plan】身份凭证 Domain 统一 CRUD 重构.md | 落地 7 章快照 | [docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md](docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md) |

## §3 架构约定

本卡为 [模型层卡](docs/wiki/knowledge/zh/身份凭证模型层：IdentityCredentialPo%20+%20CredentialDetail%20JSON%20+%20AES-256-GCM/身份凭证模型层：IdentityCredentialPo%20+%20CredentialDetail%20JSON%20+%20AES-256-GCM.md) + [Domain 层卡](docs/wiki/knowledge/zh/身份凭证%20Domain%20层：类型无关%20CRUD%20+%20Service%20Trait%20+%20事件联动/身份凭证%20Domain%20层：类型无关%20CRUD%20+%20Service%20Trait%20+%20事件联动.md) + [Handler 层卡](docs/wiki/knowledge/zh/身份凭证%20Handler%20层：REST%20API%20+%20DTO%20+%20参数校验与权限%20Gate/身份凭证%20Handler%20层：REST%20API%20+%20DTO%20+%20参数校验与权限%20Gate.md) 描述的**身份凭证四层体系**中的全链路**总卡**；按 AGENTS §2.1.3 Level 4 保留。

1. **CredentialDetail 类型无关铁律**：Domain 层（crud.rs）禁止任何 `if credential_type == GitHubPAT { let x = detail["token"].as_str() }` 这种按类型解 JSON 字段的代码——Domain 层把 detail 当 opaque Value 透传给 DAL 加密或返回给调用方。**消费方才有权 into() 成自己的强类型**（GitHub 集成代码拿到 Entity.detail 后 `let ghd: GitHubPATDetail = serde_json::from_value(detail)?`）。
2. **密文 DAO、明文 Entity 严格分层（PO 与 Entity 分层边界 §3.5）**：DAO 返回 PO.encrypted_detail = Vec<u8>（密文），DAL 统一完成解密 → 出 Domain 层的 Entity.plaintext_detail = Value（明文）。禁止 DAO 直接返回明文，禁止 Domain 层做加密（DAL 是加解密唯一边界，便于以后替换加密算法只改 DAL 一处）。
3. **新增凭证类型零 Domain 改动**：加一种新集成（如 Slack Bot Token）的流程 = ① 调 Handler create API 时传 `{ credential_type: "SLACK_BOT_TOKEN", detail: { "bot_token": "xoxb-..." } }` → ② Slack 集成代码在需要读凭证时 `IdentityDomain.get_by_type(ctx, SLACK_BOT_TOKEN).map(|e| serde_json::from_value(e.detail)?)`。Domain CRUD 代码零行修改（b4f9a560 核心收益）。
4. **响应里永不返回明文 credential**：`IdentityCredentialResponse`（common DTO）**没有 detail 字段**，只返回 `detail_hash: String`（用于前端展示「已保存 xxx_hash 的凭证，上次修改 2026-08-xx」）。明文只在 Server 内存（Entity.plaintext_detail）和加密前的 create/update 请求 body 中出现。
5. **版本号 + 哈希幂等防重存**：create 时计算 `hash = sha256(credential_type + ":" + detail_json_canonical)`，查询同 user_id + same type + same hash 是否存在——存在则返回已存在的 ID + `created: false, duplicate: true`，不重复入库（避免用户连续点保存把同一 GitHub PAT 存 10 份）。update 时 version 自增 + hash 重算，允许版本回滚（version 字段）。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 KEK 写入任何 DB / 日志 / 配置文件**：KEK（IDENTITY_KEK 环境变量）只在 `src/pkg/encryption.rs` 中 `OnceLock` 加载，`#[deny(clippy::print_stdout)]` + 测试 `kek_never_logged` 保证 log_info!/println! 打印 KEK 全失败；代码 review 中看到 `log_info!("kek={}", kek)` 一律打回。
2. ❌ **禁止 common DTO IdentityCredentialResponse 加 plaintext_detail 字段**：即使前端要做「编辑凭证时回填原值」——也不能直接返回明文。正确做法：前端编辑页面展示「当前凭证 hash=xxx，如需修改请重新输入完整值」（大多数 SaaS 产品的 Token 管理行为）。如果必须回填（业务强制），走独立 `get_plaintext_for_edit` API + SuperAdmin 权限 + 审计日志（不在本次 b4f9a560 范围）。
3. ✅ **强制加解密双向一致性测试**：测试 10 组随机 detail Value → encrypt → decrypt → assert_eq!(原, 解密后)；覆盖深嵌套 JSON + Unicode 字符 + 空字符串 + 非常大 10KB JSON（4 类全过）。
4. ✅ **强制新增凭证类型零改动回归**：集成测试中模拟一个 `credential_type = "TEST_DYNAMIC_12345"`（不在 IdentityCredentialType 枚举预设值里但允许扩展存储）→ create 成功 → get 成功 → detail 值完全一致。Domain 层代码无需新增 match arm（如果编译不过说明 Domain 里有按 enum 分支的代码，违反「类型无关」契约）。
5. ✅ **强制 hash 幂等去重测试**：同一 user_id + same type + same detail create 两次 → 第一次 created=true, id=1；第二次 created=false, duplicate=true, id=1（不重复入库，不是 id=2）。
6. ❌ **禁止外部集成 Domain 直接调用 IdentityCredentialDao（跨层）**：飞书 / GitHub / 微信等外部集成 Domain，**只能依赖 IdentityCredentialDomain（领域层）** 的 getter。禁止依赖 IdentityCredentialDal（DAL）或 DAO（跨层调用 §3.1 架构红线）。
7. ✅ **四类互引闭环**：本卡 source_files[] 必含 1 篇 Wiki 长文（身份凭证管理）+ 3 张细卡（模型/Domain/Handler）+ 1 篇 Plan；3 张细卡 source_files[] 尾都追加本总卡相对路径；Wiki 长文 cite 区回链本总卡 + 3 细卡 + Plan。
8. ✅ **硬 fail：DEK 必须每次加密随机生成，不可复用**：encrypt 函数中 `let dek = rand::rng::<[u8;32]>()`；测试 `dek_random_per_call`：连续 encrypt 两次相同 plaintext → DEK 值不同（如果相同说明 rand 没随机，密钥被复用 → 同明文同密文，CPA 安全性打折）。
