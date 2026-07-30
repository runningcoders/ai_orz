//! POST /api/v1/system/seed/save - 导出当前组织配置到文件
//!
//! 异步后台任务：提交后返回 task_id，前端轮询进度接口获取执行状态。
//! Handler 层职责：权限校验 → 创建任务 → 注册到后台任务注册中心。
//! 任务体负责编排各 domain 拉取实体组装 SeedSnapshot 并写入文件。

use crate::pkg::RequestContext;
use crate::pkg::background_task::{BackgroundTask, registry};
use ai_orz_macros::generate_http_handler;
use async_trait::async_trait;
use common::api::seed::{SaveSeedRequest, SaveSeedResponse};
use common::api::{TaskIdResponse, TaskProgressSnapshot, TaskStatus, TaskType};
use common::error::{Error, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Seed 导出后台任务（自包含进度状态）
///
/// 持有 ctx/params + 进度状态字段，`run` 方法内部更新进度，
/// 外部通过 `progress()` 读取快照。注册到通用后台任务注册中心后由 registry spawn 执行。
pub struct SeedSaveTask {
    task_id: String,
    ctx: RequestContext,
    params: SaveSeedRequest,
    // 进度状态字段（任务体内部更新，外部通过 progress() 读取）
    status: Mutex<TaskStatus>,
    current_step: AtomicUsize,
    total_steps: usize,
    step_message: Mutex<String>,
    started_at: i64,
    finished_at: Mutex<Option<i64>>,
    error: Mutex<Option<String>>,
    result: Mutex<Option<serde_json::Value>>,
}

impl SeedSaveTask {
    /// 导出阶段：组织(1) + 用户(2) + Provider(3) + Agent(4) + Skill(5) + 写文件(6)
    const TOTAL_STEPS: usize = 6;

    /// 创建新的导出任务对象（状态为 Pending，等待 registry spawn 后执行）
    pub fn new(ctx: RequestContext, params: SaveSeedRequest) -> Self {
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            ctx,
            params,
            status: Mutex::new(TaskStatus::Pending),
            current_step: AtomicUsize::new(0),
            total_steps: Self::TOTAL_STEPS,
            step_message: Mutex::new("等待开始".to_string()),
            started_at: chrono::Utc::now().timestamp_millis(),
            finished_at: Mutex::new(None),
            error: Mutex::new(None),
            result: Mutex::new(None),
        }
    }

    /// 更新当前步骤进度（任务体内部调用）
    fn set_step(&self, step: usize, message: &str) {
        self.current_step.store(step, Ordering::SeqCst);
        *self.step_message.lock().unwrap() = message.to_string();
    }

    /// 标记完成并写入结果
    fn set_completed(&self, result: serde_json::Value) {
        *self.status.lock().unwrap() = TaskStatus::Completed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "导出完成".to_string();
        *self.result.lock().unwrap() = Some(result);
    }

    /// 标记失败
    fn set_failed(&self, error: String) {
        *self.status.lock().unwrap() = TaskStatus::Failed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "导出失败".to_string();
        *self.error.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl BackgroundTask for SeedSaveTask {
    fn task_id(&self) -> &str {
        &self.task_id
    }

    fn task_type(&self) -> TaskType {
        TaskType::SeedSave
    }

    fn progress(&self) -> TaskProgressSnapshot {
        TaskProgressSnapshot {
            task_id: self.task_id.clone(),
            task_type: self.task_type().as_str().to_string(),
            status: *self.status.lock().unwrap(),
            current_step: self.current_step.load(Ordering::SeqCst),
            total_steps: self.total_steps,
            step_message: self.step_message.lock().unwrap().clone(),
            started_at: self.started_at,
            finished_at: *self.finished_at.lock().unwrap(),
            error: self.error.lock().unwrap().clone(),
            result: self.result.lock().unwrap().clone(),
        }
    }

    async fn run(&self) -> Result<serde_json::Value> {
        *self.status.lock().unwrap() = TaskStatus::Running;

        match self.run_steps().await {
            Ok(resp) => {
                let v = serde_json::to_value(&resp)
                    .map_err(|e| Error::internal(format!("序列化结果失败: {}", e)))?;
                self.set_completed(v.clone());
                Ok(v)
            }
            Err(e) => {
                self.set_failed(e.to_string());
                Err(e)
            }
        }
    }
}

impl SeedSaveTask {
    /// 执行导出步骤（从原 save_seed handler 迁移逻辑）
    ///
    /// Step 1-5: 组装 SeedSnapshot（进度由 assemble_snapshot_from_db_with_progress 内部更新）
    /// Step 6: 写入文件
    async fn run_steps(&self) -> Result<SaveSeedResponse> {
        let ctx = self.ctx.clone();
        let params = self.params.clone();

        let org_id = ctx
            .organization_id()
            .ok_or_else(|| Error::bad_request("缺少 organization_id".to_string()))?
            .clone();

        // Step 1-5: 组装快照（进度回调更新 current_step）
        let snapshot = super::assemble_snapshot_from_db_with_progress(
            ctx.clone(),
            &org_id,
            params.description.clone(),
            &|step, msg| self.set_step(step, msg),
        )
        .await?;

        // Step 6: 写入文件
        self.set_step(Self::TOTAL_STEPS, "正在写入文件");
        let content = serde_json::to_string_pretty(&snapshot)?;
        let dir = crate::service::domain::system::seed::store::seeds_dir();
        let size =
            crate::service::domain::system::seed::store::write_file(&dir, &params.name, &content)
                .await?;

        Ok(SaveSeedResponse {
            name: params.name,
            size,
        })
    }
}

/// 导出当前组织配置到文件（异步提交，返回 task_id）
///
/// 创建自包含任务对象，注册到通用后台任务注册中心，由 registry spawn 执行。
#[generate_http_handler]
pub async fn save_seed(ctx: RequestContext, params: SaveSeedRequest) -> Result<TaskIdResponse> {
    super::check_super_admin(&ctx)?;
    let task = Arc::new(SeedSaveTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
