---
kind: RAG 原子知识卡
name: 跨组织业务调用鉴权模型：dual-mode auth + federation_identity + delegation + audit + capabilities
category: 业务模块 / 联邦鉴权
scope:
  - "src/middleware/a2a_auth.rs"
  - "src/middleware/federation_identity.rs"
  - "src/pkg/jwt.rs"
  - "src/pkg/request_context.rs"
  - "src/service/dao/agent_runtime/a2a.rs"
  - "src/service/dao/organization_link/http.rs"
  - "src/service/domain/organization/org.rs"
  - "src/service/dal/organization.rs"
  - "common/src/api/organization_link.rs"
  - "common/src/constants/http_header.rs"
  - "src/router.rs"
source_files:
  - src/middleware/a2a_auth.rs#L1-L108 (A2A 双模鉴权：先 try_jwt_auth(本地用户 Cookie/Bearer) → 次 authenticate_link_call(对端 Bearer + X-Federation-Caller 声明) → 身份解析全部下沉 federation_identity::resolve_federation_identity 纯函数；fail-closed 声明头非法 JSON → 401)
  - src/middleware/federation_identity.rs#L1-L117 (联邦身份解析纯函数：输入凭证字符串 + 已解析声明 JSON → FederationIdentity{local_org_id, peer_org_id, receptionist_user_id, caller_type}；**不绑定传输层**——HTTP 中间件 + WS 长连接握手共用同一份实现)
  - src/pkg/jwt.rs#L1-L221 (Claims R2/R3 扩展：iss(签发方组织 ID) + aud(目标组织 ID)；本地登录 token 二者相等；均为 Option 兼容存量 token；本期不 enforce 校验，Phase 2 跨组织互调时按 aud 做目标匹配后收紧)
  - src/pkg/request_context.rs#L1-L855 (RequestContext 扩展 caller_organization_id: Option<String> + caller_peer_org_id: Option<String> 字段——联邦调用时注入对端组织身份，供审计统计 + federation_identity 消费)
  - src/service/dao/agent_runtime/a2a.rs (A2aRuntimeDao 联邦扩展：FederatedCallConfig{link, caller_org_id, capabilities} 结构；execute_federated_agent_call 在出站 Bearer token(link.access_token) 基础上追加 X-Federation-Caller 声明头)
  - src/service/dao/organization_link/http.rs (OrganizationLinkDao HTTP 出站实现：带 access_token Bearer + FederationCallerDeclaration header；GET directory / POST create_link / GET capabilities 端点出站)
  - src/service/domain/organization/org.rs#L1-L1294 (capabilities 白名单校验：FederationIdentity.capabilities 与 link.capabilities JSON 数组交集判定；receptionist_user_id 接待模型)
  - src/service/dal/organization.rs#L1-L515 (call_peer facade 联邦调用入口：FEDERATED_CALL_DEADLINE_SECS=120 + FEDERATED_CALL_POLL_INTERVAL_MS=1000；先查 WS registry → 有则 publish federation.outbound，无则回退 HTTP A2aRuntimeDao)
  - common/src/api/organization_link.rs#L1-L329 (FederationCallerDeclaration DTO：caller_org_id + caller_type(A2aTaskDelegate) + agent_name + task_id；明文 JSON header X-Federation-Caller)
  - common/src/constants/http_header.rs (FEDERATION_CALLER_HEADER = "X-Federation-Caller"; CALLER_ORG_HEADER = "X-Federation-Caller-Org")
  - src/router.rs (/a2a 路由组挂载 a2a_auth_middleware(双模)；非 JWT protected——对端 Bearer + 声明头合法时放行)
  - src/consumer/federation_inbound_task.rs#L1-L132 (跨组织审计：入站命令 publish FederationOutboundEvent 时携带 CALLER_AUDIT_KEY{peer_org_id, event_id, receptionist_user_id})
  - docs/plan/跨组织业务调用方案.md#L1-L364 (Phase 2 设计稿：双模鉴权 P1+P2 → 能力白名单 P3 → A2A delegation e2e P4 → capabilities endpoint P3P4P5 → receptionist identity P6 → 方案②凭证直传最终拍板)
  - docs/wiki/zh/content/功能模块/用户与组织管理/跨组织业务调用鉴权.md (鉴权模型长文：双模鉴权时序 + federation_identity 解析链 + receptionist 接待模型 + 审计全链路)
  - docs/wiki/zh/content/核心模块/路由和中间件.md (/a2a 路由组双模鉴权扩展 + 新 middleware 注册点)
  - docs/wiki/zh/content/项目概述/核心功能特性/A2A 协议支持/A2A Server 模式.md (A2A Server handler /a2a 入口：现在接受两种合法 Bearer——本地 JWT 或 link access_token)
  - 【兄弟卡】docs/wiki/knowledge/zh/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接.md (组网地基主主题——负责连接建立，本卡负责跨组织调用时的鉴权与身份识别)
  - 【关联卡】docs/wiki/knowledge/zh/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建/A2A Server Handler 层：JSON-RPC 方法路由 + 公开无鉴权路由 + notification_url 回调渠道自动创建.md (/a2a handler 路由现在接受双模鉴权，不再纯 JWT protected)
  - 【关联卡】docs/wiki/knowledge/zh/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步.md (JWT 地基 iss/aud 扩展 + RequestContext caller_organization_id)
