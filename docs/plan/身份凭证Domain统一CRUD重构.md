# 身份凭证 Domain 层统一 CRUD 重构

> 🎯 **本文档定位**：重构规划 + 落地结果快照（概览级，不包含代码细节；具体实现以代码路径为准）
>
> 文档角色：plan（要去哪 + 完成状态快照），归档后查阅意图：
> - 新增凭证类型时，回看"改动清单 + 扩展模式"两处即可，无需通读全文
> - 若需了解字段级加密/校验细节，直接跳转对应代码文件（见 §涉及文件）
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — 分层架构规范 §3.1 Trait 位置约定
> - [api_protocol_convention.md](../design/api_protocol_convention.md) — 前后端 DTO 契约规范
> - [logging_design.md](../design/logging_design.md) — 日志宏规范（联动 lark WS 移交的告警语义）

---

## 一、重构目标（为什么做）

`trait IdentityCredentialManage` 原设计按凭证类型复制 8 个 CRUD 方法（`create/update/delete/set_default` × lark + github），每新增一种类型（微信/Slack/...）trait 膨胀 4~5 个方法，且两套实现骨架完全同构，类型差异只有三类：

| 差异类型 | 下沉方式 |
|---------|---------|
| (a) detail 结构 + 必填校验 + trim 规范化 | 下沉到 `CredentialDetail` 内部行为（信息专家原则，与 `Vectorizable` 同模式） |
| (b) 敏感字段加密（哪些字段算敏感） | 同上，加密原语以闭包注入（common 不依赖后端 crypto） |
| (c) 生命周期副作用（lark WS 移交 / 渠道引用检查 / github 清登录态） | 保留在 Domain，`match kind` 分发（仅 2 种类型，暂不引入策略模式） |

**收敛后效果**：trait 封顶 5 个类型无关方法（`get + create/update/delete + set_default`），新增类型时 **trait 零改动**。

---

## 二、架构思路（怎么做的）

三层收敛，信息逐层下沉：

```
Handler（8 个类型专属 handler，保持不变）
  │  只改调用方式：构造 Command → 调统一 domain 方法
  ▼
Domain（trait 收敛）
  │  删 8 个 *_lark_* / *_github_* 方法
  │  加 2 个 Command（CreateCredentialCmd / UpdateCredentialCmd）
  │  加 4 个统一方法（create/update/delete/set_default）
  │  副作用（c）→ match kind 分发
  ▼
common 模型（知识下沉）
  ├─ CredentialDetail 获得：kind / primary_id / normalized / validate / encrypt_sensitive / apply_patch
  ├─ 新增：CredentialDetailPatch 枚举 + CredentialUpdateImpact 影响摘要
  └─ UserIdentityCredentials 获得：set_default_for / clear_default_for / default_slot_mut
```

**关键边界（行为红线，回归必保）**：
1. lark update 后：清该用户 HOME 的 lark-cli config + WS 监听移交（`secret_changed` 为 `app_secret` 或 `encrypt_key` 任一实际写入）；失败仅告警，不阻断主流程
2. lark delete 前：被渠道引用报 `Conflict` 拒删
3. github delete 后：删的是当前生效凭证 → 清 HOME 登录态；失败仅告警
4. `verification_token` 更新语义：`Some("")` 清除、`None` 保持、非空覆盖
5. set_default 凭证不存在 → `NotFound`（原 lark `InvalidRequest` / github `NotFound` 统一）

---

## 三、涉及文件（改动清单 → 查代码直接跳）

按 AGENTS.md §3.2 目录结构索引：

