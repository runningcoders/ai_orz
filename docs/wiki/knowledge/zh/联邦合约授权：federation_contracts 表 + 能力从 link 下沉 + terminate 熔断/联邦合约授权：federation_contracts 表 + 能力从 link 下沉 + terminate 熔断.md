---
kind: RAG 原子知识卡
name: 联邦合约授权：federation_contracts 表 + 能力从 link 下沉 + terminate 熔断
category: 业务模块 / 联邦鉴权
scope:
  - "src/models/federation_contract.rs"
  - "src/service/dal/organization/contract.rs"
  - "src/service/dao/federation_contract/**"
  - "src/handlers/organization/contracts/**"
  - "src/service/domain/organization/org.rs"
  - "src/service/dal/organization/mod.rs"
  - "common/src/enums/organization.rs"
  - "migrations/20260909000003_*"
source_files:
  - src/models/federation_contract.rs#L1-L80 (FederationContractPo + capabilities_list() + has_capability() 方法；kind=basic / state=active|terminated)
  - src/service/dal/organization/contract.rs#L1-L100 (联邦合约 DAL 层：封装 FederationContractDao；find_active_by_pair / update_capabilities / terminate / list；Domain 层通过此子 DAL 消费合约能力)
  - src/service/dao/federation_contract/mod.rs#L1-L60 (FederationContractDao trait：insert / find_by_id / find_active_by_pair / update_capabilities / terminate / list)
  - src/service/dao/federation_contract/sqlite.rs#L1-L150 (SQLite 实现；unique(local_org_id, peer_org_id) 保证两组织间只有一条合约；terminate 是 UPDATE state=terminated + updated_at)
  - src/handlers/organization/contracts/list_contracts.rs (GET /api/v1/organization/contracts：列出本端全量 federation_contracts；按 local_org_id 隔离)
  - src/handlers/organization/contracts/terminate_contract.rs (POST /api/v1/organization/contracts/{id}/terminate：state → terminated；验签链路发现 terminated 直接 403 熔断)
  - src/handlers/organization/contracts/update_contract_capabilities.rs (PUT /api/v1/organization/contracts/{id}/capabilities：capabilities JSON 校验；非法解析返回空数组 fail-closed)
  - src/service/domain/organization/org.rs (capabilities 校验下沉：先查 federation_contracts(state) → terminated 403 → active 时 contract.capabilities 与请求能力交集；**不再查 link.capabilities**)
  - common/src/enums/organization.rs#L1-L200 (FederationContractKind::Basic=0 / FederationContractState::Active=0, Terminated=1；#[repr(i32)]+sqlx::Type)
  - migrations/20260909000003_federation_contracts.sql (CREATE TABLE federation_contracts — S3 迁移：DROP 旧 link.capabilities → 新表承载合约级能力 + 熔断 + 未来合同占位)
  - docs/plan/联邦鉴权升级方案.md#L1-L300 (SSOT S3 设计稿：合约授权 + terminate 熔断 + terms_hash 未来机制)
  - docs/wiki/zh/content/功能模块/用户与组织管理/跨组织业务调用鉴权.md (Wiki 长文：合约授权建模 + terminate 熔断时序)
  - 【总卡声明】docs/wiki/knowledge/zh/跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌/跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌.md (Level 4 总卡——描述签名基建和鉴权全链路，本卡是其子卡专注合约授权建模)
---

## §1 概述

本卡是 **【跨组织业务调用鉴权模型】** 总卡下的合约授权细卡（Level 4 总分关系）。描述：`federation_contracts` 新表承载连接级能力白名单（从旧 `organization_links.capabilities` JSON 字段下沉）、**basic 合约**（建联即自动成立，默认 state=active）、**terminate 熔断**（state=terminated 后验签链路直接 403，不进入能力校验）、`terms_hash` + `local_signature` + `peer_signature` 三列（为未来非互信合同机制占位，basic 合约时为空/固定常量）。

**定位**：排查 "为什么跨组织调用 403"（看合约 state）、理解 "能力白名单在哪改"（从 link 改到 contract）、排查 terminate 熔断为什么不生效、理解未来合同扩展点时读。

核心设计：
- **federation_contracts 表是能力门禁 SSOT**：从 organization_links 拆分出来后，能力白名单的唯一来源就是 federation_contracts.capabilities。任何代码路径不得绕过此表查能力。
- **basic 合约建联即自动成立**：create_link 事务内同步 INSERT federation_contracts（kind=basic, state=active, capabilities=DEFAULT '[]'）。管理员通过 `/contracts/{id}/capabilities` 端点显式追加能力。
- **terminate 熔断是硬拒绝**：验签链路发现 contract.state=terminated → 立即 403，**不走到 capabilities 交集判定**。终止合约 = 关闭所有跨组织调用通道，而不是只撤销某几个能力。
- **terms_hash + 双签名列是未来占位**：basic 合约时 terms_hash = `BASIC_CONTRACT_TERMS_HASH` 固定常量、local_signature/peer_signature 为空字符串。未来引入非互信合同机制时（双方需要明确签署条款），这三列承载合同哈希和双方签名，验证链路复用 S2 Ed25519 签名代码。

