# Tool/Skill 搜索式安装 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 Agent 详情页的 4 处工具/技能安装交互从「文本输入框 + 全量卡片」改为「带搜索的下拉框 + 已装卡片网格」，并补齐单技能卸载能力。

**Architecture:** 后端新增 2 个 tags 聚合接口（`GET /finance/tools/tags`、`GET /hr/skills/tags`）作为工具包/技能包下拉数据源；单工具/技能搜索复用已有 `POST /query` 接口；新增单技能卸载接口 `DELETE /agents/{id}/skills/{skill_id}` 和技能包卸载+删副本接口 `DELETE /agents/{id}/skill-packs/{tag}?delete_copies=true`。前端新增 `SearchableSelect` 通用组件，4 处安装区统一改为搜索→选中→安装，已装项统一用卡片网格展示。

**Tech Stack:** Rust (axum + sqlx + SQLite json_each), Dioxus 0.5 + DaisyUI v5, Tailwind CSS v4

---

## File Structure

### 后端新增文件
- `src/handlers/finance/tool/list_tool_tags.rs` — Tool tags 聚合 handler
- `src/handlers/hr/skill/list_skill_tags.rs` — Skill tags 聚合 handler
- `src/handlers/hr/skill/uninstall_skill_from_agent.rs` — 单技能卸载 handler
- `src/handlers/hr/agent/uninstall_skill_pack_with_copies.rs` — 技能包卸载+删副本 handler

### 后端修改文件
- `src/service/dao/tool/sqlite.rs` — 新增 `list_distinct_tags` DAO 方法
- `src/service/dao/skill/sqlite.rs` — 新增 `list_distinct_tags` DAO 方法
- `src/service/dao/tool/mod.rs` — ToolDao trait 加 `list_distinct_tags`
- `src/service/dao/skill/mod.rs` — SkillDao trait 加 `list_distinct_tags`
- `src/service/dal/tool.rs` — ToolDal trait + impl 加 `list_tags`
- `src/service/dal/skill.rs` — SkillDal trait + impl 加 `list_tags`
- `src/service/domain/finance/tool_provider.rs` — ToolProviderManage trait 加 `list_tool_tags`
- `src/service/domain/finance/mod.rs` — trait 定义
- `src/service/domain/hr/skill.rs` — SkillManage trait 加 `list_skill_tags` + `uninstall_from_agent`
- `src/service/domain/hr/mod.rs` — trait 定义
- `src/service/domain/hr/agent.rs` — AgentManage 加 `uninstall_skill_pack_with_copies`
- `src/handlers/finance/tool/mod.rs` — 注册 `list_tool_tags` 模块
- `src/handlers/hr/skill/mod.rs` — 注册 `list_skill_tags` + `uninstall_skill_from_agent` 模块
- `src/handlers/hr/agent/mod.rs` — 注册 `uninstall_skill_pack_with_copies` 模块
- `src/router.rs` — 注册 4 条新路由
- `common/src/api/tool.rs` — 新增 `ListToolTagsResponse`
- `common/src/api/skill.rs` — 新增 `ListSkillTagsResponse`、`UninstallSkillFromAgentRequest/Response`
- `common/src/api/agent.rs` — 新增 `UninstallSkillPackRequest`（含 `delete_copies` 字段）

### 前端新增文件
- `frontend/src/components/searchable_select.rs` — 通用搜索下拉框组件

### 前端修改文件
- `frontend/src/components/mod.rs` — 注册 `searchable_select` 模块
- `frontend/src/api/hr.rs` — 新增 `list_tool_tags`、`list_skill_tags`、`install_skill_to_agent`、`uninstall_skill_from_agent`、`uninstall_skill_pack_with_copies` 方法
- `frontend/src/api/finance.rs` — 新增 `list_tool_tags` 方法（或从 hr.rs 重导出）
- `frontend/src/pages/hr/agent_detail.rs` — 4 处安装区改为搜索下拉框 + 已装卡片网格 + 技能包卸载确认对话框

---

### Task 1: Backend — Tool tags 聚合接口

**Files:**
- Modify: `src/service/dao/tool/mod.rs` — ToolDao trait 加方法
- Modify: `src/service/dao/tool/sqlite.rs` — 实现 `list_distinct_tags`
- Modify: `src/service/dal/tool.rs` — ToolDal trait + impl
- Modify: `src/service/domain/finance/tool_provider.rs` — domain trait + impl
- Modify: `src/service/domain/finance/mod.rs` — trait 定义
- Modify: `common/src/api/tool.rs` — 新增 `ListToolTagsResponse`
- Create: `src/handlers/finance/tool/list_tool_tags.rs`
- Modify: `src/handlers/finance/tool/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 新增 DTO**

在 `common/src/api/tool.rs` 末尾追加：

```rust
/// Tool tags 聚合响应（distinct tags from enabled tools）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListToolTagsResponse {
    /// 所有启用工具的不重复 tag 列表
    pub tags: Vec<String>,
}
```

在 `common/src/api/mod.rs` 确认 `pub use tool::ListToolTagsResponse;` 已导出（如未导出则补上）。

- [ ] **Step 2: DAO trait 加方法**

在 `src/service/dao/tool/mod.rs` 的 `ToolDao` trait 中追加：

```rust
/// 列出所有启用工具的 distinct tags
async fn list_distinct_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;
```

- [ ] **Step 3: DAO 实现**

在 `src/service/dao/tool/sqlite.rs` 的 `impl ToolDao for SqliteToolDao` 中追加方法：

```rust
async fn list_distinct_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
    let pool = ctx.db_pool();
    // 只取 status=1 (Enabled) 的工具 tags，distinct 去重，排除空字符串
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT json_each.value \
         FROM tools, json_each(tools.tags) \
         WHERE tools.status = 1 AND json_each.value != '' \
         ORDER BY json_each.value ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(v,)| v).collect())
}
```

- [ ] **Step 4: DAL trait + impl**

在 `src/service/dal/tool.rs` 的 `ToolDal` trait 中追加：

```rust
/// 列出所有启用工具的 distinct tags
async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;
```

在 `ToolDalImpl` 的 impl 中追加：

```rust
async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
    self.tool_dao.list_distinct_tags(ctx).await
}
```

- [ ] **Step 5: Domain trait + impl**

在 `src/service/domain/finance/mod.rs` 的 `ToolProviderManage` trait 中追加：

```rust
/// 列出所有启用工具的 distinct tags（用于前端下拉框数据源）
async fn list_tool_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;
```

在 `src/service/domain/finance/tool_provider.rs` 的 impl 中追加：

```rust
async fn list_tool_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
    self.tool_dal.list_tags(ctx).await
}
```

- [ ] **Step 6: Handler**

创建 `src/handlers/finance/tool/list_tool_tags.rs`：

```rust
//! Handler: GET /api/v1/finance/tools/tags - List distinct tags from enabled tools

