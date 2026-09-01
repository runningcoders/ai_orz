# Agent 详情页过期技能独立区 + 恢复按钮 与 DTO 清理 实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Agent 详情页「技能」tab 下，把 Expired 技能放到一个独立卡片分组区，每条附「恢复」按钮（点击把 Expired → Draft）；同时清理 `GetAgentResponse.tools` 字段的废弃认知。

**Architecture:**
- 后端加一个专门的「Agent 过期技能」方法链（DAL → Domain → Handler response），避免改动现有 `list_for_agent` 的"只含有效技能"语义。
- 独立 `restore_skill` Handler + Domain 方法，只接受当前 `SkillStatus::Expired` 的技能做状态迁移 → Draft，复用既有 `ensure_skill_access` 做权限（Agent 创建者可操作 Agent 的副本）。
- 前端 `agent_detail.rs` 在现有分组全景下面追加一个独立分区「📦 已过期（恢复可用）」；只在列表非空时渲染。点击恢复走 API 并原地用 signal 剔除该 item，无需全量 reload。
- `tools` 字段判定：前端 chat_side_panel 还有读取 `a.tools.len()` 的调用，不能删除。给用户确认是否要删别的 `tools` 字段或迁移语义。

**Tech Stack:** Rust (Axum + sqlx 0.8), Dioxus 0.7 + Tailwind v4 + DaisyUI v5, common/api skill & agent DTOs.

---

### Task 0: 先确认 `GetAgentResponse.tools` 是否真要删（做调查、不做改动）

**Files:**
- Read: `common/src/api/agent.rs#L176-L227`
- Read: `frontend/src/components/chat/chat_side_panel.rs#L660-L680`
- Read: `src/handlers/hr/agent/get_agent.rs#L131-L137`

- [ ] **Step 1: 再次确认 `tools` 的所有消费点**

Run:
```
rg '\.tools\b|resp\.tools|GetAgentResponse.*tools' frontend/ src/ common/ -n
```
Expect: 能命中 `chat_side_panel.rs:674` "已绑定工具：{a.tools.len()} 个"；此外 `get_agent.rs` 的 `tools = get_agent_bound_tool_ids(...)` 是真实的后端装配；没有 `tools: vec![]` 之类硬编码。

- [ ] **Step 2: 输出结论给用户，等待确认**

**结论先行写在计划里但不 commit：**
> `GetAgentResponse.tools: Vec<String>` 实际仍被 `chat_side_panel` 用于显示「已绑定工具 N 个」，后端是 `get_agent_bound_tool_ids` 真实查询。该字段未废弃，建议保留。
>
> 如果你想删的是 **另一个** `tools` 字段（比如 `CreateAgentRequest.tools`、`models/agent.tools`、或别的 DTO），告诉我具体结构体名字，我再针对性做移除。

在没有用户二次确认前，**跳过 Task 0 对应代码改动**，不影响本计划其余任务。

---

### Task 1: 后端加 "Agent 过期技能" 查询链路（DAL + Domain + 装配）

#### 1.1 DAL：新增 `list_expired_for_agent`

**Files:**
- Modify trait signature: `src/service/dal/skill.rs#L80-L100`
- Modify implementation: `src/service/dal/skill.rs#L300-L315`（紧挨着 `list_for_agent`）
- Test: `src/service/dal/skill_test.rs`

- [ ] **Step 1: 在 SkillDal trait 加签名**

在 `src/service/dal/skill.rs` `async fn list_for_agent` 下面加：

```rust
/// 返回 Agent 当前所有 Expired 状态的技能副本（不改变 list_for_agent 的排除语义）
/// 用于 Agent 详情页「过期技能区」独立分组的展示与「恢复」按钮。
async fn list_expired_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>>;
```

- [ ] **Step 2: DAL Impl 实现**

`impl SkillDal for SkillDalImpl` 里紧接 `list_for_agent`：

```rust
async fn list_expired_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>> {
    let page = self
        .query(
            ctx,
            SkillQuery {
                author_id: Some(agent_id.to_string()),
                status: Some(SkillStatus::Expired),
                exclude_status: None,
                pagination: Pagination::unlimited(),
                ..Default::default()
            },
        )
        .await?;
    Ok(page.items)
}
```
要点：用 `status: Some(Expired)` 而非 `exclude_status`，保证只返回过期。

