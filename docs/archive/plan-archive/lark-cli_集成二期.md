# lark-cli 集成二期：绑定体验 + 用户身份 + WS 稳定性

> 📦 归档标记（2026-08-16）：归档冻结。保留原因：lark-cli_集成二期 功能已完成并通过验收，文档转为历史快照。生效方案：见源码和 wiki 长文。

> **文档状态**：进行中（一期已上线，二期实施计划已冻结）
> 查阅场景：
> - 新接手飞书集成时，快速理解凭证存储从渠道内联 → 用户级 JSON 列的迁移思路
> - 调试 WS 重连或渠道凭证联动问题时，按 §二 架构分层定位 Domain 编排联动链路
> - 新增凭证类型时，参考 §四 速查表确认改动面（DAL/DAO/Handler 三处）
> 关联文档：
> - [ARCHITECTURE.md](../ARCHITECTURE.md) — 唯一权威架构总纲
> - [lark_cli_integration.md](../archive/design-archive/lark_cli_integration.md) — 底层设计决策（§4 二期三项）
> - [身份凭证Domain统一CRUD重构.md](身份凭证Domain统一CRUD重构.md) — 凭证类型统一 CRUD 的后续收敛方案

---

## 一、目标（为什么做）

一期飞书集成已打通基础链路，但在用户体验（渠道 Modal 手填凭证繁琐）、身份模式（无用户级 OAuth 区分 bot/用户身份）、WS 稳定性（断线死连无退避）三方面存在缺口。

| 问题维度 | 解决方式 |
|---------|---------|
| 凭证内联在渠道 ChannelConfig，建多条渠道重复录入 | 凭证升级为**用户级独立实体**（users.identity_credentials JSON 列），渠道只存引用 ID（lark_credential_id） |
| 无法区分应用身份 / 用户 OAuth 身份发送 | 渠道级 `lark_identity_mode` 配置（auto/bot/user），lark-cli `config default-as` 幂等切换 |
| WS 断线不复原 / token 过期死连 | ws.rs 拆 supervisor + run_connection_once，指数退避 1s→60s 抖动 + `WsTokenSource` 刷新过期 token |
| 用户绑定应用流程繁琐（手填 app_id/secret） | `config init --new` 自动化绑定：spawn CLI → 抓取验证 URL → 轮询会话状态 |
| 凭证变更渠道侧不同步（改 app_id 不重建 WS） | Domain 编排联动：凭证 PUT/DELETE → 清 per-user HOME config → WS release_listener → ensure_listener_for（按 §二 架构链路） |

**收敛后效果**：凭证只在用户级绑定一次即可复用到多条渠道；用户 OAuth 设备流完成渠道身份切换；WS 零配置自愈（退避重连 + token 刷新）；凭证变更链路原子化（Domain 编排，DAL 之间零互调）。

---

## 二、架构思路（怎么做的）

凭证存储与联动的核心链路：

```
       前端（Settings「飞书集成」+ 渠道 Modal）
                    │  统一消费聚合端点 GET /user/lark-integration/status
                    ▼
   ┌───────────────────────────────────────────────┐
   │  Handler 层（src/handlers/user/lark_integration/）│
   │  11 端点：status + credentials CRUD ×3 +        │
   │  auth ×4（start/complete/status/logout）+       │
   │  bind ×3（start/status/cancel）                 │
   └───────────────────────┬───────────────────────┘
                           │  永远只调 OrganizationDomain
                           ▼
   ┌───────────────────────────────────────────────┐
   │  Domain 层（domain/organization/user.rs）       │
   │  凭证 CRUD 联动编排（核心价值在此）：            │
   │  ① user DAL：read-modify-write identity_       │
   │     credentials（secret 加密）                  │
   │  ② 返回后同层编排：                              │
   │     - lark DAL 查引用该凭证的渠道                │
   │     - 清 per-user HOME .lark-cli config         │
   │     - WS 重建联：release → ensure               │
   └─────────────┬─────────────────────┬─────────────┘
                 │                     │
   ┌─────────────▼──────┐  ┌──────────▼──────────┐
   │  user DAL          │  │  lark DAL           │
   │  users JSON 读写   │  │  渠道引用解析/查询   │
   └────────────────────┘  └──────────┬──────────┘
                                      │
                    ┌─────────────────┴──────────────────┐
                    │  lark DAO（http.rs + ws.rs）        │
                    │  HTTP token + WS 退避重连           │
                    │  pkg/lark_integration.rs：OAuth     │
                    │  device flow + bind 会话            │
                    └────────────────────────────────────┘

零互调保证：user DAL ⇄ lark DAL 之间无直接调用，
联动只发生在 Domain 层（符合分层契约：DAL 禁同层互调）。
```

