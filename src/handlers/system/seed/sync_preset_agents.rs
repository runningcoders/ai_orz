//! 预置 Agent（seed）同步到 Agent 列表
//!
//! 解决的问题：
//! - 用户误删了 seed 提供的默认 Agent（如前台接待），可一键恢复；
//! - seed 后续新增的通用默认 Agent，可无感同步到已运行的实例。
//!
//! 与「预置技能同步」（`sync_preset_skills`）同构：
//! - `GET  /api/v1/system/seed/preset-agents/preview` — 对比 seed 与 Agent 库，返回影响清单
//! - `POST /api/v1/system/seed/preset-agents/sync` — 提交同步后台任务，返回 task_id
//!
//! 匹配键是 **Agent ID**（`TEMPLATE_*`），不是名称。路由挂在 `/system` 段下，
//! 已由 `require_role_middleware(Admin)` 统一鉴权。
//!
//! 核心语义见 [`super::apply_single_preset_agent`]：
//! 缺失 → 新建 + 自动入职；软删 → 恢复 + 进修补包；在用 → 仅 Overwrite 覆写身份字段。
//! Agent 无文件、无副本概念，策略参数比技能同步少一个 `sync_installed_copies`。

use crate::pkg::RequestContext;
use crate::pkg::background_task::{BackgroundTask, registry};
use crate::service::domain::finance;
use crate::service::domain::hr;
use crate::service::domain::system::seed::default;
use ai_orz_macros::generate_http_handler;
use async_trait::async_trait;
use common::api::seed::{
    PresetAgentSyncItem, PresetAgentSyncStrategy, PreviewPresetAgentsRequest,
    PreviewPresetAgentsResponse, SyncPresetAgentsRequest, SyncPresetAgentsResponse,
};
use common::api::{TaskIdResponse, TaskProgressSnapshot, TaskStatus, TaskType};
use common::error::Result;
use std::sync::atomic::AtomicUsize;
use std::sync::{Arc, Mutex};

/// 预置 Agent 同步预览
///
/// 逐个比对 seed Agent 与库中的同 ID Agent，返回「将新增 / 将恢复 / 将覆盖」清单。
/// 额外解析每个 Agent 将绑定的对话模型 Provider，供用户在确认前发现「未配模型」问题。
/// 只读，不写任何数据。
#[generate_http_handler]
pub async fn preview_preset_agents(
    ctx: RequestContext,
    _params: PreviewPresetAgentsRequest,
) -> Result<PreviewPresetAgentsResponse> {
    let snapshot = default::embedded_default_snapshot();

    let mut items = Vec::with_capacity(snapshot.agents.len());
    for agent_def in &snapshot.agents {
        let local = hr::domain()
            .agent_manage()
            .get_agent(ctx.clone(), &agent_def.id, Default::default())
            .await?;

        // get_agent 不含软删行：单独探测同 ID 的软删记录（误删场景）
        let deleted = if local.is_some() {
            false
        } else {
            hr::domain()
                .agent_manage()
                .query(
                    ctx.clone(),
                    crate::service::dao::agent::AgentQuery {
                        ids: Some(vec![agent_def.id.clone()]),
                        ..Default::default()
                    },
                )
                .await?
                .items
                .iter()
                .any(|a| a.po.status == common::enums::AgentStatus::Deleted)
        };

        // 预解析将绑定的对话模型 Provider（供前端提示「未配模型」风险）
        let resolved_provider_id =
            super::resolve_agent_provider_id(&ctx, &agent_def.model_provider_id).await?;
        let resolved_provider_name = match &resolved_provider_id {
            Some(pid) => finance::domain()
                .model_provider_manage()
                .get_model_provider_with_options(ctx.clone(), pid, Default::default())
                .await?
                .map(|p| p.po.name),
            None => None,
        };

        items.push(PresetAgentSyncItem {
            id: agent_def.id.clone(),
            name: agent_def.name.clone(),
            description: agent_def.description.clone(),
            roles: agent_def.roles.clone(),
            exists: local.is_some() || deleted,
            deleted,
            local_name: local.as_ref().map(|a| a.po.name.clone()),
            local_status: local.as_ref().map(|a| a.po.status),
            resolved_provider_name,
        });
    }

    let existing_count = items.iter().filter(|i| i.exists && !i.deleted).count();
    let deleted_count = items.iter().filter(|i| i.deleted).count();
    Ok(PreviewPresetAgentsResponse {
        missing_count: items.len() - existing_count - deleted_count,
        existing_count,
        deleted_count,
        items,
    })
}

/// 同步预置 Agent 后台任务（自包含进度状态，骨架与 SyncPresetSkillsTask 一致）
pub struct SyncPresetAgentsTask {
    task_id: String,
    ctx: RequestContext,
    params: SyncPresetAgentsRequest,
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

impl SyncPresetAgentsTask {
    /// 创建新的同步任务对象（状态为 Pending，等待 registry spawn 后执行）
    pub fn new(ctx: RequestContext, params: SyncPresetAgentsRequest) -> Self {
        let total_steps = default::embedded_default_snapshot().agents.len();
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
    fn set_completed(&self, result: SyncPresetAgentsResponse) {
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

    /// 执行同步步骤：逐 Agent 导入（进度按 Agent 粒度上报）
    async fn run_steps(&self) -> Result<SyncPresetAgentsResponse> {
        let snapshot = default::embedded_default_snapshot();
        let skip_existing = matches!(self.params.strategy, PresetAgentSyncStrategy::OnlyMissing);

        let mut created = 0usize;
        let mut updated = 0usize;
        let mut restored = 0usize;
        let mut skipped = 0usize;

        for (idx, agent_def) in snapshot.agents.iter().enumerate() {
            self.set_step(idx + 1, format!("正在同步 Agent：{}", agent_def.name));
            match super::apply_single_preset_agent(self.ctx.clone(), agent_def, skip_existing)
                .await?
            {
                super::PresetAgentAction::Created => created += 1,
                super::PresetAgentAction::Updated => updated += 1,
                super::PresetAgentAction::Restored => restored += 1,
                super::PresetAgentAction::Skipped => skipped += 1,
            }
        }

        Ok(SyncPresetAgentsResponse {
            created,
            updated,
            restored,
            skipped,
            total: snapshot.agents.len(),
        })
    }
}

#[async_trait]
impl BackgroundTask for SyncPresetAgentsTask {
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
                let v = serde_json::to_value(&resp).map_err(|e| {
                    common::error::Error::internal(format!("序列化结果失败: {}", e))
                })?;
                self.set_completed(resp);
                log_info!(
                    &self.ctx,
                    "sync_preset_agents",
                    strategy = ?self.params.strategy,
                    "预置 Agent 同步完成"
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

/// 同步预置 Agent（异步提交，返回 task_id）
///
/// - `Overwrite`（默认）：在用的同 ID Agent 覆写身份字段（名称/角色/描述/能力/人设/模型绑定），
///   生命周期状态不动；完全缺失的新建并自动入职；已被误删的恢复并进修补包
/// - `OnlyMissing`：在用的同 ID Agent 原样保留，其余同上
///
/// 前端通过 `GET /api/v1/system/tasks/{task_id}/progress` 轮询进度与结果。
#[generate_http_handler]
pub async fn sync_preset_agents(
    ctx: RequestContext,
    params: SyncPresetAgentsRequest,
) -> Result<TaskIdResponse> {
    let task = Arc::new(SyncPresetAgentsTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}
