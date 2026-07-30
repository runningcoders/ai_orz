//! 初始化系统接口
//!
//! 当系统还没有初始化时，调用这个接口创建第一个组织、超级管理员和默认 ModelProvider
//! Handler 层负责跨 domain 编排：organization domain 创建 org+user，finance domain 创建 provider
//!
//! 初始化是异步任务：提交后返回 task_id，前端轮询进度接口获取执行状态。

use crate::pkg::RequestContext;
use crate::service::domain::{finance, organization};
use ai_orz_macros::generate_http_handler;
use common::api::{
    CheckInitializedRequest, GetInitProgressRequest, InitProgressResponse, InitStatus,
    InitializeSystemAsyncResponse, InitializeSystemRequest, InitializeSystemResponse,
};
use common::error::{Error, Result};
use std::collections::HashMap;
use std::sync::Arc;

/// 初始化任务内存状态（不含 JoinHandle，初始化只执行一次无需清理）
#[derive(Debug, Clone)]
struct InitTaskState {
    task_id: String,
    status: InitStatus,
    current_step: usize,
    total_steps: usize,
    step_message: String,
    started_at: i64,
    finished_at: Option<i64>,
    error: Option<String>,
    result: Option<InitializeSystemResponse>,
}

impl InitTaskState {
    fn new(task_id: String, total_steps: usize) -> Self {
        Self {
            task_id,
            status: InitStatus::Pending,
            current_step: 0,
            total_steps,
            step_message: "等待开始".to_string(),
            started_at: chrono::Utc::now().timestamp(),
            finished_at: None,
            error: None,
            result: None,
        }
    }

    fn to_response(&self) -> InitProgressResponse {
        InitProgressResponse {
            task_id: self.task_id.clone(),
            status: self.status,
            current_step: self.current_step,
            total_steps: self.total_steps,
            step_message: self.step_message.clone(),
            started_at: self.started_at,
            finished_at: self.finished_at,
            error: self.error.clone(),
            result: self.result.clone(),
        }
    }
}

/// 全局初始化任务状态（内存存储，进程重启后丢失）
/// 使用 HashMap 按 task_id 索引，支持多任务并存（并行测试/重试场景）。
static INIT_TASKS: once_cell::sync::OnceCell<
    Arc<tokio::sync::RwLock<HashMap<String, InitTaskState>>>,
> = once_cell::sync::OnceCell::new();

fn init_task_store() -> &'static Arc<tokio::sync::RwLock<HashMap<String, InitTaskState>>> {
    INIT_TASKS.get_or_init(|| Arc::new(tokio::sync::RwLock::new(HashMap::new())))
}

/// 检查系统是否已经初始化
#[generate_http_handler]
pub async fn check_initialized(
    ctx: RequestContext,
    _params: CheckInitializedRequest,
) -> Result<bool> {
    let domain = organization::domain();
    let initialized = domain.organization_manage().check_initialized(ctx).await?;
    Ok(initialized)
}

/// 初始化系统（异步提交，返回 task_id）
#[generate_http_handler]
pub async fn initialize_system(
    ctx: RequestContext,
    params: InitializeSystemRequest,
) -> Result<InitializeSystemAsyncResponse> {
    // 生成 task_id
    let task_id = uuid::Uuid::new_v4().to_string();

    // 确定总步骤数（embedding 可选）
    let total_steps = if params.embedding_model.is_some() {
        5
    } else {
        4
    };

    // 初始化任务状态
    let mut state = InitTaskState::new(task_id.clone(), total_steps);
    state.status = InitStatus::Running;
    {
        let mut guard = init_task_store().write().await;
        guard.insert(task_id.clone(), state.clone());
    }

    // 克隆上下文，启动异步任务
    let task_id_clone = task_id.clone();
    let ctx_clone = ctx.clone();
    tokio::spawn(async move {
        run_initialize_task(task_id_clone, ctx_clone, params, total_steps).await;
    });

    Ok(InitializeSystemAsyncResponse { task_id })
}

