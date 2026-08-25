//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织、超级管理员和默认 ModelProvider
//! Handler 层负责跨 domain 编排：organization domain 创建 org+user，finance domain 创建 provider
//!
//! 初始化是异步任务：提交后返回 task_id，前端轮询进度接口获取执行状态。
//! 任务对象自包含进度状态，通过通用后台任务模块（pkg::background_task）注册运行。

use crate::pkg::RequestContext;
use crate::pkg::background_task::{BackgroundTask, registry};
use crate::service::domain::system;
use crate::service::domain::{finance, hr, organization};
use ai_orz_macros::generate_http_handler;
use async_trait::async_trait;
use common::api::{
    CheckInitializedRequest, CheckInitializedResponse, GetInitProgressRequest,
    InitProgressResponse, InitStatus, InitializeSystemRequest, InitializeSystemResponse,
    TaskIdResponse, TaskProgressSnapshot, TaskStatus, TaskType,
};
use common::error::{Error, Result};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 系统初始化后台任务（自包含进度状态）
///
/// 持有 ctx/params + 进度状态字段，`run` 方法内部更新进度，
/// 外部通过 `progress()` 读取快照。注册到通用后台任务注册中心后由 registry spawn 执行。
pub struct InitializeSystemTask {
    task_id: String,
    ctx: RequestContext,
    params: InitializeSystemRequest,
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

impl InitializeSystemTask {
    /// 创建新的初始化任务对象（状态为 Pending，等待 registry spawn 后执行）
    pub fn new(ctx: RequestContext, params: InitializeSystemRequest) -> Self {
        // 基础 4 步（组织 + 内置工具 + 预置技能 + 预设前台 Agent）+ 对话模型(0/1) + 向量模型(0/1)
        let total_steps = 4
            + usize::from(params.chat_model.is_some())
            + usize::from(params.embedding_model.is_some());
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
    fn set_step(&self, step: usize, message: &str) {
        self.current_step.store(step, Ordering::SeqCst);
        *self.step_message.lock().unwrap() = message.to_string();
    }

    /// 标记完成并写入结果
    fn set_completed(&self, result: serde_json::Value) {
        *self.status.lock().unwrap() = TaskStatus::Completed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "初始化完成".to_string();
        *self.result.lock().unwrap() = Some(result);
    }

    /// 标记失败
    fn set_failed(&self, error: String) {
        *self.status.lock().unwrap() = TaskStatus::Failed;
        *self.finished_at.lock().unwrap() = Some(chrono::Utc::now().timestamp_millis());
        *self.step_message.lock().unwrap() = "初始化失败".to_string();
        *self.error.lock().unwrap() = Some(error);
    }
}

#[async_trait]
impl BackgroundTask for InitializeSystemTask {
    fn task_id(&self) -> &str {
        &self.task_id
    }

