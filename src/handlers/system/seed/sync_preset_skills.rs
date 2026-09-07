//! 预置技能（seed）同步到技能库
//!
//! 解决的问题：seed 内置的预置技能只在系统初始化时导入一次，后续 seed 升级
//! （新增技能 / 修订 skill.md）不会流入已运行的实例。这里提供两个接口把 seed 内容
//! 重新灌入共享库，并让用户在「覆盖重置」与「仅补缺」之间选择，可选把已安装到
//! Agent 的技能副本一并更新。
//!
//! - `GET  /api/v1/system/seed/preset-skills/preview` — 对比 seed 与技能库，返回影响清单
//! - `POST /api/v1/system/seed/preset-skills/sync` — 提交同步后台任务，返回 task_id
//!
//! 匹配键是**技能 ID**（`TEMPLATE_*` / `GIT_BRANCH_WORKFLOW`），不是名称。
//! 路由挂在 `/system` 段下，已由 `require_role_middleware(Admin)` 统一鉴权。
//!
//! 同步走通用后台任务（`pkg::background_task`）：技能多 + 涉及子 Agent 副本时
//! 耗时会变长，后台静默执行 + 前端轮询进度，避免 HTTP 请求超时。

use crate::pkg::RequestContext;
use crate::pkg::background_task::{BackgroundTask, registry};
use crate::service::domain::hr;
use crate::service::domain::system::seed::default;
use ai_orz_macros::generate_http_handler;
use async_trait::async_trait;
use common::api::seed::{
    PresetSkillSyncItem, PresetSkillSyncStrategy, PreviewPresetSkillsRequest,
    PreviewPresetSkillsResponse, SyncPresetSkillsRequest, SyncPresetSkillsResponse,
};
use common::api::{TaskIdResponse, TaskProgressSnapshot, TaskStatus, TaskType};
use common::error::{Error, Result};
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

/// 预置技能同步预览
///
/// 逐个比对 seed 技能与技能库中的同 ID 技能，返回「将新增 / 将覆盖」清单，
/// 并统计每个技能已安装到 Agent 的副本数量，供前端弹窗在用户确认前展示影响面。
/// 只读，不写任何数据。
#[generate_http_handler]
pub async fn preview_preset_skills(
    ctx: RequestContext,
    _params: PreviewPresetSkillsRequest,
) -> Result<PreviewPresetSkillsResponse> {
    let snapshot = default::embedded_default_snapshot();

    let mut items = Vec::with_capacity(snapshot.skills.len());
    for skill_def in &snapshot.skills {
        let local = hr::domain()
            .skill_manage()
            .get_skill(ctx.clone(), &skill_def.id)
            .await?;

        // 已安装副本数量（parent_skill_id 指向该预置技能的 Agent 私有副本）
        let installed_copies = hr::domain()
            .skill_manage()
            .query_skills(
                ctx.clone(),
                crate::service::dao::skill::SkillQuery {
                    parent_skill_id: Some(skill_def.id.clone()),
                    ..Default::default()
                },
            )
            .await?
            .total;

        items.push(PresetSkillSyncItem {
            id: skill_def.id.clone(),
            name: skill_def.name.clone(),
            description: skill_def.description.clone(),
            exists: local.is_some(),
            local_name: local.as_ref().map(|s| s.po.name.clone()),
            local_status: local.as_ref().map(|s| s.po.status),
            installed_copies,
        });
    }

    let existing_count = items.iter().filter(|i| i.exists).count();
    Ok(PreviewPresetSkillsResponse {
        missing_count: items.len() - existing_count,
        existing_count,
        total_installed_copies: items.iter().map(|i| i.installed_copies).sum(),
        items,
    })
}

/// 同步预置技能后台任务（自包含进度状态）
///
/// 持有 ctx/params + 进度状态字段，`run` 方法内部逐技能更新进度，
/// 外部通过 `progress()` 读取快照。注册到通用后台任务注册中心后由 registry spawn 执行。
pub struct SyncPresetSkillsTask {
    task_id: String,
    ctx: RequestContext,
    params: SyncPresetSkillsRequest,
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

impl SyncPresetSkillsTask {
    /// 创建新的同步任务对象（状态为 Pending，等待 registry spawn 后执行）
    pub fn new(ctx: RequestContext, params: SyncPresetSkillsRequest) -> Self {
        // 步骤 = 每个预置技能一步 +（可选）已安装副本同步一步
        let total_steps = default::embedded_default_snapshot().skills.len()
            + usize::from(params.sync_installed_copies);
        Self {
            task_id: uuid::Uuid::new_v4().to_string(),
            ctx,
            params,
            status: Mutex::new(TaskStatus::Pending),
            current_step: AtomicUsize::new(0),
            total_steps,
            step_message: Mutex::new("等待开始".to_string()),
            started_at: chrono::Utc::now().timestamp_millis(),
            finished_at: Mutex::new(None),
            error: Mutex::new(None),
            result: Mutex::new(None),
        }
    }

