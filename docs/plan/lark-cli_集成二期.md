# lark-cli 集成二期：绑定体验 + 用户身份 + WS 稳定性

## Summary

按设计文档 `docs/design/lark_cli_integration.md` §4 二期三项展开：
1. 前端「飞书集成」绑定区块 + `config init --new` 自动化建应用流程（§3.8）
2. 用户 OAuth token（device flow）接入 + 渠道级身份模式选择（auto/bot/user）
3. WS 断线指数退避重连 + 连接数/重连监控（挂入 health metrics）

已确认决策：范围=全部三项；工具身份=渠道配置可选（落 `config default-as`，默认 auto）；keychain 风险=失败降级 + 引导（不阻塞主流程）。

## 零、绑定关系数据模型（核心认知，用户反馈二次重构）

**凭证存 users 表 JSON 列（不新增表），渠道只存引用 ID，内联凭证字段直接删除**：

| 信息 | 存储位置 | 维度 | 说明 |
|------|----------|------|------|
| 应用凭证（app_id/secret） | **users 表新增 `identity_credentials` JSON 列**，类型化结构体约束（凭证类型/详情/关键 ID），secret 加密 | 用户级 | 绑定关系唯一事实源；Settings 绑定页管理 |
| 渠道 ↔ 凭证 | `ChannelConfig.lark_credential_id` 引用（config JSON 内，不加表列） | 渠道级 | 飞书共用凭证只留引用；**内联凭证字段删除，无兼容回退路径**（测试阶段，不做数据迁移） |
| 渠道保留的非凭证配置 | ChannelConfig | 渠道级 | `lark_open_id / lark_user_name / lark_listen_inbound`（订阅/路由行为）保留；wechat/email/slack 等渠道自有凭证维持现状（未来出现渠道级凭证需求时再在 channel 内维护） |
| 用户 OAuth token | lark-cli per-user HOME 自管（自动刷新） | 用户维度（文件系统） | 不落 DB；后端经 `auth status --json` 实时查状态 |
| 绑定过程会话（bind session） | pkg 内存态（瞬态） | — | 完成即消亡，产物落 users 表 JSON |

**凭证解析**：渠道 `lark_credential_id` → users 表 JSON 中按 ID 查找（校验 kind=LarkApp）；无引用/查无 → 引导错误。

**凭证变更联动（新增要求，Domain 编排、DAL 不互调）**：用户修改/替换凭证的完整链路从 Domain 开始往下走：

```
Handler（lark_integration credentials PUT/DELETE）
  → OrganizationDomain::user（domain/organization/user.rs 扩展）
      ① user DAL：read-modify-write users.identity_credentials（secret 加密）
      ② 返回后同层编排联动：lark DAL 查引用该凭证的渠道
         → 清该用户 per-user HOME 的 .lark-cli config（下次 lark_cli 执行重建）
         → WS 重建联：release_listener_if_unused(old_app_id) → ensure_listener_for(new_app_id)
           （app_id 变化时旧连接释放；复用 finance/message_channel.rs 同款联动函数）
```

- user DAL 与 lark/message_channel DAL 之间零互调，联动编排只发生在 Domain 层
- 凭证删除：Domain 先查引用，有渠道引用报 Conflict；删成功不联动删 HOME config（token 保留）
- 渠道侧启停联动（渠道创建/禁用触发的 ensure/release）维持现状在 FinanceDomainImpl，不变

**前端获取绑定关系的唯一途径 = 聚合端点**（新增）：

- `GET /api/v1/user/lark-integration/status`：返回当前用户的绑定快照 `{ credentials: [{ credential_id, name, app_id, channels: [{ channel_id, channel_name, status }] }], user_auth: { logged_in, user_name?, degraded_hint? } }`——credentials 读 users 表 JSON（secret 恒不回显）+ 反查引用渠道，user_auth 现场执行 `auth status --json`
- Settings 页「飞书集成」区块加载即调此端点渲染；不用 localStorage 缓存绑定态

### 路由路径约定（用户反馈确认）

后端路由按**业务域**组织（现有 nest：/hr /finance /organization /project /user），不按前端页面组织。飞书集成属用户个人维度配置 → **统一挂 `/user/lark-integration/` 中间路径**（不用 settings，settings 是前端 UI 概念）：