- [ ] **Step 3: 跑 test 编译**
```bash
cd /Users/aman/Technology/rust/ai_orz
export SDKROOT=/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX26.5.sdk
export PATH=$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH
cargo check -p ai_orz --lib 2>&1 | tail -10
```
Expect: 0 errors（下一步会给 Domain trait 加同名）。

#### 1.2 Domain trait + Impl 加 `list_expired_for_agent`

**Files:**
- Modify trait: `src/service/domain/hr/mod.rs#L508-L555`
- Modify impl: `src/service/domain/hr/skill.rs`

- [ ] **Step 1: SkillManage trait 加签名**

紧贴 `async fn list_for_agent` 下：

```rust
/// 返回 Agent 当前所有 Expired 状态的技能副本，用于详情页「过期技能区」展示 + 恢复。
async fn list_expired_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>>;
```

- [ ] **Step 2: Impl（domain hr skill.rs）直通**

紧挨着 `list_for_agent`：

```rust
async fn list_expired_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>> {
    self.skill_dal.list_expired_for_agent(ctx, agent_id).await
}
```

- [ ] **Step 3: tool_execution_test 的 mock `SkillManage` 也要补**

`src/service/domain/runtime/tool_execution_test.rs` 有一个 mock impl of `SkillManage`（带 `list_for_agent_count`）。追加空实现：

```rust
async fn list_expired_for_agent(&self, _ctx: RequestContext, _agent_id: &str) -> Result<Vec<Skill>> {
    Ok(vec![])
}
```
搜索同文件里所有 `fn list_for_agent` 的 mock 实现（有两个 mock impl），**每个后面都要补**，避免 trait object safety compile error。

- [ ] **Step 4: 编译校验**
```bash
cargo check -p ai_orz --lib 2>&1 | tail -10
```
Expect: 0 errors.

#### 1.3 `GetAgentResponse` 加 `expired_skill_list` 字段 + 装配

**Files:**
- Modify DTO: `common/src/api/agent.rs#L176-L227`
- Modify Handler `get_agent.rs`: `src/handlers/hr/agent/get_agent.rs#L138-L192`
- Modify association: `src/handlers/hr/agent/association.rs`

- [ ] **Step 1: DTO 新增字段**

在 `skill_list` 字段下面加：

```rust
/// Agent 已标记为 Expired（软删除/过期）的技能副本列表（agent_id 自身副本）；前端单独分区 + 「恢复」按钮。
/// 仅当请求 with_skills=true 时 Some；否则 None，与 skill_list 行为一致。
#[serde(skip_serializing_if = "Option::is_none")]
pub expired_skill_list: Option<Vec<SkillListItem>>,
```
注意：**不能加在 struct 中间**（会影响现有的 struct init 代码），要加在 `skill_list` 之后。

- [ ] **Step 2: `association.rs` 加 `build_flat_expired_skills`**

追加：

```rust
/// 装配 Agent 自身目录下**仅 Expired** 的技能列表（扁平，按 id 稳定排序）。
/// 用于详情页「过期技能区」独立分区展示。
pub(crate) async fn build_flat_expired_skills(
    ctx: RequestContext,
    agent_id: &str,
) -> Result<Vec<SkillListItem>> {
    let skills = hr_domain()
        .skill_manage()
        .list_expired_for_agent(ctx.clone(), agent_id)
        .await?;

    let mut items: Vec<SkillListItem> = skills.iter().map(skill_to_list_item).collect();
    items.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(items)
}
```

- [ ] **Step 3: `get_agent.rs` 装配**

import（顶部）新增：
```rust
use super::association::{build_flat_expired_skills, build_flat_skills, build_flat_tools};
```

把
```rust
let skill_list = if with_skills {
    Some(build_flat_skills(ctx.clone(), &params.id).await?)
} else {
    None
};
```
变成：
```rust
let (skill_list, expired_skill_list) = if with_skills {
    (
        Some(build_flat_skills(ctx.clone(), &params.id).await?),
        Some(build_flat_expired_skills(ctx.clone(), &params.id).await?),
    )
} else {
    (None, None)
};
```
然后在 Ok(GetAgentResponse { ... }) 末尾结构体字面量里追加：
```rust
skill_list,
expired_skill_list,
```

