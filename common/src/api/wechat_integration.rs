//! 微信 iLink 集成（用户级）API DTO - 前后端共享
//!
//! 路由统一挂 `/api/v1/finance/identity/wechat/`：
//! - 扫码登录（二维码获取 + 状态长轮询，confirmed 时自动落库 `wechat_ilink` 凭据）
//!
//! 凭据结构见 common::models::identity_credentials（`CredentialDetail::WechatIlink`）。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ==================== 扫码登录 ====================

/// 获取 iLink 登录二维码请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct WechatLoginQrcodeRequest {}

/// 获取 iLink 登录二维码响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WechatLoginQrcodeResponse {
    /// 轮询标识（status 接口的 `qrcode` 参数，非二维码渲染内容）
    pub qrcode: String,
    /// 二维码内容（前端渲染用；亦可在浏览器直接打开）
    pub qrcode_img_content: String,
}

/// 轮询 iLink 二维码状态请求（长轮询：服务端 hold ~35s 属正常现象）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct WechatLoginStatusRequest {
    /// 获取二维码时返回的轮询标识
    #[param(source = "query")]
    pub qrcode: String,
}

/// 轮询 iLink 二维码状态响应
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WechatLoginStatusResponse {
    /// wait / scaned / expired / confirmed
    pub status: String,
    /// 凭据 ID（仅 confirmed；渠道以 credential_id 引用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    /// iLink bot 标识（仅 confirmed）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    /// true = 该用户已有 iLink 凭据并完成整组轮换（仅 confirmed）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotated: Option<bool>,
}

// ==================== 状态聚合 ====================

/// 微信集成状态聚合请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct WechatIntegrationStatusRequest {}

/// 微信集成状态聚合响应
///
/// 与 lark/github 的 `GET /status` 同构：按凭证类型分组返回当前用户绑定快照。
/// 未来微信侧新增凭据类型（如企微应用）时在此扩展新的分组字段。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WechatIntegrationStatusResponse {
    /// 当前用户已绑定的 iLink 凭证（bot_token 永不回显）
    pub credentials: Vec<WechatCredentialSnapshot>,
}

/// 单个微信 iLink 凭证快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WechatCredentialSnapshot {
    /// 凭证 ID（渠道以 credential_id 引用）
    pub credential_id: String,
    /// 凭证名称
    pub name: String,
    /// iLink bot 标识
    pub bot_id: String,
    /// 是否为该用户微信 iLink 类默认凭证
    #[serde(default)]
    pub is_default: bool,
}