**关键边界 / 行为红线（回归必保）**：
1. **凭证单一事实源**：用户级 users.identity_credentials JSON；渠道内联 lark_app_id/secret/encrypt_key/verification_token 字段一律删除，无兼容回退路径
2. **DAL 零互调**：user DAL 与 lark/message_channel DAL 之间永不互调；凭证变更联动**只发生在 Domain 层**编排
3. **删除守护**：凭证删除前 Domain 先查渠道引用，存在报 Conflict；删除成功不联动删 HOME config（保留 token 可复用）
4. **WS supervisor 纯拆分**：`run_connection_once` 单次连接；外层 supervisor 负责退避重连 + 接收 shutdown 信号
5. **安全守卫**：update_current_user handler Agent 上下文时忽略 preferences 字段（见 [用户偏好双源设计.md](用户偏好双源设计.md)）；lark 凭证 secret 永不回显

---

## 三、涉及文件清单（读代码直接跳）

| 文件 | 角色 | 摘要 |
|------|------|------|
| **DB 层** | | |
| migrations/20260812xxxx_users_identity_credentials.sql | 迁移 | ALTER TABLE users ADD COLUMN identity_credentials TEXT（空串默认） |
| **common 层** | | |
| [common/src/models/identity_credentials.rs](common/src/models/identity_credentials.rs) | 凭证模型 | UserIdentityCredentials / CredentialKind / CredentialDetail（LarkApp，加密后落库） |
| **models 层** | | |
| [src/models/user.rs](src/models/user.rs) | UserPo | 增 identity_credentials: String 字段；to_basic_info_prompt 拼偏好行（含凭证） |
| **DAO 层** | | |
| [src/service/dao/user/](src/service/dao/user/) | user DAO | 全部 SQL 补 identity_credentials 列；增 find_identity_credentials_by_user_id/username 便捷查询 |
| [src/service/dao/lark/ws.rs](src/service/dao/lark/ws.rs) | WS 连接 | 拆 run_connection_once + supervisor；指数退避 1s→60s 抖动；WsState 持有 JoinHandle + shutdown_tx；WsConnState 监控字段 |
| [src/service/dao/lark/http.rs](src/service/dao/lark/http.rs) | token 源 | 实现 WsTokenSource trait（get_tenant_access_token），重连时过期自愈 |
| **DAL 层** | | |
| [src/service/dal/lark.rs](src/service/dal/lark.rs) | lark DAL | 凭证读写辅助（read-modify-write JSON + 加密）；查引用指定凭证的渠道；resolve_credentials_for_user 改为引用 ID → JSON 查找 |
| src/service/dal/message.rs（options） | 推送选项 | 增 ChannelPushOptions.user: Option<User>，优先级① options 携带直用 ② 兜底 user dao 查询 |
| **Domain 层** | | |
| [src/service/domain/organization/user.rs](src/service/domain/organization/user.rs) | user Domain | 凭证 CRUD 方法（组合 user DAL + lark DAL，按 §二 联动链路编排）；lark_integration handler 只调此处 |
| finance/message_channel.rs（生命周期） | 渠道域 | 渠道创建/禁用触发的 ensure/release 维持现状（不改动） |
| **pkg 基础设施** | | |
| [src/pkg/lark_integration.rs](src/pkg/lark_integration.rs) | 会话编排 | start_device_login / complete_device_login / auth_status / auth_logout；bind 会话注册表（RwLock\<HashMap\>，单用户 TTL 10min） |
| [src/pkg/tool_registry/lark_cli.rs](src/pkg/tool_registry/lark_cli.rs) | CLI 原语 | lark_home / binary_available / ensure_cli_config 提升为 pub 复用 |
| **Handler 层** | | |
| [src/handlers/user/lark_integration/](src/handlers/user/lark_integration/) | 11 端点 | status/credentials×3/auth×4/bind×3（§一 路由路径表）；挂 user_routes 的 lark-integration 子 nest |
| **Handler 健康指标** | | |
| [src/handlers/system/health_metrics.rs](src/handlers/system/health_metrics.rs) | 健康注入 | 注入 LarkWsMetrics（active_connections / apps[{app_id, state, reconnect_count}]） |
| **Frontend 层** | | |
| [frontend/src/pages/settings.rs](frontend/src/pages/settings.rs) | Settings | 新增「飞书集成」区块：应用绑定卡（凭证列表/编辑/删除/自动绑定）+ 用户身份卡（OAuth/device flow） |
| [frontend/src/pages/finance/message_channels.rs](frontend/src/pages/finance/message_channels.rs) | 渠道 Modal | 选 Lark 时查聚合端点：无凭证 → 引导条跳 Settings；有凭证 → 凭证下拉（必填）；身份模式下拉 |
| [frontend/src/api/lark_integration.rs](frontend/src/api/lark_integration.rs) | API 客户端 | 11 个方法（对应 11 端点） |