- [ ] **Step 4: 编译 + clippy**
```bash
cargo check -p ai_orz --lib 2>&1 | tail -10
cargo check -p common 2>&1 | tail -5
```
Expect: 0 errors.

---

### Task 2: 新增 "恢复技能" 后端 API（restore_skill）

#### 2.1 DTO（common）

**File:** `common/src/api/skill.rs`（文件末尾）

- [ ] **Step 1: 添加请求/响应结构**

```rust
/// 恢复技能请求（把 Expired → Draft）。只允许当前 status=Expired 的技能。
/// 权限：作者本人 / Admin / SuperAdmin /（当作者是 Agent 时）Agent 的 created_by 用户。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RestoreSkillRequest {
    /// Skill ID（path）。
    #[param(source = "path")]
    pub skill_id: String,
}

/// 恢复技能响应。
pub type RestoreSkillResponse = SkillDetail;
```

#### 2.2 SkillManage/Domain 加 `restore_skill`；复用 `ensure_skill_access`

**Files:**
- Modify trait: `src/service/domain/hr/mod.rs#L508-L582`
- Modify impl: `src/service/domain/hr/skill.rs`
- Test: `src/service/domain/hr/skill_test.rs`（末尾）

- [ ] **Step 1: SkillManage trait 加 `restore_skill`**

放 "C. Agent 技能安装" 小节下、`uninstall_from_agent` 后：

```rust
/// 把一个 Expired 技能恢复为 Draft（用于 Agent 详情页的过期技能「恢复」按钮）。
///
/// 前置约束：
/// - skill.po.status == SkillStatus::Expired（否则返回 Conflict 409）
/// - 调用方有权限（ensure_skill_access，含 Agent 创建者 → Agent 私有副本通路）
async fn restore_skill(&self, ctx: RequestContext, skill_id: &str) -> Result<Skill>;
```

- [ ] **Step 2: Impl（domain hr skill.rs）**

```rust
async fn restore_skill(&self, ctx: RequestContext, skill_id: &str) -> Result<Skill> {
    let Some(skill) = self.skill_dal.find(ctx.clone(), skill_id).await? else {
        bail_err!(NotFound, "Skill {} not found", skill_id);
    };

    ensure_err!(
        matches!(skill.po.status, SkillStatus::Expired),
        Conflict,
        "Skill {} 状态非 Expired（当前={:?}），无需恢复",
        skill.po.id,
        skill.po.status
    );

    self.ensure_skill_access(&ctx, &skill.po).await?;

    let mut skill = skill;
    skill.po.status = SkillStatus::Draft;
    skill.po.modifier_id = ctx.uid().to_string();
    skill.po.updated_at = chrono::Utc::now().timestamp_millis();

    self.skill_dal.update(ctx, &skill).await?;

    // 如果有向量索引，删除旧（expired）索引 + 重建新（Draft）索引
    let _ = self.skill_dal.delete_vector(ctx.clone(), &skill.po.id).await;
    if let Err(e) = embed_entity::embed_entity(&*self.skill_dal, &skill, ctx.org_id()).await {
        log_error!(action = "restore_skill_vectorize", skill_id = skill.po.id, error = %e; "向量索引重建失败，不阻塞恢复主流程");
    }

    Ok(skill)
}
```

- [ ] **Step 3: mock（tool_execution_test.rs）补空**

两处 SkillManage mock 再各加：
```rust
async fn restore_skill(&self, _ctx: RequestContext, _skill_id: &str) -> Result<Skill> {
    bail_err!(NotImplemented, "mock restore_skill");
}
```

- [ ] **Step 4: 先写失败用例测试**

`src/service/domain/hr/skill_test.rs` 末尾（紧接 `test_skill_access_allows_agent_creator` 后）追加：