use crate::pkg::RequestContext;
use crate::service::domain::finance::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ListToolTagsResponse;
use common::error::Result;

/// List all distinct tags from enabled tools. Used for tool pack install dropdown.
#[register_handler_tool(
    id = "list_tool_tags",
    name = "list_tool_tags",
    description = "List all distinct tags from enabled tools for dropdown data source",
    params = "common::api::EmptyRequest",
    tags = "tool_management"
)]
#[generate_http_handler]
pub async fn list_tool_tags(ctx: RequestContext) -> Result<ListToolTagsResponse> {
    let tags = domain().tool_provider_manage().list_tool_tags(ctx).await?;
    Ok(ListToolTagsResponse { tags })
}
```

> 注：`EmptyRequest` 如不存在，用 `()` 或新建一个空结构体。检查 `common::api` 中是否已有空请求类型，若无则 handler 签名用 `#[generate_http_handler] pub async fn list_tool_tags(ctx: RequestContext) -> Result<ListToolTagsResponse>` 不带 params 参数（参考现有无参数 handler 模式）。

在 `src/handlers/finance/tool/mod.rs` 追加 `pub mod list_tool_tags;` 并声明 `pub use list_tool_tags::list_tool_tags_handler;`。

- [ ] **Step 7: Router 注册**

在 `src/router.rs` 的 finance 工具路由区块中，在 `/tools/query` 路由之后追加：

```rust
.route(
    "/tools/tags",
    get(handlers::finance::tool::list_tool_tags::list_tool_tags_handler),
)
```

- [ ] **Step 8: 编译验证**

Run: `cargo check -p ai_orz 2>&1 | tail -5`
Expected: 编译通过，0 errors

- [ ] **Step 9: 测试**

在 `src/service/dao/tool/sqlite.rs` 的 `#[cfg(test)]` 测试模块中追加（如果不存在测试模块则新建 `src/service/dao/tool/sqlite_test.rs`）：

```rust
#[tokio::test]
async fn test_list_distinct_tags_returns_unique_enabled_tags() {
    let pool = setup_test_db().await;
    // 插入 3 条工具：2 个 enabled（tag1, tag2, tag1）+ 1 个 disabled（tag3）
    sqlx::query("INSERT INTO tools (id, name, description, protocol, status, tags, config, created_at, updated_at) VALUES ('t1', 'Tool1', '', 0, 1, '[\"tag1\",\"tag2\"]', '{}', 0, 0)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO tools (id, name, description, protocol, status, tags, config, created_at, updated_at) VALUES ('t2', 'Tool2', '', 0, 1, '[\"tag1\",\"tag3\"]', '{}', 0, 0)")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO tools (id, name, description, protocol, status, tags, config, created_at, updated_at) VALUES ('t3', 'Tool3', '', 0, 0, '[\"tag4\"]', '{}', 0, 0)")
        .execute(&pool).await.unwrap();

    let dao = SqliteToolDao;
    let ctx = RequestContext::for_test(pool);
    let tags = dao.list_distinct_tags(ctx).await.unwrap();
    // 应返回 tag1, tag2, tag3（tag4 属于 disabled 工具，不返回）
    assert_eq!(tags, vec!["tag1".to_string(), "tag2".to_string(), "tag3".to_string()]);
}
```

Run: `cargo test -p ai_orz --lib tool 2>&1 | tail -5`
Expected: 测试通过

- [ ] **Step 10: Commit**

```bash
git add common/src/api/tool.rs src/service/dao/tool/ src/service/dal/tool.rs src/service/domain/finance/ src/handlers/finance/tool/ src/router.rs
git commit -m "feat(tool): add GET /finance/tools/tags endpoint for distinct tag aggregation"
```

---

### Task 2: Backend — Skill tags 聚合接口

**Files:**
- Modify: `src/service/dao/skill/mod.rs` — SkillDao trait 加方法
- Modify: `src/service/dao/skill/sqlite.rs` — 实现 `list_distinct_tags`
- Modify: `src/service/dal/skill.rs` — SkillDal trait + impl
- Modify: `src/service/domain/hr/skill.rs` — domain trait + impl
- Modify: `src/service/domain/hr/mod.rs` — trait 定义
- Modify: `common/src/api/skill.rs` — 新增 `ListSkillTagsResponse`
- Create: `src/handlers/hr/skill/list_skill_tags.rs`
- Modify: `src/handlers/hr/skill/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 新增 DTO**

在 `common/src/api/skill.rs` 末尾追加：

```rust
/// Skill tags 聚合响应（distinct tags from Published skills）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListSkillTagsResponse {
    /// 所有已发布技能的不重复 tag 列表
    pub tags: Vec<String>,
}
```

在 `common/src/api/mod.rs` 确认导出 `pub use skill::ListSkillTagsResponse;`。

- [ ] **Step 2: DAO trait 加方法**

在 `src/service/dao/skill/mod.rs` 的 `SkillDao` trait 中追加：

```rust
/// 列出所有已发布技能的 distinct tags
async fn list_distinct_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;
```

- [ ] **Step 3: DAO 实现**

在 `src/service/dao/skill/sqlite.rs` 的 impl 中追加：

```rust
async fn list_distinct_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
    let pool = ctx.db_pool();
    // 只取 status=2 (Published) 的技能 tags，distinct 去重
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT json_each.value \
         FROM skills, json_each(skills.tags) \
         WHERE skills.status = 2 AND json_each.value != '' \
         ORDER BY json_each.value ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(v,)| v).collect())
}
```

- [ ] **Step 4: DAL trait + impl**

在 `src/service/dal/skill.rs` 的 `SkillDal` trait 中追加：

```rust
/// 列出所有已发布技能的 distinct tags
async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;
```

在 `SkillDalImpl` impl 中追加：

```rust
async fn list_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
    self.skill_dao.list_distinct_tags(ctx).await
}
```

- [ ] **Step 5: Domain trait + impl**

在 `src/service/domain/hr/mod.rs` 的 `SkillManage` trait 中追加：

```rust
/// 列出所有已发布技能的 distinct tags（用于前端技能包下拉框数据源）
async fn list_skill_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;
```

在 `src/service/domain/hr/skill.rs` 的 impl 中追加：

```rust
async fn list_skill_tags(&self, ctx: RequestContext) -> Result<Vec<String>> {
    self.skill_dal.list_tags(ctx).await
}
```

- [ ] **Step 6: Handler**

创建 `src/handlers/hr/skill/list_skill_tags.rs`：

```rust
//! Handler: GET /api/v1/hr/skills/tags - List distinct tags from published skills

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ListSkillTagsResponse;
use common::error::Result;

