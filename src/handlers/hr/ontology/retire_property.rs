//! Handler: DELETE /api/v1/hr/ontology/properties/{id} - 退役属性定义（软删除）

use crate::pkg::RequestContext;
use crate::service::domain::hr::domain;
use ai_orz_macros::{generate_http_handler, register_handler_tool};
use common::api::ontology::{RetireOntologyPropertyRequest, RetireOntologyPropertyResponse};
use common::error::{Result, err};

/// Retire an ontology property definition (soft delete)
#[register_handler_tool(
    id = "retire_ontology_property",
    name = "Retire Ontology Property",
    description = "Retire an ontology property definition (soft delete, status=retired). Property values already stored on entities are kept and remain readable - retiring only hides the definition from the management list. Retiring an already-retired definition succeeds idempotently. Fails with NotFound if the definition does not exist.",
    params = "common::api::RetireOntologyPropertyRequest",
    tags = "ontology_management"
)]
#[generate_http_handler]
pub async fn retire_ontology_property(
    ctx: RequestContext,
    params: RetireOntologyPropertyRequest,
) -> Result<RetireOntologyPropertyResponse> {
    // 校验存在性（不存在即 404；重复退役由 DAO 幂等处理）
    domain()
        .ontology_domain()
        .get_property(ctx.clone(), &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "属性 {} 不存在", params.id))?;
    domain()
        .ontology_domain()
        .retire_property(ctx.clone(), &params.id)
        .await?;
    // 重查返回退役后的最新行（status=retired + DAO 刷新的 updated_at）
    let latest = domain()
        .ontology_domain()
        .get_property(ctx, &params.id)
        .await?
        .ok_or_else(|| err!(NotFound, "属性 {} 不存在", params.id))?;
    Ok(super::property_item(latest.into_po()))
}
