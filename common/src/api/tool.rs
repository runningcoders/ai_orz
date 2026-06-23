//! Built-in tool related API request/response DTOs - shared between backend and frontend

use crate::enums::{ControlMode, ToolProtocol, ToolStatus};
use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Create built-in tool request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateToolRequest {
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool protocol type
    pub protocol: ToolProtocol,
    /// Protocol configuration JSON
    pub config: Option<serde_json::Value>,
    /// Parameters JSON Schema (required for dynamic tools, optional for built-in tools)
    pub parameters_schema: Option<serde_json::Value>,
    /// Tags list for capability matching and filtering
    pub tags: Option<Vec<String>>,
    /// Control mode: auto (rig native) / manual (custom pipeline)
    pub control_mode: Option<ControlMode>,
    /// Whether this tool is enabled
    pub enabled: Option<bool>,
}

/// Create built-in tool response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateToolResponse {
    /// Tool ID
    pub id: String,
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool type
    pub tool_type: String,
    /// Created timestamp
    pub created_at: i64,
}

/// Get tool request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetToolRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,
}

/// Get tool response (alias for tool detail)
pub type ToolDetail = GetToolResponse;

/// Get tool response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetToolResponse {
    /// Tool ID
    pub id: String,
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: String,
    /// Tool protocol type
    pub protocol: ToolProtocol,
    /// Control mode: auto (rig native) / manual (custom pipeline)
    pub control_mode: ControlMode,
    /// Protocol configuration JSON
    pub config: Option<serde_json::Value>,
    /// Whether there is any non-empty configuration
    pub has_config: bool,
    /// Parameters JSON Schema
    pub parameters_schema: Option<serde_json::Value>,
    /// Tags list
    pub tags: Vec<String>,
    /// Whether this tool is enabled
    pub enabled: bool,
    /// Tool status
    pub status: ToolStatus,
    /// Created by user ID
    pub created_by: Option<String>,
    /// Updated by user ID
    pub updated_by: Option<String>,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}

/// Delete tool request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteToolRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,
}

/// Delete tool response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteToolResponse {
    /// Whether deletion succeeded
    pub success: bool,
}

/// List tools request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListToolsRequest {
    /// Filter by bound agent ID
    pub agent_id: Option<String>,
    /// Search by keyword in name/description
    pub keyword: Option<String>,
    /// Filter by enabled status
    pub only_enabled: Option<bool>,
}

/// List tools response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListToolsResponse {
    /// List of all built-in tools
    pub tools: Vec<ToolListItem>,
}

/// Tool list item
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ToolListItem {
    /// Tool ID
    pub id: String,
    /// Tool display name
    pub name: String,
    /// Tool description
    pub description: Option<String>,
    /// Tool protocol
    pub protocol: ToolProtocol,
    /// Control mode
    pub control_mode: ControlMode,
    /// Parameters JSON Schema (for MCP/HTTP tools)
    pub parameters_schema: Option<serde_json::Value>,
    /// Tags for categorization
    pub tags: Vec<String>,
    /// Tool status
    pub status: ToolStatus,
    /// Whether there is any non-empty configuration
    pub has_config: bool,
    /// Whether this tool is enabled
    pub enabled: bool,
    /// Creator ID
    pub created_by: String,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}

/// Update tool request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateToolRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,

    /// New tool display name
    pub name: Option<String>,
    /// New tool description
    pub description: Option<String>,
    /// New tool protocol type
    pub protocol: Option<ToolProtocol>,
    /// New control mode: auto (rig native) / manual (custom pipeline)
    pub control_mode: Option<ControlMode>,
    /// New protocol configuration JSON
    pub config: Option<serde_json::Value>,
    /// New parameters JSON Schema
    pub parameters_schema: Option<serde_json::Value>,
    /// New tags list
    pub tags: Option<Vec<String>>,
    /// New enabled status
    pub enabled: Option<bool>,
}

/// Update tool response
pub type UpdateToolResponse = GetToolResponse;

/// Update tool status request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateToolStatusRequest {
    /// Tool ID
    #[param(source = "path")]
    pub id: String,
    /// New status
    pub status: ToolStatus,
}

/// Update tool status response
pub type UpdateToolStatusResponse = GetToolResponse;

/// Bind tool to agent request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct BindToolToAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
    /// Tool ID to bind
    #[param(source = "path")]
    pub tool_id: String,
}

/// Bind tool to agent response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BindToolToAgentResponse {
    /// Whether binding succeeded
    pub success: bool,
}

/// Unbind tool from agent request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UnbindToolFromAgentRequest {
    /// Agent ID
    #[param(source = "path")]
    pub agent_id: String,
    /// Tool ID to unbind
    #[param(source = "path")]
    pub tool_id: String,
}

/// Unbind tool from agent response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UnbindToolFromAgentResponse {
    /// Whether unbinding succeeded
    pub success: bool,
}
