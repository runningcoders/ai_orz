//! Organization DAO SQLite 实现

use crate::models::organization::OrganizationPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::{OrganizationDao, OrganizationQuery};
use chrono::Utc;
use common::api::OrganizationConfig;
use common::enums::{OrganizationScope, OrganizationStatus};
use common::error::Result;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
// ==================== 工厂方法 + 单例管理 ====================

static ORGANIZATION_DAO: OnceLock<Arc<dyn OrganizationDao>> = OnceLock::new();

/// 组织级配置缓存（读穿 + 写穿）
///
/// 真正存放配置的是 organizations 表的 `config` JSON 列；此处缓存避免每条消息落库时
/// 都回查 DB。key 为 org_id，value 为解析后的 `OrganizationConfig`。
/// - 读：先查缓存，未命中回退 DB 并回填（见 `get_org_config`）。
/// - 写：更新 DB 后同步刷新缓存（见 `set_org_config`）。
static ORG_CONFIG_CACHE: LazyLock<Mutex<HashMap<String, OrganizationConfig>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// 创建一个全新的 Organization DAO 实例（用于测试）
pub fn new() -> Arc<dyn OrganizationDao> {
    Arc::new(OrganizationDaoSqliteImpl::new())
}

/// 获取 Organization DAO 单例
pub fn dao() -> Arc<dyn OrganizationDao> {
    ORGANIZATION_DAO.get().cloned().unwrap()
}

/// 初始化单例
pub fn init() {
    let _ = ORGANIZATION_DAO.set(new());
}

// ==================== 实现 ====================

struct OrganizationDaoSqliteImpl;