```rust
/// 场景：Agent 私有技能被标记 Expired → Agent 创建者点「恢复」→ 恢复为 Draft，向量索引重建。
#[sqlx::test]
async fn test_restore_skill_revives_expired_to_draft(pool: SqlitePool) -> Result<()> {
    let (domain, _tmp) = build_domain(pool.clone()).await;
    let admin_ctx = new_ctx_with_role("u-admin", "org-1", UserRole::Admin, pool.clone());

    // 1. 建共享库 Published 源技能
    let src_id = uuid::Uuid::now_v7().to_string();
    let src_content_path = format!("skills/{}/", src_id);
    let mut src_po = SkillPo::new(
        src_id.clone(),
        "src-for-restore".to_string(),
        "source".to_string(),
        vec!["tag-a".to_string()],
        "shared".to_string(),
        "".to_string(),
        "u-admin".to_string(),
        SkillAuthorType::User,
        src_content_path.clone(),
    );
    src_po.status = SkillStatus::Published;
    domain.skill_dal.create(admin_ctx.clone(), &src_po).await?;
    domain.skill_dal.write_main_content(&src_po, "# source\n")?;

    // 2. 给 agent-id=ag-restore 装一个副本（Draft）
    domain.skill_dal.install_to_agent(admin_ctx.clone(), &src_id, "ag-restore").await?;
    // 手动把副本标 Expired 模拟过期
    let q = crate::service::dao::skill::SkillQuery {
        author_id: Some("ag-restore".to_string()),
        parent_skill_id: Some(src_id.clone()),
        author_type: None,
        ..Default::default()
    };
    let copy = domain.skill_manage().query_skills(admin_ctx.clone(), q).await?.items.into_iter().next().unwrap();
    let mut copy_po = copy.po.clone();
    copy_po.status = SkillStatus::Expired;
    let mut copy_entity = copy.clone();
    copy_entity.po = copy_po;
    domain.skill_dal.update(admin_ctx.clone(), &copy_entity).await?;

    // 3. list_expired_for_agent 应能命中 1 条
    let expired = domain.skill_manage().list_expired_for_agent(admin_ctx.clone(), "ag-restore").await?;
    assert_eq!(expired.len(), 1);
    assert_eq!(expired[0].po.status, SkillStatus::Expired);

    // 4. 恢复
    let revived = domain.skill_manage().restore_skill(admin_ctx.clone(), &expired[0].po.id).await?;
    assert_eq!(revived.po.status, SkillStatus::Draft);
    assert_eq!(revived.po.modifier_id, "u-admin");

    // 5. 再查过期：0 条；有效：1 条
    let expired2 = domain.skill_manage().list_expired_for_agent(admin_ctx.clone(), "ag-restore").await?;
    assert_eq!(expired2.len(), 0);
    let list = domain.skill_manage().list_for_agent(admin_ctx.clone(), "ag-restore").await?;
    assert_eq!(list.len(), 1);

    // 6. 重复点恢复：冲突
    let err = domain.skill_manage().restore_skill(admin_ctx, &revived.po.id).await.unwrap_err();
    assert_eq!(err.code(), common::error::Code::Conflict);

    Ok(())
}
```

- [ ] **Step 5: 运行测试**
```bash
cargo test -p ai_orz --lib test_restore_skill_revives_expired_to_draft -- --test-threads=1 --nocapture 2>&1 | tail -20
```
Expect: PASS.

#### 2.3 Handler 新增 restore_skill_handler + 路由注册

**Files:**
- Create: `src/handlers/hr/skill/restore_skill.rs`
- Modify: `src/handlers/hr/skill/mod.rs`

- [ ] **Step 1: 创建 Handler 文件**

```rust
//! Handler: POST /api/v1/skills/{skill_id}/restore - 将 Expired 技能恢复为 Draft

use crate::handlers::hr::skill::response::to_detail;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{RestoreSkillRequest, RestoreSkillResponse};
use common::error::Result;

#[register_handler_tool(
    id = "restore_skill",
    name = "restore_skill",
    description = "将一个 Expired 状态的 Skill 恢复为 Draft。仅作者/管理员/Agent 创建者可操作。",
    params = "common::api::RestoreSkillRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn restore_skill(
    ctx: RequestContext,
    params: RestoreSkillRequest,
) -> Result<RestoreSkillResponse> {
    let skill = domain()
        .skill_manage()
        .restore_skill(ctx, &params.skill_id)
        .await?;
    Ok(to_detail(&skill))
}
```

- [ ] **Step 2: 注册到 `mod.rs`**

```rust
pub mod restore_skill;
...
pub use restore_skill::restore_skill_handler;
```

（按已有顺序；不要和现有 create/get/delete 的 mod 声明混错位置。）

- [ ] **Step 3: 路由注册处（HR router）确认**