/// List all distinct tags from published skills. Used for skill pack install dropdown.
#[register_handler_tool(
    id = "list_skill_tags",
    name = "list_skill_tags",
    description = "List all distinct tags from published skills for dropdown data source",
    params = "common::api::EmptyRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn list_skill_tags(ctx: RequestContext) -> Result<ListSkillTagsResponse> {
    let tags = domain().skill_manage().list_skill_tags(ctx).await?;
    Ok(ListSkillTagsResponse { tags })
}
```

在 `src/handlers/hr/skill/mod.rs` 追加 `pub mod list_skill_tags;` 并声明 handler。

- [ ] **Step 7: Router 注册**

在 `src/router.rs` 的 hr skills 路由区块中，在 `/skills/query` 之后追加：

```rust
.route(
    "/skills/tags",
    get(handlers::hr::skill::list_skill_tags::list_skill_tags_handler),
)
```

- [ ] **Step 8: 编译 + 测试 + Commit**

Run: `cargo check -p ai_orz 2>&1 | tail -5`
Expected: 0 errors

```bash
git add common/src/api/skill.rs src/service/dao/skill/ src/service/dal/skill.rs src/service/domain/hr/ src/handlers/hr/skill/ src/router.rs
git commit -m "feat(skill): add GET /hr/skills/tags endpoint for distinct tag aggregation"
```

---

### Task 3: Backend — 单技能卸载接口

**Files:**
- Modify: `common/src/api/skill.rs` — 新增 `UninstallSkillFromAgentRequest/Response`
- Modify: `src/service/domain/hr/skill.rs` — domain trait + impl 加 `uninstall_from_agent`
- Modify: `src/service/domain/hr/mod.rs` — trait 定义
- Modify: `src/service/dal/skill.rs` — DAL 加 `delete_agent_skill_copy`
- Modify: `src/service/dao/skill/mod.rs` — DAO trait 加 `delete_by_id`
- Modify: `src/service/dao/skill/sqlite.rs` — DAO 实现
- Create: `src/handlers/hr/skill/uninstall_skill_from_agent.rs`
- Modify: `src/handlers/hr/skill/mod.rs`
- Modify: `src/router.rs`

- [ ] **Step 1: 新增 DTO**

在 `common/src/api/skill.rs` 追加：

```rust
/// 单技能卸载请求（从 Agent 目录删除技能副本）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UninstallSkillFromAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
    /// Skill ID（Agent 目录中的副本 ID）
    #[param(source = "path")]
    pub skill_id: String,
}

