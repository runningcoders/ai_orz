---
kind: RAG 原子知识卡
name: 跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌
category: 业务模块 / 联邦鉴权
scope:
  - "src/pkg/crypto/**"
  - "src/pkg/nonce.rs"
  - "src/middleware/a2a_auth.rs"
  - "src/middleware/federation_identity.rs"
  - "src/middleware/proxy.rs"
  - "src/pkg/request_context.rs"
  - "src/pkg/jwt.rs"
  - "src/models/federation_contract.rs"
  - "src/service/dal/organization/contract.rs"
  - "src/service/dao/federation_contract/**"
  - "src/service/domain/organization/org.rs"
  - "src/service/dal/organization.rs"
  - "common/src/api/organization_link.rs"
  - "common/src/constants/http_header.rs"
  - "common/src/enums/organization.rs"
  - "src/router.rs"
  - "migrations/2026090900000*_*.sql"
source_files:
  - src/pkg/crypto/mod.rs#L1-L50 (联邦身份密钥模块入口；re-export did.rs + task_token.rs)
  - src/pkg/crypto/did.rs#L1-L150 (Ed25519-dalek 密钥对生成 + did:key 编解码(multicodec=ed25519-pub 0xed 0x01 + 32B pubkey base58btc) + 签名原语 sign_message / verify_signature + 待签串拼接 build_signing_string(key_id, timestamp, nonce, method, path, body))
  - src/pkg/crypto/task_token.rs (异步回调专用任务令牌：签名派生，agent 异步回调 /a2a/task/notify 时带)
  - src/pkg/nonce.rs#L1-L60 (进程内 HashMap LRU 去重 + 防重放；NONCE_TTL=600s > ±300s 时间窗；MAX_ENTRIES=65536 容量触发清理；check_and_insert 原子查重登记)
  - src/middleware/federation_identity.rs#L1-L200 (**重写**：第 1 步从「凭证哈希匹配」改为「解析 peer DID + 验签」——读 X-Federation-Key-Id / Timestamp / Nonce / Signature 四头 → timestamp ±300s 窗口 → nonce LRU 查重 → 从 organization_links 反查 peer_did → crypto::verify_signature → 成功返回 FederationIdentity；WS 握手复用同一函数)
  - src/middleware/a2a_auth.rs#L1-L130 (**重写**：删除联邦 token 分支；显式分流「有 X-Federation-Key-Id 等签名头 → federation_identity::resolve 验签链路；否则 → try_jwt_auth 本地 JWT」；fail-closed：签名头缺一即 401)
  - src/middleware/proxy.rs#L1-L80 (新增：网关反向代理头解析；trust_proxy 安全前提——部署在反代后时从 X-Forwarded-* 取真实来源；直接暴露时跳过)
  - src/pkg/request_context.rs (RequestContext 扩展 caller_organization_id + caller_peer_org_id；联邦验签成功后注入)
  - src/pkg/jwt.rs (Claims iss/aud 扩展；本地 JWT 通道不变)
  - src/models/federation_contract.rs#L1-L80 (FederationContractPo：连接级能力白名单从 organization_links.capabilities 下沉到此；kind=basic / state=active|terminated；terms_hash + local_signature + peer_signature 为未来合同机制占位；capabilities_list() + has_capability() 方法)
  - src/service/dal/organization/contract.rs#L1-L100 (联邦合约 DAL 层：封装 FederationContractDao；find_active_by_pair / update_capabilities / terminate)
  - src/service/dao/federation_contract/mod.rs#L1-L60 (FederationContractDao trait：insert / find_by_id / find_active_by_pair / update_capabilities / terminate / list)
  - src/service/dao/federation_contract/sqlite.rs#L1-L150 (SQLite 实现；unique(local_org_id, peer_org_id) 约束)
  - src/handlers/organization/contracts/list_contracts.rs (合约列表端点)
  - src/handlers/organization/contracts/terminate_contract.rs (终止合约 → state=terminated → 后续验签链路发现 terminated 直接 403 熔断)
  - src/handlers/organization/contracts/update_contract_capabilities.rs (更新能力白名单 → capabilities JSON 校验 → 非法解析返回空数组 fail-closed)
  - src/service/domain/organization/org.rs (capabilities 校验从 link.capabilities JSON 改为 contract.capabilities + state 检查；terminated 直接 403)
  - common/src/enums/organization.rs (FederationContractKind 枚举 Basic=0 / FederationContractState 枚举 Active=0 / Terminated=1)
  - common/src/api/organization_link.rs (CreateLinkRequest/Response 扩展 peer_did + verification_key 字段)
  - common/src/constants/http_header.rs (FEDERATION_KEY_ID_HEADER="X-Federation-Key-Id" / FEDERATION_TIMESTAMP_HEADER / FEDERATION_NONCE_HEADER / FEDERATION_SIGNATURE_HEADER 四头)
  - src/router.rs (/a2a 路由组挂载 a2a_auth_middleware(双模分流)；organization/contracts/* 路由组)
  - migrations/20260909000001_add_federation_identity_keys.sql (organizations 表加 did + verification_key + encrypted_signing_key 列；organization_links 表加 peer_did 列；DROP token_hash/access_token 两列)
  - migrations/20260909000002_federation_signature_upgrade.sql (organization_links.capabilities 从 link 迁移到 federation_contracts 表)
  - migrations/20260909000003_federation_contracts.sql (CREATE TABLE federation_contracts — 新表承载合约级能力 + terminate 熔断 + 未来合同机制占位)
  - docs/plan/联邦鉴权升级方案.md#L1-L300 (SSOT 设计稿：S1 did:key 身份层 + S2 Ed25519 每请求签名 + S3 合约授权；跨 commit 2a1fbde3)
  - docs/wiki/zh/content/功能模块/用户与组织管理/跨组织业务调用鉴权.md (Wiki 长文：签名验签时序 + federation_identity 解析链 + 合约能力门禁 + 审计全链路)
  - 【兄弟卡】docs/wiki/knowledge/zh/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接/联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接.md (组网地基主主题——负责连接建立与身份密钥注入，本卡负责跨组织调用时的签名验签与合约能力门禁；WS 握手现在复用本卡签名链路)
  - 【父子卡】docs/wiki/knowledge/zh/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断.md (Level 4 子卡——本卡描述签名基建和鉴权全链路，子卡专注 federation_contracts 合约授权建模与 terminate 熔断机制)
  - 【关联卡】docs/wiki/knowledge/zh/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步.md (本地 JWT 通道不变；RequestContext caller_organization_id 字段)
---

## §1 概述

**本卡角色**：跨组织业务调用鉴权模型的知识卡。覆盖 **did:key 身份层 + Ed25519 每请求签名 + nonce 防重放 + 合约派生能力门禁 + 异步任务令牌** 全新安全架构，替代旧架构的「静态 token hash 匹配 + capabilities JSON 白名单」。**定位：排查跨组织调用 401/403、调试签名验签失败、理解 nonce 去重窗口、排查合约 terminate 熔断、理解 WS 握手签名链路时读。**

本卡为联邦组网的鉴权子主题，与 **【联邦组网地基】** 互为兄弟卡（Level 3 视角兄弟卡），组网地基负责连接建立（scope 三态 + 配对码 + 身份密钥落库 + 目录 + WS），本卡负责跨组织调用时的签名验签与合约能力门禁。同时本卡作为 Level 4 总卡，下挂 **【联邦合约授权】** 细卡——总卡描述签名基建和鉴权全链路，细卡专注 federation_contracts 表建模与 terminate 熔断机制。

- **did:key 身份层（pkg/crypto/did.rs）**：每个组织拥有 Ed25519 密钥对，编码为 `did:key:z<multibase>`（multicodec=ed25519-pub 0xed 0x01 + 32B 公钥 base58btc）。身份标识符**绑定密钥而非域名/IP/端口**，组织换 endpoint 身份不变。密钥生成后 signing_key（32B seed base64）经 `encrypt_channel_secret` 加密落库（organizations 表 encrypted_signing_key 列），verification_key 随目录同步公开。

- **Ed25519 每请求签名 + 四头协议（middleware/federation_identity.rs）**：每次跨组织 HTTP 调用必须携带四个签名头——`X-Federation-Key-Id`（本端 did:key 标识符）、`X-Federation-Timestamp`（Unix 毫秒时间戳）、`X-Federation-Nonce`（16B hex 随机数）、`X-Federation-Signature`（base64url 无 padding）。验签链路：时间戳 ±300s 窗口 → nonce LRU 查重（进程内 HashMap 600s TTL，MAX_ENTRIES=65536）→ 从 organization_links 反查 peer_did → crypto::verify_signature。**四头缺一即 401，fail-closed**。

- **合约派生能力门禁（federation_contracts 表，替代旧 link.capabilities）**：能力白名单从 organization_links.capabilities JSON 字段下沉到 federation_contracts.capabilities。验签成功后查 federation_contracts（local_org_id=本端, peer_org_id=对端）→ state=terminated 直接 403（熔断）→ state=active 时校验 capabilities 交集。**禁止在 organization_links 另行配置能力（SSOT 约束）**。

- **异步任务令牌（pkg/crypto/task_token.rs）**：异步回调专用——对端 agent 异步执行完任务后回调 /a2a/task/notify 时带此令牌。令牌用同一 Ed25519 密钥对签名派生，验证链路复用 federation_identity::resolve。

- **WS 长连接握手复用签名链路**：WS upgrade 请求携带同样的四签名头，upgrade handler 调用 federation_identity::resolve 验签 → 成功后挂 FederationIdentity 在 WsConnectionState。**HTTP 和 WS 握手共用同一份签名验签纯函数，彻底避免漂移**。

- **网关反向代理安全前提（middleware/proxy.rs）**：trust_proxy 开关——部署在 Nginx/Caddy 反代后时从 X-Forwarded-* 取真实来源；直接暴露时跳过。签名链路依赖来源 IP 一致性（nonce 防重放窗口内）。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| pkg/crypto/mod.rs | 联邦密钥模块入口 | re-export did.rs + task_token.rs；统一出口 | `:L1-L50` |
| pkg/crypto/did.rs | Ed25519 + did:key 签名原语 | generate_keypair → encode_did_key(multicodec 0xed 0x01 + base58btc) → sign_message / verify_signature → build_signing_string(key_id, ts, nonce, method, path, body) 拼接待签串 | `:L1-L150` |
| pkg/crypto/task_token.rs | 异步回调任务令牌 | 签名派生令牌；agent 回调 /a2a/task/notify 时携带 | 见文件 |
| pkg/nonce.rs | 进程内 LRU 去重防重放 | HashMap + Mutex + OnceLock；NONCE_TTL=600s > ±300s；MAX_ENTRIES=65536 容量触发清理；check_and_insert 原子查重登记 | `:L1-L60` |
| middleware/federation_identity.rs | **重写**：签名验签纯函数 | 读四签名头 → timestamp ±300s → nonce 查重 → 反查 peer_did → verify_signature → FederationIdentity{local_org_id, peer_org_id, receptionist_user_id, contract_state, capabilities}；**HTTP + WS 握手共用** | `:L1-L200` |
| middleware/a2a_auth.rs | **重写**：双模显式分流 | 删除联邦 token 分支；有 X-Federation-Key-Id 等签名头 → federation_identity::resolve；否则 → try_jwt_auth（本地 JWT 通道不变）；签名头缺一即 401 | `:L1-L130` |
| middleware/proxy.rs | 新增：反代头解析 | trust_proxy 开关；部署反代后取 X-Forwarded-*；直接暴露跳过 | `:L1-L80` |
| models/federation_contract.rs | 联邦合约 PO | FederationContractPo：id + local_org_id + peer_org_id + kind(basic) + state(active/terminated) + capabilities(JSON 数组) + terms_hash + local_signature + peer_signature；new() + capabilities_list() + has_capability() | `:L1-L80` |
| service/dal/organization/contract.rs | 联邦合约 DAL | 封装 FederationContractDao；find_active_by_pair / update_capabilities / terminate | `:L1-L100` |
| service/dao/federation_contract/mod.rs | 合约 DAO trait | insert / find_by_id / find_active_by_pair / update_capabilities / terminate / list | `:L1-L60` |
| service/dao/federation_contract/sqlite.rs | 合约 DAO SQLite 实现 | unique(local_org_id, peer_org_id) 约束 | `:L1-L150` |
| handlers/organization/contracts/list_contracts.rs | 合约列表端点 | 列出本端所有 federation_contracts | 见文件 |
| handlers/organization/contracts/terminate_contract.rs | 终止合约端点 | state → terminated；后续验签链路发现 terminated 直接 403 熔断 | 见文件 |
| handlers/organization/contracts/update_contract_capabilities.rs | 更新能力端点 | capabilities JSON 校验 → 非法解析返回空数组 fail-closed | 见文件 |
| service/domain/organization/org.rs | Domain 层能力门禁 | 查 federation_contracts.state → terminated 直接 403；active 时 contract.capabilities 与请求能力交集 | 见 org.rs |
| common/enums/organization.rs | 联邦合约枚举 | FederationContractKind::Basic=0 / FederationContractState::Active=0, Terminated=1 | 见文件 |
| common/constants/http_header.rs | 签名头常量 | FEDERATION_KEY_ID_HEADER="X-Federation-Key-Id" / FEDERATION_TIMESTAMP_HEADER / FEDERATION_NONCE_HEADER / FEDERATION_SIGNATURE_HEADER 四头 | 见文件 |
| migrations/20260909000001_add_federation_identity_keys.sql | S1 身份密钥迁移 | organizations 加 did + verification_key + encrypted_signing_key；organization_links 加 peer_did；DROP token_hash/access_token 两列 | 见文件 |
| migrations/20260909000002_federation_signature_upgrade.sql | S2 签名升级迁移 | capabilities 从 organization_links 迁移到 federation_contracts | 见文件 |
| migrations/20260909000003_federation_contracts.sql | S3 合约授权迁移 | CREATE TABLE federation_contracts | 见文件 |
| docs/plan/联邦鉴权升级方案.md | SSOT 设计稿 | S1 did:key → S2 Ed25519 签名 → S3 合约授权 | `:L1-L300` |

---

## §3 架构约定

本卡与 **【联邦组网地基】** 构成 Level 3 视角兄弟卡——组网地基管连接建立与身份密钥落库（scope 三态 + 配对码 + organization_links + 身份密钥 + 目录 + WS 长连接生命周期），本卡管跨组织调用时的签名验签与合约能力门禁。与 **【组织权限与用户偏好】** 构成父子关系——本地 JWT（Cookie/Bearer 双模式）是其存量语义，联邦鉴权在此基础上叠加了 Ed25519 签名验签 + 合约授权通道。同时本卡作为 **Level 4 总卡**，下挂 **【联邦合约授权】** 细卡（`docs/wiki/knowledge/zh/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断/联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断.md`），总卡描述签名基建和鉴权全链路，细卡专注合约授权建模与 terminate 熔断。

### 签名验签链路

```
Request: POST /a2a  (JSON-RPC task/send)
  Headers:
    X-Federation-Key-Id: did:key:z...        ← 本端 did:key
    X-Federation-Timestamp: 1726000000000     ← Unix ms
    X-Federation-Nonce: a1b2c3d4e5f6...      ← 16B hex
    X-Federation-Signature: <base64url>       ← Ed25519 签名
  ↓
a2a_auth_middleware (middleware/a2a_auth.rs)
  ├─ 有 X-Federation-Key-Id 签名头？
  │   ├─ YES → federation_identity::resolve (验签链路)
  │   └─ NO  → try_jwt_auth (本地 JWT 通道)
  │
  ├─ federation_identity::resolve  ← 纯函数，HTTP + WS 共用
  │   ├─ 四签名头缺一 → 401 (fail-closed)
  │   ├─ timestamp ±300s 窗口外 → 401
  │   ├─ nonce 已出现过 → 401 (防重放)
  │   ├─ nonce 原子登记 → 继续
  │   ├─ 从 organization_links 反查 peer_did
  │   │   └─ 不匹配 → 401 (防跨连接冒充)
  │   ├─ crypto::verify_signature(verifying_key, signing_string, signature)
  │   │   └─ 验签失败 → 401
  │   ├─ 查 federation_contracts(local_org_id, peer_org_id)
  │   │   ├─ state=terminated → 403 (熔断)
  │   │   └─ state=active → 继续
  │   └─ 返回 FederationIdentity{
  │         local_org_id, peer_org_id, receptionist_user_id,
  │         contract_state, capabilities(contract.capabilities ∩ request)
  │       }
  │
  └─ 401 if both paths failed; 403 if capabilities ∅ 或 contract terminated
  ↓
request_context_middleware (注入 ctx.caller_organization_id = peer_org_id)
  ↓
A2A handler → 复用既有 HTTP path 代码零改动
```

### WS 长连接握手（复用同一签名链路）

```
WS upgrade handshake
  HTTP upgrade headers: 同四签名头 (X-Federation-Key-Id / Timestamp / Nonce / Signature)
  ↓
upgrade handler → federation_identity::resolve (同上验签链路)
  └─ 成功 → 挂 FederationIdentity 在 WsConnectionState
  ↓
WS 运行期收到帧 → adapter 解析 → 关联 FederationIdentity → FederationInboundTaskConsumer
```

### 出站签名构造（对端组织发起调用时）

```
call_peer facade (src/service/dal/organization.rs)
  ↓
crypto::build_signing_string(key_id, timestamp, nonce, "POST", "/a2a", request_body)
  ↓
crypto::sign_message(signing_key, signing_string)
  ↓
HTTP request with 四签名头 + task_token(异步)
  ↓
对端 a2a_auth_middleware → federation_identity::resolve 验签
```

---

## §4 硬约束与回归红线

1. **签名四头缺一即 401，fail-closed**：X-Federation-Key-Id / X-Federation-Timestamp / X-Federation-Nonce / X-Federation-Signature 任何一头缺失或为空字符串 → 直接返回 401 unauthorized，**不尝试任何其他鉴权路径**。静态 grep 确认 a2a_auth.rs 的联邦分支里没有 if let Some + continue 跳过的宽松写法。
2. **时间戳窗口 ±300s 硬编码**：federation_identity.rs 验签时 `abs(now - timestamp) > 300000` 立即 401。不得为了兼容漂移放宽窗口——nonce 防重放窗口已经覆盖 ±300s，时间戳窗口必须与之匹配。配置改窗口需同时改 nonce TTL 和迁移文档。
3. **nonce 去重是原子操作**：pkg/nonce.rs 的 `check_and_insert` 必须在同一个 Mutex lock 内完成查重 + 插入。不得拆为先查后插两步——并发同一 nonce 会穿透。静态确认函数内 HashMap 的 get + insert 在同一个 lock guard 作用域内。
4. **peer_did 必须与 organization_links 表记录一致**：验签通过后，从签名头的 key_id 提取 peer_did，必须反查到 organization_links（local_org_id=对端, peer_org_id=本端）上记录的 peer_did。不匹配 → 401 防跨连接冒充（攻击者用 A 的签名头调 B 的 endpoint，B 查自己的 link 表找不到 A → 拒绝）。
5. **能力门禁 SSOT：只从 federation_contracts 派生**：禁止在 organization_links 表另行配置 capabilities（迁移 000002 已将旧 link.capabilities 数据迁移到 federation_contracts）。任何代码路径不得 grep `organization_links.capabilities` 或 `link.capabilities`。Domain 层 org.rs 的能力校验必须先查 federation_contracts → state 检查 → capabilities 交集。
6. **token_hash/access_token 两列已 DROP，不再有链路共享密钥**：迁移 000001 执行 `ALTER TABLE organization_links DROP COLUMN token_hash` 和 `DROP COLUMN access_token`。任何残留代码引用这两列将编译失败——这是设计意图。
7. **federation_identity::resolve 纯函数不绑定传输层**：该函数输入是「已解析的四签名头字符串 + 已构造的待签串」，不读 axum::Request、不碰 ws::Frame。HTTP 中间件和 WS 握手层负责 HTTP 头/WsFrame → 纯数据的转换。federation_identity.rs 禁止出现 `use axum::` 或 `use tokio_tungstenite::` 导入。
8. **contract.state=terminated 时入站调用一律 403，不进入能力校验**：验签成功 → 查 federation_contracts → state=terminated → 立即返回 403 "contract terminated"，**不走到 capabilities 交集判定**。这是熔断语义——终止合约后连 "你有没有 A2A 任务能力" 都不问了，直接拒绝所有跨组织调用。
9. **nonce 进程内去重是单实例常态，多实例需换 Redis 时再动**：pkg/nonce.rs 顶部注释明确写了"方案明确不做多实例共享"。如果未来做多实例部署，**不要在 nonce.rs 里加 Redis 依赖**——应该新建 pkg/nonce_redis.rs 实现同一 `check_and_insert` trait，然后在 mod.rs 里做 feature flag 切换。保持纯算法层无外部依赖。
10. **crypto::did.rs 不得读取任何配置或 DB**：该模块只放纯算法——密钥生成、did:key 编解码、签名/验签、待签串拼接。密钥的生成时机（建联时调用）、持久化（organizations 表 encrypted_signing_key 列）、加解密（encrypt_channel_secret）——全部在 domain 层组装。静态 grep crypto/did.rs 无 `use sqlx` / `use sea_orm` / `use std::fs` 等导入。
11. **出站签名必须携带 task_token（异步场景）**：call_peer facade 发起异步 A2A delegation 时，HTTP 请求除了四签名头，还必须附加 `X-Federation-Task-Token` 头（值为 task_token.rs 生成的签名令牌）。对端收到后验签 + 解析出 task_id → 关联 FederationInboundTaskConsumer。同步调用不带 task_token。
