//! 向量索引重建任务对象
//!
//! 自包含的 `BackgroundTask` 实现：持有 ctx + 进度状态字段，
//! `run` 方法内部更新进度，外部通过 `progress()` 读取快照。
//! 由 `switch_embedding_provider` handler 注册到全局 registry。

use crate::pkg::RequestContext;
use crate::pkg::background_task::BackgroundTask;
use crate::pkg::background_task::TaskProgressCounter;
use crate::service::dal;
use crate::service::dal::agent::AgentDal;
use crate::service::dal::attachment::AttachmentDal;
use crate::service::dal::memory::MemoryDal;
use crate::service::dal::message::MessageDal;
use crate::service::dal::project::ProjectDal;
use crate::service::dal::skill::SkillDal;
use crate::service::dal::task::TaskDal;
use crate::service::dal::tool::ToolDal;
use async_trait::async_trait;
use common::api::{TaskProgressSnapshot, TaskStatus, TaskType};
use common::enums::VectorRebuildMode;
use common::error::{Error, Result};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 可重建的实体清单：(key, 展示名)。顺序即执行顺序。
pub const ALL_ENTITIES: [(&str, &str); 8] = [
    ("agent", "Agent"),
    ("memory", "Memory"),
    ("skill", "Skill"),
    ("task", "Task"),
    ("project", "Project"),
    ("message", "Message"),
    ("tool", "Tool"),
    ("attachment", "Attachment"),
];

/// 把请求里的实体名解析成执行清单。
///
/// `None` 或空 → 全部 8 类（向后兼容：老前端不传该字段）。
/// 未知名字**报错**而不是忽略：拼错一个字母就「任务成功但一条都没重建」，
/// 是最难排查的那类静默失败。
pub fn resolve_entities(names: Option<&[String]>) -> Result<Vec<(&'static str, &'static str)>> {
    let Some(names) = names.filter(|v| !v.is_empty()) else {
        return Ok(ALL_ENTITIES.to_vec());
    };

    let mut picked = Vec::with_capacity(names.len());
    for name in names {
        let key = name.trim().to_ascii_lowercase();
        let Some(found) = ALL_ENTITIES.iter().find(|(k, _)| *k == key) else {
            let legal: Vec<&str> = ALL_ENTITIES.iter().map(|(k, _)| *k).collect();
            return Err(Error::bad_request(format!(
                "未知的向量集合: {}（合法值：{}）",
                name,
                legal.join(" / ")
            )));
        };
        // 去重：重复传同一实体只会白跑一遍（每次都要 clear + 全量重算）
        if !picked.iter().any(|(k, _)| *k == found.0) {
            picked.push(*found);
        }
    }
    Ok(picked)
}

/// 单个实体的向量重建动作。
///
/// 任务只依赖这个窄接口，不直接取 8 个 DAL 全局单例：生产在边界处由
/// [`default_targets`] 组装，测试可注入 fake。
#[async_trait]
pub trait VectorRebuildTarget: Send + Sync {
    async fn rebuild(
        &self,
        ctx: RequestContext,
        progress: &TaskProgressCounter,
        mode: VectorRebuildMode,
    ) -> Result<()>;
}

/// 实体 key → 重建动作
pub type RebuildTargets = BTreeMap<&'static str, Arc<dyn VectorRebuildTarget>>;

macro_rules! dal_target {
    ($target:ident, $dal:ty) => {
        struct $target(Arc<$dal>);

        #[async_trait]
        impl VectorRebuildTarget for $target {
            async fn rebuild(
                &self,
                ctx: RequestContext,
                progress: &TaskProgressCounter,
                mode: VectorRebuildMode,
            ) -> Result<()> {
                self.0.rebuild_vectors(ctx, progress, mode).await
            }
        }
    };
}

dal_target!(AgentTarget, dyn AgentDal);
dal_target!(MemoryTarget, dyn MemoryDal);
dal_target!(SkillTarget, dyn SkillDal);
dal_target!(TaskTarget, dyn TaskDal);
dal_target!(ProjectTarget, dyn ProjectDal);
dal_target!(MessageTarget, dyn MessageDal);
dal_target!(ToolTarget, dyn ToolDal);
dal_target!(AttachmentTarget, dyn AttachmentDal);

/// 生产环境的目标表：组合根在此把 DAL 单例适配成窄接口（唯一取全局的位置）。
pub fn default_targets() -> RebuildTargets {
    let mut targets: RebuildTargets = BTreeMap::new();
    targets.insert("agent", Arc::new(AgentTarget(dal::agent::dal())));
    targets.insert("memory", Arc::new(MemoryTarget(dal::memory::dal())));
    targets.insert("skill", Arc::new(SkillTarget(dal::skill::dal())));
    targets.insert("task", Arc::new(TaskTarget(dal::task::dal())));
    targets.insert("project", Arc::new(ProjectTarget(dal::project::dal())));
    targets.insert("message", Arc::new(MessageTarget(dal::message::dal())));
    targets.insert("tool", Arc::new(ToolTarget(dal::tool::dal())));
    targets.insert(
        "attachment",
        Arc::new(AttachmentTarget(dal::attachment::dal())),
    );
    targets
}

