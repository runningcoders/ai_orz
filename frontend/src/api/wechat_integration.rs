//! 微信集成（finance domain：身份凭证资产 + 扫码登录）API 客户端
//!
//! 对应后端 `/api/v1/finance/identity/wechat/` 路由组：
//! 状态聚合 / 扫码登录（二维码获取 + 状态长轮询，confirmed 时凭据自动落库）。

use common::api::{
    WechatIntegrationStatusResponse, WechatLoginQrcodeRequest, WechatLoginQrcodeResponse,
    WechatLoginStatusResponse,
};

use super::{ApiError, api_get_or_default, api_post};

const BASE: &str = "/api/v1/finance/identity/wechat";

// ===== 状态聚合 =====

/// 获取当前用户微信集成绑定快照（iLink 凭证列表）
pub async fn get_wechat_integration_status() -> Result<WechatIntegrationStatusResponse, ApiError> {
    api_get_or_default(&format!("{}/status", BASE)).await
}

// ===== 扫码登录 =====

/// 获取 iLink 登录二维码（`qrcode` 为轮询标识，`qrcode_img_content` 为渲染内容）
pub async fn get_wechat_login_qrcode() -> Result<WechatLoginQrcodeResponse, ApiError> {
    api_post(&format!("{}/qrcode", BASE), &WechatLoginQrcodeRequest {}).await
}

/// 轮询 iLink 二维码状态（服务端 hold ~35s 属正常长轮询语义）
///
/// - `verify_code`：`need_verifycode` 场景下手机微信显示的配对码。**被服务端接受前
///   每轮都要带**（服务端回 `scaned` 即表示已接受，调用方可清空）；
/// - `redirect_host`：上一次 `scaned_but_redirect` 返回的接入点裸主机名，
///   回传后后端以 `https://{host}` 作为本次轮询的接入点。
pub async fn poll_wechat_login_status(
    qrcode: &str,
    verify_code: Option<&str>,
    redirect_host: Option<&str>,
) -> Result<WechatLoginStatusResponse, ApiError> {
    let mut url = format!("{}/qrcode/status?qrcode={}", BASE, percent_encode(qrcode));
    if let Some(code) = verify_code.filter(|c| !c.trim().is_empty()) {
        url.push_str("&verify_code=");
        url.push_str(&percent_encode(code));
    }
    if let Some(host) = redirect_host.filter(|h| !h.trim().is_empty()) {
        url.push_str("&redirect_host=");
        url.push_str(&percent_encode(host));
    }
    api_get_or_default(&url).await
}

/// 最小 percent-encode（RFC 3986 unreserved 之外全部转义；用于 query 参数）
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::percent_encode;

    #[test]
    fn test_percent_encode_unreserved_kept() {
        assert_eq!(percent_encode("abcXYZ019-_.~"), "abcXYZ019-_.~");
    }

    #[test]
    fn test_percent_encode_base64_chars() {
        assert_eq!(percent_encode("a+b/c="), "a%2Bb%2Fc%3D");
    }
}
