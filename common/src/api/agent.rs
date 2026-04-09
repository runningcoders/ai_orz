//! Agent (AI智能体) related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// Agent information for list display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListItem {
    /// Agent ID
    pub id: String,
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
    /// Model provider ID (which model this agent uses
    pub model_provider_id: String,
    /// Model provider name (for display)
    pub model_provider_name: String,
    /// Whether this agent is enabled
    pub enabled: bool,
    /// Creation timestamp
    pub created_at: i64,
}

/// List agents response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListAgentsResponse {
    /// List of agents
    pub data: Vec<AgentListItem>,
    /// Total count
    pub total: u64,
}

/// Get agent detail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetAgentResponse {
    /// Agent detail data
    pub data: AgentDetail,
}

/// Detailed agent information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDetail {
    /// Agent ID
    pub id: String,
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
    /// System prompt (soul/personality)
    pub system_prompt: String,
    /// Model provider ID
    pub model_provider_id: String,
    /// Agent enabled flag
    pub enabled: bool,
    /// Creation timestamp
    pub created_at: i64,
    /// Update timestamp
    pub updated_at: i64,
}

/// Create agent request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateAgentRequest {
    /// Agent name
    pub name: String,
    /// Agent description
    pub description: String,
    /// System prompt (personality instructions)
    pub system_prompt: String,
    /// Model provider ID
    pub model_provider_id: String,
    /// Whether enabled
    pub enabled: bool,
}

/// Update agent request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateAgentRequest {
    /// New name (None = no change)
    pub name: Option<String>,
    /// New description (None = no change)
    pub description: Option<String>,
    /// New system prompt (None = no change)
    pub system_prompt: Option<String>,
    /// New model provider ID (None = no change)
    pub model_provider_id: Option<String>,
    /// New enabled flag (None = no change)
    pub enabled: Option<bool>,
}
