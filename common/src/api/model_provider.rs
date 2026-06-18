//! Model Provider related API request/response DTOs - shared between backend and frontend

use ai_orz_macros::Params;
use crate::enums::{ModelCapability, ProviderType};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Create Model Provider request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateModelProviderRequest {
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model capability
    pub capability: ModelCapability,
    /// Model name
    pub model_name: String,
    /// API Key
    pub api_key: String,
    /// Custom Base URL
    pub base_url: Option<String>,
    /// Description
    pub description: Option<String>,
}

/// Create Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model name
    pub model_name: String,
    /// Description
    pub description: Option<String>,
    /// Created timestamp
    pub created_at: i64,
}

/// Model Provider list item response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModelProviderListItem {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model name
    pub model_name: String,
    /// Description
    pub description: Option<String>,
    /// Created timestamp
    pub created_at: i64,
}

/// Get Model Provider request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct GetModelProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
}

/// Get Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GetModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model name
    pub model_name: String,
    /// Custom Base URL
    pub base_url: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Created timestamp
    pub created_at: i64,
    /// Updated timestamp
    pub updated_at: i64,
}

/// Update Model Provider request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateModelProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,

    /// New provider name
    pub name: Option<String>,
    /// New provider type
    pub provider_type: Option<ProviderType>,
    /// New model name
    pub model_name: Option<String>,
    /// New API Key
    pub api_key: Option<String>,
    /// New custom Base URL
    pub base_url: Option<String>,
    /// New description
    pub description: Option<String>,
}

/// Update Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: ProviderType,
    /// Model name
    pub model_name: String,
    /// Custom Base URL
    pub base_url: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Updated timestamp
    pub updated_at: i64,
}

/// Test connection request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct TestConnectionRequest {
    /// Optional test prompt
    pub prompt: Option<String>,
}

/// Test connection response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TestConnectionResponse {
    /// Whether the test succeeded
    pub success: bool,
    /// Model response (on success)
    pub response: Option<String>,
    /// Error message (on failure)
    pub error: Option<String>,
}

/// Test Model Provider connection request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct TestModelProviderConnectionRequest {
    /// Provider ID to test
    #[param(source = "path")]
    pub id: String,
    /// Optional test prompt
    pub prompt: Option<String>,
}

/// Call model request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CallModelRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
    /// Prompt to call
    pub prompt: String,
}

/// Call model response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CallModelResponse {
    /// Generated result
    pub result: String,
}

/// Delete Model Provider request
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DeleteModelProviderRequest {
    /// Provider ID
    #[param(source = "path")]
    pub id: String,
}

/// Delete Model Provider response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeleteModelProviderResponse {
    /// Whether deletion succeeded
    pub success: bool,
}

/// List Model Providers request
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListModelProvidersRequest {
    // No parameters needed - all providers are returned
}

/// List Model Providers response
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListModelProvidersResponse {
    /// List of all model providers
    pub providers: Vec<ModelProviderListItem>,
}

/// Test Model Provider connectivity response (alias for frontend compatibility)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TestModelProviderConnectionResponse {
    /// Whether the test succeeded
    pub success: bool,
    /// Test result message
    pub message: String,
    /// Model result (on success)
    pub result: Option<String>,
}