/// 单技能卸载响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UninstallSkillFromAgentResponse {
    pub agent_id: String,
    pub skill_id: String,
    pub deleted: bool,
}
```

- [ ] **Step 2: DAO 层 — 确保 delete_by_id 存在**

检查 `src/service/dao/skill/mod.rs` 是否已有 `delete_by_id` 方法。如果没有，在 trait 中追加：

```rust
/// 根据 ID 删除技能记录
async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<()>;
```

在 `src/service/dao/skill/sqlite.rs` 实现（如果不存在）：

```rust
async fn delete_by_id(&self, ctx: RequestContext, id: &str) -> Result<()> {
    let pool = ctx.db_pool();
    // 先删除文件目录（skills/{id}/），再删除 DB 记录
    let content_path: Option<(String,)> = sqlx::query_as("SELECT content_path FROM skills WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    if let Some((path,)) = content_path {
        let _ = std::fs::remove_dir_all(&path);
    }
    sqlx::query("DELETE FROM skills WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
```

> 注：检查现有代码是否已有类似删除逻辑（如 `delete_skill` handler 调用的 DAO 方法），如果有则复用，避免重复。

- [ ] **Step 3: DAL 层**

在 `src/service/dal/skill.rs` 的 `SkillDal` trait 中确认有 `delete` 方法（或追加）：

```rust
/// 删除技能（DB 记录 + 文件目录）
async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()>;
```

实现中调用 `self.skill_dao.delete_by_id(ctx, id).await`。

- [ ] **Step 4: Domain trait + impl**

在 `src/service/domain/hr/mod.rs` 的 `SkillManage` trait 中追加：

```rust
/// 从 Agent 目录卸载技能副本（删除 DB 记录 + 文件目录）
async fn uninstall_from_agent(
    &self,
    ctx: RequestContext,
    skill_id: &str,
    agent_id: &str,
) -> Result<()>;
```

在 `src/service/domain/hr/skill.rs` 实现：

```rust
async fn uninstall_from_agent(
    &self,
    ctx: RequestContext,
    skill_id: &str,
    agent_id: &str,
) -> Result<()> {
    // 查找技能，验证属于该 Agent 且是副本（parent_skill_id 不为空）
    let Some(skill) = self.skill_dal.find_by_id(ctx.clone(), skill_id).await? else {
        bail_err!(NotFound, "Skill not found: {}", skill_id);
    };
    if skill.po.author_id != agent_id {
        bail_err!(InvalidRequest, "Skill {} does not belong to agent {}", skill_id, agent_id);
    }
    if skill.po.parent_skill_id.is_none() {
        bail_err!(InvalidRequest, "Skill {} is not an installed copy, cannot uninstall", skill_id);
    }
    self.skill_dal.delete(ctx, skill_id).await
}
```

- [ ] **Step 5: Handler**

创建 `src/handlers/hr/skill/uninstall_skill_from_agent.rs`：

```rust
//! Handler: DELETE /api/v1/hr/agents/{agent_id}/skills/{skill_id} - Uninstall skill from agent

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{UninstallSkillFromAgentRequest, UninstallSkillFromAgentResponse};
use common::error::Result;

/// Uninstall a skill copy from an agent. Deletes the agent's private copy (DB + files).
/// Only applies to installed copies (parent_skill_id is not null).
#[register_handler_tool(
    id = "uninstall_skill_from_agent",
    name = "uninstall_skill_from_agent",
    description = "Uninstall a skill copy from an agent. Deletes the agent's private copy.",
    params = "common::api::UninstallSkillFromAgentRequest",
    tags = "skill_management"
)]
#[generate_http_handler]
pub async fn uninstall_skill_from_agent(
    ctx: RequestContext,
    params: UninstallSkillFromAgentRequest,
) -> Result<UninstallSkillFromAgentResponse> {
    let ctx = ctx.to_builder().agent_id(&params.agent_id).build();
    domain()
        .skill_manage()
        .uninstall_from_agent(ctx, &params.skill_id, &params.agent_id)
        .await?;
    Ok(UninstallSkillFromAgentResponse {
        agent_id: params.agent_id,
        skill_id: params.skill_id,
        deleted: true,
    })
}
```

在 `src/handlers/hr/skill/mod.rs` 追加模块声明和 handler 导出。

- [ ] **Step 6: Router 注册**

在 `src/router.rs` 中，找到 `/agents/{agent_id}/skills/{skill_id}` 路由（当前只有 `post`），追加 `delete`：

```rust
.route(
    "/agents/{agent_id}/skills/{skill_id}",
    post(handlers::hr::skill::install_skill_to_agent_handler)
        .delete(handlers::hr::skill::uninstall_skill_from_agent_handler),
)
```

- [ ] **Step 7: 编译验证 + Commit**

Run: `cargo check -p ai_orz 2>&1 | tail -5`
Expected: 0 errors

```bash
git add common/src/api/skill.rs src/service/dao/skill/ src/service/dal/skill.rs src/service/domain/hr/ src/handlers/hr/skill/ src/router.rs
git commit -m "feat(skill): add DELETE /agents/{id}/skills/{skill_id} for single skill uninstall"
```

---

### Task 4: Backend — 技能包卸载+删副本接口

**Files:**
- Modify: `common/src/api/agent.rs` — 扩展 `UninstallSkillPackRequest` 加 `delete_copies` 字段
- Modify: `src/service/domain/hr/agent.rs` — 扩展 `uninstall_skill_pack` 逻辑
- Modify: `src/handlers/hr/agent/uninstall_skill_pack.rs` — 传递 `delete_copies` 参数

- [ ] **Step 1: 扩展 DTO**

在 `common/src/api/agent.rs` 中找到 `UninstallSkillPackRequest`，追加 `delete_copies` 字段：

```rust
/// 卸载技能包请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UninstallSkillPackRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
    /// 技能包 tag
    #[param(source = "path")]
    pub tag: String,
    /// 是否同时删除 Agent 侧的技能副本（默认 false，仅移除 tag 关联）
    #[param(source = "query")]
    pub delete_copies: Option<bool>,
}
```

- [ ] **Step 2: 扩展 Domain 逻辑**

在 `src/service/domain/hr/agent.rs` 的 `uninstall_skill_pack` 方法中，追加删副本逻辑。修改方法签名加 `delete_copies: bool` 参数：

```rust
async fn uninstall_skill_pack(
    &self,
    ctx: RequestContext,
    agent_id: &str,
    tag: &str,
    delete_copies: bool,
) -> Result<()> {
    let mut agent = self.get_agent_internal(ctx.clone(), agent_id).await?;
    // 移除 tag 标记
    agent.po.uninstall_skill_pack_tag(tag);

    // 如果 delete_copies=true，删除该 tag 下 Agent 的技能副本
    if delete_copies {
        let copies = self.skill_dal.query(ctx.clone(), SkillQuery {
            author_id: Some(agent_id.to_string()),
            parent_skill_id: Some("__not_null__".to_string()), // 标记查副本
            tags: Some(vec![tag.to_string()]),
            ..Default::default()
        }).await?;
        for skill in copies.items {
            let _ = self.skill_dal.delete(ctx.clone(), &skill.po.id).await;
        }
    }

    self.agent_dal.update(ctx, &agent).await?;
    Ok(())
}
```

> 注：`parent_skill_id: Some("__not_null__".to_string())` 不能直接工作，因为 DAO 的 push_query_filters 只做等值匹配。需要改用 `parent_skill_id: Some(String::new())` + DAO 层特殊处理（IS NOT NULL），或者在 DAO 层新增 `has_parent: Option<bool>` 过滤字段。**推荐方案**：在 `SkillQuery` 中新增 `has_parent: Option<bool>` 字段，DAO 层 `push_query_filters` 中处理 `IS NOT NULL` / `IS NULL`。

- [ ] **Step 3: SkillQuery 加 has_parent 字段**

在 `src/service/dao/skill/mod.rs` 的 `SkillQuery` 结构体中追加：

```rust
/// 是否有父技能（true = 只查副本，false = 只查原始技能，None = 不过滤）
pub has_parent: Option<bool>,
```

在 `src/service/dao/skill/sqlite.rs` 的 `push_query_filters` 中追加：

```rust
if let Some(has_parent) = query.has_parent {
    if has_parent {
        builder.push(" AND parent_skill_id IS NOT NULL");
    } else {
        builder.push(" AND parent_skill_id IS NULL");
    }
}
```

- [ ] **Step 4: 修改 handler**

在 `src/handlers/hr/agent/uninstall_skill_pack.rs` 中，传递 `delete_copies`：

```rust
pub async fn uninstall_skill_pack(
    ctx: RequestContext,
    params: UninstallSkillPackRequest,
) -> Result<()> {
    let delete_copies = params.delete_copies.unwrap_or(false);
    domain()
        .agent_manage()
        .uninstall_skill_pack(ctx, &params.agent_id, &params.tag, delete_copies)
        .await?;
    Ok(())
}
```

- [ ] **Step 5: 修改 domain trait 定义**

在 `src/service/domain/hr/mod.rs` 的 `AgentManage` trait 中，修改 `uninstall_skill_pack` 签名加 `delete_copies: bool` 参数。

- [ ] **Step 6: 编译验证 + Commit**

Run: `cargo check -p ai_orz 2>&1 | tail -5`
Expected: 0 errors

```bash
git add common/src/api/agent.rs src/service/dao/skill/ src/service/domain/hr/ src/handlers/hr/agent/uninstall_skill_pack.rs
git commit -m "feat(skill): support delete_copies param in uninstall_skill_pack endpoint"
```

---

### Task 5: Frontend — API 方法补齐

**Files:**
- Modify: `frontend/src/api/hr.rs`
- Modify: `frontend/src/api/finance.rs`

- [ ] **Step 1: 新增前端 API 方法**

在 `frontend/src/api/finance.rs` 中追加 tool tags 方法：

```rust
pub async fn list_tool_tags() -> Result<common::api::ListToolTagsResponse, ApiError> {
    api_get("/api/v1/finance/tools/tags").await
}
```

在 `frontend/src/api/hr.rs` 中追加 skill tags + 单技能安装/卸载方法：

```rust
pub async fn list_skill_tags() -> Result<common::api::ListSkillTagsResponse, ApiError> {
    api_get("/api/v1/hr/skills/tags").await
}

pub async fn install_skill_to_agent(
    req: common::api::InstallSkillToAgentRequest,
) -> Result<common::api::InstallSkillToAgentResponse, ApiError> {
    api_post(
        &format!("/api/v1/hr/agents/{}/skills/{}", req.agent_id, req.skill_id),
        &serde_json::json!({}),
    )
    .await
}

pub async fn uninstall_skill_from_agent(
    req: common::api::UninstallSkillFromAgentRequest,
) -> Result<(), ApiError> {
    api_delete(&format!(
        "/api/v1/hr/agents/{}/skills/{}",
        req.agent_id, req.skill_id
    ))
    .await
}
```

修改现有 `uninstall_skill_pack` 方法，支持 `delete_copies` 参数：

```rust
pub async fn uninstall_skill_pack(
    req: UninstallSkillPackRequest,
) -> Result<(), ApiError> {
    let qs = super::build_query_string(&[
        ("delete_copies", req.delete_copies.map(|v| v.to_string())),
    ]);
    api_delete(&format!(
        "/api/v1/hr/agents/{}/skill-packs/{}{}",
        req.agent_id, req.tag, qs
    ))
    .await
}
```

- [ ] **Step 2: 确认 DTO 导入**

确保 `frontend/src/api/hr.rs` 顶部 use 语句包含新 DTO：

```rust
use common::api::{
    // ... 现有导入 ...
    UninstallSkillFromAgentRequest, ListSkillTagsResponse,
};
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p frontend 2>&1 | tail -5`
Expected: 0 errors（可能因未使用而有 warnings，正常）

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/hr.rs frontend/src/api/finance.rs
git commit -m "feat(frontend): add list_tool_tags, list_skill_tags, install/uninstall single skill API methods"
```