---

## §1 概述

**本卡角色**：跨组织业务调用鉴权模型的知识卡。覆盖 `/a2a` 双模鉴权中间件（本地 JWT Cookie/Bearer + 对端 link access_token Bearer + X-Federation-Caller 身份声明头）、联邦身份解析纯函数（`resolve_federation_identity` 不绑定传输层——HTTP 和 WS 长连接握手共用同一份逻辑）、receptionist 接待身份模型、连接级 capabilities 白名单、跨组织 Agent 委派（A2A delegation）e2e 全链路审计。**定位：排查跨组织调用 401/403、调试 federation_identity 解析失败、理解 capabilities 白名单门禁时读。**

本卡为联邦组网的鉴权子主题，与 **【联邦组网地基】** 互为兄弟卡（Level 3 视角兄弟卡），组网地基负责连接建立（scope 三态 + 配对码 + 连接表 + 目录 + WS），本卡负责跨组织调用时的鉴权与身份识别。

- **双模鉴权（middleware/a2a_auth.rs）**：`/a2a` JSON-RPC 入口现在接受两类合法调用方——① **本地用户**：JWT（Cookie ai_orz_token 或 Header Authorization: Bearer \<user_jwt\>，既有语义原样保留）；② **建联对端节点**：`Authorization: Bearer <link_access_token>`（连接级凭证，通过 `authenticate_link_call` 哈希匹配 Active 连接的 peer_token_hash）+ 可选 `X-Federation-Caller: <FederationCallerDeclaration JSON>` 身份声明头。声明头合法 JSON 且 capability 在白名单内时放行；声明头非法 JSON → 401（fail-closed）。**身份解析全部下沉到 `resolve_federation_identity` 纯函数**——中间件只做 HTTP 协议适配（提取 Bearer + 解析声明头 + 错误码映射），不写业务判定。
- **联邦身份解析纯函数（middleware/federation_identity.rs）**：输入是**纯数据**（凭证字符串 + 已解析的 FederationCallerDeclaration），不碰 axum::Request，也不感知来自 HTTP 头还是 WS 帧。解析结果 `FederationIdentity { local_org_id(本端), peer_org_id(对端), receptionist_user_id(接待用户), capabilities(白名单交集) }`。HTTP 中间件解析写 header 交给 request_context_middleware；WS 长连接握手阶段调用同样函数解析后挂在会话上。**一套逻辑服务两条链路，彻底避免漂移**。
- **receptionist 接待身份模型（P6 落地）**：联邦访客没有本端的 JWT/密码——本端为每个建联对端分配一个"接待用户"（receptionist_user_id，scope=Internal 或类似），所有跨组织调用被视为"receptionist 用户在调用本端资源"。好处：① 权限系统不需要为联邦调用写特殊分支——receptionist 的 UserRole 决定它能调什么；② 审计日志天然有 user_id 维度，和本地用户调用的审计格式一致。
- **连接级 capabilities 白名单（P3 落地）**：每条 organization_links 记录有 `capabilities` JSON 数组（如 `["a2a_task"]`）。对端调用本端 `/a2a` 时，federation_identity 解析出的 caller 声明 capabilities 与本端白名单**交集判定**——交集为空或请求能力不在交集内 → 403 "capability not allowed"。白名单默认 `'[]'`（JSON 空数组字符串），fail-closed 设计，建联后需显式追加能力。
- **caller_organization_id 计量管道（R2/R3 地基）**：RequestContext 新增 `caller_organization_id: Option<String>` 字段。联邦调用时由 request_context_middleware 从 federation_identity.peer_org_id 注入；本地调用时为 None。Stats collector 已就绪——按 caller_organization_id 维度统计 "A 组织调了 B 组织多少次"，为后续计费结算留口子。
- **JWT Claims 扩展 iss/aud（R2/R3 地基）**：`pkg/jwt.rs` 的 Claims 新增 `iss: Option<String>`（签发方组织 ID）+ `aud: Option<String>`（目标组织 ID）。本地登录 token 二者相等。均为 Option 兼容存量 token（无 iss/aud）——本期 decode 不 enforce 校验，Phase 2 跨组织互调时收紧。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| middleware/a2a_auth.rs | /a2a 双模鉴权中间件 | try_jwt_auth 本地用户 → authenticate_link_call 对端 Bearer → resolve_federation_identity 纯函数解析 → 错误 HTTP 状态码映射 | `:L1-L108` |
| middleware/federation_identity.rs | 联邦身份解析纯函数 | 输入凭证 + 声明 → FederationIdentity{local_org_id, peer_org_id, receptionist_user_id, capabilities}; **不绑定传输层** | `:L1-L117` |
| pkg/jwt.rs Claims | JWT 签发/验证 R2/R3 扩展 | iss(签发方 org_id) + aud(目标 org_id)；本地 token iss==aud；Option 兼容存量 | `:L1-L221` |
| pkg/request_context.rs | RequestContext 联邦字段扩展 | caller_organization_id + caller_peer_org_id; enrich_ctx! 宏自动注入; Stats 计量管道 | `:L1-L855` |
| service/dao/agent_runtime/a2a.rs | A2aRuntimeDao 联邦扩展 | FederatedCallConfig + execute_federated_agent_call; 出站 Bearer(link.access_token) + X-Federation-Caller header | 见文件 |
| service/domain/organization/org.rs capabilities 校验 | Domain 层白名单门禁 | link.capabilities(JSON) 反序列化 → 与 federation_identity.capabilities 交集判定 → 空则 Error::forbidden | 见 org.rs |
| service/dal/organization.rs call_peer facade | DAL 层统一出站入口 | 先查 WS registry → publish federation.outbound; 无则 HTTP A2aRuntimeDao; FEDERATED_CALL_DEADLINE_SECS=120 | `:L1-L515` |
| common/api/organization_link.rs FederationCallerDeclaration | 身份声明 DTO | caller_org_id + caller_type + agent_name + task_id; 明文 JSON header | `:L1-L329` |
| router.rs /a2a 路由组 | 双模鉴权挂载 | 非 JWT protected——a2a_auth_middleware 独立双模中间件 | 见 router.rs |
| docs/plan/跨组织业务调用方案.md | Phase 2 设计稿 | P1+P2 双模鉴权 → P3 capabilities endpoint → P4 A2A delegation → P5 federation_agents directory → P6 receptionist identity → 方案②凭证直传拍板 | `:L1-L364` |

