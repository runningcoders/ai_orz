---
kind: wiki_knowledge_card
name: 身份凭证 Domain 统一 CRUD：5 类型无关方法 + 2 Command + match kind 分发生命周期副作用
category: finance领域层
scope:
  - "src/service/domain/finance/identity_credential.rs"
  - "src/service/domain/finance/mod.rs"
  - "src/service/dal/user.rs"
  - "src/service/dal/lark.rs"
  - "src/service/dal/message_channel.rs"
source_files:
  - src/service/domain/finance/mod.rs:Ln-Lm（IdentityCredentialManage trait + Commands）
  - src/service/domain/finance/identity_credential.rs:Ln-Lm（统一 CRUD 实现）
  - src/service/dal/user.rs:Ln-Lm（save_identity_credentials 乐观锁并发）
  - src/service/dal/lark.rs:Ln-Lm（find_channels_by_credential_id + handover_listeners）
  - src/service/dal/message_channel.rs:Ln-Lm（渠道引用检查）
  - src/pkg/tool_registry/lark_cli.rs:Ln-Lm（clear_cli_config）
  - src/pkg/tool_registry/gh_cli.rs:Ln-Lm（clear_gh_auth）
  - docs/archive/design-archive/message_channel_design.md
  - docs/archive/design-archive/lark_cli_integration.md
  - docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md
  - docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一 Domain CRUD 加密存储与生命周期联动）.md
  - docs/wiki/knowledge/zh/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立/身份凭证模型层信息下沉：CredentialDetail 行为 + CredentialDetailPatch 补丁语义 + 默认槽位独立.md
---

# 身份凭证 Domain 统一 CRUD

## §1 整体方案
重构前 IdentityCredentialManage trait 按凭证类型膨胀：8 个方法（create_lark_credential/update_lark_credential/delete_lark_credential/set_default_lark + github 同 4 方法），每新增类型 +4~5 方法，实现骨架完全同构。**重构收敛后 trait 封顶 5 个类型无关方法**：`get_identity_credentials(ctx, user_id) -> Option<UserIdentityCredentials>`（读）+ `create_credential(ctx, user_id, CreateCredentialCmd) -> credential_id`（创建）+ `update_credential(ctx, user_id, UpdateCredentialCmd)`（更新补丁）+ `delete_credential(ctx, user_id, credential_id)`（删除）+ `set_default_credential(ctx, user_id, kind, Option<credential_id>)`（设默认）。新增凭证类型时，**trait 零改动**（仅 common 扩 2 个变体 + domain 两处 match 分支）。

两种 Command（Domain 输入，表达业务意图）：
- `CreateCredentialCmd { name: String, detail: CredentialDetail }`：明文 detail → Domain 内部自动 normalized() → validate() → encrypt_sensitive(encrypt_channel_secret 闭包注入) → 生成 id + created_at/updated_at 落库。
- `UpdateCredentialCmd { credential_id, name: Option<String>, patch: CredentialDetailPatch }`：补丁语义；name 非空才覆盖；detail.apply_patch(patch, encrypt_fn) → 返回 impact.secret_changed 驱动 WS 移交。

类型差异的生命周期副作用**仅通过 `match credential.kind` 分发**两处，严格禁止用策略模式/模板模式过度抽象：
(a) `update_credential` 尾段后置联动：LarkApp → `secret_changed=true` 时 清 HOME lark-cli config + 调用 lark_dal.handover_listeners_after_credential_change(old_id, new_id, secret_changed)（WS 断连重建/移交）；GithubToken → 无（gh_cli marker 指纹机制自动重登录）。**失败仅告警，不阻断主流程**（凭证已成功保存，联动失败只写 log_warn! 不返回 Err）。
(b) `delete_credential` 前置检查 + 后置副作用：LarkApp → **前置检查** `lark_dal.find_channels_by_credential_id(id)` 渠道引用数 > 0 → 报 Conflict 拒删；GithubToken → **前置快照** 判断删除的凭证是否为当前 resolve_github_credential() 指向的活动凭证 = github_was_active；**删除完成后** → 清除对应默认槽位（clear_default_for）→ GithubToken && github_was_active → 清 HOME gh auth（同样失败只告警）。

## §2 关键文件路径表格（读代码直接跳）

