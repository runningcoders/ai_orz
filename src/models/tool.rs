//! Tool 持久化对象和完整实体

use async_trait::async_trait;
use common::enums::tool::ControlMode;
use common::enums::{ToolProtocol, ToolStatus};
use rig::tool::{ToolDyn, ToolError};
use crate::pkg::request_context::RequestContext;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;
use dyn_clone::DynClone;
use futures_util::FutureExt;

/// 核心工具 trait - 所有工具都必须实现这个
/// 
/// 提供带 RequestContext 的调用接口，并且能获取到对应的数据库持久化对象
#[async_trait]
pub trait CoreTool: Send + Sync + DynClone {
    /// 执行工具调用
    async fn call(&self, ctx: &RequestContext, args: Value) -> Result<Value, ToolError>;
    
    /// 获取工具对应的数据库持久化对象
    fn po(&self) -> &ToolPo;
}

dyn_clone::clone_trait_object!(CoreTool);

/// Rig 适配层 - 将我们的 CoreTool trait 转换为 Rig 的 ToolDyn trait
/// 
/// 用于 auto 模式，让 Rig 可以直接调用我们的工具
/// Rig 调用接口不传递 RequestContext，所以需要创建时持有
pub struct RigToolAdapter {
    ctx: RequestContext,
    inner: Box<dyn CoreTool>,
}

impl RigToolAdapter {
    pub fn new(ctx: RequestContext, inner: Box<dyn CoreTool>) -> Self {
        Self { ctx, inner }
    }
}

impl ToolDyn for RigToolAdapter {
    fn name(&self) -> String {
        self.inner.po().name.clone()
    }

    fn definition<'a>(
        &'a self,
        _: String,
    ) -> std::pin::Pin<Box<
        dyn futures_util::Future<Output = rig::completion::ToolDefinition>
        + std::marker::Send
        + 'a,
    >> {
        
        let definition = rig::completion::ToolDefinition {
            name: self.inner.po().name.clone(),
            description: self.inner.po().description.clone(),
            parameters: self.inner.po().parameters_schema.clone().unwrap_or_default(),
        };
        Box::pin(async move { definition })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> std::pin::Pin<Box<
        dyn futures_util::Future<Output = Result<String, ToolError>>
        + std::marker::Send
        + 'a,
    >> {
        use futures_util::FutureExt;
        let ctx = &self.ctx;
        let inner = &self.inner;

        async move {
            // Parse args from String JSON to Value
            let args: Value = match serde_json::from_str(&args) {
                Ok(v) => v,
                Err(e) => {
                    return Err(ToolError::JsonError(e));
                }
            };

            // Call our core tool
            let result = inner.call(ctx, args).await;

            // Serialize result back to string
            match result {
                Ok(v) => Ok(serde_json::to_string(&v)?),
                Err(e) => Err(rig::tool::ToolError::ToolCallError(e.to_string().into())),
            }
        }
        .boxed()
    }
}

/// Tool 持久化对象
///
/// 对应 SQL 建表语句：`migrations/20260420000000_initial.sql`
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ToolPo {
    /// 工具 ID
    pub id: String,
    /// 工具名称（唯一）
    pub name: String,
    /// 工具描述
    pub description: String,
    /// 工具协议类型
    pub protocol: ToolProtocol,
    /// 控制模式：auto (rig原生) / manual (自建链路)
    pub control_mode: ControlMode,
    /// 协议配置（JSON）
    pub config: serde_json::Value,
    /// 参数 JSON Schema（动态工具必填，内置工具可选）
    pub parameters_schema: Option<serde_json::Value>,
    /// 工具状态
    pub status: ToolStatus,
    /// 创建时间
    pub created_at: i64,
    /// 更新时间
    pub updated_at: i64,
    /// 创建者
    pub created_by: Option<String>,
    /// 更新者
    pub updated_by: Option<String>,
}

/// Tool - complete tool entity with PO and boxed trait object
///
/// Contains persistent metadata + actual executable tool object
/// Based on control_mode:
/// - Auto:  rig_tool is populated (rig's ToolDyn, already adapted)
/// - Manual: rig_tool is None, use our_tool directly
pub struct Tool {
    /// Persistent metadata from DB
    pub po: ToolPo,
    /// Control mode (stored redundantly for easy matching)
    pub control_mode: ControlMode,
    /// Rig adapter (Auto mode only)
    pub rig_tool: Option<Box<dyn ToolDyn + Send + Sync>>,
    /// Our core interface tool (already wrapped with logging decorator)
    pub our_tool: Box<dyn CoreTool + Send + Sync>,
}

// Manual Debug implementation - skip the dyn fields
impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("po", &self.po)
            .field("control_mode", &self.control_mode)
            .field("rig_tool", &format_args!("Option<Box<dyn ToolDyn + Send + Sync>>"))
            .field("our_tool", &format_args!("Box<dyn CoreTool + Send + Sync>"))
            .finish()
    }
}

// Manual Clone implementation - Agent derives Clone, but dyn Trait can't be cloned
// In practice, Agent is wrapped in Arc when shared, so this unreachable is safe
impl Clone for Tool {
    fn clone(&self) -> Self {
        unreachable!("Tool cannot be cloned due to dyn Trait object. Use Arc<Tool> for sharing.");
    }
}

impl ToolPo {
    /// 创建新 ToolPo（如果 id 为空自动生成 Uuid v7）
    pub fn new(
        id: String,
        name: String,
        description: String,
        protocol: ToolProtocol,
        config: serde_json::Value,
        parameters_schema: Option<serde_json::Value>,
        creator: Option<String>,
    ) -> Self {
        let id = if id.is_empty() {
            Uuid::now_v7().to_string()
        } else {
            id
        };
        let now = common::constants::utils::current_timestamp();
        Self {
            id,
            name,
            description,
            protocol,
            control_mode: ControlMode::Auto, // Default to Auto for backward compatibility
            config,
            parameters_schema,
            status: ToolStatus::Enabled,
            created_at: now,
            updated_at: now,
            created_by: creator.clone(),
            updated_by: creator,
        }
    }

    /// 创建 built-in 工具的默认 ToolPo
    /// id == name for built-in tools since they are constants
    pub fn new_builtin(
        id: String,
        name: String,
        description: String,
    ) -> Self {
        Self::new(
            id,
            name,
            description,
            ToolProtocol::Builtin,
            serde_json::Value::Null, // No extra config for built-in tools
            None, // Parameters can be extracted from trait at runtime if needed
            None, // System built-in, no specific creator
        )
    }

    /// 更新时间戳和修改者
    pub fn touch(&mut self, modifier: Option<String>) {
        self.updated_at = common::constants::utils::current_timestamp();
        self.updated_by = modifier;
    }
}
