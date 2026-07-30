//! 向量索引重建任务对象
//!
//! 自包含的 `BackgroundTask` 实现：持有 ctx + 进度状态字段，
//! `run` 方法内部更新进度，外部通过 `progress()` 读取快照。
//! 由 `switch_embedding_provider` handler 注册到全局 registry。

use crate::pkg::RequestContext;
use crate::pkg::background_task::BackgroundTask;
use crate::service::dal;
use async_trait::async_trait;
use common::api::{TaskProgressSnapshot, TaskStatus, TaskType};
use common::error::{Error, Result};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 向量索引重建任务
///
/// 遍历 7 个实体（agent/memory/skill/task/project/message/tool）调用各 DAL 的
/// `rebuild_vectors(ctx)`。同一时刻仅允许一个 RebuildVectors 任务运行（由 `run` 内部检查）。
pub struct RebuildVectorsTask {
    /// 任务唯一 ID
    task_id: String,
    /// 请求上下文（包含用户/组织信息，供 DAL 日志与权限校验使用）
    ctx: RequestContext,
    /// 任务状态（Pending → Running → Completed/Failed）
    status: Mutex<TaskStatus>,
    /// 当前步骤编号（1-based，0 表示尚未开始）
    current_step: AtomicUsize,
    /// 总步骤数（7 个实体）
    total_steps: usize,
    /// 当前步骤描述（人类可读）
    step_message: Mutex<String>,
    /// 开始时间戳（毫秒）
    started_at: i64,
    /// 结束时间戳（毫秒，运行中为 None）
    finished_at: Mutex<Option<i64>>,
    /// 失败时的错误信息
    error: Mutex<Option<String>>,
    /// 任务结果（完成时写入 JSON 结果）
    result: Mutex<Option<serde_json::Value>>,
}

impl RebuildVectorsTask {
    /// 创建新的重建任务实例（状态为 Pending，尚未注册到 registry）
    pub fn new(ctx: RequestContext) -> Self {
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            ctx,
            status: Mutex::new(TaskStatus::Pending),
            current_step: AtomicUsize::new(0),
            total_steps: 7,
            step_message: Mutex::new("等待开始".to_string()),
            started_at: chrono::Utc::now().timestamp_millis(),
            finished_at: Mutex::new(None),
            error: Mutex::new(None),
            result: Mutex::new(None),
        }
    }

    /// 更新当前步骤（任务体内部调用）
    fn set_step(&self, step: usize, message: &str) {
        self.current_step.store(step, Ordering::SeqCst);
        *self.step_message.lock().unwrap() = message.to_string();
    }

    /// 标记完成并写入结果
    fn set_completed(&self, result: serde_json::Value) {
        *self.status.lock().unwrap() = TaskStatus::Completed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "重建完成".to_string();
        *self.result.lock().unwrap() = Some(result);
    }

    /// 标记失败
    fn set_failed(&self, error: String) {
        *self.status.lock().unwrap() = TaskStatus::Failed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "重建失败".to_string();
        *self.error.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl BackgroundTask for RebuildVectorsTask {
    fn task_id(&self) -> &str {
        &self.task_id
    }

    fn task_type(&self) -> TaskType {
        TaskType::RebuildVectors
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
        // 并发检查：向量重建独占，不允许同时运行多个
        let existing = crate::pkg::background_task::registry()
            .list_progress_by_type(TaskType::RebuildVectors)
            .await;
        for p in existing {
            if p.status == TaskStatus::Running && p.task_id != self.task_id {
                return Err(Error::conflict(format!(
                    "向量重建任务正在执行中（task_id={}），请等待完成",
                    p.task_id
                )));
            }
        }

        *self.status.lock().unwrap() = TaskStatus::Running;

        log_info!(
            &self.ctx,
            "rebuild_vectors",
            "开始异步重建所有向量索引 task_id={}",
            self.task_id
        );

        let entities: [(&str, &str); 7] = [
            ("agent", "Agent"),
            ("memory", "Memory"),
            ("skill", "Skill"),
            ("task", "Task"),
            ("project", "Project"),
            ("message", "Message"),
            ("tool", "Tool"),
        ];

        let result: Result<()> = async {
            for (i, (entity, label)) in entities.iter().enumerate() {
                self.set_step(i + 1, &format!("正在重建 {} 向量索引", label));
                match *entity {
                    "agent" => dal::agent::dal().rebuild_vectors(self.ctx.clone()).await?,
                    "memory" => dal::memory::dal().rebuild_vectors(self.ctx.clone()).await?,
                    "skill" => dal::skill::dal().rebuild_vectors(self.ctx.clone()).await?,
                    "task" => dal::task::dal().rebuild_vectors(self.ctx.clone()).await?,
                    "project" => {
                        dal::project::dal()
                            .rebuild_vectors(self.ctx.clone())
                            .await?
                    }
                    "message" => {
                        dal::message::dal()
                            .rebuild_vectors(self.ctx.clone())
                            .await?
                    }
                    "tool" => dal::tool::dal().rebuild_vectors(self.ctx.clone()).await?,
                    _ => {}
                }
            }
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                let v = serde_json::json!({"rebuilt": true, "entities": 7});
                self.set_completed(v.clone());
                log_info!(
                    &self.ctx,
                    "rebuild_vectors",
                    "异步重建所有向量索引完成 task_id={}",
                    self.task_id
                );
                Ok(v)
            }
            Err(e) => {
                let msg = e.to_string();
                log_warn!(
                    &self.ctx,
                    "rebuild_vectors",
                    error = ?e,
                    "向量重建失败 task_id={}",
                    self.task_id
                );
                self.set_failed(msg.clone());
                Err(e)
            }
        }
    }
}
