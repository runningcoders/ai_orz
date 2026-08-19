# 用户身份凭证独立表设计（user_credentials）

> 🎯 **定位**：① Design 决策快照——记录「users.identity_credentials JSON 列拆分为独立 user_credentials 表」的动机、表结构契约（资产 + 可见性 + 作用域派生的默认标记，外部绑定归使用方）、kind/visibility 枚举存储决策（项目首个 TEXT 字符串枚举先例）
> 状态：定稿 v1.0（2026-08-19，经 v0.2 职责二分 → v0.3 默认双语义 → v0.4 作用域优先链序三轮迭代收敛）
> 触发场景：实现/修改用户身份凭证 CRUD、新增凭证类型、评估凭据可见性/绑定关系归属、评估 kind 枚举存储方式（TEXT vs INTEGER）、或评估 JSON 列 vs 独立表取舍时打开
>
> 关联文档：
> - [AGENTS.md](../../AGENTS.md) — §4.3 枚举类型安全、§4.4 SQLite 规范、§3.5 软删除约定
> - [SQL 规范](sqlx_guide.md) — STRICT 表、sqlx::Type 派生、`.sqlx` 纳管
> - [身份凭证Domain统一CRUD重构](../archive/plan-archive/身份凭证Domain统一CRUD重构.md) — 前序落地快照（JSON 列方案），本设计取代其存储层
> - 暂无对应 plan 文档（落地时补写）

---

## 一、设计目标

### 1.1 设计哲学

凭证是用户级资产，随工具生态持续扩容（Lark → GitHub → Tavily → …）。JSON 列把「资产集合」压进一个单元格，集合的每次增删改都要整列读-改-写；独立表把每条凭证还原为一行，让数据库的行级并发控制与索引约束直接发挥作用。

**职责二分（v0.2 确立，v0.3 精化）**：凭据表只承载「资产 + 自身属性」——什么类型（kind）、什么内容（detail）、对谁可见（visibility）、是否所在作用域默认（is_default，**作用域由 visibility 派生**：private→个人默认 / public→组织默认，D15）；「谁在用哪条凭据」的**外部使用方绑定**（Agent、工具、渠道）一律归使用方实体，以 `credential_id` 引用传递。判定标准：引用外部实体的维度（agent_id、bound_tool…）上凭据表必膨胀——这正是 JSON 方案三个 `default_*_id` 槽位问题的结构性复发；而默认标记的作用域（owner/org）是表内在维度（列已存在），零新增引用，不在此列。

枚举存储上，`kind` / `visibility` 是**分类型枚举**（映射外部系统/访问语义），与 `TaskStatus` 类**状态型枚举**（内部状态机）本质不同——分类型用 TEXT 字符串，状态型沿用 INTEGER，本次为该区分立下首个先例。

### 1.2 关键决策表

