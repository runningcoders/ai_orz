//! Handler: POST /api/v1/skills/{skill_id}/publish - 发布技能（Draft → Published）
//!
//! ## 为什么单独一个接口，而不是继续复用 `update_skill`
//!
//! 发布不是「改一个字段」，而是一次带副作用的治理动作：Agent 自有根技能发布后，
//! 要为作者 Agent 回写一条本地工作副本。把它塞在通用 update 里有两个问题：
//! - 语义不显：前端只有一个「编辑」入口，`status=Published` 对用户不可见、不可解释，
//!   于是界面长期没有发布按钮（本次要补的那个）；
//! - 副作用误触：改描述、删附件也走同一个 handler，发布逻辑只能靠「状态是否跃迁」
//!   猜，一旦有人调整 update 的字段语义就容易被带进发布分支。
//!
//! 现在职责切开：`update_skill` 只改元数据/内容，并把「非 Published → Published」
//! 跃迁**明确拒绝**；发布只有本接口一个入口。
//!
//! ## 为什么不注册成 Agent 工具
//!
//! 发布属共享库治理，Agent 的自进化只发生在自己的 Draft 副本里（M2 修复 C 的口径）。
//! 这里刻意**不加** `register_handler_tool`：该能力不应出现在任何 Agent 的工具表里。
//! 领域层 `publish_skill` 另有「Agent 上下文直接拒绝」的硬校验兜底，不依赖调用方自律。

use crate::handlers::hr::skill::response::to_detail;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::generate_http_handler;
use common::api::{PublishSkillRequest, PublishSkillResponse};
use common::error::Result;

#[generate_http_handler]
pub async fn publish_skill(
    ctx: RequestContext,
    params: PublishSkillRequest,
) -> Result<PublishSkillResponse> {
    let skill = domain()
        .skill_manage()
        .publish_skill(ctx, &params.skill_id)
        .await?;
    Ok(to_detail(&skill))
}