---

### Task 6: Frontend — SearchableSelect 通用组件

**Files:**
- Create: `frontend/src/components/searchable_select.rs`
- Modify: `frontend/src/components/mod.rs`

- [ ] **Step 1: 创建组件**

创建 `frontend/src/components/searchable_select.rs`：

```rust
//! 通用搜索下拉框组件
//!
//! 支持两种数据源模式：
//! - Static: 传入完整候选列表，前端 filter
//! - Dynamic: 传入搜索函数，输入时实时调接口

use dioxus::prelude::*;

/// 搜索下拉框 Props
#[derive(Props, Clone, PartialEq)]
pub struct SearchableSelectProps {
    /// 输入框 placeholder
    pub placeholder: String,
    /// 当前选中的值
    pub selected: Option<String>,
    /// 候选列表（静态模式）
    pub options: Vec<String>,
    /// 选中值时的回调
    pub on_select: EventHandler<String>,
    /// 输入文本变化时的回调（动态搜索模式，可选）
    pub on_search: Option<EventHandler<String>>,
    /// 是否正在搜索（动态模式显示 loading）
    #[props(default = false)]
    pub loading: bool,
}

#[component]
pub fn SearchableSelect(props: SearchableSelectProps) -> Element {
    let mut input_value = use_signal(String::new);
    let mut show_dropdown = use_signal(false);
    let mut focused_index = use_signal(|| 0usize);

    // 根据输入文本过滤候选（静态模式）
    let filtered_options: Vec<String> = props
        .options
        .iter()
        .filter(|opt| {
            input_value
                .read()
                .is_empty()
                || opt.to_lowercase().contains(&input_value.read().to_lowercase())
        })
        .cloned()
        .collect();

    rsx! {
        div { class: "relative w-full",
            // 输入框
            input {
                class: "input input-bordered input-sm w-full",
                r#type: "text",
                placeholder: "{props.placeholder}",
                value: "{input_value}",
                onfocus: move |_| show_dropdown.set(true),
                oninput: move |e| {
                    input_value.set(e.value().clone());
                    focused_index.set(0);
                    if let Some(handler) = &props.on_search {
                        handler.call(e.value().clone());
                    }
                },
                onkeydown: move |e| {
                    match e.key().as_str() {
                        "ArrowDown" => {
                            if focused_index() + 1 < filtered_options.len() {
                                focused_index.set(focused_index() + 1);
                            }
                        }
                        "ArrowUp" => {
                            if focused_index() > 0 {
                                focused_index.set(focused_index() - 1);
                            }
                        }
                        "Enter" => {
                            if let Some(opt) = filtered_options.get(focused_index()) {
                                props.on_select.call(opt.clone());
                                input_value.set(String::new());
                                show_dropdown.set(false);
                            }
                        }
                        "Escape" => {
                            show_dropdown.set(false);
                        }
                        _ => {}
                    }
                },
            }

            // 下拉列表
            if show_dropdown() && !filtered_options.is_empty() {
                div {
                    class: "absolute z-50 mt-1 w-full max-h-60 overflow-auto bg-base-100 border border-base-300 rounded-lg shadow-lg",
                    onmouseleave: move |_| show_dropdown.set(false),

                    for (i, opt) in filtered_options.iter().enumerate() {
                        div {
                            class: if i == focused_index() {
                                "px-3 py-2 cursor-pointer bg-primary text-primary-content text-sm"
                            } else {
                                "px-3 py-2 cursor-pointer hover:bg-base-200 text-sm"
                            },
                            onclick: move |_| {
                                props.on_select.call(opt.clone());
                                input_value.set(String::new());
                                show_dropdown.set(false);
                            },
                            "{opt}"
                        }
                    }
                }
            }

            // Loading 指示器
            if props.loading {
                div {
                    class: "absolute right-2 top-1/2 -translate-y-1/2 loading loading-spinner loading-sm"
                }
            }
        }
    }
}
```

- [ ] **Step 2: 注册模块**

在 `frontend/src/components/mod.rs` 追加：

```rust
pub mod searchable_select;
pub use searchable_select::SearchableSelect;
```

- [ ] **Step 3: 编译验证**

Run: `cargo check -p frontend 2>&1 | tail -5`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/searchable_select.rs frontend/src/components/mod.rs
git commit -m "feat(frontend): add SearchableSelect reusable component with static/dynamic search"
```

---

### Task 7: Frontend — 工具包安装改为搜索下拉框

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

- [ ] **Step 1: 加载 tool tags 数据源**

在 `agent_detail.rs` 组件中，找到现有 signal 声明区域（约第 90-100 行），追加：

```rust
let mut tool_tags = use_signal(Vec::<String>::new);
```

在 `use_resource` 初始化区块中（约第 130-145 行），追加并行加载：

```rust
// 在现有 spawn 块中追加
match list_tool_tags().await {
    Ok(resp) => tool_tags.set(resp.tags),
    Err(e) => toast.error(&format!("获取工具包标签失败: {}", e)),
}
```

- [ ] **Step 2: 替换工具包安装输入框为 SearchableSelect**

找到工具包区块（约第 526-596 行），将现有的 `input + button` 替换为：

```rust
// === 工具包安装 ===
div { class: "mb-6",
    h3 { class: "text-lg font-semibold mb-3", "工具包安装" }
    div { class: "flex gap-2 items-center",
        div { class: "flex-1",
            SearchableSelect {
                placeholder: "搜索工具包 tag...".to_string(),
                selected: None,
                options: tool_tags.read().clone(),
                on_select: move |tag: String| {
                    let aid = agent_id.clone();
                    spawn(async move {
                        match install_tool_pack(InstallToolPackRequest {
                            agent_id: aid.clone(),
                            tag: tag.clone(),
                        }).await {
                            Ok(_) => {
                                toast.success(&format!("工具包 [{}] 已安装", tag));
                                match list_installed_tool_packs(&aid).await {
                                    Ok(resp) => tool_packs.set(resp.installed_tags),
                                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                }
                            }
                            Err(e) => toast.error(&format!("安装失败: {}", e)),
                        }
                    });
                },
                on_search: None,
            }
        }
    }

    // 已安装工具包 badge 列表（保持现有逻辑不变）
    if !tool_packs.read().is_empty() {
        div { class: "flex flex-wrap gap-2 mt-3",
            for tag in tool_packs.read().iter() {
                div { class: "badge badge-accent gap-1",
                    "{tag}"
                    button {
                        class: "btn btn-ghost btn-xs btn-circle",
                        onclick: move |_| {
                            let aid = agent_id.clone();
                            let t = tag.clone();
                            spawn(async move {
                                match uninstall_tool_pack(UninstallSkillPackRequest {
                                    agent_id: aid.clone(),
                                    tag: t.clone(),
                                    delete_copies: None,
                                }).await {
                                    Ok(_) => {
                                        toast.success(&format!("工具包 [{}] 已卸载", t));
                                        match list_installed_tool_packs(&aid).await {
                                            Ok(resp) => tool_packs.set(resp.installed_tags),
                                            Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                        }
                                    }
                                    Err(e) => toast.error(&format!("卸载失败: {}", e)),
                                }
                            });
                        },
                        "×"
                    }
                }
            }
        }
    }
}
```

> 注：`uninstall_tool_pack` 的 DTO 类型需要确认是 `UninstallToolPackRequest` 还是 `UninstallSkillPackRequest`。查看 `common/src/api/agent.rs` 中的实际类型名。如果是 `UninstallToolPackRequest`，需要相应调整。代码中应使用正确的类型名。

- [ ] **Step 3: 编译验证**

Run: `cargo check -p frontend 2>&1 | tail -10`
Expected: 0 errors

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(frontend): replace tool pack install input with SearchableSelect"
```

