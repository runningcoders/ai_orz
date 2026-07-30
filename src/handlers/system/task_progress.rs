//! 统一后台任务进度查询
//!
//! 所有后台任务共用此接口，前端通过 task_id 查询进度。
//! 业务 handler 可在此基础上装饰为各自的响应 DTO。

use crate::pkg::RequestContext;
use crate::service::domain::system;
use ai_orz_macros::generate_http_handler;
use common::api::{GetTaskProgressRequest, TaskProgressSnapshot};
use common::error::{Error, Result};

#[generate_http_handler]
pub async fn get_task_progress(
    _ctx: RequestContext,
    params: GetTaskProgressRequest,
) -> Result<TaskProgressSnapshot> {
    match system::domain()
        .background_task_registry()
        .get_progress(&params.task_id)
        .await
    {
        Some(snapshot) => Ok(snapshot),
        None => Err(Error::not_found(format!("任务不存在: {}", params.task_id))),
    }
}