```
/api/v1/user/lark-integration/status              GET    绑定快照聚合
/api/v1/user/lark-integration/credentials         POST   手动录入创建凭证
/api/v1/user/lark-integration/credentials/{id}    PUT    更新凭证（触发渠道联动）
/api/v1/user/lark-integration/credentials/{id}    DELETE 删除凭证（有引用报 Conflict）
/api/v1/user/lark-integration/auth/start          POST   device flow 发起
/api/v1/user/lark-integration/auth/complete       POST   device code 完成
/api/v1/user/lark-integration/auth/status         GET    用户授权状态
/api/v1/user/lark-integration/auth/logout         POST   取消授权
/api/v1/user/lark-integration/bind/start          POST   config init --new 发起
/api/v1/user/lark-integration/bind/status         GET    绑定会话轮询
/api/v1/user/lark-integration/bind/cancel         POST   取消绑定会话
```

Handler 目录同构：`src/handlers/user/lark_integration/`（每方法一文件），挂入现有 `user_routes()` nest 下的 `lark-integration` 子 nest。

### 页面分工与引导链路（用户反馈确认）

**结论：渠道与集成分开两页（实例级 vs 身份级），不合并；靠三处打通体验：**

| 页面 | 职责 | 维度 |
|------|------|------|
| 消息渠道页（/finance/message-channels） | 实例管理：创建/启停/删除通道、绑 Agent、监听开关 | 渠道级（一个凭证可建多条） |
| Settings「飞书集成」区块 | 身份管理：凭证绑定、用户授权、身份模式 | 用户级（凭证/授权生命周期独立于渠道） |

不合并的理由：渠道类型已有 5 种，塞完整绑定+OAuth 流程渠道 Modal 爆炸；删渠道不应丢绑定、删凭证不应连带删渠道；后端域归属也不同（渠道 finance / 集成 user，与路由约定自洽）。

1. **创建引导（主入口）**：渠道创建 Modal 选 Lark 时 → 查聚合端点：无凭证 → 顶部引导条「前往飞书集成绑定应用」；有凭证 → 下拉选择已有凭证（传 `lark_credential_id`，飞书渠道建渠道必须引用凭证，不再支持手填内联凭证）
2. **生命周期约束**：删渠道不动凭证；删/改凭证按 §零 联动规则；删凭证不联动删 HOME config（token 保留，下次绑定可复用授权）
3. **互为详情**：渠道详情展示集成状态卡（引用凭证名 + 用户授权徽标 + 身份模式 + 跳 Settings）；Settings 凭证卡展示关联渠道列表（跳渠道详情）——均消费聚合端点，不做独立详情页

## 一、用户级凭证实体（users 表 JSON 列）

### 表与迁移

新 migration `20260812xxxx_users_identity_credentials.sql`（对齐 preferences 列惯例，STRICT 表）：

```sql
ALTER TABLE users ADD COLUMN identity_credentials TEXT NOT NULL DEFAULT '';
```

空串表示无凭证；非空为 JSON。不加表级 lark_credential_id 列（引用存 ChannelConfig JSON）。

### 类型化结构体（common，前后端共享）

```rust
/// 用户身份凭证库（users.identity_credentials JSON 列）
pub struct UserIdentityCredentials { pub items: Vec<UserIdentityCredential> }

pub struct UserIdentityCredential {
    pub id: String,                     // 凭证关键 ID（渠道引用键，uuid）
    pub kind: CredentialKind,           // 枚举约束凭证类型，可扩展
    pub name: String,                   // 用户自命名
    pub created_at: String, pub updated_at: String,
    pub detail: CredentialDetail,       // 按 kind 区分的详情（serde tag）
}

pub enum CredentialKind { LarkApp }    // 后续扩展（如 WechatApp）

pub enum CredentialDetail {
    LarkApp {
        app_id: String,
        app_secret: String,             // 落库前经 encrypt_channel_secret 加密
        encrypt_key: Option<String>,    // 同样加密存储
        verification_token: Option<String>,
    },
}
```