检查 `src/handlers/hr/mod.rs`（或路由装配文件，如 `src/handlers/mod.rs`）。通常 `#[generate_http_handler]` 宏会自动在宏展开里按约定前缀生成路由；但本项目 restore 是新 path：`/skills/{skill_id}/restore` 是 POST。若宏的自动注册对该"自定义 suffix verb"不友好，在路由装配处按现有 `install_skill_to_agent_handler` 的模式手加一行。Grep 先看：

```bash
rg 'install_skill_to_agent_handler' src/ --no-filename -n
```

如果有显式注册（而不是宏的 auto-dispatch via generate_http_handler 生成的路由 map），则 restore 也要加。
本项目 generate_http_handler 是基于 Params 中的 `#[param(source = "path")]` 自动推导 `method + route`；如果 RestoreSkillRequest 只有 path `skill_id`，宏会产出默认是 `GET /skills/{skill_id}/restore_skill` 的路由——这里要的是 **POST**。处理方式：查 `install_skill_to_agent`（POST `agents/{agent_id}/skills/{skill_id}/install` 的实现模式），在宏调用上抄它的 `#[generate_http_handler(method = "...", route = "...")]` 属性。若此参数不存在，退化为：

```rust
// 查 install_skill_to_agent.rs generate_http_handler 是怎么写的，直接复用相同 pattern
```

- [ ] **Step 4: 编译 + clippy**
```bash
cargo check -p ai_orz --lib 2>&1 | tail -10
```

#### 2.4 DAL 侧单测：`list_expired_for_agent` 与已有 `install_to_agent` 的软删除重装共存性

**File:** `src/service/dal/skill_test.rs`（末尾追加）

- [ ] **Step 1: 追加测试**

```rust
/// 验证 list_expired_for_agent 只返回 status=Expired 且 author_id=agent_id 的条目。
#[sqlx::test]
async fn test_list_expired_for_agent_only_returns_expired(pool: SqlitePool) -> Result<()> {
    let skill_dal = init_test(pool.clone()).await;
    let ctx = new_ctx("u1", pool);

    // 建 Published 源
    let src_id = uuid::Uuid::now_v7().to_string();
    let cp = format!("skills/{}/", src_id);
    let mut src_po = SkillPo::new(
        src_id.clone(), "expired-src".into(), "".into(), vec!["x".into()],
        "shared".into(), "".into(), "u1".into(), SkillAuthorType::User, cp.clone(),
    );
    src_po.status = SkillStatus::Published;
    skill_dal.create(ctx.clone(), &src_po).await?;
    skill_dal.write_main_content(&src_po, "# src\n")?;

    // 安装两个 agent：ag-A（装完后软删除 → Expired），ag-B（保留 Draft）
    let a = skill_dal.install_to_agent(ctx.clone(), &src_id, "ag-A").await?;
    let _b = skill_dal.install_to_agent(ctx.clone(), &src_id, "ag-B").await?;
    skill_dal.delete(ctx.clone(), a.id()).await?;

    let expired_a = skill_dal.list_expired_for_agent(ctx.clone(), "ag-A").await?;
    assert_eq!(expired_a.len(), 1);
    assert_eq!(expired_a[0].po.status, SkillStatus::Expired);

    let expired_b = skill_dal.list_expired_for_agent(ctx.clone(), "ag-B").await?;
    assert_eq!(expired_b.len(), 0);

    // list_for_agent(ag-A) 返回 0（过期被排除）
    let list_a = skill_dal.list_for_agent(ctx, "ag-A").await?;
    assert_eq!(list_a.len(), 0);
    Ok(())
}
```

- [ ] **Step 2: 运行**
```bash
cargo test -p ai_orz --lib test_list_expired_for_agent_only_returns_expired -- --test-threads=1 2>&1 | tail -5
```
Expect: PASS.

