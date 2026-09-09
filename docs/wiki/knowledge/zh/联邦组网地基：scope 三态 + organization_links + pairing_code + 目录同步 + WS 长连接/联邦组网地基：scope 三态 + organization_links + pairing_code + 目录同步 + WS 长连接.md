---
kind: RAG 原子知识卡
name: 联邦组网地基：scope 三态 + organization_links + pairing_code + 目录同步 + WS 长连接
category: 业务模块 / 联邦组网
scope:
  - "common/src/enums/organization.rs"
  - "common/src/api/organization_link.rs"
  - "src/models/organization_link.rs"
  - "src/models/organization_pairing_code.rs"
  - "src/service/dao/organization_link/**"
  - "src/service/dao/organization_pairing/**"
  - "src/service/dal/organization/**"
  - "src/service/domain/organization/org.rs"
  - "src/handlers/organization/links/**"
  - "src/consumer/federation_directory.rs"
  - "src/consumer/federation_ws_outbound.rs"
  - "src/consumer/federation_inbound_task.rs"
  - "src/models/events/federation.rs"
  - "migrations/202609040000*.sql"
  - "migrations/20260905000001*.sql"
source_files:
  - common/src/enums/organization.rs#L1-L183 (OrganizationScope 三态 Local/Linked/Remote；scope 字段扩展 + 默认值 Local；OrganizationLinkStatus 枚举定义)
  - common/src/api/organization_link.rs#L1-L329 (配对码 DTO：IssuePairingCodeRequest/Response；凭证交换 DTO：CreateLinkRequest/Response；FederationCallerDeclaration 身份声明；PAIRING_CODE_TTL_MS=600_000 + PAIRING_CODE_LEN=24)
  - src/models/organization_link.rs#L1-L89 (OrganizationLinkPo：连接契约表——local_org_id / peer_org_id / endpoint / access_token(明文出站) / peer_token_hash(SHA256 入站校验) / capabilities(白名单 JSON) / status)
  - src/models/organization_pairing_code.rs#L1-L28 (OrganizationPairingCodePo：配对码短时效单用途——org_id + code_hash(SHA256 仅存哈希) + expires_at + consumed_at(消费判定))
  - src/service/dao/organization_link/mod.rs#L1-L72 (OrganizationLinkDao trait：insert / find_by_id / find_by_pair(local+peer 唯一约束) / find_by_token_hash / list / revoke；OrganizationLinkQuery 结构体)
  - src/service/dao/organization_link/sqlite.rs#L1-L175 (OrganizationLinkDaoSqliteImpl：OnceLock 单例管理；所有 DAO 共享 OnceLock init 模式；按 pair 查询 + 按 token_hash 匹配 Active 连接)
  - src/service/dao/organization_pairing/mod.rs#L1-L34 (OrganizationPairingDao trait：insert + consume(原子消费——同时判定哈希匹配 + 未消费 + 未过期 + 置 consumed_at + 返回签发 org_id)；错误统一返回 None 防枚举探测)
  - src/service/dao/organization_pairing/sqlite.rs#L1-L77 (配对码 SQLite 实现：consume 单条 UPDATE 原子完成四判定；任何不匹配返回 None 由上层转 Error::unauthorized)
  - src/service/domain/organization/org.rs#L1-L1294 (OrganizationManage 联邦扩展：issue_pairing_code(签发配对码 + SHA256 哈希入库) / verify_pairing_code(调用 PairingDao::consume) / create_link(凭证双向交换 + shadow upsert 对端组织 + 目录拉取) / list_links / revoke_link / push_directory_to_peers / reconcile_directories；call_peer facade 联邦调用入口)
  - src/service/dal/organization/mod.rs#L1-L177 (OrganizationDal 总 trait：组织 CRUD + 两种影子静默 upsert(upsert_remote_shadow/upsert_linked_shadow，均不发事件) + revoke_link + send_federated_agent_task 联邦委派传输；OnceLock 单例 + init 入口；link/pairing 子 DAL 引用导出)
  - src/service/dal/organization/impl.rs#L1-L362 (OrganizationDalImpl：实现 OrganizationDal 总 trait；组合 OrganizationDao(organizations 表) + OrganizationLinkDao(links 表，revoke_link 组合用)；FEDERATED_CALL_DEADLINE_SECS=120 + POLL_INTERVAL_MS=1000 配置；send_federated_agent_task 经 link.access_token 出站 A2A + 轮询 tasks/get 到终态)
  - src/service/dal/organization/link.rs#L1-L198 (OrganizationLinkDal trait + 实现：**全新子 DAL**——组合 OrganizationLinkDao(持久化) + FederationHttpClient(对端 HTTP 出站)；links CRUD：find_by_pair / insert / update / query / find_active_by_peer_token_hash；联邦出站 HTTP：verify_pairing_code / fetch_directory / push_directory / fetch_capabilities；Domain 层不再直捅 DAO/HTTP client)
  - src/service/dal/organization/pairing.rs#L1-L75 (OrganizationPairingDal trait + 实现：**全新子 DAL**——封装 OrganizationPairingDao；insert(签发配对码哈希入库) + consume(原子消费：哈希匹配+未消费+未过期+置 consumed_at+返回签发 org_id，任何不匹配返回 None 防枚举))
  - src/handlers/organization/links/mod.rs#L1-L27 (联邦 Handler 模块入口：8 个端点——issue_pairing_code / verify_pairing_code / create_link / list_links / revoke_link / get_directory / sync_directory / get_capabilities / federation_ws；generate_http_handler 宏标注，**不注册 register_handler_tool** 防 Agent 误触组网)
  - src/consumer/federation_directory.rs#L1-L72 (FederationDirectoryConsumer：订阅 EventKind["organization.changed"] → push_directory_to_peers；best-effort 全量推送所有 Active 对端)
  - src/consumer/federation_ws_outbound.rs#L1-L79 (FederationWsOutboundConsumer：订阅 EventKind["federation.outbound"] → ws::connection push 出站帧；无活连接时告警丢弃——命令发起方应先查注册表决定走 WS 还是回退 HTTP)
  - src/consumer/federation_inbound_task.rs#L1-L132 (FederationInboundTaskConsumer：订阅 EventKind["federation.inbound.send_task"] → 复用 handle_send_task(ctx, params) 核心函数(HTTP/Domain 零改动) → publish FederationOutboundEvent 响应帧)
  - src/models/events/federation.rs (FederationOutboundEvent / FederationInboundEvent / FederationFrame 事件类型定义；CALLER_AUDIT_KEY 审计字段)
  - migrations/20260904000001_add_group_name_to_org.sql (ALTER TABLE organizations ADD COLUMN group_name TEXT NOT NULL DEFAULT '' — 集团展示标签)
  - migrations/20260904000002_create_organization_links.sql (CREATE TABLE organization_links — 连接契约表)
  - migrations/20260904000003_create_organization_pairing_codes.sql (CREATE TABLE organization_pairing_codes — 配对码表)
  - migrations/20260904000004_add_capabilities_to_organization_links.sql (ALTER TABLE organization_links ADD COLUMN capabilities TEXT NOT NULL DEFAULT '[]' — 连接级能力白名单)
  - migrations/20260905000001_organizations_addresses.sql (ALTER TABLE organizations ADD COLUMN addresses TEXT NOT NULL DEFAULT '[]' — 多地址自报 + scope 列 ALTER)
  - docs/plan/组织组网与去中心化联邦方案.md#L1-L245 (Phase 1 评审稿：ADR D1-D7 + scope 三态数据模型 + 配对码协议 + 分阶段实施；集团=group_name 纯展示标签)
  - src/pkg/crypto/did.rs#L1-L150 (签名基建被 WS 消费——Ed25519 did:key 密钥对 + 签名原语；WS 握手时对端用此签名)
  - src/pkg/nonce.rs#L1-L60 (签名基建被 WS 消费——进程内 nonce 去重防重放；WS 握手四头之一)
  - src/middleware/federation_identity.rs#L1-L200 (**增量**：WS 长连接握手阶段调用同一签名验签函数——HTTP 鉴权和 WS 握手共用 federation_identity::resolve；签名四头 X-Federation-Key-Id / Timestamp / Nonce / Signature；nonce LRU + timestamp ±300s 窗口)
  - docs/wiki/zh/content/功能模块/用户与组织管理/组织组网与联邦.md (联邦组网长文：架构总览 + Mermaid 时序图 + 核心组件详解 + ADR 决策 + 安全约束 + 故障排查 5 条)
  - docs/wiki/zh/content/功能模块/用户与组织管理/用户与组织管理.md (用户组织管理全景：scope 三态扩展 + 组织间组网关系说明)
  - docs/wiki/zh/content/架构设计/分层架构设计/Domain 层编排/Organization 领域编排.md (OrganizationManage trait 扩展：联邦能力 + 静默 shadow upsert 与事件发布分离)
  - 【兄弟卡】docs/wiki/knowledge/zh/跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌/跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌.md (鉴权子主题：组网地基负责连接建立与身份密钥落库 → 鉴权模型负责跨组织调用时的 Ed25519 签名验签 + 合约能力门禁；WS 握手复用鉴权模型的签名验签链路)
  - 【关联卡】docs/wiki/knowledge/zh/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步/组织权限与用户偏好：Organization多级 + UserRole并查集继承 + JWT双模式 + 偏好双源沉淀 + Agent入职五步.md (scope 三态扩展 + federation 相关 §4 硬约束补充)
