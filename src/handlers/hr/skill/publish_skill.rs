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
//! ## 为什么挂 `admin` 工具包（而不是 `skill_management`）
//!
//! 发布是治理动作，不该让每个装了技能管理包的 Agent 都能做。工具面授权是
//! `neural ∪ (tool.tags ∩ agent.installed_tags)`（见 `runtime::tool_execution`），
//! 所以把它挂在 `admin` 包上等价于：**只有显式装了 admin 工具包的 Agent 才看得到
//! 这个工具**——这些 Agent 就是「技能发布收口人」，由 `install_tool_pack` 逐个指定，
//! 不需要再写一套白名单。
//!
//! 刻意**不**同时挂 `skill_management`：那样任何技能管理 Agent 都会被顺带授权发布，
//! 与「只给部分 Agent」相悖。
//!
//! ## 收口人 vs 普通 Agent
//!
//! - 普通 Agent 产出 Draft 后**不需要**自己有发布权，由**收口人 Agent** 或用户管理员
//!   审核后发布（发布后其他 Agent 才能安装使用）；
//! - 收口人可发布**任何由 Agent 创作**的技能（含自己的），但不能发布用户创作的技能
//!   ——「发布别人的技能」是用户管理员及以上的权限，不下放给 Agent；
//! - 用户侧口径不变：SuperAdmin / Admin 可发布任意技能，普通用户只能发布自己的。

use crate::handlers::hr::skill::response::to_detail;
use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::{PublishSkillRequest, PublishSkillResponse};
use common::error::Result;

#[register_handler_tool(
    id = "publish_skill",
    name = "Publish Skill",
    description = "Publish a Draft skill (Draft -> Published) so it enters the shared library and other agents can install and use it. This is the review-and-release step for agent-authored drafts: call it after reviewing the skill's content. Fails with Conflict if the skill is Expired (call restore_skill first) and with InvalidRequest if it is an agent-installed copy. Idempotent: publishing an already Published skill returns it unchanged. Only available to agents that have the admin tool pack installed, and such agents may only publish skills authored by agents (never user-authored skills). Returns the updated skill detail.",
    params = "common::api::PublishSkillRequest",
    tags = "admin"
)]
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

#[cfg(test)]
mod tests {
    /// 护栏：`publish_skill` 只挂 `admin` 工具包。
    ///
    /// 「只给部分 Agent」完全由这条 tag 表达：工具面授权 = `neural ∪ (tool.tags ∩
    /// agent.installed_tags)`，所以装了 admin 包的 Agent 才有它。若有人顺手加上
    /// `skill_management`（看上去更「对口」），发布权会扩散到所有技能管理 Agent，
    /// 收口就废了——这类改动在 code review 里很容易被当成合理，故用测试钉住。
    #[test]
    fn publish_skill_tool_is_gated_by_admin_pack_only() {
        let registry = crate::pkg::tool_registry::get_registry();
        let factory = registry
            .get_builtin_factory("publish_skill")
            .expect("publish_skill 必须注册为内置工具");
        let po = factory.create_po();
        assert_eq!(
            po.get_tags(),
            vec!["admin".to_string()],
            "publish_skill 只允许挂 admin 包（多挂 skill_management 会扩散发布权）"
        );
    }
}