| 文件 | 角色 | 变更内容 |
|------|------|---------|
| **common 层（知识下沉）** | | |
| [common/src/models/identity_credentials.rs](../../common/src/models/identity_credentials.rs) | 凭证模型 | 新增 6 个 `CredentialDetail` 行为方法；新增 `CredentialDetailPatch` 枚举 + `CredentialUpdateImpact`；新增 3 个 `UserIdentityCredentials` 默认操作方法；对应单元测试 |
| **Domain 层（接口收敛）** | | |
| [src/service/domain/finance/mod.rs](../../src/service/domain/finance/mod.rs) | trait + Commands 定义 | 删 8 个类型方法；定义 `CreateCredentialCmd` / `UpdateCredentialCmd`；新增 4 个统一 CRUD 方法签名 |
| [src/service/domain/finance/identity_credential.rs](../../src/service/domain/finance/identity_credential.rs) | trait 实现 | 重写 4 个统一 CRUD；`delete` 前置检查 + 后置副作用、`update` 后置联动均经 `match kind` 分发 |
| **Handler 层（迁移调用方式）** | | |
| [src/handlers/finance/lark_integration/create_credential.rs](../../src/handlers/finance/lark_integration/create_credential.rs) | lark 创建 | 改调 `create_credential`，用 `CredentialDetail::LarkApp` 包装 |
| [src/handlers/finance/lark_integration/update_credential.rs](../../src/handlers/finance/lark_integration/update_credential.rs) | lark 更新 | 改调 `update_credential`，用 `CredentialDetailPatch::LarkApp` 包装 |
| [src/handlers/finance/lark_integration/delete_credential.rs](../../src/handlers/finance/lark_integration/delete_credential.rs) | lark 删除 | 改调 `delete_credential` |
| [src/handlers/finance/lark_integration/set_default_credential.rs](../../src/handlers/finance/lark_integration/set_default_credential.rs) | lark 设默认 | 改调 `set_default_credential(ctx, user_id, CredentialKind::LarkApp, id)` |
| [src/handlers/finance/github_integration/create_credential.rs](../../src/handlers/finance/github_integration/create_credential.rs) | github 创建 | 改调 `create_credential`，用 `CredentialDetail::GithubToken` 包装 |
| [src/handlers/finance/github_integration/update_credential.rs](../../src/handlers/finance/github_integration/update_credential.rs) | github 更新 | 改调 `update_credential`，用 `CredentialDetailPatch::GithubToken` 包装 |
| [src/handlers/finance/github_integration/delete_credential.rs](../../src/handlers/finance/github_integration/delete_credential.rs) | github 删除 | 改调 `delete_credential` |
| [src/handlers/finance/github_integration/set_default_credential.rs](../../src/handlers/finance/github_integration/set_default_credential.rs) | github 设默认 | 改调 `set_default_credential(ctx, user_id, CredentialKind::GithubToken, id)` |
| **零改动面（验证架构稳定性）** | | |
| 前端 / API DTO / 路由 / `tests/integration/lark_integration_test.rs` / `tests/integration/github_integration_test.rs` | 对外契约不变 | 无修改；集成测试断言原样通过 |

---

## 四、类型分发速查表（新增凭证类型时改这两处）

新增凭证类型（以 SlackToken 为例）时，改动点在 Domain 层**仅两处 `match` 分支**：

### 4.1 `update_credential` — 后置联动（尾部 `match kind`）

| 现有类型 | 后置动作 | 新增类型时参考 |
|---------|---------|--------------|
| LarkApp | 清 lark-cli HOME config + WS 移交（`secret_changed` 驱动断连重建） | 如需"密钥轮换→强制失效旧连接"走此分支体 |
| GithubToken | 无（gh_cli marker 指纹自动重登录） | 默认空分支即可 |

> 代码入口：[identity_credential.rs :: update_credential 尾段](../../src/service/domain/finance/identity_credential.rs)

### 4.2 `delete_credential` — 前置检查 + 后置副作用（中段 + 尾段）

| 现有类型 | 前置检查 | 后置副作用 |
|---------|---------|-----------|
| LarkApp | 被渠道引用 → `Conflict` 拒删 | 不联动（保留用户授权 token） |
| GithubToken | 快照：判断是否为当前生效凭证 | 是生效凭证 → 清 HOME gh auth |

> 代码入口：[identity_credential.rs :: delete_credential](../../src/service/domain/finance/identity_credential.rs)

---

## 五、验收清单（2026-08-15 全部达成 ✅）

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要（2026-08-15，子代理驱动）

| 模块 | 验证结果 |
|------|---------|
| common 单元测试 | 72 passed；clippy 0 警告 |
| `service::domain::finance` 测试 | 22 passed |
| 后端 lib 全量 | 946 passed / 0 failed |
| 集成测试（lark + github） | 全部 PASS |
| Clippy（后端 + 前端 wasm32） | 双端零错误 |
| 前端 | 82 passed |

### 与计划的 2 处偏离（均为文档精度问题，业务零影响）
1. `common::error::Error` 无 `Internal(String)` 变体，测试辅助改用现成 `Error::internal("test")` 构造器
2. `CredentialDetailPatch` 字段级需补 `///` 注释（common crate 强制 `missing_docs` lint）

---

## 七、后续扩展路径（新增凭证类型 4 步模板）

> **核心不变量**：trait / DTO / 路由机制不动。

1. **common 模型**：[identity_credentials.rs](../../common/src/models/identity_credentials.rs)
   - `CredentialDetail` 加变体（字段定义）
   - `CredentialDetailPatch` 加变体（补丁语义）
   - `CredentialKind` 枚举加值
   - 为新变体实现：`kind()` / `primary_id()` / `normalized()` / `validate()` / `encrypt_sensitive()`（3 个敏感字段以内直接抄现有分支）、`apply_patch()` 对应 arm
2. **domain 分发**：[identity_credential.rs](../../src/service/domain/finance/identity_credential.rs)
   - §4.1 `update_credential` 尾段加 `match kind` 分支（后置联动）
   - §4.2 `delete_credential` 加前置检查 + 后置副作用分支
3. **handler 目录**：复制 `src/handlers/finance/github_integration/`（5 文件模板）改字段名
4. **前端**：api + 区块组件，复制参考 [frontend/src/pages/finance/identity_github.rs](../../frontend/src/pages/finance/identity_github.rs)

完成。