---

## §2 关键文件与职责表

| 文件 | 角色 | 内容摘要 | 源码锚点 |
|------|------|---------|---------|
| models/federation_contract.rs | 合约 PO | FederationContractPo：id + local_org_id + peer_org_id + kind(Basic) + state(Active/Terminated) + capabilities(JSON 数组) + terms_hash(常量占位) + local_signature + peer_signature(均为空)；new() → kind=basic, state=active, capabilities='[]'；capabilities_list() + has_capability() 方法 | `:L1-L80` |
| service/dal/organization/contract.rs | 子 DAL：合约持久化 | 封装 FederationContractDao；find_active_by_pair(local_org_id, peer_org_id) → state=active 的合约；update_capabilities(id, json) → 校验 JSON 合法后写入；terminate(id) → state=terminated + updated_at；list(local_org_id) → 全量按本端隔离 | `:L1-L100` |
| service/dao/federation_contract/mod.rs | 合约 DAO trait | insert / find_by_id / find_active_by_pair / update_capabilities / terminate / list | `:L1-L60` |
| service/dao/federation_contract/sqlite.rs | 合约 DAO SQLite 实现 | CREATE TABLE federation_contracts + unique(local_org_id, peer_org_id)；terminate 是 UPDATE state=terminated + updated_at=now；capabilities JSON 合法性在 update_capabilities 前置校验 | `:L1-L150` |
| handlers/organization/contracts/list_contracts.rs | 合约列表 | GET /api/v1/organization/contracts；按 local_org_id 隔离；列出所有 state（含 terminated），前端展示时可过滤 | 见文件 |
| handlers/organization/contracts/terminate_contract.rs | 终止合约 | POST /api/v1/organization/contracts/{id}/terminate；权限：本端 organization owner；不可恢复操作（本阶段 state 从 terminated 不可逆） | 见文件 |
| handlers/organization/contracts/update_contract_capabilities.rs | 更新能力 | PUT /api/v1/organization/contracts/{id}/capabilities；capabilities 必须是合法 JSON 字符串数组 → 非法返回 400；更新后验签链路生效 | 见文件 |
| service/domain/organization/org.rs | Domain 层能力门禁 | 验签成功后 → 子 DAL contract_dal.find_active_by_pair → None 或 state=terminated → 403 → Some(contract) → contract.capabilities_list() ∩ request.capabilities → 空则 403 | 见 org.rs |
| common/enums/organization.rs | 合约枚举 | FederationContractKind::Basic=0 / FederationContractState::Active=0, Terminated=1；#[repr(i32)] + sqlx::Type | `:L1-L200` |
| migrations/20260909000003_federation_contracts.sql | 合约表创建 | CREATE TABLE federation_contracts；包含所有 PO 字段 + unique(local_org_id, peer_org_id) 约束 | 见文件 |
| docs/plan/联邦鉴权升级方案.md | SSOT S3 | 合约授权设计稿；终止熔断；未来合同占位列设计 | `:L1-L300` |

---

## §3 架构约定

**首句声明**：本卡是 **Level 4 总卡-细卡关系**——总卡是【跨组织业务调用鉴权模型：did:key + Ed25519 签名 + 合约授权 + nonce 防重放 + 任务令牌】，总卡描述签名基建和鉴权全链路，本卡专注合约授权建模与 terminate 熔断机制。

### 合约生命周期

```
create_link (建联成功)
  → 在 create_link 事务内同步 INSERT federation_contracts
    kind=basic, state=active, capabilities='[]', terms_hash=BASIC_CONTRACT_TERMS_HASH
    local_signature='', peer_signature=''
  → 返回 contract_id 给 admin 前端

admin 追加能力
  → PUT /contracts/{id}/capabilities
    capabilities = '["a2a_task", "query_knowledge"]'
  → update_capabilities JSON 校验 → 合法则写入
  → 验签链路后续能力门禁生效

terminate 熔断（管理员操作）
  → POST /contracts/{id}/terminate
    state → terminated, updated_at = now
  → 验签链路 federation_identity::resolve 中：
    查 contract → state=terminated → 立即 403
    **不进入 capabilities 交集判定**
  → 本阶段 terminated 不可逆（后续如需恢复另开 endpoint）
```

### 验签链路中的合约检查点