---

### Task 8: Frontend — 技能包安装改为搜索下拉框 + 卸载确认对话框

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

- [ ] **Step 1: 加载 skill tags 数据源**

在 signal 声明区追加：

```rust
let mut skill_tags = use_signal(Vec::<String>::new);
```

在初始化 spawn 中追加：

```rust
match list_skill_tags().await {
    Ok(resp) => skill_tags.set(resp.tags),
    Err(e) => toast.error(&format!("获取技能包标签失败: {}", e)),
}
```

- [ ] **Step 2: 新增卸载确认对话框状态**

在 signal 声明区追加：

```rust
// 技能包卸载确认对话框
let mut show_skill_pack_uninstall_dialog = use_signal(|| None::<(String,)>);
```

- [ ] **Step 3: 替换技能包安装输入框为 SearchableSelect**

找到技能包区块（约第 598-668 行），替换为：

```rust
// === 技能包安装 ===
div { class: "mb-6",
    h3 { class: "text-lg font-semibold mb-3", "技能包安装" }
    div { class: "flex gap-2 items-center",
        div { class: "flex-1",
            SearchableSelect {
                placeholder: "搜索技能包 tag...".to_string(),
                selected: None,
                options: skill_tags.read().clone(),
                on_select: move |tag: String| {
                    let aid = agent_id.clone();
                    spawn(async move {
                        match install_skill_pack(InstallSkillPackRequest {
                            agent_id: aid.clone(),
                            tag: tag.clone(),
                        }).await {
                            Ok(_) => {
                                toast.success(&format!("技能包 [{}] 已安装", tag));
                                match list_installed_skill_packs(&aid).await {
                                    Ok(resp) => skill_packs.set(resp.skill_packs),
                                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                }
                            }
                            Err(e) => toast.error(&format!("安装失败: {}", e)),
                        }
                    });
                },
                on_search: None,
            }
        }
    }

    // 已安装技能包 badge 列表
    if !skill_packs.read().is_empty() {
        div { class: "flex flex-wrap gap-2 mt-3",
            for tag in skill_packs.read().iter() {
                div { class: "badge badge-info gap-1",
                    "{tag}"
                    button {
                        class: "btn btn-ghost btn-xs btn-circle",
                        onclick: move |_| {
                            // 打开确认对话框而非直接卸载
                            show_skill_pack_uninstall_dialog.set(Some((tag.clone(),)));
                        },
                        "×"
                    }
                }
            }
        }
    }
}
```

- [ ] **Step 4: 添加卸载确认对话框**

在 rsx 底部（Tab 内容之后、组件闭合之前）追加对话框：

```rust
// 技能包卸载确认对话框
if let Some((tag,)) = show_skill_pack_uninstall_dialog.read().as_ref() {
    let tag_clone = tag.clone();
    let tag_clone2 = tag.clone();
    div {
        class: "modal modal-open",
        onclick: move |_| show_skill_pack_uninstall_dialog.set(None),
        div {
            class: "modal-box",
            onclick: move |e| e.stop_propagation(),
            h3 { class: "font-bold text-lg mb-2", "卸载技能包" }
            p { class: "text-sm text-base-content/70 mb-4",
                "即将卸载技能包 [{{tag_clone}}]，请选择卸载方式："
            }
            div { class: "flex flex-col gap-3",
                // 选项 A：仅移除关联
                button {
                    class: "btn btn-ghost justify-start text-left",
                    onclick: move |_| {
                        let aid = agent_id.clone();
                        let t = tag_clone.clone();
                        show_skill_pack_uninstall_dialog.set(None);
                        spawn(async move {
                            match uninstall_skill_pack(UninstallSkillPackRequest {
                                agent_id: aid.clone(),
                                tag: t.clone(),
                                delete_copies: Some(false),
                            }).await {
                                Ok(_) => {
                                    toast.success(&format!("技能包 [{}] 已卸载（保留副本）", t));
                                    match list_installed_skill_packs(&aid).await {
                                        Ok(resp) => skill_packs.set(resp.skill_packs),
                                        Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                    }
                                }
                                Err(e) => toast.error(&format!("卸载失败: {}", e)),
                            }
                        });
                    },
                    div {
                        p { class: "font-medium", "仅移除关联" }
                        p { class: "text-xs text-base-content/50", "移除 tag 标记，保留 Agent 侧技能副本" }
                    }
                }
                // 选项 B：同时删除副本（带风险警告）
                button {
                    class: "btn btn-error btn-outline justify-start text-left",
                    onclick: move |_| {
                        let aid = agent_id.clone();
                        let t = tag_clone2.clone();
                        show_skill_pack_uninstall_dialog.set(None);
                        spawn(async move {
                            match uninstall_skill_pack(UninstallSkillPackRequest {
                                agent_id: aid.clone(),
                                tag: t.clone(),
                                delete_copies: Some(true),
                            }).await {
                                Ok(_) => {
                                    toast.success(&format!("技能包 [{}] 已卸载（含副本删除）", t));
                                    match list_installed_skill_packs(&aid).await {
                                        Ok(resp) => skill_packs.set(resp.skill_packs),
                                        Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                    }
                                    // 刷新已安装单个技能列表
                                    match list_agent_skills(&aid).await {
                                        Ok(resp) => installed_skills.set(resp.skills),
                                        Err(_) => {}
                                    }
                                }
                                Err(e) => toast.error(&format!("卸载失败: {}", e)),
                            }
                        });
                    },
                    div {
                        p { class: "font-medium", "移除关联 + 删除副本" }
                        p { class: "text-xs text-error/70",
                            "⚠ Agent 技能可能已经进化（修改过内容），删除后无法恢复"
                        }
                    }
                }
            }
            div { class: "modal-action",
                button {
                    class: "btn btn-ghost",
                    onclick: move |_| show_skill_pack_uninstall_dialog.set(None),
                    "取消"
                }
            }
        }
    }
}
```

