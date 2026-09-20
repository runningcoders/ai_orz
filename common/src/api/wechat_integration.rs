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

/// iLink 扫码状态字面量（官方 8 态）——**前后端共用同一份**
///
/// 这几个值是 iLink 上游协议的字面量，后端原样透传、前端按值分支，所以 SSOT 放在
/// 共享层（后端 `pkg::wechat_ilink::IlinkQrStatusKind` 的 `as_str()` 亦取自这里）。
/// 取值出处：官方插件 `src/auth/login-qr.ts` 的 `StatusResponse["status"]`。
pub const WECHAT_QR_STATUS_WAIT: &str = "wait";
/// 已扫码，等待手机端确认
pub const WECHAT_QR_STATUS_SCANED: &str = "scaned";
/// 已确认授权（响应携带凭据）
pub const WECHAT_QR_STATUS_CONFIRMED: &str = "confirmed";
/// 二维码过期（前端换新码后继续轮询）
pub const WECHAT_QR_STATUS_EXPIRED: &str = "expired";
/// 已扫码但 bot 归属其它 IDC：按响应的 `redirect_host` 换接入点继续轮询
pub const WECHAT_QR_STATUS_SCANED_BUT_REDIRECT: &str = "scaned_but_redirect";
/// 需要配对码（风控场景）：把手机微信显示的数字作为 `verify_code` 回传
pub const WECHAT_QR_STATUS_NEED_VERIFYCODE: &str = "need_verifycode";
/// 配对码连续错误被风控拦截（需换新码）
pub const WECHAT_QR_STATUS_VERIFY_CODE_BLOCKED: &str = "verify_code_blocked";
/// 该 bot 已绑过本客户端（幂等成功：不签发新凭据）
pub const WECHAT_QR_STATUS_BINDED_REDIRECT: &str = "binded_redirect";

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
    /// 配对码（手机微信在 `need_verifycode` 场景下展示的数字）。
    /// **一旦服务端回 `scaned` 表示已接受，调用方即可清空**；接受前每次轮询都要带。
    #[param(source = "query")]
    pub verify_code: Option<String>,
    /// 上一次响应为 `scaned_but_redirect` 时拿到的接入点**裸主机名**。
    /// 回传后本次改为请求 `https://{redirect_host}`；后端只接受落在腾讯接入域内的
    /// 裸主机名（防出站目标被任意指定），不合规时忽略并沿用默认接入点。
    #[param(source = "query")]
    pub redirect_host: Option<String>,
}

/// 轮询 iLink 二维码状态响应
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WechatLoginStatusResponse {
    /// 8 态之一：`wait` / `scaned` / `confirmed` / `expired` /
    /// `scaned_but_redirect` / `need_verifycode` / `verify_code_blocked` / `binded_redirect`
    pub status: String,
    /// 凭据 ID（仅 `confirmed`；渠道以 credential_id 引用）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_id: Option<String>,
    /// iLink bot 标识（仅 `confirmed`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<String>,
    /// true = 该用户已有 iLink 凭据并完成整组轮换（仅 `confirmed`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotated: Option<bool>,
    /// 扫码者标识（bot 侧用户 ID；仅 `confirmed`，登录响应未返回时为 None）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// 本次绑定 / 轮换时间（epoch 毫秒；仅 `confirmed`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bound_at: Option<i64>,
    /// 应切换到的接入点裸主机名（仅 `scaned_but_redirect`）；
    /// 调用方下次轮询把它作为 `redirect_host` 回传即可完成切换
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub redirect_host: Option<String>,
    /// true = 该 bot 已绑过本客户端（仅 `binded_redirect`）：**幂等成功，不新建凭据**，
    /// 前端应按"已授权"处理并展示本地已有凭据
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub already_bound: Option<bool>,
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
    /// 扫码者标识（bot 侧用户 ID；登录响应未返回该字段时为空）
    #[serde(default)]
    pub user_id: Option<String>,
    /// 接入域（凭据落库的 base_url；历史数据可能为空串 → 前端展示默认域）
    #[serde(default)]
    pub base_url: String,
    /// 是否为该用户微信 iLink 类默认凭证
    #[serde(default)]
    pub is_default: bool,
    /// 绑定时间（epoch 毫秒）
    pub created_at: i64,
    /// 最后轮换 / 变更时间（epoch 毫秒）
    pub updated_at: i64,
}