在总卡 §3 签名验签链路中，合约检查位于「签名验证通过」之后、「能力交集判定」之前：

```
验签通过 → 查 federation_contracts(local_org_id=本端, peer_org_id=对端)
  ├─ 找不到合约 → 401 (理论上不会发生——create_link 同步创建；若发生说明数据不一致，fail-closed)
  ├─ state=terminated → 403 "contract terminated" (熔断，不问能力)
  └─ state=active → contract.capabilities_list() ∩ request.capabilities
      ├─ 交集为空 → 403 "capability not allowed"
      └─ 非空 → 放行，注入 FederationIdentity.capabilities = 交集
```

### 未来扩展点（非当前阶段实现）

- **terms_hash + local_signature + peer_signature**：basic 合约时 terms_hash=`BASIC_CONTRACT_TERMS_HASH`（固定常量，保证列非空）、双签名列为空字符串。未来引入非互信合同机制时（双方需要明确签署业务条款），terms_hash 是合同条款的 SHA256 哈希、local_signature/peer_signature 是双方 Ed25519 签名。验证链路复用 S2 crypto::did.rs 签名代码。
- **非 basic 合约 kind**：当前只有 `kind=basic`（建联即自动成立）。未来可扩展 `kind=negotiated`（双方协商后成立，需要双签验证）。枚举设计已预留扩展空间。

---

## §4 硬约束与回归红线

1. **能力白名单 SSOT：只从 federation_contracts 派生，禁止在 organization_links 另行配置**：迁移 000002 已将旧 link.capabilities 数据迁移到 federation_contracts 表，且迁移 000001 已 DROP token_hash/access_token 两列。任何代码路径（Domain / DAL / DAO / handler）不得访问 organization_links.capabilities。静态 grep `organization_links.*capabilities` 或 `link\.capabilities` 必须零命中——任何命中都是反模式。能力校验入口只有一个：`contract_dal.find_active_by_pair` → `contract.capabilities_list()`。
2. **contract.state=terminated 时入站调用一律 403，不进入后续能力校验**：验签成功后的合约检查点，state=terminated → 返回 403 "contract terminated"，**不走到 capabilities 交集判定**。这是熔断语义——终止合约后连 "你有没有 A2A 任务能力" 都不问了。如果代码走到 terminated 后还检查 capabilities，说明熔断逻辑断了。单元测试必须覆盖 terminated → 403 且不调 capabilities_list()。
3. **capabilities JSON 非法 → 解析返回空数组 → 全部拒绝（fail-closed）**：update_contract_capabilities handler 接收的 capabilities 字段必须是合法的 JSON 字符串数组（如 `'["a2a_task"]'`）。解析失败 → 返回 400 Bad Request + 不更新。**绝不能把非法 JSON 直接写入 DB**——如果 DB 里存了非法 JSON，验签链路 capabilities_list() 解析失败时应该返回空数组（全部拒绝，fail-closed），而不是 panic 或返回 Some(["*"])。单元测试覆盖：非法 JSON → DB 不变 → 后续调用 403。
4. **terms_hash/local_signature/peer_signature 在 basic 合约时为空/固定常量，不得在 basic 场景校验**：FederationContractPo::new() 中 terms_hash = BASIC_CONTRACT_TERMS_HASH（固定常量）、local_signature = ""、peer_signature = ""。验签链路和能力门禁逻辑里**不能有任何代码检查这三列**——只有未来非 basic 合约 kind 时才启用双签验证。静态 grep federation_identity.rs / a2a_auth.rs 不应匹配 terms_hash / local_signature / peer_signature。
5. **terminate 是不可逆操作，本阶段不提供恢复 endpoint**：state 从 active → terminated 是单向的。如果未来需要恢复（如误操作），另行开发 endpoint。**禁止在 handlers/organization/contracts/ 下新增 "restore_contract" 或 "reactivate_contract"**——直到设计稿明确拍板。
6. **合约创建必须与 create_link 在同一事务内**：create_link 时（B 侧），organization_links INSERT 和 federation_contracts INSERT 必须包在同一个 `.transaction(|tx| ...)` 里——成功则双写、失败则全回滚。如果 link 写成功但 contract 创建失败，验签链路查不到合约 → 入站调用 401，连接成了半残状态。静态 grep create_link 事务内应有 federation_contracts INSERT 调用。
7. **unique(local_org_id, peer_org_id) 约束双表一致**：organization_links 已经有 `unique(local_org_id, peer_org_id)`。federation_contracts 也必须加同样的 unique 约束——两个表的连接/合约必须一一对应。静态 grep migrations/20260909000003_federation_contracts.sql 应有 `UNIQUE(local_org_id, peer_org_id)`。