**章节来源**
- [a2a_auth.rs:L1-L108](src/middleware/a2a_auth.rs#L1-L108)
- [federation_identity.rs:L1-L117](src/middleware/federation_identity.rs#L1-L117)
- [jwt.rs:L1-L221](src/pkg/jwt.rs#L1-L221)

---

## §3 架构约定

本卡与 **【联邦组网地基】** 构成 Level 3 视角兄弟卡——组网地基管连接建立（scope 三态 + 配对码 + 连接表 + 目录 + WS 长连接生命周期），本卡管跨组织调用时的鉴权与身份识别。与 **【组织权限与用户偏好】** 构成父子关系——本地 JWT（Cookie/Bearer 双模式）是其存量语义，联邦双模鉴权在此基础上叠加了对端 Bearer 通道 + federation_identity 解析。与 **【A2A Server Handler 层】** 构成局部关系——`/a2a` handler 路由在鉴权之后进入，handler 层零改动，鉴权在中间件层完成。

### 双模鉴权链路

```
Request: POST /a2a  (JSON-RPC task/send)
  ↓
a2a_auth_middleware (middleware/a2a_auth.rs)
  ├─ try_jwt_auth: Cookie ai_orz_token → decode → 本地 JWT
  │   └─ 成功 → 解析 FederationIdentity.caller_type = Local
  ├─ 失败 → authenticate_link_call: Authorization: Bearer <token>
  │   ├─ 所有 Active organization_links → SHA256(token) vs peer_token_hash
  │   │   └─ 匹配 → 定位 link = OrganizationLinkPo
  │   └─ 同时解析 X-Federation-Caller: <JSON>
  │       ├─ 非法 JSON → 401 (fail-closed)
  │       └─ 合法 → FederationCallerDeclaration{caller_org_id, ...}
  ├─ resolve_federation_identity(credentials, declaration)  ← 纯函数
  │   → FederationIdentity{
  │       local_org_id = link.peer_org_id (本端在连接里是 peer)
  │       peer_org_id = link.local_org_id (对端)
  │       receptionist_user_id = 查询本端为该 link 分配的接待用户
  │       capabilities = link.capabilities ∩ declaration.capabilities 交集
  │     }
  └─ 401 if both paths failed; 403 if declaration capabilities ∩ link.capabilities = ∅
  ↓
request_context_middleware (注入 ctx.caller_organization_id = peer_org_id)
  ↓
A2A handler (send_task / get_task / cancel_task ...) → 复用既有 HTTP path 代码零改动
```

### WS 长连接鉴权路径（P8，同一份 federation_identity 函数）

```
WS upgrade handshake
  ↓
HTTP upgrade header: Authorization: Bearer <link_access_token> + X-Federation-Caller: <JSON>
  ↓
upgrade handler → 先调 resolve_federation_identity 纯函数
  └─ 成功 → 挂 FederationIdentity 在 WsConnectionState
  ↓
WS 运行期读循环 (pkg::ws 通用管理器)
  ↓
收到帧 → adapter 解析 → 关联 FederationIdentity → FederationInboundTaskConsumer
```

---

## §4 硬约束与回归红线

1. **federation_identity::resolve_federation_identity 纯函数**：该模块**严禁 import axum::http::Request**或感知 HTTP 头/WsFrame 等传输层类型。输入是 String（凭证）和已反序列化的 FederationCallerDeclaration（纯 JSON），输出是 FederationIdentity（纯数据）。HTTP 中间件和 WS 握手都调用这同一个函数——任何一处想加传输层判断，必须移到调用方。静态 grep 确认 `federation_identity.rs` 无 `use axum::` 导入。
2. **双模鉴权 fail-closed**：① Authorization header 存在但 Bearer token 空字符串 → 401；② X-Federation-Caller header 存在但 JSON 解析失败 → 401（不是 400 Bad Request——400 暗示格式问题，401 暗示身份问题更准确）；③ link 存在但 capabilities 交集为空 → 403 "capability not allowed"（不是 401——已经通过身份认证但权限不足）；④ 不声明 X-Federation-Caller 就带 link Bearer → 401（link Bearer 必须搭配声明头）。
3. **capabilities 白名单默认 '[]'，建联即默认拒绝全部跨组织能力**：任何新 organization_links insert SQL 的 capabilities 列默认值必须是 `'[]'`（空数组 JSON 字符串）。handler create_link 创建连接时不要自动追加任何 capability——让 link.owner 后续显式调 update_link_capabilities 追加。防止建联即开放 A2A delegation。
4. **caller_organization_id 计量管道不写业务逻辑**：RequestContext.caller_organization_id 只是 Stats collector 的统计维度注入入口——Stats collector 自动按 caller_organization_id + local_org_id 聚合 "A 组织调 B 组织 N 次"。任何 domain 代码里出现 `if ctx.caller_organization_id.is_some() { // 特殊逻辑 }` 都是反模式——跨组织调用在本端应该和本地调用走相同代码路径（鉴权在中间件层 gate 掉）。
5. **JWT Claims iss/aud 本期不 enforce 校验，Phase 2 跨组织互调时收紧**：decode_token 时校验 exp（过期立即 401）和 签名，但**不校验 iss/aud 的匹配关系**——存量 token 没有 iss/aud 字段，Option 回退。Phase 2 时加一个 `require_iss_aud_match: bool` 参数给 decode_token，跨组织场景传 true 执行严格匹配。
6. **receptionist 用户必须在 create_link 时同步创建**：OrganizationLinkPo insert 和 receptionist UserPo insert 必须包在同一个事务里。如果 link 写成功但 receptionist 创建失败，下次跨组织调用 federation_identity 找不到 receptionist → NPE 或 500。
7. **跨组织审计日志必须携带 CALLER_AUDIT_KEY{peer_org_id, event_id, receptionist_user_id}**：FederationOutboundEvent / FederationInboundEvent JSON payload 里必须包含这三个字段——审计系统按 peer_org_id 聚合 "A 组织发起的所有跨组织调用"。静态 grep 确认 FederationOutboundEvent payload 构建处有这三个字段。
