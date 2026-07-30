//! POST /api/v1/system/seed/apply-default - 应用默认模板
//!
//! 异步后台任务：提交后返回 task_id，前端轮询进度接口获取执行状态。
//! Handler 层职责：权限校验 → 创建任务 → 注册到后台任务注册中心。
//! 任务体负责加载内置默认模板 → 调用各 domain upsert。

use crate::pkg::RequestContext;
use crate::pkg::background_task::{BackgroundTask, registry};
use ai_orz_macros::generate_http_handler;
use async_trait::async_trait;
use common::api::seed::{ApplyDefaultSeedRequest, LoadSeedResponse};
use common::api::{TaskIdResponse, TaskProgressSnapshot, TaskStatus, TaskType};
use common::error::{Error, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 应用默认 Seed 后台任务（自包含进度状态）
///
/// 持有 ctx/params + 进度状态字段，`run` 方法内部更新进度，
/// 外部通过 `progress()` 读取快照。注册到通用后台任务注册中心后由 registry spawn 执行。
pub struct SeedApplyDefaultTask {
    task_id: String,
    ctx: RequestContext,
    params: ApplyDefaultSeedRequest,
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

impl SeedApplyDefaultTask {
    /// 导入阶段：用户(1) + Provider(2) + Agent(3) + Skill(4)
    const TOTAL_STEPS: usize = 4;

    /// 创建新的应用默认模板任务对象（状态为 Pending，等待 registry spawn 后执行）
    pub fn new(ctx: RequestContext, params: ApplyDefaultSeedRequest) -> Self {
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
        *self.step_message.lock().unwrap() = "应用默认模板完成".to_string();
        *self.result.lock().unwrap() = Some(result);
    }

    /// 标记失败
    fn set_failed(&self, error: String) {
        *self.status.lock().unwrap() = TaskStatus::Failed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "应用默认模板失败".to_string();
        *self.error.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl BackgroundTask for SeedApplyDefaultTask {
    fn task_id(&self) -> &str {
        &self.task_id
    }

    fn task_type(&self) -> TaskType {
        TaskType::SeedApplyDefault
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

impl SeedApplyDefaultTask {
    /// 执行应用默认模板步骤（从原 apply_default handler 迁移逻辑）
    ///
    /// 加载内置默认模板 → 调用 apply_snapshot_to_db_with_progress（进度回调更新 step 1-4）
    async fn run_steps(&self) -> Result<LoadSeedResponse> {
        let ctx = self.ctx.clone();
        let params = self.params.clone();

        // 加载内置默认模板（不单独计数，apply 阶段从 step 1 开始）
        *self.step_message.lock().unwrap() = "正在加载默认模板".to_string();
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();

        // Step 1-4: 应用到 DB（进度回调更新 current_step；DryRun 在 step 1 完成）
        super::apply_snapshot_to_db_with_progress(
            ctx,
            &snapshot,
            params.strategy,
            &params.sensitive_values,
            &|step, msg| self.set_step(step, msg),
        )
        .await
    }
}

/// 应用默认模板（异步提交，返回 task_id）
///
/// 创建自包含任务对象，注册到通用后台任务注册中心，由 registry spawn 执行。
#[generate_http_handler]
pub async fn apply_default(
    ctx: RequestContext,
    params: ApplyDefaultSeedRequest,
) -> Result<TaskIdResponse> {
    super::check_super_admin(&ctx)?;
    let task = Arc::new(SeedApplyDefaultTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