- [ ] **Step 3: Commit（Task 1 + Task 2 整体）**
```bash
git add common/src/api/skill.rs common/src/api/agent.rs src/service/dal/skill.rs src/service/dal/skill_test.rs src/service/domain/hr/mod.rs src/service/domain/hr/skill.rs src/service/domain/hr/skill_test.rs src/service/domain/runtime/tool_execution_test.rs src/handlers/hr/skill/mod.rs src/handlers/hr/skill/restore_skill.rs src/handlers/hr/agent/get_agent.rs src/handlers/hr/agent/association.rs
git commit -m "feat(skill): Agent详情页过期技能独立区 + 恢复API（后端）

- DAL/Domain 新增 list_expired_for_agent：只返回 Expired 且 author_id=agent_id 的副本，
  不改变 list_for_agent 排除 Expired 的既有语义。
- DTO GetAgentResponse 加 expired_skill_list 字段（与 skill_list 同 with_skills 开关）；
  association 层 build_flat_expired_skills 装配，排序稳定。
- 新增 restore_skill：校验 status==Expired→Conflict，调用 ensure_skill_access 权限，
  状态重置为 Draft，更新 modifier/updated_at，重建向量索引（失败不阻塞主流程）。
- 新增 restore_skill_handler（双宏：register_handler_tool + generate_http_handler），
  参数 RestoreSkillRequest / RestoreSkillResponse = SkillDetail。
- DAL test: list_expired_for_agent 与 list_for_agent 互斥性；
  Domain test: restore_skill 端到端（expired→Draft + 幂等冲突）。
- runtime tool_execution_test 的 2 处 SkillManage mock 补 trait 方法桩（编译通过）。"
```

---

### Task 3: 前端 Agent 详情页追加「过期技能独立区」+ 恢复按钮

**Files:**
- Modify API layer: `frontend/src/api/hr.rs`
- Modify page: `frontend/src/pages/hr/agent_detail.rs`（Tab=1 技能部分，SkillCard 下方追加）

#### 3.1 前端 API：补 `restore_skill` 调用

**File:** `frontend/src/api/hr.rs`

- [ ] **Step 1: import + fn 新增**

```rust
use common::api::{RestoreSkillRequest, RestoreSkillResponse, ...};
// ... 现有其他 import

pub async fn restore_skill(req: RestoreSkillRequest) -> Result<RestoreSkillResponse, ApiError> {
    post("/api/v1/skills/:skill_id/restore", &req).await
}
```

注意：`post` 的路径替换依赖 `:skill_id` 与 `#[param(source = "path")]` 一致；若项目统一走 `Params` 宏约定的 URL 模式，对照 `uninstall_skill_from_agent` 等 handler 的前端 API 写法。先 grep：
```bash
rg 'uninstall_skill_from_agent|install_skill_to_agent' frontend/src/api/hr.rs
```

#### 3.2 Agent Detail 组件：展示过期技能独立区

**File:** `frontend/src/pages/hr/agent_detail.rs#L649-L669` + `#L1560-L1626`

- [ ] **Step 1: 取 expired skill_list 信号层**

在 `let skill_list: Vec<SkillListItem> = a.skill_list.clone().unwrap_or_default();` 之后加：

```rust
let expired_skill_list: Vec<SkillListItem> = a.expired_skill_list.clone().unwrap_or_default();
let expired_count = expired_skill_list.len();

// 恢复操作回调：点恢复按钮后调 restore_skill(skill_id)，成功时：
// 1. 从 expired_skill_list 移除该项（按 id）
// 2. 把恢复后的 skill（返回的 detail → SkillListItem）追加回 active skill_list 并刷新分组
// 3. toast 成功
let on_restore_success = move |restored: RestoreSkillResponse| {
    let list_item: SkillListItem = SkillListItem {
        id: restored.id.clone(),
        name: restored.name.clone(),
        description: restored.description.clone(),
        tags: restored.tags.clone(),
        category: restored.category.clone(),
        parent_skill_id: restored.parent_skill_id.clone(),
        author_id: restored.author_id.clone(),
        author_type: restored.author_type,
        status: restored.status,
        created_at: restored.created_at,
        updated_at: restored.updated_at,
    };

    // 从已过期列表移除
    let mut curr = expired_skill_list.peek().clone();
    curr.retain(|s| s.id != restored.id);
    expired_skill_list.set(curr);

    // 追加到活动技能列表
    let mut actives = skill_list.peek().clone();
    if !actives.iter().any(|s| s.id == restored.id) {
        actives.push(list_item);
        skill_list.set(actives);
    }
    toast.success(format!("技能已恢复：{}", restored.name));
};
```

注意：signals 类型必须是用 `use_signal` 包的 Vec。如果 `skill_list` 现在是 `let` 绑定（普通解构），就需要改成信号。请先确认当前 agent_detail.rs 的 skill_list 绑定方式：