impl OrganizationDaoSqliteImpl {
    fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl OrganizationDao for OrganizationDaoSqliteImpl {
    async fn insert(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        let status = org.status as i32;
        let scope = org.scope as i32;
        let invite_code = org.invite_code.clone();
        let group_name = org.group_name.clone();
        sqlx::query!(
            "INSERT INTO organizations (id, name, description, base_url, group_name, status, scope, invite_code, created_by, modified_by, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            org.id,
            org.name,
            org.description,
            org.base_url,
            group_name,
            status,
            scope,
            invite_code,
            org.created_by,
            org.modified_by,
            org.created_at,
            org.updated_at
        )
            .execute(ctx.db_pool())
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, ctx: RequestContext, id: &str) -> Result<Option<OrganizationPo>> {
        let org = sqlx::query_as!(
            OrganizationPo,
            r#"
SELECT id, name, description, base_url, group_name, status as 'status: OrganizationStatus', scope as 'scope: OrganizationScope', invite_code, created_by, modified_by, created_at, updated_at
FROM organizations WHERE id = ? AND status != 0
            "#,
            id
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(org)
    }

    async fn find_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>> {
        let org = sqlx::query_as!(
            OrganizationPo,
            r#"
SELECT id, name, description, base_url, group_name, status as 'status: OrganizationStatus', scope as 'scope: OrganizationScope', invite_code, created_by, modified_by, created_at, updated_at
FROM organizations WHERE invite_code = ? AND status != 0
            "#,
            invite_code
        )
            .fetch_optional(ctx.db_pool())
            .await?;

        Ok(org)
    }

    async fn get_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<OrganizationConfig> {
        // (b) 默认读缓存，命中直接返回
        if let Some(cfg) = ORG_CONFIG_CACHE.lock().unwrap().get(org_id) {
            return Ok(cfg.clone());
        }
        // 缓存未命中，回退到 DB 并回填
        let cfg = read_org_config_from_db(ctx.db_pool(), org_id).await?;
        ORG_CONFIG_CACHE
            .lock()
            .unwrap()
            .insert(org_id.to_string(), cfg.clone());
        Ok(cfg)
    }

    async fn set_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()> {
        let json = serde_json::to_string(config)?;
        sqlx::query("UPDATE organizations SET config = ? WHERE id = ?")
            .bind(json)
            .bind(org_id)
            .execute(ctx.db_pool())
            .await?;
        // (a) 写穿缓存：DB 落盘后同步刷新
        ORG_CONFIG_CACHE
            .lock()
            .unwrap()
            .insert(org_id.to_string(), config.clone());
        Ok(())
    }

    async fn query(
        &self,
        ctx: RequestContext,
        query: OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT id, name, description, base_url, group_name, status, scope, invite_code, created_by, modified_by, created_at, updated_at FROM organizations WHERE status != 0"#,
        );

        if let Some(scope) = query.scope {
            builder.push(" AND scope = ").push_bind(scope as i32);
        }

        // 排序
        builder.push(" ORDER BY created_at DESC");

        // 限制数量
        if let Some(limit) = query.limit {
            builder.push(" LIMIT ").push_bind(limit as i64);
        }

        let rows = builder.build_query_as().fetch_all(pool).await?;

        Ok(rows)
    }

    async fn find_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>> {
        // 语法糖：调用通用查询
        self.query(ctx, OrganizationQuery::default()).await
    }

    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        let current_timestamp = Utc::now().timestamp_millis();
        let uid = ctx.caller_id_or_system();
        let status = org.status as i32;
        let scope = org.scope as i32;
        let invite_code = org.invite_code.clone();
        let group_name = org.group_name.clone();
        sqlx::query!(
            r#"
UPDATE organizations
SET name = ?, description = ?, base_url = ?, group_name = ?, status = ?, scope = ?, invite_code = ?, modified_by = ?, updated_at = ?
WHERE id = ?
            "#,
            org.name,
            org.description,
            org.base_url,
            group_name,
            status,
            scope,
            invite_code,
            uid,
            current_timestamp,
            org.id
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn delete(&self, ctx: RequestContext, id: &str) -> Result<()> {
        let current_timestamp = Utc::now().timestamp_millis();
        let uid = ctx.caller_id_or_system();
        sqlx::query!(
            r#"
UPDATE organizations SET status = 0, modified_by = ?, updated_at = ? WHERE id = ?
            "#,
            uid,
            current_timestamp,
            id
        )
        .execute(ctx.db_pool())
        .await?;

        Ok(())
    }

    async fn count_all(&self, ctx: RequestContext) -> Result<u64> {
        // 语法糖：调用通用 count
        self.count(ctx, OrganizationQuery::default()).await
    }

    async fn count(&self, ctx: RequestContext, query: OrganizationQuery) -> Result<u64> {
        let pool = ctx.db_pool();
        let mut builder = sqlx::QueryBuilder::new(
            r#"SELECT COUNT(*) as count FROM organizations WHERE status != 0"#,
        );

        if let Some(scope) = query.scope {
            builder.push(" AND scope = ").push_bind(scope as i32);
        }

        let row = builder.build_query_scalar::<i64>().fetch_one(pool).await?;

        Ok(row as u64)
    }
}

/// 从 DB 读取 organizations.config 列并解析为 `OrganizationConfig`
///
/// - 组织不存在或 config 列为空/非法 JSON → 回退默认（enable_message_vector = false）
/// - 该查询使用裸 `sqlx::query`（非宏），不依赖 `.sqlx` 离线缓存
async fn read_org_config_from_db(pool: &SqlitePool, org_id: &str) -> Result<OrganizationConfig> {
    let row = sqlx::query("SELECT config FROM organizations WHERE id = ? AND status != 0")
        .bind(org_id)
        .fetch_optional(pool)
        .await?;

    let config = match row {
        Some(row) => {
            let raw: Option<String> = row.try_get("config").ok().flatten();
            match raw {
                Some(s) if !s.trim().is_empty() => serde_json::from_str(&s).unwrap_or_default(),
                _ => OrganizationConfig::default(),
            }
        }
        None => OrganizationConfig::default(),
    };
    Ok(config)
}