### 分层实现

- **models/DAO**：`UserPo` 增 `identity_credentials: String` 字段（User 实体自身持有）；[user dao](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/user/mod.rs) 全部 SQL 补列（insert/select/update 对齐 preferences 模式），并新增凭证便捷查询方法 `find_identity_credentials_by_user_id` / `find_identity_credentials_by_username`（返回解析后的 `UserIdentityCredentials`，供消息链路兜底直查）
- **推送链路凭证解析（options 附带 + DAO 兜底双路径，用户反馈确认）**：
  - 新增 `ChannelPushOptions { user: Option<User> }`（项目既有 options 参数模式），沿 deliver 入口 → `push_to_channel` → `LarkDao::push` 传透（trait 签名增参，其余渠道 DAO 接收但不消费）
  - 解析优先级：① options.user 已携带 → 直接用其 identity_credentials 按 `lark_credential_id` 查找（上层已加载 User 时免重复查库）；② options 无 → 兜底经 user dao `find_identity_credentials_by_user_id(channel.user_id)` 查询
  - 两个方向不冲突：仅消息链路取必要字段走 options/兜底；其他业务需要完整用户信息时照常加载 User 实体并顺带下传
- **DAL**：`src/service/dal/lark.rs` 新增凭证读写辅助（依赖 user dao，read-modify-write JSON；创建/更新时 secret 走一期 crypto 加密）与「查引用指定凭证的渠道」方法（供 Domain 联动编排）；`resolve_credentials_for_user` 改为「渠道引用 ID → users 表 JSON 查找」，删除内联回退分支
- **Domain 编排**：[domain/organization/user.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/domain/organization/user.rs) 扩展凭证 CRUD 方法（组合 user DAL + lark DAL，按 §零 联动链路编排）；lark_integration handler 只调 OrganizationDomain，不直调 DAL
- **渠道模型清理（删字段）**：`ChannelConfig` 与 common 四处 DTO 删除 `lark_app_id / lark_app_secret / lark_encrypt_key / lark_verification_token`；新增 `lark_credential_id: Option<String>` + `lark_identity_mode: Option<String>`；保留 `lark_open_id / lark_user_name / lark_listen_inbound`。存量 config_json 旧键随 serde 忽略未知字段自然失效（测试阶段不做数据迁移）
- **消费点同步改造**：dao/lark 的 `channel_credentials`/`validate_config`、dal/lark.rs、domain/finance/message_channel.rs 生命周期触发点、create/update handler（`validate_lark_credentials` 改为校验 `lark_credential_id` 存在且归属当前用户且 kind=LarkApp）、response.rs、message_channel_lifecycle_test 及前端表单
- **WS 联动取值**：同 app_id 去重建连逻辑不变，仅凭证来源改为引用解析

### 身份模式（渠道级配置）

- `ChannelConfig.lark_identity_mode: Option<String>`（auto/bot/user，缺省 auto）：common DTO（Create/Update 请求 + Response）、models ChannelConfig、create/update handler 映射
- `LarkCredentialResolver::resolve` 返回值扩展为 `(app_id, app_secret, identity_mode)`；lark_cli 工具在 ensure_cli_config 后幂等执行 `config default-as <mode>`（HOME 下 `.default_as_marker` 文件记当前值，一致则跳过，避免每次调用多 spawn）
- 前端渠道 Modal Lark 区块：凭证选择下拉（必填）+ 身份模式下拉（自动/应用身份/用户身份）

## 二、WS 断线退避重连 + 监控

改造 [src/service/dao/lark/ws.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/lark/ws.rs)：

- 将现有 `start_event_loop` 拆为 `run_connection_once`（取端点 → 连接 → 心跳 + recv，返回退出原因 `Exited { shutdown: bool }`）与外层 **supervisor 任务**：
  - recv 错误/服务端关闭（非 shutdown）→ 指数退避重连：1s 起、倍增至 60s 封顶、加 ±20% 抖动；连接成功后重置退避
  - shutdown 信号 → 立即退出；`WsState` 改为持有 supervisor JoinHandle + shutdown_tx，`stop_event_loop` 发信号并 await supervisor
  - 纯函数 `next_backoff(current: Duration) -> Duration` 抽离可测
