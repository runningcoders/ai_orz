//! Handler: GET /api/v1/hr/skills/tags - 列出所有已发布技能的 distinct tags
//!
//! 用于前端技能包安装下拉框数据源

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{ListSkillTagsRequest, ListSkillTagsResponse};
use common::error::Result;

/// 列出所有已发布技能（status=Published）的不重复 tag 列表（按字母升序）
#[generate_http_handler]
pub async fn list_skill_tags(
    ctx: RequestContext,
    _params: ListSkillTagsRequest,
) -> Result<ListSkillTagsResponse> {
    let tags = domain().skill_manage().list_skill_tags(ctx).await?;
    Ok(ListSkillTagsResponse { tags })
}
