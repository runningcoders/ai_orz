//! 负责 Agent 的可用性校验（handler 层共享）。
//!
//! 为什么在 handler 而不是 domain：Agent 属于 hr 域，`system` 域不得引用另一个域，
//! 跨域编排必须由上层完成（ARCHITECTURE 原则 1）。等第三个调用方出现时再考虑
//! 抽成独立服务，现在两处调用共用这个小函数即可。
//!
//! ⚠️ 只能做「存在 + 在役」校验：`agents` 表没有 `organization_id`，全库也没有
//! Agent↔组织关联表，同组织租户门做不了（见 `service/domain/runtime/cancel.rs`）。

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain as hr_domain;
use common::enums::AgentStatus;
use common::error::{Result, bail_err};

/// 校验 Agent 存在且处于 `Onboarded`（可接活）状态。
pub(super) async fn ensure_onboarded(ctx: &RequestContext, agent_id: &str) -> Result<()> {
    let agent = hr_domain()
        .agent_manage()
        .get_agent(ctx.clone(), agent_id, Default::default())
        .await?
        .ok_or_else(|| common::error::err!(NotFound, "负责 Agent {} 不存在", agent_id))?;
    if agent.po.status != AgentStatus::Onboarded {
        bail_err!(
            InvalidRequest,
            "负责 Agent {} 当前不可用（状态 {:?}，需 Onboarded）",
            agent_id,
            agent.po.status
        );
    }
    Ok(())
}