    fn task_type(&self) -> TaskType {
        TaskType::InitializeSystem
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
        self.set_step(1, "正在创建组织和超级管理员");

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

impl InitializeSystemTask {
    /// 执行初始化步骤（从原 run_initialize_steps 迁移逻辑）
    ///
    /// 每步通过 `set_step` 更新进度，保持原有业务逻辑不变：
    /// 创建组织+Owner → chat provider（可选）→ embedding provider（可选）→ 同步内置工具 → 导入预置技能
    async fn run_steps(&self) -> Result<InitializeSystemResponse> {
        let ctx = self.ctx.clone();
        let params = self.params.clone();

        // Step 1: 创建组织 + Owner
        self.set_step(1, "正在创建组织和超级管理员");
        let (org_id, user_id) = organization::domain()
            .organization_manage()
            .create_org_and_owner(ctx.clone(), params.clone())
            .await?;

        let mut step = 2;

        // Step（可选）: 创建 chat provider — 未配置时跳过，后续在模型管理中补配
        let chat_provider_id = if let Some(chat_config) = params.chat_model.clone() {
            self.set_step(step, "正在配置对话模型");
            let chat_provider = crate::models::model_provider::ModelProvider::new(
                chat_config.name,
                common::enums::ProviderType::from_i32(chat_config.provider_type),
                common::enums::ModelCapability::Agent,
                chat_config.model_name,
                chat_config.api_key,
                chat_config.base_url,
                chat_config.description,
                user_id.clone(),
            );
            let provider_id = chat_provider.po.id.clone();
            finance::domain()
                .model_provider_manage()
                .create_model_provider(ctx.clone(), &chat_provider)
                .await?;
            step += 1;
            Some(provider_id)
        } else {
            None
        };

        // Step（可选）: 创建 embedding provider — 未配置时跳过向量索引
        let embedding_provider_id = if let Some(embedding_config) = params.embedding_model {
            self.set_step(step, "正在配置向量模型");
            let embedding_provider = crate::models::model_provider::ModelProvider::new(
                embedding_config.name,
                common::enums::ProviderType::from_i32(embedding_config.provider_type),
                common::enums::ModelCapability::Embedding,
                embedding_config.model_name,
                embedding_config.api_key,
                embedding_config.base_url,
                embedding_config.description,
                user_id.clone(),
            );
            let provider_id = embedding_provider.po.id.clone();
            finance::domain()
                .model_provider_manage()
                .create_model_provider(ctx.clone(), &embedding_provider)
                .await?;
            step += 1;
            Some(provider_id)
        } else {
            None
        };

        // Step: 同步内置工具到 DB
        self.set_step(step, "正在同步内置工具");
        let tool_count = finance::domain()
            .tool_provider_manage()
            .sync_builtin_tools(ctx.clone())
            .await?;
        sys_info!("initialize_system: 同步 {} 个内置工具到 DB", tool_count);

        // Step: 导入预置技能
        self.set_step(step + 1, "正在导入预置技能");
        let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
        let skill_result = crate::handlers::system::seed::apply_preset_skills(
            ctx.clone(),
            &snapshot.skills,
            Some(&user_id),
            false,
        )
        .await?;
        sys_info!(
            "initialize_system: 导入预置技能（created={}, updated={}, skipped={}, author={}）",
            skill_result.created,
            skill_result.updated,
            skill_result.skipped,
            user_id
        );

        // Step: 无条件创建预设前台 Agent（降低使用门槛）
        // - roles: ["reception"]：Web 前台通道精确命中；飞书/A2A 等场景经渐进匹配（子串/语义）自动回退
        // - 初始化配置了 chat provider → 直接绑定；未配置 → 留空，wake 时自动回退默认对话模型
        // - 额外安装 project_management 技能，支持把复杂请求升级为任务流转
        self.set_step(step + 2, "正在创建预设前台接待 Agent");
        let reception_agent_id =
            create_preset_reception_agent(ctx.clone(), &user_id, chat_provider_id.clone()).await?;
        sys_info!(
            "initialize_system: 创建预设前台 Agent {:?} (provider={:?})",
            reception_agent_id,
            chat_provider_id
        );

        Ok(InitializeSystemResponse {
            organization_id: org_id,
            user_id,
            chat_provider_id,
            embedding_provider_id,
            reception_agent_id,
        })
    }
}

/// 无条件创建预设前台接待 Agent，并安装项目管理技能。
///
/// 返回新创建的 Agent ID（或已存在前台 Agent 的 ID）。
async fn create_preset_reception_agent(
    ctx: RequestContext,
    owner_id: &str,
    chat_provider_id: Option<String>,
) -> Result<Option<String>> {
    use crate::models::agent::{Agent, AgentPo};
    use common::constants::agent_roles::ROLE_RECEPTION;
    use common::enums::AgentStatus;

    // 幂等：若已存在前台角色 Agent，直接复用，不重复创建
    let existing = hr::domain()
        .agent_manage()
        .query(
            ctx.clone(),
            crate::service::dao::agent::AgentQuery {
                roles: Some(vec![ROLE_RECEPTION.to_string()]),
                status: Some(AgentStatus::Onboarded),
                pagination: common::api::PaginationParams {
                    limit: Some(1),
                    offset: None,
                },
                ..Default::default()
            },
        )
        .await?;
    if let Some(existing_agent) = existing.items.into_iter().next() {
        sys_info!(
            "initialize_system: 已存在前台 Agent {}, 复用",
            existing_agent.po.id
        );
        return Ok(Some(existing_agent.po.id));
    }

    let mut po = AgentPo::new(
        "前台接待".to_string(),
        vec![ROLE_RECEPTION.to_string()],
        "负责接待访客、引导用户找到对应能力，并把复杂请求升级为任务流转的通用前台 Agent"
            .to_string(),
        vec![
            "chat".to_string(),
            "task".to_string(),
            "knowledge".to_string(),
        ],
        PRESET_RECEPTION_SOUL.to_string(),
        chat_provider_id.unwrap_or_default(),
        owner_id.to_string(),
    );
    po.status = AgentStatus::Onboarded;

    let agent = Agent::from_po(po);
    hr::domain()
        .agent_manage()
        .create_bootstrap_agent(ctx.clone(), &agent)
        .await?;
    let agent_id = agent.po.id.clone();

    // 安装项目管理技能（非 neural，需 match_keys 命中才进必加载；失败不阻塞初始化）
    match hr::domain()
        .agent_manage()
        .install_skill_pack(ctx, &agent_id, "project_management")
        .await
    {
        Ok(n) => sys_info!(
            "initialize_system: 前台 Agent 安装 project_management 技能完成，{} 条",
            n
        ),
        Err(e) => {
            sys_warn!("initialize_system: 前台 Agent 安装 project_management 技能失败（忽略）: {e}")
        }
    }

    Ok(Some(agent_id))
}

/// 预设前台接待 Agent 的灵魂设定
const PRESET_RECEPTION_SOUL: &str = "你是「前台接待」，是组织的对外接待智能体，也是用户进入系统后第一个接触的角色。

你的使命：让用户以最快速度找到对的人、对的答案、对的能力。

行为准则：
1. 快速识别意图：问候、咨询、闲聊直接回应；涉及具体业务（人事、财务、代码、项目等）先判断是否有更专业的 Agent 或工具可以承接。
2. 能答就答：基于记忆与知识回答简单问题；不确定时不编造，明确告知用户并转派。
3. 该转则转：遇到专业问题，优先引导到对应 Agent 或创建任务流转，不越俎代庖、不冒充专家。
4. 亲切而克制：语气友好自然，但不做做不到的承诺；不暴露内部实现细节。
5. 沉淀记忆：记住用户的关键偏好与常用需求，写进工作记忆与偏好，下次更快服务。";

/// 检查系统是否已经初始化
#[generate_http_handler]
pub async fn check_initialized(
    ctx: RequestContext,
    _params: CheckInitializedRequest,
) -> Result<CheckInitializedResponse> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx).await?;
    // 协议规范：即使只返回一个字段也使用标准 Response 结构体，禁止裸 bool
    Ok(CheckInitializedResponse { initialized })
}

/// 初始化系统（异步提交，返回 task_id）
///
/// 创建自包含任务对象，注册到通用后台任务注册中心，由 registry spawn 执行。
#[generate_http_handler]
pub async fn initialize_system(
    ctx: RequestContext,
    params: InitializeSystemRequest,
) -> Result<TaskIdResponse> {
    // 边界校验：配置了模型则字段必须完整（前端分步校验只覆盖正常路径）
    if let Some(chat) = params.chat_model.as_ref()
        && (chat.name.trim().is_empty()
            || chat.model_name.trim().is_empty()
            || chat.api_key.trim().is_empty())
    {
        return Err(Error::bad_request(
            "chat_model provided but name / model_name / api_key is empty",
        ));
    }
    if let Some(emb) = params.embedding_model.as_ref()
        && (emb.name.trim().is_empty() || emb.model_name.trim().is_empty())
    {
        return Err(Error::bad_request(
            "embedding_model provided but name / model_name is empty",
        ));
    }

    let task = Arc::new(InitializeSystemTask::new(ctx, params));
    let task_id = registry().register(task).await;
    Ok(TaskIdResponse { task_id })
}

/// 查询初始化进度
///
/// 从通用后台任务注册中心获取基础进度快照，装饰为 `InitProgressResponse` 返回：
/// - 将 `TaskStatus` 映射为 `InitStatus`
/// - 解析 `result` JSON 字段为 `InitializeSystemResponse`
#[generate_http_handler]
pub async fn get_initialize_progress(
    _ctx: RequestContext,
    params: GetInitProgressRequest,
) -> Result<InitProgressResponse> {
    let snapshot = system::domain()
        .background_task_registry()
        .get_progress(&params.task_id)
        .await
        .ok_or_else(|| Error::not_found(format!("初始化任务不存在: {}", params.task_id)))?;

    Ok(InitProgressResponse {
        task_id: snapshot.task_id,
        status: match snapshot.status {
            TaskStatus::Pending => InitStatus::Pending,
            TaskStatus::Running => InitStatus::Running,
            TaskStatus::Completed => InitStatus::Completed,
            TaskStatus::Failed => InitStatus::Failed,
        },
        current_step: snapshot.current_step,
        total_steps: snapshot.total_steps,
        step_message: snapshot.step_message,
        started_at: snapshot.started_at,
        finished_at: snapshot.finished_at,
        error: snapshot.error,
        result: snapshot.result.and_then(|v| serde_json::from_value(v).ok()),
    })
}
