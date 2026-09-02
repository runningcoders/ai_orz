//! Handler: POST /api/v1/users/query - User 通用查询接口
//!
//! 与 list_users_by_* 的区别：list 是按组织维度的语法糖（GET），
//! query 是完整查询能力（POST + body），支持分页和更多过滤条件。

use crate::pkg::RequestContext;
use crate::service::dao::user::UserQuery;
use crate::service::domain::organization;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PagedResult, UserListItem, UserQueryRequest};
use common::error::Result;

/// User 通用查询（POST body，支持完整查询能力）
#[register_handler_tool(
    id = "query_users",
    name = "Query Users (Advanced)",
    description = "Query users of an organization with limit/offset pagination; organization_id defaults to the caller's organization when omitted. Returns a paged list of user summaries (ID, username, email, role, status). Use it for paginated browsing of large orgs; the list_users_by_* handlers give a one-shot unfiltered list.",
    params = "common::api::UserQueryRequest",
    neural
)]
#[generate_http_handler]
pub async fn query_users(
    ctx: RequestContext,
    params: UserQueryRequest,
) -> Result<PagedResult<UserListItem>> {
    let org_id = params
        .organization_id
        .clone()
        .or_else(|| ctx.organization_id.clone())
        .ok_or_else(|| common::error::Error::bad_request("未找到组织信息".to_string()))?;

    let page = organization::domain()
        .user_manage()
        .query(
            ctx,
            UserQuery {
                organization_id: Some(org_id),
                pagination: params.pagination,
            },
        )
        .await?;

    Ok(page.map(|user| UserListItem {
        user_id: user.id.clone(),
        username: user.username.clone(),
        display_name: if user.display_name.is_empty() {
            None
        } else {
            Some(user.display_name.clone())
        },
        email: if user.email.is_empty() {
            None
        } else {
            Some(user.email.clone())
        },
        role: user.user_role() as i32,
        role_name: user.user_role().display_name().to_string(),
        status: user.status.to_i32(),
        created_at: user.created_at,
    }))
}