/// 异步执行初始化任务（后台运行，更新内存状态）
async fn run_initialize_task(
    task_id: String,
    ctx: RequestContext,
    params: InitializeSystemRequest,
    total_steps: usize,
) {
    let result = run_initialize_steps(&task_id, ctx, params, total_steps).await;

    let mut guard = init_task_store().write().await;
    if let Some(state) = guard.get_mut(&task_id) {
        state.finished_at = Some(chrono::Utc::now().timestamp());
        match result {
            Ok(resp) => {
                state.status = InitStatus::Completed;
                state.current_step = total_steps;
                state.step_message = "初始化完成".to_string();
                state.result = Some(resp);
            }
            Err(e) => {
                state.status = InitStatus::Failed;
                state.step_message = "初始化失败".to_string();
                state.error = Some(e.to_string());
            }
        }
    }
}

/// 执行初始化步骤，每步更新进度
async fn run_initialize_steps(
    task_id: &str,
    ctx: RequestContext,
    params: InitializeSystemRequest,
    total_steps: usize,
) -> Result<InitializeSystemResponse> {
    // Step 1: 创建组织 + Owner
    update_progress(task_id, 1, total_steps, "正在创建组织和超级管理员").await;
    let (org_id, user_id) = organization::domain()
        .organization_manage()
        .create_org_and_owner(ctx.clone(), params.clone())
        .await?;

    // Step 2: 创建 chat provider
    update_progress(task_id, 2, total_steps, "正在配置对话模型").await;
    let chat_provider = crate::models::model_provider::ModelProvider::new(
        params.chat_model.name,
        common::enums::ProviderType::from_i32(params.chat_model.provider_type),
        common::enums::ModelCapability::Agent,
        params.chat_model.model_name,
        params.chat_model.api_key,
        params.chat_model.base_url,
        params.chat_model.description,
        user_id.clone(),
    );
    let chat_provider_id = chat_provider.po.id.clone();
    finance::domain()
        .model_provider_manage()
        .create_model_provider(ctx.clone(), &chat_provider)
        .await?;

    // Step 3: 创建 embedding provider（可选）
    let mut next_step = 3;
    let embedding_provider_id = if let Some(embedding_config) = params.embedding_model {
        update_progress(task_id, 3, total_steps, "正在配置向量模型").await;
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
        next_step = 4;
        Some(provider_id)
    } else {
        None
    };

    // Step 4/5: 同步内置工具到 DB
    update_progress(task_id, next_step, total_steps, "正在同步内置工具").await;
    let tool_count = finance::domain()
        .tool_provider_manage()
        .sync_builtin_tools(ctx.clone())
        .await?;
    sys_info!("initialize_system: 同步 {} 个内置工具到 DB", tool_count);

    // Step 5/6: 导入预置技能
    update_progress(task_id, next_step + 1, total_steps, "正在导入预置技能").await;
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

    Ok(InitializeSystemResponse {
        organization_id: org_id,
        user_id,
        chat_provider_id,
        embedding_provider_id,
    })
}

/// 更新指定 task 的进度状态
async fn update_progress(task_id: &str, step: usize, total_steps: usize, message: &str) {
    let mut guard = init_task_store().write().await;
    if let Some(state) = guard.get_mut(task_id) {
        state.current_step = step;
        state.step_message = message.to_string();
    }
    let _ = total_steps;
}

/// 查询初始化进度
#[generate_http_handler]
pub async fn get_initialize_progress(
    _ctx: RequestContext,
    params: GetInitProgressRequest,
) -> Result<InitProgressResponse> {
    let guard = init_task_store().read().await;
    match guard.get(&params.task_id) {
        Some(state) => Ok(state.to_response()),
        None => Err(Error::not_found(format!(
            "初始化任务不存在: {}",
            params.task_id
        ))),
    }
}