---

## 四、分发速查表（新增同类功能第一站）

### 4.1 新增凭证类型（类似 LarkApp → 微信/Slack 凭证）

| 改动点 | 位置 | 新增时参考 |
|--------|------|-----------|
| CredentialKind 枚举加变体 | [common/src/models/identity_credentials.rs](common/src/models/identity_credentials.rs) | CredentialKind::LarkApp 之后按序追加 |
| CredentialDetail 枚举加变体 | 同上 | CredentialDetail::LarkApp { app_id, app_secret(加密), encrypt_key, verification_token } 同款模式 |
| Domain 联动分发 | [src/service/domain/organization/user.rs](src/service/domain/organization/user.rs) | match kind 分发凭证变更后的联动动作（目前仅 lark 有 WS/渠道联动，其余类型默认空分支） |

> 代码入口：[identity_credentials.rs 枚举区](common/src/models/identity_credentials.rs)

### 4.2 新增用户级集成页面（类似 Settings「飞书集成」）

| 改动点 | 位置 | 新增时参考 |
|--------|------|-----------|
| 后端 Handler 目录 | `src/handlers/user/<集成名>/` 每方法一文件 | 参考 `lark_integration/` 11 文件 × N 端点分组模式 |
| 前端 Settings 区块 | `frontend/src/pages/settings.rs` 加卡片区块 | 复用飞书区块：聚合端点加载 → 绑定卡 + 授权卡双布局 |
| 路由挂载 | user_routes() 加子 nest | 统一挂 `/user/<集成名>/` 中间路径（不挂 settings，域归属清晰） |

> 代码入口：[handlers/user/ 根目录](src/handlers/user/)

---

## 五、验收清单

见 Plan 文档对应 Git 提交记录 / 对应执行任务。

---

## 六、执行结果摘要

| 模块 | 验证结果 |
|------|---------|
| 0 步分支实测（config init --new） | 待执行：决定分支 A/B 实施路径 |
| DAO/DAL 单测 | 待编写：users preferences/credentials 读写、解析两路径优先级、联动 mock 调用链 |
| 集成测试 lark_integration_test.rs | 待编写：无 CLI 引导错误契约 + 凭证-渠道引用生命周期 5 场景 |
| 纯函数单测（前端） | 待编写：JSON 解析、表单校验、TTL/单会话约束 |
| Clippy 双端 | 待执行 |
| 质量门槛 | fmt / clippy -D warnings / 全量测试 三关全绿 |

### 与计划的偏离（如有）
1. 假设 CI 无 lark-cli 二进制，集成测试只断言引导错误（非 500、不含 secret 明文），不真跑 CLI
2. keychain headless Linux 兼容性以降级引导兜底，实测留部署阶段

---

## 七、后续扩展路径（4 步模板）

> **核心不变量**：凭证单一事实源（users JSON 列）、DAL 零互调、渠道只存引用。

1. **common 模型扩展**：[identity_credentials.rs](common/src/models/identity_credentials.rs) — 新增凭证类型：CredentialKind 加变体 + CredentialDetail 加带标签的详情结构；加密字段用 encrypt_channel_secret 原语
2. **Domain 联动分发**：[organization/user.rs](src/service/domain/organization/user.rs) — 凭证 PUT/DELETE 后 match kind 追加对应类型的 WS/渠道/HomeConfig 联动（目前仅 lark 分支体完整）
3. **前端 Handler 目录**：复制 [src/handlers/user/lark_integration/](src/handlers/user/lark_integration/) 目录模式，按集成名改 11 端点 × 路由分组
4. **前端 Settings 区块**：复制 [frontend/src/pages/settings.rs](frontend/src/pages/settings.rs) 飞书区块模式，聚合端点加载后渲染绑定卡 + 授权卡；渠道 Modal 加新类型凭证下拉