    /// 更新当前步骤进度（任务体内部调用）
    fn set_step(&self, step: usize, message: String) {
        self.current_step
            .store(step, std::sync::atomic::Ordering::SeqCst);
        *self.step_message.lock().unwrap() = message;
    }

    /// 标记完成并写入结果
    fn set_completed(&self, result: SyncPresetSkillsResponse) {
        *self.status.lock().unwrap() = TaskStatus::Completed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "同步完成".to_string();
        *self.result.lock().unwrap() = Some(serde_json::to_value(&result).unwrap_or_default());
    }

    /// 标记失败
    fn set_failed(&self, error: String) {
        *self.status.lock().unwrap() = TaskStatus::Failed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "同步失败".to_string();
        *self.error.lock().unwrap() = Some(error);
    }

    /// 执行同步步骤
    ///
    /// 逐技能导入（进度按技能粒度上报）→（可选）逐技能同步已安装副本。
    /// 新建技能的 author 记到当前操作者名下；已存在技能继承原作者。
    async fn run_steps(&self) -> Result<SyncPresetSkillsResponse> {
        let snapshot = default::embedded_default_snapshot();
        let operator_id = self.ctx.uid();
        let skip_existing = matches!(self.params.strategy, PresetSkillSyncStrategy::OnlyMissing);

        let mut created = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;

        for (idx, skill_def) in snapshot.skills.iter().enumerate() {
            self.set_step(idx + 1, format!("正在同步技能：{}", skill_def.name));
            match super::apply_single_preset_skill(
                self.ctx.clone(),
                skill_def,
                Some(operator_id.as_str()),
                skip_existing,
            )
            .await?
            {
                super::PresetSkillAction::Created => created += 1,
                super::PresetSkillAction::Updated => updated += 1,
                super::PresetSkillAction::Skipped => skipped += 1,
            }
        }

        // 可选：把已安装到 Agent 的副本一并对齐到源技能最新版本
        let mut updated_copies = 0usize;
        if self.params.sync_installed_copies {
            let copy_step = snapshot.skills.len() + 1;
            for skill_def in &snapshot.skills {
                self.set_step(copy_step, format!("正在同步已安装副本：{}", skill_def.name));
                updated_copies += hr::domain()
                    .skill_manage()
                    .sync_installed_copies(self.ctx.clone(), &skill_def.id)
                    .await?;
            }
        }

        Ok(SyncPresetSkillsResponse {
            created,
            updated,
            skipped,
            total: snapshot.skills.len(),
            updated_copies,
        })
    }
}

#[async_trait]
impl BackgroundTask for SyncPresetSkillsTask {
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
            current_step: self.current_step.load(std::sync::atomic::Ordering::SeqCst),
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
                self.set_completed(resp);
                log_info!(
                    &self.ctx,
                    "sync_preset_skills",
                    strategy = ?self.params.strategy,
                    sync_installed_copies = self.params.sync_installed_copies,
                    "预置技能同步完成"
                );
                Ok(v)
            }
            Err(e) => {
                self.set_failed(e.to_string());
                Err(e)
            }
        }
    }
}

/// 同步预置技能到技能库（异步提交，返回 task_id）
///
/// - `Overwrite`：已存在的同 ID 技能走 update，覆写元数据 + seed 声明的文件
///   （技能目录下用户自建的额外文件不会被删除）
/// - `OnlyMissing`：跳过已存在技能，只创建缺失的
/// - `sync_installed_copies`：可选，把 parent_skill_id 指向预置技能的 Agent
///   已安装副本一并对齐到源技能最新版本（副本状态保持 Agent 私有 Draft 不变）
///
/// 前端通过 `GET /api/v1/system/tasks/{task_id}/progress` 轮询进度与结果。
#[generate_http_handler]
pub async fn sync_preset_skills(
    ctx: RequestContext,
    params: SyncPresetSkillsRequest,
) -> Result<TaskIdResponse> {
    let task = Arc::new(SyncPresetSkillsTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