```rust
// 如果当前是：
let skill_list = a.skill_list.clone().unwrap_or_default();
// 则替换为：
let mut skill_list = use_signal(|| a.skill_list.clone().unwrap_or_default());
let mut expired_skill_list = use_signal(|| a.expired_skill_list.clone().unwrap_or_default());
// 下游所有用 skill_list 的地方加 .read()：skill_list.read().iter()、skill_list.read().len() 等
```

**这一步是关键改动点**。现有 `skill_list` 在 rsx! 嵌套 scope 内按引用传，若不改为 signal，恢复后没法局部刷新。

- [ ] **Step 2: 在"已安装技能全景"下方追加过期区（L1626 结束的闭合块后）**

```rust
// ====== 过期技能独立区（📦 Expired）======
let expired = expired_skill_list.read().clone();
if !expired.is_empty() {
    div { class: "mt-8 pt-6 border-t border-base-200",
        div { class: "flex items-center justify-between mb-3",
            div { class: "flex items-center gap-2",
                h3 { class: "text-lg font-semibold", "📦 已过期技能（可恢复）" }
                span { class: "badge orz-tag badge-warning", "{expired.len()}" }
            }
            span { class: "text-xs text-base-content/50",
                "过期是软删除：文件保留，点「恢复」即可重新启用为 Draft 状态"
            }
        }
        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4",
            for skill in expired.iter() {
                div { class: "rounded-lg bg-base-100 border-2 border-warning/40 hover:border-warning/80 p-4 shadow-sm",
                    // 行 1：标题 + 状态 badge
                    div { class: "flex items-start justify-between gap-2",
                        div { class: "flex items-center gap-2",
                            // 过期标题：灰色加删除线
                            span { class: "font-medium text-base-content/60 line-through",
                                "{skill.name}"
                            }
                        }
                        span { class: "badge badge-warning badge-xs", "Expired" }
                    }
                    // 行 2：描述
                    if !skill.description.is_empty() {
                        p { class: "text-xs text-base-content/50 mt-2 line-clamp-2",
                            "{skill.description}"
                        }
                    }
                    // 行 3：tags（灰）
                    if !skill.tags.is_empty() {
                        div { class: "flex flex-wrap gap-1 mt-2",
                            for t in skill.tags.iter() {
                                span { class: "badge badge-ghost badge-xs opacity-60", "{t}" }
                            }
                        }
                    }
                    // 行 4：操作区 — 恢复按钮
                    div { class: "flex items-center justify-end gap-2 mt-3",
                        Link {
                            class: "btn hud-btn btn-ghost btn-xs",
                            to: crate::pages::Route::HrSkillDetail { id: skill.id.clone() },
                            "查看文件 →"
                        }
                        button {
                            class: "btn hud-btn btn-warning btn-xs",
                            title: "将该技能从 Expired 恢复为 Draft",
                            onclick: {
                                let id = skill.id.clone();
                                let on_ok = on_restore_success;
                                move |_| {
                                    let id = id.clone();
                                    spawn(async move {
                                        match restore_skill(RestoreSkillRequest { skill_id: id }).await {
                                            Ok(r) => on_ok(r),
                                            Err(e) => toast.error(format!("恢复失败：{}", e)),
                                        }
                                    });
                                }
                            },
                            "↻ 恢复为草稿"
                        }
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 3: 同步调整 SkillCard 与全量 skill 分组渲染（skill_list 改为信号后）**

把 `skill_list.iter()` → `skill_list.read().iter()`，`skill_list.len()` → `skill_list.read().len()`，`skill_filtered` 的构造基于 `skill_list.read().clone()`。
`all_skill_count` 改为 `let all_skill_count = skill_list.read().len();`
`installed_skill_ids` 也用 `skill_list.read()`。
`for tag in skill_group_tags.iter()` 里分组过滤迭代 `skill_list.read().iter()`。
`standalone_skills` 同上基于 `skill_list.read().iter()`。

- [ ] **Step 4: 检查 all_skill_count == 0 的空态与过期区显示**

当活动技能 0 但过期技能>0 时：
- 活动区显示"暂无已安装技能"；
- 过期区仍在下方显示（两个分区独立，不互相依赖）。

不要把"无活动技能"和"有过期技能"合并成同一个空态判断。

#### 3.3 Import 与类型校验

- [ ] **Step 1: 顶部 use 补全**

```rust
use common::api::{RestoreSkillRequest, RestoreSkillResponse, ...};
use crate::api::hr::restore_skill;
```

如果 `frontend/src/api/hr.rs` 没有 `RestoreSkillRequest/Response` 的 `use`，需要加。

#### 3.4 前端编译 + clippy

- [ ] **Step 1: 检查**
```bash
cd /Users/aman/Technology/rust/ai_orz/frontend
export PATH=$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH
cargo check --target wasm32-unknown-unknown 2>&1 | tail -20
cargo clippy --target wasm32-unknown-unknown -- -D warnings 2>&1 | tail -15
```
Expect: 0 errors, 0 warnings.

- [ ] **Step 2: Commit**
```bash
git add frontend/src/api/hr.rs frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(frontend): Agent详情页新增过期技能独立区 + 恢复按钮