/// 向量索引重建任务
///
/// 遍历指定实体调用注入的 `rebuild` 动作。
/// 同一时刻仅允许一个 RebuildVectors 任务运行（由 `run` 内部检查）。
pub struct RebuildVectorsTask {
    /// 任务唯一 ID
    task_id: String,
    /// 请求上下文（包含用户/组织信息，供 DAL 日志与权限校验使用）
    ctx: RequestContext,
    /// 本次要重建的实体（缺省为全部 8 类）
    entities: Vec<(&'static str, &'static str)>,
    /// 实体 → 重建动作（构造时注入）
    targets: RebuildTargets,
    /// 重建模式（`auto` 幂等 / `missing` 查漏补缺 / `force` 强制全量）
    mode: VectorRebuildMode,
    /// 任务状态（Pending → Running → Completed/Failed）
    status: Mutex<TaskStatus>,
    /// 当前步骤编号（1-based，0 表示尚未开始）
    current_step: AtomicUsize,
    /// 总步骤数（等于实体数，只重建技能时为 1）
    total_steps: usize,
    /// 当前步骤描述（人类可读）
    step_message: Mutex<String>,
    /// 已处理条数（跨实体累计，由各 DAL 逐页上报）
    progress: TaskProgressCounter,
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
    /// 创建全量重建任务（非强制，尊重幂等短路）。
    ///
    /// 这是「首次配置 embedding provider 后自动补齐存量」的入口：那时确实不该
    /// 把已经新鲜的索引再算一遍。
    pub fn new(ctx: RequestContext) -> Self {
        Self::with_scope(ctx, ALL_ENTITIES.to_vec(), VectorRebuildMode::Auto)
    }

    /// 创建限定范围的重建任务实例（状态为 Pending，尚未注册到 registry）
    ///
    /// `mode` 决定是否忽略各 DAL 的幂等短路、是否逐条比对补缺，语义见
    /// [`VectorRebuildMode`]。
    pub fn with_scope(
        ctx: RequestContext,
        entities: Vec<(&'static str, &'static str)>,
        mode: VectorRebuildMode,
    ) -> Self {
        Self::with_targets(ctx, entities, mode, default_targets())
    }

    /// 注入目标表的构造入口（测试/自定义组合根使用）。
    pub fn with_targets(
        ctx: RequestContext,
        entities: Vec<(&'static str, &'static str)>,
        mode: VectorRebuildMode,
        targets: RebuildTargets,
    ) -> Self {
        Self {
            task_id: common::id::TimeId::generate().to_string(),
            ctx,
            total_steps: entities.len(),
            entities,
            targets,
            mode,
            status: Mutex::new(TaskStatus::Pending),
            current_step: AtomicUsize::new(0),
            step_message: Mutex::new("等待开始".to_string()),
            progress: TaskProgressCounter::new(),
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

    /// 按实体顺序执行重建（不碰全局 registry，便于单测）。
    ///
    /// 目标表里没有的实体沿用原语义：跳过（`resolve_entities` 已保证不会出现未知 key）。
    async fn run_rebuilds(&self) -> Result<()> {
        for (i, (entity, label)) in self.entities.iter().enumerate() {
            self.set_step(
                i + 1,
                &format!(
                    "({}/{}) 正在重建 {} 向量索引",
                    i + 1,
                    self.total_steps,
                    label
                ),
            );
            let Some(target) = self.targets.get(*entity).cloned() else {
                continue;
            };
            target
                .rebuild(self.ctx.clone(), &self.progress, self.mode)
                .await?;
        }
        Ok(())
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
        // 实时已处理条数拼进文案：进度条表达「第几步/共几步」，计数表达「共处理多少条」
        let mut step_message = self.step_message.lock().unwrap().clone();
        let processed = self.progress.processed();
        if processed > 0 {
            step_message.push_str(&format!("（已处理 {} 条）", processed));
        }
        TaskProgressSnapshot {
            task_id: self.task_id.clone(),
            task_type: self.task_type().as_str().to_string(),
            status: *self.status.lock().unwrap(),
            current_step: self.current_step.load(Ordering::SeqCst),
            total_steps: self.total_steps,
            step_message,
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
            "开始异步重建向量索引 task_id={} mode={} entities={}",
            self.task_id,
            self.mode.as_str(),
            self.entities.len()
        );

        let result = self.run_rebuilds().await;

        match result {
            Ok(()) => {
                let v = serde_json::json!({
                    "rebuilt": true,
                    "entities": self.entities.len(),
                    "processed": self.progress.processed(),
                    "mode": self.mode.as_str(),
                });
                self.set_completed(v.clone());
                log_info!(
                    &self.ctx,
                    "rebuild_vectors",
                    "异步重建向量索引完成 task_id={} entities={} processed={}",
                    self.task_id,
                    self.entities.len(),
                    self.progress.processed()
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

#[cfg(test)]
#[path = "rebuild_vectors_task_tests.rs"]
mod tests;