- [ ] **Step 5: 编译验证 + Commit**

Run: `cargo check -p frontend 2>&1 | tail -10`
Expected: 0 errors

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(frontend): replace skill pack install input with SearchableSelect + uninstall confirmation dialog"
```

---

### Task 9: Frontend — 单个工具绑定改为搜索下拉框

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

- [ ] **Step 1: 新增工具搜索状态**

在 signal 声明区追加：

```rust
let mut tool_search_results = use_signal(Vec::<ToolListItem>::new);
let mut tool_search_loading = use_signal(|| false);
```

- [ ] **Step 2: 替换工具绑定区块**

找到工具绑定区块（约第 670-743 行），将全量卡片网格改为「搜索框 + 已绑定卡片网格」：

```rust
// === 工具绑定 ===
div { class: "mb-6",
    h3 { class: "text-lg font-semibold mb-3", "工具绑定" }

    // 搜索框
    div { class: "mb-4",
        SearchableSelect {
            placeholder: "搜索工具名称...".to_string(),
            selected: None,
            options: tool_search_results.read().iter().map(|t| {
                format!("{} ({})", t.name, t.id)
            }).collect(),
            on_select: move |selection: String| {
                // 从 "name (id)" 格式中提取 id
                if let Some(id_start) = selection.rfind("(") {
                    let tool_id = selection[id_start+1..selection.len()-1].to_string();
                    let aid = agent_id.clone();
                    spawn(async move {
                        match bind_tool_to_agent(BindToolToAgentRequest {
                            agent_id: aid.clone(),
                            tool_id: tool_id.clone(),
                        }).await {
                            Ok(_) => {
                                toast.success("工具已绑定");
                                match get_agent(build_agent_stats_request(aid.clone())).await {
                                    Ok(a) => agent_data.set(Some(a)),
                                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                }
                            }
                            Err(e) => toast.error(&format!("绑定失败: {}", e)),
                        }
                    });
                }
            },
            on_search: Some({
                let mut tool_search_loading = tool_search_loading.clone();
                move |keyword: String| {
                    let mut results = tool_search_results.clone();
                    spawn(async move {
                        if keyword.trim().is_empty() {
                            results.set(Vec::new());
                            return;
                        }
                        tool_search_loading.set(true);
                        match query_tools(&ToolQueryRequest {
                            keyword: Some(keyword),
                            enabled_only: Some(true),
                            pagination: PaginationParams { limit: Some(20), offset: None },
                            ..Default::default()
                        }).await {
                            Ok(resp) => results.set(resp.items),
                            Err(_) => results.set(Vec::new()),
                        }
                        tool_search_loading.set(false);
                    });
                }
            }),
            loading: *tool_search_loading.read(),
        }
    }

    // 已绑定工具卡片网格（只显示已绑定的）
    let bound_tools: Vec<_> = all_tools_list.iter()
        .filter(|t| agent_data.read().as_ref()
            .map(|a| a.tools.iter().any(|at| at.id == t.id))
            .unwrap_or(false))
        .collect();

    if !bound_tools.is_empty() {
        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3",
            for tool in bound_tools {
                // 复用现有卡片渲染逻辑，但只显示「解绑」按钮
                div { class: "card bg-base-200",
                    div { class: "card-body p-3",
                        div { class: "flex justify-between items-start",
                            div {
                                h4 { class: "font-medium text-sm", "{tool.name}" }
                                span { class: "badge badge-success badge-sm", "已绑定" }
                            }
                            button {
                                class: "btn btn-error btn-xs",
                                onclick: move |_| {
                                    let aid = agent_id.clone();
                                    let tid = tool.id.clone();
                                    let tname = tool.name.clone();
                                    spawn(async move {
                                        match unbind_tool_from_agent(UnbindToolFromAgentRequest {
                                            agent_id: aid.clone(),
                                            tool_id: tid.clone(),
                                        }).await {
                                            Ok(_) => {
                                                toast.success(&format!("工具 {} 已解绑", tname));
                                                match get_agent(build_agent_stats_request(aid)).await {
                                                    Ok(a) => agent_data.set(Some(a)),
                                                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(&format!("解绑失败: {}", e)),
                                        }
                                    });
                                },
                                "解绑"
                            }
                        }
                        if !tool.description.is_empty() {
                            p { class: "text-xs text-base-content/60 mt-1", "{tool.description}" }
                        }
                    }
                }
            }
        }
    } else {
        p { class: "text-sm text-base-content/40", "暂无已绑定工具" }
    }
}
```

- [ ] **Step 3: 确认导入**

确保页面顶部 use 语句包含：

```rust
use common::api::{ToolQueryRequest, PaginationParams};
use crate::api::finance::query_tools;
```

- [ ] **Step 4: 编译验证 + Commit**

Run: `cargo check -p frontend 2>&1 | tail -10`
Expected: 0 errors

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(frontend): replace tool bind card grid with SearchableSelect + bound-only cards"
```

---

### Task 10: Frontend — 单个技能安装改为搜索下拉框

**Files:**
- Modify: `frontend/src/pages/hr/agent_detail.rs`

- [ ] **Step 1: 新增技能搜索状态**

在 signal 声明区追加：

```rust
let mut skill_search_results = use_signal(Vec::<SkillListItem>::new);
let mut skill_search_loading = use_signal(|| false);
// 已安装的单个技能列表（Agent 的技能副本）
let mut installed_skills = use_signal(Vec::<SkillListItem>::new);
```

- [ ] **Step 2: 加载已安装技能列表**

在初始化 spawn 中追加：

```rust
match list_agent_skills(&agent_id).await {
    Ok(resp) => installed_skills.set(resp.skills),
    Err(e) => toast.error(&format!("获取已安装技能失败: {}", e)),
}
```

> 注：确认 `list_agent_skills` 的返回结构，`resp.skills` 字段名以实际 DTO 为准。如果该方法不存在，检查 `frontend/src/api/hr.rs` 中是否有 `list_agent_skills` 方法，如果没有则补上：

```rust
pub async fn list_agent_skills(agent_id: &str) -> Result<common::api::ListAgentSkillsResponse, ApiError> {
    api_get_or_default(&format!("/api/v1/hr/agents/{}/skills", agent_id)).await
}
```

- [ ] **Step 3: 新增技能安装区块**

在技能包区块之后、工具绑定区块之前，追加单技能安装区块：

```rust
// === 单个技能安装 ===
div { class: "mb-6",
    h3 { class: "text-lg font-semibold mb-3", "单个技能安装" }

    // 搜索框
    div { class: "mb-4",
        SearchableSelect {
            placeholder: "搜索技能名称...".to_string(),
            selected: None,
            options: skill_search_results.read().iter().map(|s| {
                format!("{} ({})", s.name, s.id)
            }).collect(),
            on_select: move |selection: String| {
                if let Some(id_start) = selection.rfind("(") {
                    let skill_id = selection[id_start+1..selection.len()-1].to_string();
                    let aid = agent_id.clone();
                    spawn(async move {
                        match install_skill_to_agent(common::api::InstallSkillToAgentRequest {
                            agent_id: aid.clone(),
                            skill_id: skill_id.clone(),
                        }).await {
                            Ok(_) => {
                                toast.success("技能已安装");
                                match list_agent_skills(&aid).await {
                                    Ok(resp) => installed_skills.set(resp.skills),
                                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                }
                            }
                            Err(e) => toast.error(&format!("安装失败: {}", e)),
                        }
                    });
                }
            },
            on_search: Some({
                let mut loading = skill_search_loading.clone();
                move |keyword: String| {
                    let mut results = skill_search_results.clone();
                    spawn(async move {
                        if keyword.trim().is_empty() {
                            results.set(Vec::new());
                            return;
                        }
                        loading.set(true);
                        match query_skills(&SkillQueryRequest {
                            keyword: Some(keyword),
                            status: Some(SkillStatus::Published),
                            pagination: PaginationParams { limit: Some(20), offset: None },
                            ..Default::default()
                        }).await {
                            Ok(resp) => results.set(resp.items),
                            Err(_) => results.set(Vec::new()),
                        }
                        loading.set(false);
                    });
                }
            }),
            loading: *skill_search_loading.read(),
        }
    }

    // 已安装技能卡片网格
    if !installed_skills.read().is_empty() {
        div { class: "grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3",
            for skill in installed_skills.read().iter() {
                let skill_clone = skill.clone();
                div { class: "card bg-base-200",
                    div { class: "card-body p-3",
                        div { class: "flex justify-between items-start",
                            div {
                                h4 { class: "font-medium text-sm", "{skill.name}" }
                                span { class: "badge badge-success badge-sm", "已安装" }
                            }
                            button {
                                class: "btn btn-error btn-xs",
                                onclick: move |_| {
                                    let aid = agent_id.clone();
                                    let sid = skill_clone.id.clone();
                                    let sname = skill_clone.name.clone();
                                    spawn(async move {
                                        match uninstall_skill_from_agent(common::api::UninstallSkillFromAgentRequest {
                                            agent_id: aid.clone(),
                                            skill_id: sid.clone(),
                                        }).await {
                                            Ok(_) => {
                                                toast.success(&format!("技能 {} 已卸载", sname));
                                                match list_agent_skills(&aid).await {
                                                    Ok(resp) => installed_skills.set(resp.skills),
                                                    Err(e) => toast.error(&format!("刷新失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(&format!("卸载失败: {}", e)),
                                        }
                                    });
                                },
                                "卸载"
                            }
                        }
                        if !skill.description.is_empty() {
                            p { class: "text-xs text-base-content/60 mt-1", "{skill.description}" }
                        }
                    }
                }
            }
        }
    } else {
        p { class: "text-sm text-base-content/40", "暂无已安装技能" }
    }
}
```

- [ ] **Step 4: 确认导入**

```rust
use common::api::{SkillQueryRequest, SkillStatus};
use common::enums::SkillStatus;
use crate::api::hr::{query_skills, install_skill_to_agent, uninstall_skill_from_agent, list_agent_skills};
```

- [ ] **Step 5: 编译验证 + Commit**

Run: `cargo check -p frontend 2>&1 | tail -10`
Expected: 0 errors

```bash
git add frontend/src/pages/hr/agent_detail.rs
git commit -m "feat(frontend): add single skill install/uninstall with SearchableSelect"
```

---

### Task 11: 集成验证

- [ ] **Step 1: 后端编译 + 测试**

Run: `cargo check -p ai_orz 2>&1 | tail -5`
Expected: 0 errors

Run: `cargo test -p ai_orz --lib 2>&1 | tail -10`
Expected: 全部通过

- [ ] **Step 2: 前端编译**

Run: `cargo check -p frontend 2>&1 | tail -10`
Expected: 0 errors

- [ ] **Step 3: 更新 AGENTS.md**

在 `AGENTS.md` 中追加 2026-07-30 里程碑：

```markdown
### 2026-07-30 里程碑（精简）
**✅ Agent 工具/技能搜索式安装**
- **后端 tags 聚合接口**：新增 `GET /finance/tools/tags`（distinct tags from enabled tools）和 `GET /hr/skills/tags`（distinct tags from published skills），DAO 层用 `SELECT DISTINCT json_each.value` 实现
- **单技能卸载接口**：新增 `DELETE /agents/{id}/skills/{skill_id}`，删除 Agent 私有副本（DB + 文件），仅限 parent_skill_id 不为空的副本
- **技能包卸载扩展**：`UninstallSkillPackRequest` 新增 `delete_copies: Option<bool>` 参数，`true` 时同时删除该 tag 下 Agent 的技能副本；SkillQuery 新增 `has_parent: Option<bool>` 字段支持过滤副本
- **前端 SearchableSelect 组件**：新增 `frontend/src/components/searchable_select.rs`，支持静态候选列表（前端 filter）和动态搜索（on_search 回调 + loading 指示器）两种模式
- **4 处安装区改造**：工具包/技能包安装改为 SearchableSelect（静态 tags 数据源）+ badge 已装列表；单个工具绑定/技能安装改为 SearchableSelect（动态 query 搜索）+ 卡片网格已装列表
- **技能包卸载确认对话框**：新增两选项确认对话框（仅移除关联 / 移除关联+删除副本），删除副本选项带风险警告
```

- [ ] **Step 4: 更新 project_memory.md**

在 `project_memory.md` 追加：

```markdown
- SearchableSelect 组件位于 frontend/src/components/searchable_select.rs，支持静态（options + 前端 filter）和动态（on_search + loading）两种模式
- Tool tags 聚合只取 enabled 工具，Skill tags 聚合只取 Published 技能，均通过 SELECT DISTINCT json_each.value 实现
- 单技能卸载（DELETE /agents/{id}/skills/{skill_id}）仅限 parent_skill_id 不为空的副本，删除 DB 记录 + 文件目录
- 技能包卸载支持 delete_copies 参数（query 参数），true 时通过 SkillQuery.has_parent=true 过滤副本并批量删除
- SkillQuery 新增 has_parent: Option<bool> 字段，DAO 层转译为 parent_skill_id IS NOT NULL / IS NULL
```

- [ ] **Step 5: Commit + Push**

```bash
git add AGENTS.md
git commit -m "docs: add 2026-07-30 milestone for tool/skill search install"
git push
```