| # | 问题 | 方案 | 原因 |
|---|------|------|------|
| D1 | JSON 列读-改-写并发丢更新（lost update） | 拆独立表 `user_credentials`，行级 CRUD | JSON 整列覆盖使两个并发保存互相吞掉对方写入；行级 INSERT/UPDATE 天然消解，无需乐观锁 |
| D2 | 默认凭证槽位随类型膨胀（schema 膨胀） | **单一 `is_default` 列吸收全部默认槽位**（作用域由 visibility 派生，D15）；存量三个 `default_*_id` 槽位迁移为对应凭据行 `is_default=1`（个人默认语义保留，存量用户零行为变化） | JSON 顶层每加一种凭证类型就要加一个 `default_xxx_credential_id` 字段（[现状已 3 个](../../common/src/models/identity_credentials.rs#L17-L24)）——教训在「绑定配置寄生存储侧」；新表一个标记列覆盖所有 kind × 作用域组合，新类型零结构改动 |
| D3 | kind 用什么类型存储 | **TEXT 字符串枚举**：Rust 枚举 + `#[sqlx(rename_all = "snake_case")]`，DB 值 = `'lark_app'` / `'github_token'` / `'tavily_key'` | ① API/JSON 层已是 snake_case 字符串（serde 同名派生），DB↔API↔前端全链路值空间一致；② 存量迁移 `json_extract` 直插零转换；③ 调试 `WHERE kind='lark_app'` 自解释；④ 凭证表千行级，TEXT vs INTEGER 性能无感 |
| D4 | 枚举类型安全如何保 | 依然必须是 Rust 枚举（`#[derive(sqlx::Type)]`），查询标注 `kind as "kind: CredentialKind"` | D3 只改底层存储，不放松 AGENTS §4.3「禁止裸类型」红线；未知值 decode 报错与 INTEGER 枚举行为一致 |
| D5 | 创建者/更新者审计 | `created_by` + `modified_by` 两列 | 与凭证归属者（`user_id`）解耦：管理员可代用户操作，审计需区分「谁的凭证」与「谁操作的」；沿 [message_channels 先例](../../migrations/20260508000000_message_channels.sql#L19-L20) |
| D6 | 删除语义 | 软删除（`status`：1=Active, 0=Deleted） | 凭证被渠道引用（`ChannelConfig.lark_credential_id`），硬删断链；沿 AGENTS §3.5 软删除约定 |
| D7 | 凭据可用范围 | `visibility TEXT NOT NULL DEFAULT 'private'`：`'private'`（仅所有者用户及其下游引用） / `'public'`（同 org 用户可显式引用，如管理员配置的组织级凭据）；可见性控制**显式引用的准入**，并派生默认标记的作用域（D15） | 绑定关系复杂度（agent 配置/人的配置/工具配置都可能引用凭据）无法在凭据表单侧表达；可见性是唯一真正属于凭据自身的访问语义；public 为组织共享场景预留，本次默认全 private |
| D8 | org_id 是否保留 | 保留 `NOT NULL` | 多租户隔离与组织级管理查询；凭证与渠道（message_channels）同族资产，结构对齐 |
| D9 | 时间戳格式 | INTEGER 毫秒 | 弃 JSON 项内 RFC3339 字符串，对齐 DB 层统一约定（message_channels 同款） |
| D10 | 旧实现处置 | **直接替换**：同一迁移内搬迁存量 + `DROP COLUMN identity_credentials`，旧 JSON 读写代码全量删除，不留兼容层 | 项目初期无生产包袱；双写/兼容 shim 的维护成本远大于收益，长痛不如短痛 |
| D11 | 凭证 DAL 归属 | **不新增 DAL**：UserDal 仍是凭证数据操作中心，两个 JSON 方法在原 trait 原位替换为行级方法；新表按「一表一 DAO」惯例新增 UserCredentialDao，由 UserDalImpl 组合 | 凭证是用户资产的延伸，Domain 与各解析器已围绕 user_dal 组织；新增纯转发 DAL 只添间接层与文件数。DAL 依赖多 DAO 是标准形态（AGENTS §3.1）；lark DAL 等消费侧直接组合该 DAO（DAL 互调禁止） |
| D12 | ~~Agent 专用凭证~~（v0.2 废止） | ~~`agent_id` 列~~ → **凭据表零外部使用方引用**：不加 `agent_id`、`bound_tool` 等任何指向使用方实体的列（`is_default` 不在此列，见 D15——其作用域由自身 visibility 派生，非外部引用） | 专属关系若放凭据表，既表达不了完整绑定复杂度（无法阻止他处按 ID 引用），又引入维度膨胀；正确宿主是使用方（D13） |
| D13 | 绑定关系归属（v0.2 核心） | **使用方实体持有绑定，credential_id 直传**：Agent 专用凭据 → Agent 配置存 credential_id 引用（后续演进）；工具指定 → 工具配置；渠道绑定现状已是此模式（`ChannelConfig.lark_credential_id`） | 信息专家原则：绑定是使用方的信息；凭据表不感知谁在用自己；解析传递一律以 credential_id 为核心（现状 `default_*_id` 槽位本质缺陷即绑定配置寄生在凭据存储侧） |
| D14 | DAO 查询能力优先 | DAO 以通用 `query` + `UserCredentialQuery` 结构体为核心查询能力先行设计，特定查询一律构造 Query 走通用路径；COUNT 与 LIST 复用 `push_query_filters` 同一套 WHERE | AGENTS §4.9 既定规范（query 是核心，list 是语法糖）；凭证管理页/API 过滤等后续消费场景直接扩展 Query 字段即可，避免 DAO 方法族膨胀 |
| D15 | 默认语义（v0.3 核心，v0.4 修正链序） | `is_default` 标记 + **作用域由 visibility 派生**：`private + is_default=1` = 个人默认（该用户对该 kind 的默认选择）；`public + is_default=1` = 组织默认（该 org 对该 kind 的默认选择）；解析链**作用域优先**：个人默认 > 个人其他 > 组织默认 > 组织其他（§2.3） | 双语义由现有两列组合表达，零新增维度——个人默认/组织默认各恰好一个宿主字段组合；存量 `default_*_id` 槽位语义完整迁移（D2）；链序作用域优先保证组织默认只兜底「没有自己凭据的人」，设立组织默认对已有个人凭据用户零扰动 |
| D16 | 默认唯一性与承载形式 | `is_default` **专用列** + 双部分唯一索引（个人 `(user_id, kind) WHERE visibility='private' AND is_default=1`；组织 `(org_id, kind) WHERE visibility='public' AND is_default=1`）；**拒绝 tags 承载功能标记** | ① 「默认」是选择状态，选择在作用域内天然唯一——不变量该由数据库兜底（D1 初衷），逻辑层优先级无法裁决同作用域并列默认；② SQLite 对「数组包含」建不了部分唯一索引，tags 方案唯一性失守；③ tags 字面量匹配违反红线 6（裸字符串）；④ 功能状态（驱动解析）与分类标签（展示检索）语义生命周期不同，不得混装；tags 作为纯分类能力需要时再加列（纯增量，YAGNI） |

## 二、架构思路

### 2.1 数据流对比

```
【现状】users 表单行 JSON 列（读-改-写整列覆盖）
  Handler ──► Domain(identity_credential)
                 │ get / save_identity_credentials（整列读写）
                 ▼
             UserDal ──► UserDao（find_identity_credentials_* JSON 列读；
                 │        save 更是 find → 改列 → 整行 update users）
                 ▼
        users.identity_credentials TEXT（JSON：items + 3 个默认槽位）
                 ▲  并发写互相覆盖（lost update）

【目标】user_credentials 独立表（资产 + 可见性 + 默认标记，零外部使用方引用）
  Handler ──► Domain(校验/加密编排) ──► UserDal（行级凭证方法，原位替换 D11）
                                          │  组合（DAL 依赖多 DAO）
                                          ▼
                                  UserCredentialDao（新，一表一 DAO）
                                          ▼
        user_credentials 行级操作：INSERT / UPDATE / 软 DELETE / query /
        set_default（同事务清旧立新，作用域由目标凭据 visibility 派生）

  消费侧（外部绑定在使用方，credential_id 直传；默认解析走双语义标记）：
          lark DAL ──组合──► UserCredentialDao（渠道 lark_credential_id
                             显式引用 + 可见性校验；无引用走 find_default；
                             UserDao 依赖整体移除）
          gh/tavily 解析器 ──► UserDal find_default（§2.3 链 2→5 层单点）
          （后续）Agent 配置/工具配置 ──各持 credential_id──┘
```

### 2.2 表结构契约（SQL）

```sql
CREATE TABLE IF NOT EXISTS user_credentials (
    id TEXT PRIMARY KEY NOT NULL,                -- 凭证 ID（UUID v7，使用方引用键）
    org_id TEXT NOT NULL,                        -- 组织 ID，多租户隔离
    user_id TEXT NOT NULL,                       -- 凭证归属用户 ID（资产所有者）
    kind TEXT NOT NULL,                          -- 凭证类型（字符串枚举）：lark_app / github_token / tavily_key
    name TEXT NOT NULL,                          -- 用户自定义名称（仅展示，不参与解析）
    detail TEXT NOT NULL,                        -- 凭证详情 JSON（secret 类字段落库前已加密）
    visibility TEXT NOT NULL DEFAULT 'private',  -- 可见性（字符串枚举，D7）：private / public
    is_default INTEGER NOT NULL DEFAULT 0,       -- 默认标记（D15）：作用域由 visibility 派生——private=个人默认 / public=组织默认
    status INTEGER NOT NULL DEFAULT 1,           -- 软删除：1=Active, 0=Deleted
    created_by TEXT NOT NULL,                    -- 创建人 ID
    modified_by TEXT NOT NULL,                   -- 最后修改人 ID
    created_at INTEGER NOT NULL,                 -- 创建时间戳（毫秒）
    updated_at INTEGER NOT NULL                  -- 更新时间戳（毫秒）
);

-- 默认唯一性（D16 双部分唯一索引，作用域由 visibility 派生）：
-- 个人默认：同 (user_id, kind) 最多一条 private 默认
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_credentials_default_private
ON user_credentials(user_id, kind)
WHERE is_default = 1 AND visibility = 'private' AND status = 1;

-- 组织默认：同 (org_id, kind) 最多一条 public 默认
CREATE UNIQUE INDEX IF NOT EXISTS uq_user_credentials_default_public
ON user_credentials(org_id, kind)
WHERE is_default = 1 AND visibility = 'public' AND status = 1;

-- 常规查询索引
CREATE INDEX IF NOT EXISTS idx_user_credentials_org_id ON user_credentials(org_id);
CREATE INDEX IF NOT EXISTS idx_user_credentials_user_id ON user_credentials(user_id);
CREATE INDEX IF NOT EXISTS idx_user_credentials_kind ON user_credentials(kind);
CREATE INDEX IF NOT EXISTS idx_user_credentials_visibility ON user_credentials(visibility);
CREATE INDEX IF NOT EXISTS idx_user_credentials_status ON user_credentials(status);
```

> 当前实现：迁移脚本尚未创建，落地时置于 `migrations/`（时间戳前缀 + `user_credentials.sql`，含 STRICT 声明、存量 JSON 搬迁语句与 `ALTER TABLE users DROP COLUMN identity_credentials` 收尾删列——同一迁移一步到位，D10）。存量搬迁：`visibility` 全部置 `'private'`；三个 `default_*_id` 槽位**迁移**为对应凭据行 `is_default=1`（个人默认语义保留，存量用户零行为变化，D2/D15）；RFC3339 时间串转毫秒。

### 2.3 凭证解析优先级链（v0.4：作用域优先，先个人后组织）

```
解析入口（工具执行 / 渠道建联）
    │
    ▼
1. 显式引用 credential_id         使用方持有的绑定（渠道 ChannelConfig.lark_credential_id、
    │                             后续 Agent 配置 / 工具配置）
    │                             引用准入校验（D7 可见性）：
    │                               owner 自己 → private/public 均可
    │                               同 org 他人 → 仅 public
    │ 未指定
    ▼
2. 个人默认                       (user_id, kind, visibility='private',
    │                              is_default=1, status=1)
    │ 未命中
    ▼
3. 个人其他活跃凭据               (user_id, kind, status=1, created_at 升序)
    │                             现状「default 未命中回退第一条」语义平移；
    │                             含自己的 public 凭据（自己的资产总能用）
    │ 未命中
    ▼
4. 组织默认                       (org_id, kind, visibility='public',
    │                              is_default=1, status=1)
    │ 未命中
    ▼
5. 组织首条 public 活跃凭据       (org_id, kind, visibility='public',
                                   status=1, created_at 升序)
```

**作用域优先（scope-major）而非默认优先（default-major）**：个人作用域整体（默认 + 其他）先于组织作用域整体（默认 + 其他）。若把组织默认插在个人默认与个人其他之间（v0.3 原链），则：① 违背「先个人后组织」原则本身；② 管理员设立组织默认的瞬间，所有「无个人默认但有个人凭据」的用户被静默切换——影响面不可控，而组织默认的本义是**兜底给没有自己凭据的人**；③ 共享凭据（配额/速率限制/审计归因）被不必要地优先使用，违反最小暴露。

链 2→5 单点收敛在 `UserCredentialDao::find_default(ctx, user_id, kind)`，一条 SQL 实现（`WHERE kind=? AND status=1 AND (user_id=? OR (org_id=? AND visibility='public')) ORDER BY (user_id=?) DESC, is_default DESC, created_at ASC LIMIT 1`——排序键依次对应：**个人作用域优先、作用域内默认优先、创建序**），gh/tavily 解析器、lark DAL 共用，禁止各自实现。**名称不参与解析**（查证结论：现状亦无按名称解析逻辑，`name` 仅展示用；v0.2 起文档钉死此边界）。

### 2.4 visibility × is_default 语义矩阵（D7 + D15）

| visibility | is_default | 语义 | 自动解析（§2.3） | 显式引用 |
|-----------|-----------|------|-----------------|---------|
| private | 1 | 个人默认 | 链第 2 层 | owner 可用 |
| private | 0 | 普通私有凭据 | 链第 3 层候选 | owner 可用 |
| public | 1 | 组织默认 | 链第 4 层 | 同 org 可用 |
| public | 0 | 普通共享凭据 | 链第 5 层候选 | 同 org 可用 |

设定规则（Domain 层执行）：

- **set_default(credential_id) 作用域自动派生**：目标凭据 private → 置为个人默认（仅 owner 可操作）；目标凭据 public → 置为组织默认（需 org 管理权限，防成员劫持组织默认）；同事务「清同作用域旧默认 → 立新默认」（D16 唯一索引兜底并发）
- **visibility 切换清除默认标记**：public→private 或 private→public 时 `is_default` 置 0——避免组织默认静默变个人默认（或反之）的双重意外；如需默认请重新显式设定
- owner 自己的解析链不受 visibility 影响（自己的凭据自己总能用，含自己 public 的共享凭据）

### 2.5 kind / visibility 枚举契约

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum CredentialKind {
    LarkApp,
    GithubToken,
    TavilyKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[sqlx(rename_all = "snake_case")]
pub enum CredentialVisibility {
    Private,   // 'private'：仅所有者用户
    Public,    // 'public'：同 org 用户可显式引用
}
```

> 当前实现：[CredentialKind 定义](../../common/src/models/identity_credentials.rs#L177-L186)（现仅 serde 派生，落地时补 `sqlx::Type` + `#[sqlx(rename_all)]`）；`CredentialVisibility` 落地时新增于同文件。serde 与 sqlx 同用 snake_case，DB 值 = API 值 = JSON 存量值，映射只此一处。

### 2.6 UserCredentialQuery 契约（D14，定义于 DAO 模块 mod.rs，沿 MessageChannelQuery 惯例）

```rust
pub struct UserCredentialQuery {
    pub id: Option<String>,
    pub org_id: Option<String>,
    pub user_id: Option<String>,
    pub kind: Option<CredentialKind>,
    pub visibility: Option<CredentialVisibility>,
    pub is_default: Option<bool>,
    /// 按状态 IN 查询（软删过滤默认 Active，查历史走显式 status_in）
    pub status_in: Option<Vec<i32>>,
    /// 凭证名模糊匹配（LIKE，凭证千行级无需 FTS5；仅展示检索，不参与解析）
    pub keyword: Option<String>,
    pub pagination: common::api::PaginationParams,
    pub order_by: Option<String>,
}
```

`push_query_filters` 抽取全部条件供 COUNT/LIST 复用（AGENTS §4.9）。无外部使用方维度过滤字段（agent_id/bound_tool 不存在，D12）——「我的凭据」「组织共享凭据」「个人默认」「某类型凭据」均由现有字段组合表达。

## 三、涉及文件清单

| 层 | 文件 | 角色 | 状态 |
|----|------|------|------|
| 迁移 | `migrations/` 新增 `*_user_credentials.sql` | 建表 + 双部分唯一索引 + 存量 JSON 搬迁（`json_extract` 展开 items、visibility 置 'private'、三个默认槽位迁移为 `is_default=1`、RFC3339→毫秒）+ 删 `users.identity_credentials` 列（D10 一步到位） | 待创建 |
| Models | `src/models/user_credential.rs` — `UserCredentialPo` + 业务实体 `UserCredential`（PO 内嵌模式） | PO：表行 1:1 映射，含 `kind`/`visibility` 枚举标注；实体经 UserDal 对上输出（PO 不越层，AGENTS §3.5） | 待创建 |
| common | [common/src/models/identity_credentials.rs](../../common/src/models/identity_credentials.rs#L177-L186) | `CredentialKind` 补 sqlx 派生；新增 `CredentialVisibility`；旧 JSON 结构（`UserIdentityCredentials` + `UserIdentityCredential`）**直接删除**；`CredentialDetail` 保留 | 修改 |
| DAO(新) | `src/service/dao/user_credential/`（mod.rs 定义 trait + Query 结构体，sqlite.rs 实现） | **query 优先（D14）**：通用 `query(ctx, UserCredentialQuery) -> PagedResult<UserCredentialPo>` + `count` 复用 `push_query_filters`；行级写方法 insert / update / soft_delete / set_default（§2.4 设定规则，同事务清旧立新）；语义读方法 find_by_id（主键）+ find_default（§2.3 链 2→5 单点）；特定查询一律构造 Query 走通用路径，不另立方法 | 待创建 |
| DAO(旧) | [src/service/dao/user/mod.rs](../../src/service/dao/user/mod.rs#L59-L66) | **删除** `find_identity_credentials_by_user_id` / `find_identity_credentials_by_username` 两个 JSON 列读方法（后者无调用方，一并清退） | 修改 |
| DAL | [src/service/dal/user.rs](../../src/service/dal/user.rs#L73-L86) | **原位替换，不新增 DAL（D11）**：trait 上两个 JSON 方法换为凭证方法（query / count / insert / find_by_id / update / soft_delete / set_default / find_default，D14 对齐）；`UserDalImpl` 组合 `UserCredentialDao`，对外返回业务实体（PO 不出 DAL）；同文件 `GhDalCredentialResolver` 改用 find_default | 修改 |
| Domain | [src/service/domain/finance/identity_credential.rs](../../src/service/domain/finance/identity_credential.rs#L22-L31) | 去 read-modify-write：删除 `load_credential_library`，CRUD 改调 user_dal 行级方法；默认/回退解析（[GitHub L232](../../src/service/domain/finance/identity_credential.rs#L232) / [Tavily L284](../../src/service/domain/finance/identity_credential.rs#L284)）统一改 `find_default`；`set_default_credential` 接口保留并升级为 §2.4 语义（作用域自动派生 + 权限门控 + 同事务清旧立新）；visibility 切换清默认标记；校验/加密编排留在 Domain | 修改 |
| 消费侧 | [src/service/dal/lark.rs](../../src/service/dal/lark.rs#L131-L142) | **移除 UserDao 依赖**：`user_dao: Option<Arc<dyn UserDao>>` 字段、`new_with_user_dao` / `new_for_test_with_user_dao` 构造入参整体替换为 `UserCredentialDao`（`init()` 注入源同步替换）；凭证解析改直查——渠道 `lark_credential_id` 显式引用（含可见性校验 §2.4）+ 无引用走 `find_default`（DAL→DAO 合法，DAL 互调禁止） | 修改 |
| Handler | [src/handlers/finance/lark_integration/](../../src/handlers/finance/lark_integration) | DTO 结构不变，透传 Domain 改造结果 | 微调 |

**零改动面**：

- `message_channels` 表结构不动——渠道引用键 `credential_id` 语义不变，仅数据来源从 JSON 查找变为查表
- 加密链路不动——`pkg::crypto::encrypt_channel_secret` 沿用，`detail` 列仍存「secret 已加密」的 JSON
- `CredentialDetail` 结构不动——类型化字段集、`validate`/`normalized`/`encrypt_sensitive` 行为原样迁移
- `users` 表其余字段不动——仅删 `identity_credentials` 一列（D10，无回滚窗口）

## 四、关键边界 / 行为红线

1. **凭据表零外部使用方引用（D12 精化）**：禁止在 `user_credentials` 上新增任何指向使用方实体的列（`agent_id`、`bound_tool`、`default_for_*`…）——外部绑定一律在使用方实体上以 `credential_id` 引用实现；`is_default` 不在此列（作用域由自身 visibility 派生，D15），这是防止 schema 膨胀复发的结构性红线
2. **默认唯一由数据库兜底（D16）**：双部分唯一索引（个人 `(user_id, kind)` / 组织 `(org_id, kind)`，各限 `is_default=1 AND status=1` 且对应 visibility）；切换默认必须同一事务内「清同作用域旧默认 → 立新默认」，禁止拆两次独立写入、禁止以「逻辑层优先级」替代唯一性约束
3. **默认作用域派生与权限门控（D15 + §2.4）**：set_default 作用域由目标凭据 visibility 自动派生（private→个人默认仅 owner 可设；public→组织默认需 org 管理权限）；visibility 切换必须清除 `is_default`；owner 解析链不受 visibility 影响
4. **可见性是唯一访问属性（D7）**：`visibility` 只控制显式引用准入（private=仅 owner，public=同 org）；引用校验失败返回明确错误
5. **功能标记专用列（D16）**：参与解析/选择的功能状态（当前仅 `is_default`）必须是专用列 + 唯一索引；`tags` 若未来引入仅做分类展示检索，**禁止承载任何功能语义**（无法索引唯一性、字面量匹配违反红线 6）
6. **kind/visibility 禁止裸字符串比较**：Rust 侧一律 `match` 枚举分发，SQL 侧一律 `kind as "kind: CredentialKind"` / `visibility as "visibility: CredentialVisibility"` 解码，禁止字面量比较散落
7. **detail 密封边界不变**：DAO 层不感知 detail 字段结构，加密/解密发生在 Domain/DAL 编排层（现状语义平移）
8. **凭证禁止向量化**：`UserCredentialPo` 不得实现 `Vectorizable`、不得进入任何搜索索引——凭证含 secret，可检索性是安全漏洞
9. **枚举存储二分**：本设计确立「分类型枚举（映射外部系统/访问语义）用 TEXT，状态型枚举（内部状态机）沿用 INTEGER」——`TaskStatus`、`UserRole` 等存量 INTEGER 枚举不因本先例改存储，新枚举按此二分裁定（`CredentialKind` 与 `CredentialVisibility` 均为 TEXT）
10. **迁移幂等**：搬迁脚本先查后插（幂等），存量 JSON 值域与新表 TEXT 值域同为 snake_case 字符串，零转换直插；三个 `default_*_id` 槽位迁移为 `is_default=1`（D2/D15）
11. **无兼容层（D10）**：落地即删旧——`UserDal` 两个 JSON 方法、`UserDao` 两个 JSON 列读方法、`UserIdentityCredentials`/`UserIdentityCredential` 结构、`users.identity_credentials` 列一次性清除；禁止双写过渡、兼容 shim 或「暂时保留旧读取路径」
12. **解析链单点收敛（§2.3）**：显式引用（含可见性校验）> 个人默认 > 个人其他活跃 > 组织默认 > 组织其他 public 活跃（**作用域优先**，禁止把组织默认插到个人作用域内部）；链 2→5 只允许实现在 `UserCredentialDao::find_default` 一处（一条 SQL，排序键：个人作用域优先、作用域内默认优先、创建序），消费侧禁止各自实现；**名称（`name` 字段）永不参与解析**，仅作展示
13. **ID 为核心传递（D13）**：一切凭据选择/绑定/引用以 `credential_id` 传递（渠道配置、后续 Agent/工具配置）；禁止以名称、序号等间接键定位凭据
14. **DAO 查询唯一入口（D14）**：条件查询一律走 `query(ctx, UserCredentialQuery)`，禁止为单一调用方新增 `list_by_xxx` / `find_by_xxx_and_yyy` 方法族；语义读方法仅限 `find_by_id`（主键）、`find_default`（§2.3 单点）与 `set_default`（§2.4 设定规则）

## 五、扩展模式

**新增凭证类型（如 WechatApp）标准路径**：

1. `CredentialKind` 加变体——DB 无 DDL（TEXT 枚举核心收益：新增类型不动表结构）
2. `CredentialDetail` 加变体 + 实现 `validate` / `normalized` / `encrypt_sensitive` 行为
3. Domain 按 `match kind` 分发处补分支（联动渠道建联等下游）
4. 前端类型标签与表单渲染分支

**新增凭据使用方（绑定关系演进路径，D13 核心兑现点）**：**不改凭据表**，在使用方实体上加 `credential_id` 引用——

| 使用方 | 绑定落点 | 解析链插入位置 |
|--------|---------|---------------|
| Agent 专用凭据 | Agent 配置（如 agent 表凭据引用字段/JSON） | 显式引用层：Agent 执行上下文优先取自己的绑定，未绑定回退 §2.3 链 |
| 工具指定凭据 | 工具配置（ToolPo.config） | 同上 |
| 用户跨作用域默认 | 用户配置实体（如 users.preferences，存 credential_id） | 显式引用层之后、§2.3 链第 2 层之前——用于「用户默认选用他人的 public 凭据」这类跨作用域选择（个人默认已由 D15 内建，无需配置实体） |

每新增一种使用方 = 使用方自己加一个字段 + 解析链插一层，凭据表与 DAO 零改动——这正是「绑定归使用方」相对于「绑定上凭据表」（v0.1 已废止）的结构性优势。

**枚举存储裁定速查**（后续新枚举套用）：

| 枚举性质 | 存储 | 例 |
|----------|------|-----|
| 分类型：映射外部系统/协议，无序，API 层天然字符串 | TEXT（`#[sqlx(rename_all = "snake_case")]`） | `CredentialKind` |
| 状态型：内部状态机，有序流转，项目私有语义 | INTEGER（`#[repr(i32)]` + `#[derive(sqlx::Type)]`） | `TaskStatus`、`UserRole`、渠道 `status` |
