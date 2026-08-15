---
kind: wiki_knowledge_card
name: 身份凭证 Handler 八文件迁移：DTO 零改动 + CreateCredentialCmd 构造器 + 统一调用方式
category: finance适配器层（Handler）
scope:
  - "src/handlers/finance/lark_integration/create_credential.rs"
  - "src/handlers/finance/lark_integration/update_credential.rs"
  - "src/handlers/finance/lark_integration/delete_credential.rs"
  - "src/handlers/finance/lark_integration/set_default_credential.rs"
  - "src/handlers/finance/github_integration/create_credential.rs"
  - "src/handlers/finance/github_integration/update_credential.rs"
  - "src/handlers/finance/github_integration/delete_credential.rs"
  - "src/handlers/finance/github_integration/set_default_credential.rs"
source_files:
  - src/handlers/finance/lark_integration/create_credential.rs:Ln-Lm
  - src/handlers/finance/lark_integration/update_credential.rs:Ln-Lm
  - src/handlers/finance/lark_integration/delete_credential.rs:Ln-Lm
  - src/handlers/finance/lark_integration/set_default_credential.rs:Ln-Lm
  - src/handlers/finance/github_integration/create_credential.rs:Ln-Lm
  - src/handlers/finance/github_integration/update_credential.rs:Ln-Lm
  - src/handlers/finance/github_integration/delete_credential.rs:Ln-Lm
  - src/handlers/finance/github_integration/set_default_credential.rs:Ln-Lm
  - common/src/api/finance_credential.rs:Ln-Lm（若命名不同则 common/src/api/ 下对应凭证模块）
  - src/service/domain/finance/mod.rs:Ln-Lm（CreateCredentialCmd / UpdateCredentialCmd 定义）
  - docs/design/api_protocol_convention.md
  - docs/plan/身份凭证Domain统一CRUD重构.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/飞书集成系统.md
---

# 身份凭证 Handler 八文件迁移

## §1 整体方案
重构前每个凭证类型独立一套方法名（create_lark_credential / create_github_credential），Handler 直接调对应 domain 方法。重构后 Handler 保持 8 个独立文件的组织形式不变（用户视角不变——lark 集成页的凭证管理接口路由不变；前端零改动），只**内部更换调用方式：构造通用 Command → 调统一 create_credential / update_credential / delete_credential / set_default_credential(ctx, user_id, kind, id_opt)**。

