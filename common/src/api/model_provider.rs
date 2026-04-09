//! Model Provider related API request/response DTOs - shared between backend and frontend

use serde::{Deserialize, Serialize};

/// Provider type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderType {
    /// OpenAI compatible API (OpenAI itself, Azure, custom, etc.)
    OpenAI = 1,
    /// DeepSeek API
    DeepSeek = 2,
    /// Tongyi Qianwen (Alibaba Cloud)
    TongyiQianwen = 3,
    /// Doubao (ByteDance)
    Doubao = 4,
    /// Ollama (local)
    Ollama = 5,
    /// Claude (Anthropic)
    Claude = 6,
    /// Gemini (Google)
    Gemini = 7,
}

impl ProviderType {
    /// Convert from i32
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::OpenAI),
            2 => Some(Self::DeepSeek),
            3 => Some(Self::TongyiQianwen),
            4 => Some(Self::Doubao),
            5 => Some(Self::Ollama),
            6 => Some(Self::Claude),
            7 => Some(Self::Gemini),
            _ => None,
        }
    }

    /// Get display name
    pub fn display_name(self) -> &'static str {
        match self {
            Self::OpenAI => "OpenAI",
            Self::DeepSeek => "DeepSeek",
            Self::TongyiQianwen => "通义千问",
            Self::Doubao => "豆包",
            Self::Ollama => "Ollama",
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
        }
    }
}

/// Model provider list item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderListItem {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: i32,
    /// Provider type name for display
    pub provider_type_name: String,
    /// Base URL for API
    pub base_url: String,
    /// Whether this provider is enabled
    pub enabled: bool,
    /// Creation timestamp
    pub created_at: i64,
}

/// List model providers response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListModelProvidersResponse {
    /// List of providers
    pub data: Vec<ModelProviderListItem>,
    /// Total count
    pub total: u64,
}

/// Get model provider detail response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelProviderResponse {
    /// Provider detail data
    pub data: ModelProviderDetail,
}

/// Detailed model provider information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderDetail {
    /// Provider ID
    pub id: String,
    /// Provider name
    pub name: String,
    /// Provider type
    pub provider_type: i32,
    /// Base URL for API
    pub base_url: String,
    /// API key (masked for security, only shows first few chars)
    pub api_key_masked: String,
    /// Default model name to use
    pub default_model: String,
    /// Whether this provider is enabled
    pub enabled: bool,
    /// Creation timestamp
    pub created_at: i64,
    /// Update timestamp
    pub updated_at: i64,
}

/// Create model provider request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateModelProviderRequest {
    /// Provider name
    pub name: String,
    /// Provider type (as integer)
    pub provider_type: i32,
    /// Base URL for API
    pub base_url: String,
    /// API key
    pub api_key: String,
    /// Default model name
    pub default_model: String,
    /// Whether enabled
    pub enabled: bool,
}

/// Update model provider request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateModelProviderRequest {
    /// New name (None = no change)
    pub name: Option<String>,
    /// New provider type (None = no change)
    pub provider_type: Option<i32>,
    /// New base URL (None = no change)
    pub base_url: Option<String>,
    /// New API key (None = no change, empty doesn't clear)
    pub api_key: Option<String>,
    /// New default model (None = no change)
    pub default_model: Option<String>,
    /// New enabled flag (None = no change)
    pub enabled: Option<bool>,
}

/// Test connection request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestConnectionRequest {
    /// Optional test prompt (uses default if not provided)
    pub prompt: Option<String>,
}

/// Test connection response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConnectionResponse {
    /// Test success flag
    pub success: bool,
    /// Response from model (if successful)
    pub response: Option<String>,
    /// Error message (if failed)
    pub error: Option<String>,
}

/// Generic model call request
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GenericCallRequest {
    /// Prompt text to send
    pub prompt: String,
}

/// Generic model call response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericCallResponse {
    /// Completion text from model
    pub completion: String,
}