---

## §1 概述

**本卡角色**：联邦组网的地基知识卡。覆盖 OrganizationScope 三态（Local/Linked/Remote）扩展组织表语义、organization_links 点对点连接契约表、配对码（pairing code）短时效单用途握手协议、目录推拉结合同步（推送保证时效 + cron 定时对账保证最终一致）、WS 长连接（pkg::ws 通用管理器 + adapter 模式业务解耦）四层。**定位：新增组织间组网能力、排查建联失败、调试目录同步卡住、理解 shadow upsert 静默写入时读。**

- **DAL 层子模块拆分（2026-09 拆分重构，学习 agent/ / lark/ 模式）**：`src/service/dal/organization.rs` 单文件 → `src/service/dal/organization/` 4 个子模块。`OrganizationLinkDal`（link.rs）吸收 OrganizationLinkDao + FederationHttpClient，`OrganizationPairingDal`（pairing.rs）封装 OrganizationPairingDao，Domain 层不再直捅 DAO/HTTP。总 trait + 单例管理留在 mod.rs。详见 §3 DAL 子模块拆分模式。
- **OrganizationScope 三态扩展（common/src/enums/organization.rs）**：`Local(0)` 本地组织（自建/自管，默认值）→ `Linked(1)` 已建联对端（organizations 表影子记录，scope=Linked 表示已建 organization_links 连接契约，双向可通信）→ `Remote(2)` 仅目录同步所得（影子记录，未建联，只读展示）。scope 决定"能否通信"——只有 Linked 可发跨组织命令。**集团=group_name 纯展示标签，不参与任何逻辑判断**（ADR D1，消解分布式一致性问题）。
- **连接契约与实体分离（ADR D4）**：organizations 表描述组织本身（高频被全系统 join）；organization_links 表承载点对点连接的 endpoint + 双向凭证（access_token 明文出站 + peer_token_hash SHA256 入站校验）+ capabilities 连接级能力白名单 JSON。凭证不进 organizations 表，防止放大泄漏面。唯一约束 `(local_org_id, peer_org_id)` 保证两个组织间只有一条有效连接。
- **配对码协议（ADR D5，复用邀请码范式）**：签发（用户侧 JWT）→ verify + 凭证交换（机器侧，配对码鉴权）→ create_link（双向凭证落库 + shadow upsert 对端影子记录 + 目录拉取）。**配对码 24 字符去 0/O/1/I 字符集 + 10 分钟 TTL + 用后即焚**。`OrganizationPairingDal::consume` 单条 UPDATE 原子完成四判定（哈希匹配 + 未消费 + 未过期 + 置 consumed_at），任何不匹配返回 None——上层统一转 `Error::unauthorized`，**不区分原因防枚举探测**（评审稿 §6.3）。
- **shadow upsert 静默写入（src/service/dal/organization/impl.rs）**：对端组织影子写入 organizations 表时走 `OrganizationDal::upsert_remote_shadow` / `upsert_linked_shadow`，**不发事件**——与普通组织创建（发 `organization.changed` 事件触发 FederationDirectoryConsumer）严格分离，防止影子记录无限触发推送。静默写入逻辑封装在 organization DAL 层，domain 调用时显式走静默路径。
- **目录推拉结合同步**（src/consumer/federation_directory.rs + scheduler cron）：① **推送保证时效**——本地组织变更（创建/更新/删除）→ publish `organization.changed` → FederationDirectoryConsumer → `push_directory_to_peers` 全量推送所有 Active 对端（best-effort，推送失败不阻断主流程）；② **cron 定时对账保证最终一致**——SchedulerConsumer 每分钟触发 `directory_reconcile` → 查所有 Active 连接 → 双向 GET directory → 对比差异 → 差异方 pull 补齐。两条链路同源（最终调 OrganizationManage::push_directory_to_peers / reconcile_directories），推送快、对账稳。
- **WS 长连接架构（P8 落地）**：`pkg::ws` 通用管理器（client 侧 supervisor 指数退避重连 + 心跳 + 读循环；server 侧被动接受 + 心跳 + 优雅关闭）**不含任何业务语义**——帧解析与处置由 `WsClientAdapter` / `WsServerHandler` adapter 实现方全权解释。联邦 WS 出站 consumer 订阅 `federation.outbound` → ws::connection push 帧；入站 consumer 订阅 `federation.inbound.send_task` → 复用 HTTP send_task 核心函数。**命令发起方（call_peer facade）先查注册表决定走 WS 还是回退 HTTP**——无活连接时 WS consumer 告警丢弃不重试，避免自动 fallback 掩盖问题。**【增量 2026-09 签名升级】WS 长连接握手现在复用每请求签名链路（Ed25519 四头协议 + nonce 防重放 + timestamp ±300s 窗口），与 HTTP 鉴权同一套 federation_identity::resolve 函数，彻底避免漂移。**

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| common/enums/organization.rs OrganizationScope | scope 三态枚举 | Local=0(默认) / Linked=1(已建联) / Remote=2(仅目录影子)；#[repr(i32)]+sqlx::Type；scope 列 migration ALTER | `:L1-L183` |
| common/api/organization_link.rs DTO 单一事实源 | 配对码 + 凭证交换 + 身份声明 | PAIRING_CODE_TTL_MS=600_000(10min) / PAIRING_CODE_LEN=24；IssuePairingCodeRequest/Response(签发) / VerifyPairingCodeRequest/Response(验证) / CreateLinkRequest/Response(建联) / FederationCallerDeclaration(明文身份声明 JSON，可选) | `:L1-L329` |
| models/organization_link.rs OrganizationLinkPo | 连接契约 PO | local_org_id + peer_org_id + endpoint + access_token(明文出站 32 字节随机) + peer_token_hash(SHA256 入站校验) + capabilities(JSON 数组) + status + (local_org_id, peer_org_id) 唯一约束 | `:L1-L89` |
| models/organization_pairing_code.rs OrganizationPairingCodePo | 配对码 PO | org_id + code_hash(仅存 SHA256) + expires_at + consumed_at(消费判定) | `:L1-L28` |
| service/dao/organization_link/mod.rs OrganizationLinkDao | 连接契约 DAO trait | insert / find_by_id / find_by_pair / find_by_token_hash(机器侧端点鉴权) / list / revoke | `:L1-L72` |
| service/dao/organization_pairing/mod.rs OrganizationPairingDao | 配对码 DAO trait | insert + consume(原子消费：哈希匹配 + 未消费 + 未过期 + 置 consumed_at + 返回签发 org_id；错误统一 None) | `:L1-L34` |
| service/domain/organization/org.rs OrganizationManage 联邦扩展 | Domain 层联邦能力编排 | issue_pairing_code(Rand 24 字符 → SHA256 → INSERT) / verify_pairing_code(consume) / create_link(双向凭证生成 + shadow upsert 对端影子 + 目录拉取) / push_directory_to_peers / reconcile_directories | `:L1-L1294` |
| service/dal/organization/mod.rs OrganizationDal | DAL 总 trait + 单例 | 组织 CRUD / upsert_remote_shadow(静默 Remote) / upsert_linked_shadow(静默 Linked) / revoke_link(连接置 Revoked + 影子降 Remote) / list_addresses / resolve_peer_endpoint / send_federated_agent_task(联邦委派传输)；link/pairing 子 DAL 引用导出 | `:L1-L177` |
| service/dal/organization/link.rs OrganizationLinkDal | **新** 子 DAL：连接持久化 + 联邦出站 HTTP | 组合 OrganizationLinkDao + FederationHttpClient；links CRUD(find_by_pair/insert/update/query/find_active_by_peer_token_hash) + 对端出站 HTTP(verify_pairing_code / fetch_directory / push_directory / fetch_capabilities)；Domain 不再直捅 DAO/HTTP | `:L1-L198` |
| service/dal/organization/pairing.rs OrganizationPairingDal | **新** 子 DAL：配对码持久化 | 封装 OrganizationPairingDao；insert(哈希入库) + consume(原子消费四判定 + 不区分原因返回 None 防枚举) | `:L1-L75` |
| handlers/organization/links/mod.rs 联邦 Handler 模块 | 8 端点 HTTP 接口 | 统一前缀 /api/v1/organization/links/*；generate_http_handler 宏标注，**不注册 register_handler_tool** | `:L1-L27` |
| consumer/federation_directory.rs FederationDirectoryConsumer | 目录推送消费者 | 订阅 EventKind["organization.changed"] → push_directory_to_peers best-effort | `:L1-L72` |
| consumer/federation_ws_outbound.rs FederationWsOutboundConsumer | WS 出站帧投递 | 订阅 EventKind["federation.outbound"] → ws::connection push；无活连接告警丢弃 | `:L1-L79` |
| consumer/federation_inbound_task.rs FederationInboundTaskConsumer | WS 入站命令执行 | 订阅 EventKind["federation.inbound.send_task"] → 复用 handle_send_task → publish response 帧 | `:L1-L132` |
| pkg/ws/mod.rs 通用 WS 管理器 | WS 基建层 | client: supervisor 指数退避重连 + 心跳 + 读循环；server: 被动接受 + 心跳；adapter 模式业务解耦；**不含业务语义** | `:L1-L635` |
| middleware/federation_identity.rs (增量) | WS 握手签名验签 | WS upgrade 握手阶段调用 federation_identity::resolve —— 复用 HTTP 鉴权的 Ed25519 四头验签链路（timestamp ±300s + nonce LRU + peer_did 匹配）；HTTP 和 WS 共用同一纯函数 | `:L1-L200` |
| pkg/crypto/did.rs (增量) | Ed25519 did:key 签名原语 | WS 握手对端用此签名；build_signing_string / verify_signature 被 federation_identity::resolve 消费 | `:L1-L150` |
| pkg/nonce.rs (增量) | nonce 防重放 | WS 握手四头之一；进程内 LRU HashMap 去重，600s TTL，65536 容量上限 | `:L1-L60` |
| migrations/20260904000001-000004 + 20260905000001 | 5 个联邦迁移 | group_name(展示) → organization_links(连接) → organization_pairing_codes(配对码) → capabilities(白名单) → addresses(多地址自报 + scope ALTER) | 见 migration files |
| docs/plan/组织组网与去中心化联邦方案.md | Phase 1 评审稿 | ADR D1-D7 + scope 三态数据模型 + 配对码协议 + 分阶段实施；集团=group_name 纯展示标签 | `:L1-L245` |

**章节来源**
- [organization.rs:L1-L183](common/src/enums/organization.rs#L1-L183)
- [org.rs:L1-L1294](src/service/domain/organization/org.rs#L1-L1294)
- [mod.rs:L1-L177](src/service/dal/organization/mod.rs#L1-L177)
- [link.rs:L1-L198](src/service/dal/organization/link.rs#L1-L198)
- [pairing.rs:L1-L75](src/service/dal/organization/pairing.rs#L1-L75)

---

## §3 架构约定

本卡为联邦组网的主主题，与 **【跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌】** 互为兄弟卡（Level 3 视角兄弟卡）——本卡负责**连接建立与身份密钥落库**（scope 三态 + 连接表 + 配对码 + 目录 + WS 长连接生命周期），兄弟卡负责**跨组织调用时的签名验签与合约能力门禁**（Ed25519 每请求签名 + 合约授权 + nonce 防重放 + 任务令牌）。**WS 长连接握手现在复用兄弟卡的签名验签链路**——HTTP 和 WS upgrade 握手共用 federation_identity::resolve 纯函数，彻底避免鉴权逻辑漂移。

### 分层架构（Adapter → Domain → DAL → DAO，单向）

```mermaid
graph TB
subgraph "Adapter(HTTP/AOP)"
H1["handlers/organization/links/*<br/>generate_http_handler 宏<br/>无 register_handler_tool"]
H2["FederationDirectoryConsumer<br/>订阅 organization.changed"]
H3["FederationWsOutboundConsumer<br/>订阅 federation.outbound"]
H4["FederationInboundTaskConsumer<br/>订阅 federation.inbound.send_task"]
end
subgraph "Domain"
D["OrganizationManage 联邦扩展<br/>issue/verify/create_link<br/>push_directory_to_peers<br/>call_peer facade"]
end
subgraph "DAL（organization/ 目录拆分，学习 agent/ / lark/ 模式）"
L["OrganizationDal (mod.rs)<br/>总 trait + 单例管理"]
LL["OrganizationLinkDal (link.rs)<br/>连接 CRUD + 对端出站 HTTP"]
LP["OrganizationPairingDal (pairing.rs)<br/>配对码 insert + consume"]
end
subgraph "DAO"
C1["OrganizationLinkDao<br/>organization_links 表"]
C2["OrganizationPairingDao<br/>organization_pairing_codes 表"]
C3["OrganizationDao<br/>organizations 表<br/>(shadow upsert)"]
end
subgraph "pkg::ws"
WS["WsClientAdapter / WsServerHandler<br/>通用管理器不含业务语义"]
end
H1 --> D
H2 --> D
H3 --> WS
H4 --> D
D --> L
D --> LL
D --> LP
L --> C3
LL --> C1
LP --> C2
D --> WS
```

### DAL 层子模块拆分模式

`src/service/dal/organization/` 从单文件拆为 4 个子模块（**学习 `agent/` / `lark/` 目录模式**），职责按消费面纵向切分：

| 子模块 | 职责 | 依赖 |
|--------|------|------|
| `mod.rs` (177行) | `OrganizationDal` 总 trait + OnceLock 单例 + `init()` 入口；re-export 子 DAL 供 Domain 引用 | OrganizationDao + OrganizationLinkDao（revoke_link 组合用） |
| `impl.rs` (362行) | `OrganizationDalImpl` 实现总 trait；组织 CRUD + 两种影子静默 upsert + `send_federated_agent_task`（联邦委派传输） | OrganizationDao |
| `link.rs` (198行) | **全新** `OrganizationLinkDal`——组合 OrganizationLinkDao（持久化）+ `FederationHttpClient`（对端 HTTP 出站）；links CRUD + 对端出站 4 个 HTTP 方法 | OrganizationLinkDao + FederationHttpClient |
| `pairing.rs` (75行) | **全新** `OrganizationPairingDal`——封装 OrganizationPairingDao；insert + consume | OrganizationPairingDao |

**上层 Domain 的消费面简化**：`OrganizationDomainImpl` 原来持有 `link_dao` / `pairing_dao` / `http_client` 三个字段 → 现在只需 `link_dal` / `pairing_dal` 两个子 DAL 字段。持久化与出站 HTTP 的边界被 DAL 子模块吸收，Domain 不再直捅 DAO 或拼装 HTTP 调用。**反向约束**：子 DAL 对外只暴露 trait，DAO 和 HTTP 客户端都是 struct 私有字段——Domain 层代码 grep `link_dao` / `http_client` 应在 org.rs 中零命中。

**init 顺序**：`organization::init()` 先调 `link::init()` + `pairing::init()` 完成子 DAL 单例初始化，再构建 `OrganizationDal` 总单例——保证任何后续 `dal()` 调用都是已就绪的。

### 组网时序（配对码 → 建联 → 目录同步）

```
A 组织管理员发起配对
  → POST /links/issue_pairing_code (JWT)
  → OrganizationManage::issue_pairing_code
    → Rand 24 字符 → SHA256 → OrganizationPairingDal::insert
    → 返回 pairing_code + expires_at(10min) + ttl_seconds
  → 管理员把配对码给 B 组织

B 组织管理员验证配对码
  → POST /links/verify (machine-side, pairing_code in body)
  → OrganizationManage::verify_pairing_code
    → OrganizationPairingDal::consume(code_hash, now)
      → 原子 UPDATE: hash 匹配 + 未消费 + 未过期 → 置 consumed_at
      → 返回 A 的 org_id
    → OrganizationLinkDal::verify_pairing_code(调 A 端点交换凭证)
    → 返回 A 的 endpoint + org_id + B 的 peer_token_hash

凭证交换 → 建联
  → POST /links/create (B 侧, machine-side)
  → OrganizationManage::create_link
    → 生成 access_token(32 字节随机) + B 存 peer_token_hash(SHA256)
    → 双向凭证交换完成
    → OrganizationLinkDal::insert (Active 连接)
    → OrganizationDal::upsert_linked_shadow(A 作为影子 scope=Linked)
      → **静默写入，不发事件**
    → OrganizationLinkDal::fetch_directory(A endpoint, access_token)
      → 拉 A 的组织目录 → 对端静默影子 upsert
    → publish organization.changed(B) → FederationDirectoryConsumer
      → push_directory_to_peers(A) → OrganizationLinkDal::push_directory(A, 全量推 B 目录)
  → 完成，双向可通信
```

### WS 长连接命令帧投递

```
call_peer facade (发起方，domain/organization/org.rs)
  → 查 connection registry: 有活 WS → publish federation.outbound(event_id, frame, peer_org_id)
    → FederationWsOutboundConsumer → ws::connection push frame
    → 无活连接告警丢弃（发起方决定是否回退 HTTP）

接收方 WS 读循环 (pkg::ws 通用管理器)
  → adapter 解析帧 → EventKind["federation.inbound.send_task"]
    → FederationInboundTaskConsumer.consume
      → 复用 handle_send_task(ctx, params) (HTTP/Domain 零改动)
      → publish FederationOutboundEvent(response frame, event_id=请求方 event_id 配对)
        → 出站 consumer → 推回发起方
```

---

## §4 硬约束与回归红线

1. **配对码 consume 错误统一返回 None，上层转 Error::unauthorized，绝不区分无效/过期/已使用**：任何区分原因的错误返回都会被攻击者用来探测（枚举哪些配对码已过期/已用）。单元测试必须覆盖三种不匹配路径，都断言返回 None。
2. **shadow upsert 对端影子必须走 OrganizationDal 静默方法（upsert_remote_shadow / upsert_linked_shadow），不发事件**：如果 shadow upsert 发 `organization.changed`，FederationDirectoryConsumer 会把影子记录再次推送出去，形成无限循环推送风暴。代码走静态路径：create_link 里显式调 `organization::dal().upsert_linked_shadow(...)` 而非 `organization::domain().create_organization(...)`。
3. **organizations + organization_links 必须在同一个事务内写入**：create_link 时（B 侧），peer_org 影子 upsert 与 OrganizationLinkPo insert 必须包在同一个 `.transaction(|tx| ...)` 里——成功则双写、失败则全回滚。如果连接写成功但影子写失败，会出现"有连接但查不到对端组织"的半残状态，导致后续目录拉取 NPE。
4. **access_token 出站明文存本地，peer_token_hash 入站只存 SHA256**：凭证交换是对称的——A 生成 access_token 给 B，A 自己只存 peer_token_hash = SHA256(token_B)（B 调 A 时带 token_B，A 哈希后匹配）。**禁止任何一侧存对方 token 的明文**——明文泄漏（DB dump / 日志 / debug build）会导致伪造跨组织调用。
5. **capabilities 白名单默认值 '[]' 表示无能力开放**：新建联默认 capabilities='[]'（JSON 空数组字符串）——所有跨组织调用一律 403。必须显式追加允许的能力（如 `["a2a_task"]`）后才放行。这是 fail-closed 设计，防止建联即默认开放全部能力。
6. **handlers/organization/links/* 只标 generate_http_handler，绝不注册 register_handler_tool**：联邦组网端点是**管理员操作**，Agent 误触（如"帮我删了这个组织的连接"）是高危操作。Handler 模块显式无 `register_handler_tool!` 宏调用——该模块所有端点不会出现在 Agent 可调用工具列表里。全局 grep `register_handler_tool` 不应匹配任何 links 目录下的文件。
7. **call_peer facade 先查 WS registry 决定走 WS 还是回退 HTTP，WS 出站 consumer 无活连接时只告警不重试**：WS consumer 的职责是「已决定走 WS 的帧投递」，不负责策略判断。如果 consumer 检测无活连接时自动重试 HTTP fallback，会掩盖「发起方本该提前发现无连接就不走 WS」的设计缺陷，导致断线时静默失败。consumer 日志 WARN "no active WS connection for peer_org_id=xxx, dropping federation.outbound event"，由运维排障。
8. **pkg::ws 通用管理器严禁嵌入任何业务语义**：pkg::ws/mod.rs 和 server.rs 是纯 WS 生命周期组件——只关心建连、心跳、重连、读循环、优雅关闭。帧解析、业务帧类型映射、错误转业务错误——全部交给 WsClientAdapter / WsServerHandler 实现方。任何在 pkg::ws 模块里出现 "federation" / "lark" / "agent" 等业务关键词直接 fail。
9. **scope 列默认值 Local + scope=Linked/Remote 的判定规则不得修改**：现有任何代码不得新增 scope 比较逻辑，也不得把 group_name 作为判定条件。`WHERE scope IN (0, 1)` 才是"本地 + 已建联"，`scope = 1` 才是"已建联可通信"。禁止代码里出现 `scope == 2` 时执行写操作（Remote 是只读影子）。
10. **目录推送 + cron 对账双链路必须同同源最终调 OrganizationManage 方法**：推送走 `push_directory_to_peers`，对账走 `reconcile_directories` — 两条链路最终汇总到同一个 Domain 方法。禁止各 consumer 自己拼 HTTP 推送逻辑（逻辑分叉会导致 bug 难以排查）。
11. **Domain 层不得直捅 DAO（OrganizationLinkDao / OrganizationPairingDao）或直接拼装 FederationHttpClient**：DAL 子模块拆分后，Domain 的消费面只能是 `link_dal` / `pairing_dal` / `org_dal` 三个字段。DAO 和 HTTP 客户端封装在 DAL 子模块的 struct 私有字段里，不对外暴露。**代码审查时检查 org.rs 的 struct 定义——不应有 `link_dao` / `pairing_dao` / `http_client` 字段**（Grep 零命中）。新增对端 HTTP 方法时扩展 `OrganizationLinkDal` trait，不要在 Domain 里直接加 HTTP 调用。

---

## §5 历史演进

### 2026-09：DAL 层子模块拆分（organization.rs → organization/ 目录）

**拆分前**：`src/service/dal/organization.rs` 单文件 515 行，混合三类职责：① Organization CRUD + 影子静默 upsert（organizations 表）、② links 表 CRUD、③ 对端 HTTP 出站调用（verify_pairing_code / fetch_directory / push_directory / fetch_capabilities）。Domain 层 `OrganizationDomainImpl` 直接持有 `link_dao: Arc<OrganizationLinkDao>` + `pairing_dao: Arc<OrganizationPairingDao>` + `http_client: Arc<FederationHttpClient>` 三个字段，持久化与出站 HTTP 的边界跨 DAL 层被 Domain 吸收。

**拆分后**（4 个子模块 + 总 trait，共 812 行）：

| 文件 | 行数 | 职责 | 变化 |
|------|------|------|------|
| `organization/mod.rs` | 177 | OrganizationDal 总 trait + OnceLock 单例 + init() + re-export | 新增：两种静默方法 upsert_remote_shadow / upsert_linked_shadow（替代原来的 upsert_peer_org）；send_federated_agent_task 联邦委派传输 |
| `organization/impl.rs` | 362 | OrganizationDalImpl 实现 | 原有 organizations 表 CRUD + 影子静默写入 + FEDERATED_CALL_DEADLINE_SECS 配置 + send_federated_agent_task |
| `organization/link.rs` | 198 | OrganizationLinkDal（**全新**） | 吸收 OrganizationLinkDao + FederationHttpClient；links CRUD + 4 个对端出站 HTTP 方法 |
| `organization/pairing.rs` | 75 | OrganizationPairingDal（**全新**） | 封装 OrganizationPairingDao；insert + consume |

**对 Domain 的影响**：`OrganizationDomainImpl` 字段从 `link_dao` / `pairing_dao` / `http_client` 三字段 → `link_dal: Arc<dyn OrganizationLinkDal>` / `pairing_dal: Arc<dyn OrganizationPairingDal>` 两字段。所有 `self.link_dao.find_by_pair(...)` → `self.link_dal.find_by_pair(...)`，HTTP 调用从 `self.http_client.verify_pairing_code(...)` → `self.link_dal.verify_pairing_code(...)`。Domain 与底层 DAO/HTTP 客户端的耦合通过 DAL 子模块 trait 隔离。

**设计动机**：学习 agent/（mod.rs + impl.rs + a2a.rs + codex.rs + runtime.rs）和 lark/（mod.rs + impl.rs + credentials.rs + listener.rs）的目录拆分模式——职责按消费面纵向切分，每个子模块是自包含的 trait + OnceLock + 实现。organization.rs 在联邦组网上线后膨胀到 515 行且同时持有持久化和出站两类职责，拆分为后续新增 FederationHttpClient 方法（如 P7 多地址探测、P8 WS 出站能力下沉）提供干净的扩展点。

**init 顺序约束**：`organization::init()` 必须先调 `link::init()` 和 `pairing::init()` 完成子 DAL 的 OnceLock 设置，再构建总 `OrganizationDal` 单例——保证任何 Consumer / Domain 在 startup 完成后调 `organization::link::dal()` 拿到的是非 None 的 Arc。