- token 刷新：ws.rs 新增 `WsTokenSource` trait（`async fn token(&self) -> Result<String>`），[http.rs](file:///Users/aman/Technology/rust/ai_orz/src/service/dao/lark/http.rs) 侧以 `get_tenant_access_token` 实现并传入——重连时 token 过期可自愈（现状仅从缓存读，缓存失效即死连）
- 监控：`WsConnState` 增加 `{ state: Connecting/Connected/Reconnecting, reconnect_count, last_connected_at }`，`LarkDao` trait 新增 `listener_stats()` 快照方法
- health metrics：common 新增 `LarkWsMetrics { active_connections, apps: [{ app_id, state, reconnect_count }] }` 挂入 `HealthMetricsResponse`，handler [health_metrics.rs](file:///Users/aman/Technology/rust/ai_orz/src/handlers/system/health_metrics.rs) 注入（前端 Health 仪表盘后续可选展示，本期不做 UI）

## 三、用户 OAuth device flow

### pkg 层会话编排（新建 `src/pkg/lark_integration.rs`）

复用一期 HOME 隔离设施（[lark_cli.rs](file:///Users/aman/Technology/rust/ai_orz/src/pkg/tool_registry/lark_cli.rs) 的 `lark_home`/`binary_available`/`ensure_cli_config` 提升为 pub 复用）：

- `start_device_login(user_id, domains)` → `auth login --no-wait --json` → 解析出 `device_code / verification_url / expires_in` 返回
- `complete_device_login(user_id, device_code)` → `auth login --device-code X --json`
- `auth_status(user_id)` → `auth status --json` → `{ logged_in, user_name?, ... }`
- `auth_logout(user_id)` → `auth logout`
- 前置：无绑定渠道（ensure_cli_config 不可达）→ 引导错误「请先绑定飞书应用」；**keychain 失败降级**：捕获 stderr 中 keychain 类错误 → 返回 `{ success:false, degraded:true, hint: "钥匙串不可用...可继续使用应用身份" }`，不抛 500
- JSON 解析逻辑抽纯函数（fixture 测试），输出经 `sanitize_lark_output` 脱敏

### Handler + 路由

新建 `src/handlers/user/lark_integration/`（每方法一文件），挂入 `user_routes()` 的 `lark-integration` 子 nest（路径见 §零约定）：凭证 CRUD 三端点 + auth 四端点 + `status` 聚合端点（users 表 JSON + 引用渠道 + auth status 三源聚合，纯查询，编排放 Handler 请求级组合）。

DTO 进 common（遵守 api_protocol_convention，禁裸返回）。

## 四、config init --new 自动化绑定

### 第 0 步实测定分支（首个实施动作）

本机跑 `HOME=<tmp> lark-cli config init --new` + `config show`，确认完成后 **app_secret 是否可读出**：
- **分支 A（可读）**：绑定完成后**写入 users 表 identity_credentials**（kind=LarkApp、name 自动生成、secret 加密）并**联动创建首条 Lark 渠道**（引用该凭证 ID、listen_inbound 默认 true），返回 credential_id + channel_id
- **分支 B（不可读）**：返回 done + app_id，前端引导「去飞书集成补填 App Secret」（app_id 预填，调 credentials POST 写入 users 表）；绑定关系在补填前不存在于系统侧

### 后端

- `POST /api/v1/user/lark-integration/bind/start`：用户 HOME 下 spawn `config init --new`（阻塞式命令），逐行扫 stdout 抓验证 URL，**抓到即返回** `{ session_id, verification_url }`，进程继续后台跑
- `GET /api/v1/user/lark-integration/bind/status?session_id=`：pending / done（进程成功退出 → 读 config 取凭证 → 按分支 A/B 处理）/ failed
- `POST /api/v1/user/lark-integration/bind/cancel`：kill 进程
- 会话注册表：pkg 内存态（`RwLock<HashMap<session_id, BindSession>>`），per-user 同时仅一个活跃会话，TTL 10 分钟惰性清理；输出脱敏同前

## 五、前端「飞书集成」区块（Settings 页）

[settings.rs](file:///Users/aman/Technology/rust/ai_orz/frontend/src/pages/settings.rs) 新增卡片区块（沿用 DaisyUI card 风格），**区块加载时先调 `GET /user/lark-integration/status` 渲染绑定现状**（不用 localStorage 缓存绑定态）：

- **应用绑定卡**：已绑定 → 展示凭证列表（app_id/名称/关联渠道，跳渠道详情）+ 编辑/删除凭证（有引用时后端报 Conflict 前端提示；编辑提示「关联渠道将重建联」）；未绑定 → 手动录入（Modal 填 app_id/secret 调 credentials POST）+ 自动绑定（start → 展示验证 URL 与「打开」按钮 → 3s 轮询 status → 完成 toast + 分支 A 刷新聚合状态 / 分支 B 引导文案）
- **用户身份卡**：auth status 徽标（已授权/未授权）、「授权用户身份」→ device URL 弹窗 → 用户确认后点「已完成授权」调 complete；「取消授权」调 logout；keychain 降级提示条
- **渠道创建 Modal 改造**（[message_channels.rs](file:///Users/aman/Technology/rust/ai_orz/frontend/src/pages/finance/message_channels.rs)）：删除原 app_id/secret 手填输入（内联字段已删）；选 Lark 时查聚合状态 → 无凭证展示引导条（跳 Settings）；有凭证展示凭证下拉（必选，传 `lark_credential_id`）
- **渠道详情**（message_channel_detail.rs）：集成状态卡（引用凭证名 + 用户授权徽标 + 身份模式 + 跳 Settings 链接）
- api 客户端新增 `frontend/src/api/lark_integration.rs`（11 个方法），纯函数（状态解析/轮询判定）补测试

## 六、测试与质量门槛

- 单测：backoff 序列、device-login/status JSON 解析 fixture、init 输出 URL 提取、identity_mode 传递、会话注册表 TTL/单会话约束、凭证引用校验纯函数（归属/kind/删除守护）、UserIdentityCredentials JSON 序列化往返（多类型凭据共存）
- DAO/DAL 测试：users.identity_credentials 读写、按 user_id/username 查凭证、凭证 CRUD + 引用解析、options 附带 vs DAO 兜底两路径优先级、凭证变更联动（HOME 清理/WS 重建调用链，mock dao）、resolver 返回 identity_mode（lark_test.rs 扩展）
- 集成测试：`tests/integration/lark_integration_test.rs`——无 lark-cli 二进制/未绑定凭证时各端点返回引导性错误 JSON（非 500、不含 secret 明文）+ 凭证-渠道引用生命周期（建凭证建渠道、删渠道留凭证、删凭证被引用拦截、改凭证触发渠道字段更新），注册 `[[test]]`；同步修订 message_channel_lifecycle_test（lark 渠道改走凭证引用创建）
- 门槛：`cargo fmt --all --check`、前后端 clippy `-D warnings`、全量测试（后端单元 + 集成 + 前端 + common）

## 实施顺序

一（users 表凭证 JSON 列 + ChannelConfig 删内联字段 + 引用解析 + 变更联动，地基）→ 二（WS 重连，独立可测）→ 三（OAuth + 身份模式）→ 四（自动绑定，含第 0 步实测）→ 五（前端统一消费）→ 六（门槛收尾）

## Assumptions

- 开发机 lark-cli 已安装（已验证 `/Users/aman/.local/bin/lark-cli`）；CI 无 lark-cli，集成测试只断言引导错误契约，不真跑 CLI
- keychain 在 macOS 正常可用；headless Linux 兼容性以降级引导兜底，实测留部署阶段
- 不引入新依赖（解析 CLI JSON 用 serde_json；会话注册表纯内存）
- 一期约定延续：secret 永不回显、日志宏禁带 secret、凭证经 stdin 传递
- **渠道内联 lark 凭证字段直接删除、无兼容回退、无数据迁移**（测试阶段；存量 config_json 旧键被 serde 忽略）；wechat/email/slack 渠道自有凭证不动；修正一期设计文档 §2.3 认知，落地后更新 lark_cli_integration.md 分期记录与旧决策记忆