- HrAgentDetail skill_list/expired_skill_list 改为 use_signal 包装，
  恢复成功后局部剔除/追加，避免全量 reload Agent。
- 过期区独立分区（📦 已过期技能，warning 风格），卡片：
  标题带删除线灰色、badge Expired、操作区提供「查看文件」与「↻ 恢复为草稿」。
- 恢复走 restore_skill(RestoreSkillRequest { skill_id }) API；
  成功时同步移除 expired 列表 + 追加到 active，toast 提示。
- 活动区空态与过期区独立，互不抢占。"
```

---

### Task 4: 全量验证（双端 Clippy + 单测 + fmt）

- [ ] **Step 1: 后端单元测试（skill 相关 + handler a2a）**
```bash
export SDKROOT=/Applications/Xcode.app/Contents/Developer/Platforms/MacOSX.platform/Developer/SDKs/MacOSX26.5.sdk
export PATH=$HOME/.cargo/bin:/usr/bin:/bin:/usr/sbin:/sbin:$PATH
cargo test -p ai_orz --lib "skill_test::" -- --test-threads=1 2>&1 | tail -5
cargo test -p ai_orz --lib "handlers::a2a" -- --test-threads=1 2>&1 | tail -5
```
Expect: all passed.

- [ ] **Step 2: Clippy 三端**
```bash
cargo clippy -p ai_orz -- -D warnings 2>&1 | tail -5
cargo clippy -p common  -- -D warnings 2>&1 | tail -5
cargo clippy -p frontend-dioxus --target wasm32-unknown-unknown -- -D warnings 2>&1 | tail -5
```
Expect: 0 warnings.

- [ ] **Step 3: fmt + push**
```bash
cargo fmt --all
git add -A
git status --short
# 若仅 0/无 变更则直接 push，否则 amend 或补一个 fmt commit
git push origin main
```

---

### Spec Coverage Checklist

| 需求 | 对应实现 |
|------|---------|
| Agent 详情页技能区展示过期技能（独立区域） | Task 1.3（expired_skill_list DTO）+ Task 3.2（过期独立分区 UI） |
| 每条附"恢复"按钮，点击更新状态 Expired→Draft | Task 2（restore_skill API 后端）+ Task 3.2（按钮 onclick → restore → 局部 signal 更新） |
| `GetAgentResponse.tools` 是否废弃、可否删除 | Task 0 结论：`chat_side_panel.rs:674` 仍在用 `a.tools.len()` 显示计数，字段**未废弃、不可直接删**。计划保留且在交付时同步结论，等用户确认想删的是"哪个 tools 字段"。 |

### 类型一致性自检

- 新增 `list_expired_for_agent`：DAL trait → impl → Domain trait → impl → 2 处 SkillManage mock → handler 装配 **都已具名声明**，类型一致。
- `expired_skill_list` DTO 字段与后端装配处命名**完全一致**。
- `RestoreSkillRequest/Response`、`restore_skill_handler`、前端 `restore_skill()` fn **命名对齐**，`Response = SkillDetail` 避免结构重复。
- 「恢复」后 status 变 `Draft`，与 `install_to_agent` 重建副本的目标状态一致（不会产生 Published 私有 Agent 副本导致混乱）。

### 无占位符检查

- 所有步骤均含代码块与具体命令，无"TBD/待补充"。
- 所有 trait 方法声明含完整签名 + mock 桩（runtime test 中 2 处 mock impl 明确需要跟进），避免"看起来实现了但编译不通过"。
