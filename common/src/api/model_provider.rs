//! Model Provider related API request/response DTOs - shared between backend and frontend

use crate::constants::ProviderType;
use serde::{Deserialize, Serialize};

/// 创建 Model Provider 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateModelProviderRequest {
    /// Provider 名称
    pub name: String,
    /// Provider 类型
    pub provider_type: ProviderType,
    /// 模型名称
    pub model_name: String,
    /// API Key
    pub api_key: String,
    /// 自定义 Base URL
    pub base_url: Option<String>,
    /// 描述
    pub description: Option<String>,
}

/// 创建 Model Provider 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider 名称
    pub name: String,
    /// Provider 类型
    pub provider_type: ProviderType,
    /// 模型名称
    pub model_name: String,
    /// 描述
    pub description: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
}

/// Model Provider 列表项响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderListItem {
    /// Provider ID
    pub id: String,
    /// Provider 名称
    pub name: String,
    /// Provider 类型
    pub provider_type: ProviderType,
    /// 模型名称
    pub model_name: String,
    /// 描述
    pub description: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
}

/// 获取 Model Provider 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider 名称
    pub name: String,
    /// Provider 类型
    pub provider_type: ProviderType,
    /// 模型名称
    pub model_name: String,
    /// 自定义 Base URL
    pub base_url: Option<String>,
    /// 描述
    pub description: Option<String>,
    /// 创建时间戳
    pub created_at: i64,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 更新 Model Provider 请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateModelProviderRequest {
    /// Provider 名称
    pub name: Option<String>,
    /// Provider 类型
    pub provider_type: Option<ProviderType>,
    /// 模型名称
    pub model_name: Option<String>,
    /// API Key
    pub api_key: Option<String>,
    /// 自定义 Base URL
    pub base_url: Option<String>,
    /// 描述
    pub description: Option<String>,
}

/// 更新 Model Provider 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateModelProviderResponse {
    /// Provider ID
    pub id: String,
    /// Provider 名称
    pub name: String,
    /// Provider 类型
    pub provider_type: ProviderType,
    /// 模型名称
    pub model_name: String,
    /// 自定义 Base URL
    pub base_url: Option<String>,
    /// 描述
    pub description: Option<String>,
    /// 更新时间戳
    pub updated_at: i64,
}

/// 测试连接请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TestConnectionRequest {
    /// 可选的测试提示词
    pub prompt: Option<String>,
}

/// 测试连接响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConnectionResponse {
    /// 测试是否成功
    pub success: bool,
    /// 模型响应（成功时）
    pub response: Option<String>,
    /// 错误信息（失败时）
    pub error: Option<String>,
}

/// 调用模型请求
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CallModelRequest {
    /// 调用提示词
    pub prompt: String,
}

/// 调用模型响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallModelResponse {
    /// 生成结果
    pub result: String,
}

/// 删除 Model Provider 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteModelProviderResponse {
    /// 是否删除成功
    pub success: bool,
}

/// 测试 Model Provider 连通性响应（别名兼容前端命名）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestModelProviderConnectionResponse {
    /// 测试是否成功
    pub success: bool,
    /// 测试结果信息
    pub message: String,
    /// 模型响应结果（成功时）
    pub result: Option<String>,
}