| 文件 | 角色 | 关键结构/入口 |
|------|------|-------------|
| [src/service/domain/finance/mod.rs](src/service/domain/finance/mod.rs) | Finance 总 trait 聚合 + IdentityCredentialManage trait 定义 + 2 Commands | `IdentityCredentialManage`（5 个方法签名：get_identity_credentials / create_credential / update_credential / delete_credential / set_default_credential）；`CreateCredentialCmd { name, detail }`、`UpdateCredentialCmd { credential_id, name, patch }`；FinanceDomain.identity_credential_manage() 暴露能力 |
| [src/service/domain/finance/identity_credential.rs](src/service/domain/finance/identity_credential.rs) | IdentityCredentialManage for FinanceDomainImpl 的唯一实现文件 | create_credential（流程：load_library → normalize → validate → encrypt_sensitive → push id/时间戳 → save）；update_credential（流程：查 → patch 应用 → 保存 → match kind 后置联动）；delete_credential（流程：match kind 前置检查 → 删除 + 清默认 → 保存 + 后置副作用）；set_default_credential（直接调用模型 set_default_for）；load_credential_library 辅助（用户不存在 NotFound / 空凭证返回空库）|
| [src/service/dal/user.rs](src/service/dal/user.rs) | UserDal：get_identity_credentials / save_identity_credentials（乐观锁）| save_identity_credentials 使用 UPDATE ... WHERE version=$version 乐观锁，并发冲突重试最多 3 次（防止同一用户同时编辑凭证导致后写覆盖前写）|
| [src/service/dal/lark.rs](src/service/dal/lark.rs) | LarkChannelDal：find_channels_by_credential_id / handover_listeners_after_credential_change | find_channels_by_credential_id：查「LarkApp 凭证被哪些 MessageChannel 引用」（前置检查）；handover_listeners_after_credential_change(old_app_id, new_app_id, secret_changed)：凭证变更后 WS 监听移交（断连重建）|
| [src/pkg/tool_registry/lark_cli.rs](src/pkg/tool_registry/lark_cli.rs) | clear_cli_config(home)：清 HOME 下 lark-cli config | 凭证 update（secret_changed=true）后触发；失败 log_warn 不阻断 |
| [src/pkg/tool_registry/gh_cli.rs](src/pkg/tool_registry/gh_cli.rs) | clear_gh_auth(home)：清 HOME gh 登录态 | 删除的 GithubToken 是当前活动凭证时触发；失败 log_warn 不阻断 |
| 【对应 Wiki 长文】身份凭证管理.md | 系统化上下文 §5 Domain 统一 CRUD 小节 | [身份凭证管理](docs/wiki/zh/content/核心模块/服务层/领域层/财务领域/身份凭证管理（统一%20Domain%20CRUD%20加密存储与生命周期联动）.md) |
| 【② Plan 定稿】§四 类型分发速查表 + §七 4 步模板 | 新增凭证类型 domain 2 处 match 扩展模板 | docs/archive/plan-archive/身份凭证Domain统一CRUD重构.md |
| 【① Design 1】message_channel_design.md §4.1 渠道引用拒删 | Conflict 错误来源 | docs/archive/design-archive/message_channel_design.md |
| 【① Design 2】lark_cli_integration.md §四 WS 移交 + 清 HOME config | 后置联动设计动机 | docs/archive/design-archive/lark_cli_integration.md |
| 【平行卡】模型层信息下沉（6 行为方法 + 默认槽位）| 模型层基础能力定义 | docs/wiki/knowledge/zh/身份凭证模型层信息下沉：CredentialDetail%20行为%20+%20CredentialDetailPatch%20补丁语义%20+%20默认槽位独立/身份凭证模型层信息下沉：CredentialDetail%20行为%20+%20CredentialDetailPatch%20补丁语义%20+%20默认槽位独立.md |

## §3 架构约定

