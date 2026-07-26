//! POST /api/v1/system/seed/save - 导出当前组织配置到文件
//!
//! Handler 编排：调用各 domain 拉取实体 → 组装 SeedSnapshot → 写入文件

use ai_orz_macros::generate_http_handler;
use common::api::seed::{SaveSeedRequest, SaveSeedResponse};
use common::error::{Error, Result};

use crate::pkg::RequestContext;
use crate::service::domain::system::seed::store;

#[generate_http_handler]
pub async fn save_seed(ctx: RequestContext, params: SaveSeedRequest) -> Result<SaveSeedResponse> {
    super::check_super_admin(&ctx)?;
    let org_id = ctx
        .organization_id()
        .ok_or_else(|| Error::bad_request("缺少 organization_id".to_string()))?
        .clone();

    // 编排各 domain 拉取数据
    let snapshot = super::assemble_snapshot_from_db(ctx, &org_id, params.description).await?;

    let content = serde_json::to_string_pretty(&snapshot)?;
    let dir = store::seeds_dir();
    let size = store::write_file(&dir, &params.name, &content).await?;

    Ok(SaveSeedResponse {
        name: params.name,
        size,
    })
}