**零改动面（对外契约不变验证）**：前端/API DTO/路由/lark + github 集成测试 0 修改。Handler 迁移仅涉及 3 类代码改写：
(a) **Create Handler**：RequestBody（LarkCredentialCreateRequest / GithubCredentialCreateRequest）→ 实例化 CredentialDetail::LarkApp { ... 字段从 request 拷贝 } 或 CredentialDetail::GithubToken { ... } → 组装 `CreateCredentialCmd { name: req.name, detail }` → `finance_domain.identity_credential_manage().create_credential(ctx, user_id, cmd).await` → 返回 ApiResponse<{ credential_id }>（原返回结构不变）。
(b) **Update Handler**：RequestBody → 组装 CredentialDetailPatch::LarkApp { 每个 request 字段 wrap 成三态 Option(Option<String>)：None=保持；Some("")=清除；Some(val)=覆盖 } → 构造 `UpdateCredentialCmd { credential_id, name: req.name.map(Into::into), patch }` → 调统一 update_credential；返回 ApiResponse<UpdateCredentialResponse { success, secret_changed: bool }>（**新增响应字段 secret_changed 向前兼容：前端不显示即忽略**）。
(c) **Delete Handler**：原调 `delete_lark_credential(ctx, user_id, id)` → 统一改成 `delete_credential(ctx, user_id, credential_id)`（domain 内部 match kind 自动做前置检查+副作用）；返回原 Delete 响应。SetDefault Handler：原调 `set_default_lark_credential(ctx, user_id, Some(id)) / None` → 统一改成 `set_default_credential(ctx, user_id, CredentialKind::LarkApp, credential_id_opt)`（domain 调模型 set_default_for 统一校验）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键改写内容 |
|------|------|------------|
| [lark_integration/create_credential.rs](src/handlers/finance/lark_integration/create_credential.rs) | lark 创建 Handler | 构造 CredentialDetail::LarkApp { app_id, app_secret 等字段从 req } → CreateCredentialCmd → 调 create_credential；响应不变 |
| [lark_integration/update_credential.rs](src/handlers/finance/lark_integration/update_credential.rs) | lark 更新 Handler | 构造 CredentialDetailPatch::LarkApp { app_id: req.app_id.map(三态)，app_secret: req.app_secret.map(三态)，... } → UpdateCredentialCmd → 调 update_credential；响应新增 secret_changed 字段（向前兼容）|
| [lark_integration/delete_credential.rs](src/handlers/finance/lark_integration/delete_credential.rs) | lark 删除 Handler | 直接调统一 delete_credential（domain 内部做渠道引用 Conflict 拒删）|
| [lark_integration/set_default_credential.rs](src/handlers/finance/lark_integration/set_default_credential.rs) | lark 设默认 Handler | 统一调 set_default_credential(ctx, user_id, CredentialKind::LarkApp, credential_id_opt) |
| [github_integration/*_credential.rs](src/handlers/finance/github_integration/create_credential.rs)（4 个文件模式完全一致）| github CRUD Handler | CredentialDetail::GithubToken / CredentialDetailPatch::GithubToken / CredentialKind::GithubToken 对应 |
| common/src/api/finance_credential.rs（或 common/src/api/finance.rs）| DTO（前后端共用，Handler Request/Response）| LarkCredentialCreateRequest / GithubCredentialUpdateRequest 等结构体**零字段改动**；仅 Update Response 新增可选 `secret_changed: bool` 字段（向前兼容）|
| src/service/domain/finance/mod.rs Commands | CreateCredentialCmd / UpdateCredentialCmd | Handler → Domain 的中间表达 |
| 【对应 Wiki 长文 1】身份凭证管理.md | 系统化上下文 §5 Handler 迁移小节 | /Users/aman/Technology/rust/ai_orz/docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md |
| 【对应 Wiki 长文 2】飞书集成系统.md | 飞书凭证 Handler 字段细节 + WS 移交 | /Users/aman/Technology/rust/ai_orz/docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/飞书集成系统.md |
| 【② Plan 定稿】§三 涉及文件 Handler 8 个迁移表 | 每个 Handler 改写摘要 | docs/plan/身份凭证Domain统一CRUD重构.md |
| 【① Design】api_protocol_convention.md | Handler 拆分规范（每方法一文件）/ DTO 定义位置（common）/ 禁止裸响应 / 结构体化请求参数 | docs/design/api_protocol_convention.md |

## §3 架构约定

1. **Handler 只做三件事（严格分层 AGENTS §3.1）**：① 鉴权（确保操作人是凭证所属 user_id 本人或 SuperAdmin；禁止跨 userId 操作他人凭证）→ ② 参数提取 + RequestBody → CredentialDetail / Patch / Command 转换（纯字段搬运无业务判断）→ ③ 调 Domain 统一方法 + 映射 ApiResponse 返回。**禁止 Handler 内写业务逻辑（如渠道引用检查/默认槽位判断/加密/校验）**——这些必须下沉到 Domain/模型层。
2. **Update 三态补丁构造严格规则**：前端 update 表单字段为「已清空」时，Body 传该字段 = ""（空串）→ Handler 侧包装成 `Some("")` → Patch 三态语义 = 清除字段；前端未传字段 = None → 保持原值；前端传了非空 = `Some(val)` → 覆盖。任何 Handler 不得把「空串」处理成 None（会导致「用户想清除 verification_token 字段」无法生效；这在 CredentialDetailPatch arm 中就是 3 态）。
3. **凭证归属校验（userId 匹配）在 Handler 层做，Domain 不再重复做**：Handler 路径参数从 JWT ctx 拿到 uid = 当前登录用户，凭证 userId = 路径 uid，两者不一致直接 403；Domain 层接收 user_id 视为已经鉴权通过，不重复查当前 uid（减少重复逻辑）。
4. **新增凭证类型 Handler 目录复制 github 目录（5 文件模板），绝不复制粘贴写死字段**：新增 SlackToken 类型 → 复制 `handlers/finance/github_integration/` 整个目录到 `handlers/finance/slack_integration/` → 重命名 5 个文件名 → 全局替换 `GithubToken` → `SlackToken`、`github` → `slack`、对应字段名。Handler 内部调用方式保持 create_credential / update_credential / delete_credential / set_default_credential 4 统一方法不变。
5. **响应禁止裸 bool/空 tuple**：所有 Delete/SetDefault 操作返回 `{ success: bool }` 结构体（即使只有 1 字段），Update 返回 `{ success: bool, secret_changed: bool }`，Create 返回 `{ credential_id: String }`——符合 AGENTS §4.11「禁止裸原始类型响应」规范。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 Handler 层出现 `encrypt_channel_secret` / 任何加密调用**：加密是 Domain 层在 validate 通过后统一调用。Handler 必须传明文 request 字段。原因：如果 Handler 先加密，validate()（要求明文长度/前缀格式校验）无法工作——对 app_id 校验前缀「cli_」时，密文是乱码 base64，前缀校验永远失败。
2. ❌ **禁止 Handler 层调用 resolve_lark_credential_ref / 任何凭证库读校验**：Handler 只负责把 request 搬成 Command，具体业务判断 Domain/DAL/模型 各自负责。Handler 层重复做「存在+类型匹配」会直接造成校验文案漂移。
3. ❌ **禁止合并 8 个 Handler 成「统一通用 create_credential Handler 带 kind 路径参数」**：保持 8 个 Handler 分目录（lark_integration/、github_integration/、新增 slack_integration/）的组织形式，因为：① 每个凭证类型的 Request 字段结构完全不同（Lark vs Github 字段数不同，新增 Slack 字段又不同），强行统一会变成一大坨字段可选 HashMap；② 路由权限可按目录级做（例如未来接入企业微信凭证时独立的审计日志）。保持 Handler 目录颗粒度 = 凭证类型颗粒度，新增类型复制目录即可。
4. ✅ **DTO 响应向前兼容强约束**：Update 响应新增 `secret_changed` 字段必须 `#[serde(default)]` + `Option<bool>`（老前端不传字段就忽略，不反序列化失败）；**禁止删字段、禁止重命名已有字段**。API DTO 任何字段变动必须走 common crate 的 Request/Response，且对应集成测试必须通过（Plan §验收清单 5：集成测试零改动，确保契约不变）。
5. ✅ **请求参数结构体化**：Create/Update 请求 Body 必须有对应 Request 结构体定义在 common/src/api/ 下，禁止 Handler 签名直接 `Json<HashMap<String, Value>>` 裸提取；path/query 参数同样结构体化（如 `Path { user_id: String, credential_id: String }`），符合 AGENTS §4.11 规范。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 1 wiki 长文（身份凭证管理.md）+ 飞书集成系统长文引用路径 + Design API 协议 + Plan（真实定稿）；对应 Wiki 长文 cite 段回链本卡 + Design + Plan。