1. **「副作用失败仅告警，不阻断主流程」原则（软耦合）**：所有后置联动（清 HOME cli config / WS 移交 / 清 gh auth）都不是凭证主流程的一部分——凭证落库成功即视为整体成功，联动失败只 log_warn! 不返回 Err。原因：联动失败可手动补救（重新登录 / 重启 WS 监听），但凭证本身已成功保存，不能因联动失败回滚（否则造成「凭证实际存在前端显示失败/重复创建」）。
2. **match kind 分发固定位置（新增凭证类型时 grep 精准定位）**：update 后置联动必须在 `update_credential` **最后一行（save_identity_credentials 成功之后，return Ok(()) 之前）** 的 `match credential.kind { ... }` 单一 match 块；delete 前置检查必须在 `delete_credential` **load 之后、remove_by_id 之前** 的 `match credential.kind { ... }` 单一 match 块；delete 后置副作用必须在 `save_identity_credentials 成功之后` 的单独 match。**不得在 2 个不同位置写同类型 match**（否则新增类型时容易漏掉一处）。
3. **并发控制：乐观锁在 DAL 层兜底，Domain 不做显式加锁**：同一用户并发写凭证库（前端多 Tab 同时更新）时 DAL 层 version 乐观锁自动重试最多 3 次，重试耗尽返回 Err（前端 toast 提示「操作冲突请重试」）。Domain 层不需要也禁止使用 Mutex 锁用户级凭证库（避免全局锁争用）。
4. **Commands 不带任何敏感字段加密**：CreateCredentialCmd.detail / UpdateCredentialCmd.patch 必须传入**明文值**（因为加密统一由 Domain 层在 validate 通过后执行）。禁止前端/Handler 先加密再传入——会导致 validate() 无法校验字段格式（加密后是密文，长度/前缀校验失败）。
5. **set_default_credential 语义：None = 清除，空串 = 清除，非空 = 设为默认（校验存在+类型匹配）**：set_default_credential(kind, None) 对应模型层 clear_default_for；set_default_credential(kind, Some("")) 等价 None；set_default_credential(kind, Some("id")) 对应模型层 set_default_for(kind, Some("id"))——不存在 → NotFound；类型不匹配 → InvalidRequest。

## §4 约束清单（最高权重，硬红线）

1. ❌ **禁止 Domain 直接操作 CredentialDetail 字段**：Domain 只能通过模型层的 6 个行为方法（normalized/validate/encrypt_sensitive/apply_patch/kind/primary_id）+ 3 个默认槽位方法操作凭证。禁止出现 `if let CredentialDetail::LarkApp { app_secret } = &mut detail { app_secret.clear() }` 这类字段级硬编码——因为新增凭证类型时这类分支会漏写，与「信息下沉」重构初衷相违背。
2. ❌ **禁止新增凭证类型时拆分成独立方法（不允许引入 create_slack_credential 这种命名）**：必须复用 5 个统一方法；类型差异**只允许通过 match kind 分支**表达。任何新增独立方法都会破坏「trait 封顶 5 方法」的重构目标，造成 trait 再次膨胀回到原设计。
3. ✅ **前置检查/后置副作用的 3 条固定顺序（违反 = 业务 bug）**：
   - **删除流程顺序**：① load_credential_library → ② find_by_id 查目标凭证 + clone（前置检查需要 kind 信息）→ ③ **前置检查 match kind（Lark 渠道引用检查 / Github 活动凭证快照）** → ④ remove_by_id 删除本凭证 + clear_default_for 联动清默认 → ⑤ save_identity_credentials 落库 → ⑥ **后置副作用 match kind（Github 活动凭证清 gh auth）**。顺序错误：步骤 ③ 和 ④ 颠倒 → remove 之后 clone 出来的 kind 还在，但 resolve_github_credential 已经找不到删除凭证 → github_was_active 永远 false → 无法正确触发清登录态。
   - **更新流程顺序**：① load → ② find_by_id_mut 获取可变引用 → ③ apply_patch（返回 impact.secret_changed + 更新字段）→ ④ updated_at 刷新 → ⑤ save → ⑥ **后置联动 match kind（secret_changed=true 时清 cli config + WS 移交）**。顺序错误：⑥ 在 ⑤ 之前调用 → 保存失败但 WS 已经移交 → 凭证未变但旧 WS 已经断连 → 飞书消息断流。
4. ✅ **set_default/delete 的默认槽位联动强绑定**：
   - delete 成功后必须无条件调用 `library.clear_default_for(credential.kind, credential_id)`（如果被删凭证恰是该类型默认，自动清除默认槽位；防止出现「默认凭证 ID 指向不存在凭证」→ resolve 返回 None → 渠道创建 InvalidRequest）。
   - set_default_credential 内部必须**完全委托模型层 set_default_for**（模型层已经做了「存在+类型匹配」校验），禁止 Domain 层复制一套匹配逻辑写死。
5. ✅ **新增凭证类型 4 处必须同步**：(1) common CredentialDetail + CredentialDetailPatch 变体（含 6 行为方法 arm）→ (2) CredentialKind 枚举 + default_slot_mut 新字段 → (3) identity_credential.rs update 后置联动 match 加 arm → (4) identity_credential.rs delete 前置检查 + 后置副作用 match 加 arm。4 处不同步 = 编译错误（若模型缺变体）或运行时业务 bug（若 match arm 缺 arm，副作用永不触发）。
6. ✅ **四类互引闭环**：本卡 source_files[] 含 wiki 长文绝对路径 1 + Design 2 + Plan 1（真实定稿 plan）+ 平行卡 1；Wiki 长文 cite 段回链本卡 + Design + Plan。
