//! 微信 iLink（ClawBot）扫码登录协议客户端（pkg 层配置面基建，与 lark_integration 平行）
//!
//! 登录协议（两接口，均 GET，无需 bot_token）：
//! - `GET {base}/ilink/bot/get_bot_qrcode?bot_type=3`
//!   → `{ qrcode, qrcode_img_content, ... }`（qrcode 为轮询标识，img_content 为渲染内容）
//! - `GET {base}/ilink/bot/get_qrcode_status?qrcode=<urlencoded>`
//!   （header `iLink-App-ClientVersion: 1`；服务端 hold ~35s 长轮询）
//!   → `{ status: wait|scaned|expired|confirmed, ... }`
//!   confirmed 时返回 `bot_token` / `ilink_bot_id` / `ilink_user_id` / `baseurl`
//!
//! 协议来源：`@tencent-weixin/openclaw-weixin` 插件源码 + 社区整理，
//! 详见 docs/design/wechat_channel_integration_design.md §5.1/§5.2。
//! 协议字段如有变更，只需修改本文件。

use common::error::{Result, err};
use serde::Deserialize;
use std::sync::OnceLock;

/// iLink 接入域默认值（登录响应 `baseurl` 优先，不硬编码假设）
pub const ILINK_DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

/// bot 类型（插件源码常量；微信 ClawBot 固定 "3"）
const BOT_TYPE: &str = "3";

/// 二维码状态长轮询：服务端 hold ~35s，客户端超时必须大于它
const QR_POLL_TIMEOUT_MS: u64 = 45_000;

// ==================== 数据结构 ====================

/// 登录二维码
#[derive(Debug, Clone)]
pub struct IlinkQrCode {
    /// 轮询标识（`get_qrcode_status` 的 `qrcode` 参数，非二维码渲染内容）
    pub qrcode: String,
    /// 二维码内容（前端渲染用；也可在浏览器直接打开）
    pub qrcode_img_content: String,
}

/// 二维码状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IlinkQrStatusKind {
    /// 等待扫码
    Wait,
    /// 已扫码，等待手机确认
    Scaned,
    /// 二维码过期（调用方负责换新码重试）
    Expired,
    /// 确认登录（confirmed 字段携带凭据）
    Confirmed,
}

/// confirmed 状态的登录凭据
#[derive(Debug, Clone)]
pub struct IlinkQrConfirmed {
    pub bot_token: String,
    pub bot_id: String,
    /// 部分登录响应未返回
    pub user_id: Option<String>,
    /// 接入域（响应缺省时回落 [`ILINK_DEFAULT_BASE_URL`]）
    pub base_url: String,
}

/// 长轮询单次返回
#[derive(Debug, Clone)]
pub struct IlinkQrStatus {
    pub status: IlinkQrStatusKind,
    /// 仅 status == Confirmed 时存在
    pub confirmed: Option<IlinkQrConfirmed>,
}

// ==================== 协议客户端 ====================

/// 共享出站客户端（登录为低频配置面操作，无需每调用新建连接池）
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // 长轮询专用：45s > 服务端 hold 35s；两接口共用（宽松超时对即取接口无影响）
        crate::pkg::http::presets::with_timeout_ms(QR_POLL_TIMEOUT_MS)
            .and_then(|opts| opts.build())
            .expect("构建 iLink HTTP 客户端失败")
    })
}

fn from_http(op: &str, e: reqwest::Error) -> common::error::Error {
    err!(ThirdPartyError, "ilink {} http error: {}", op, e)
}

/// 获取登录二维码
pub async fn get_login_qrcode() -> Result<IlinkQrCode> {
    let url = format!("{ILINK_DEFAULT_BASE_URL}/ilink/bot/get_bot_qrcode");
    let resp = client()
        .get(&url)
        .query(&[("bot_type", BOT_TYPE)])
        .send()
        .await
        .map_err(|e| from_http("get_bot_qrcode", e))?
        .error_for_status()
        .map_err(|e| from_http("get_bot_qrcode", e))?;

    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        qrcode: String,
        #[serde(default)]
        qrcode_img_content: String,
    }
    let raw: Raw = resp
        .json()
        .await
        .map_err(|e| from_http("get_bot_qrcode", e))?;
    if raw.qrcode.is_empty() || raw.qrcode_img_content.is_empty() {
        return Err(err!(
            ThirdPartyError,
            "iLink 返回的登录二维码缺少 qrcode / qrcode_img_content 字段"
        ));
    }
    Ok(IlinkQrCode {
        qrcode: raw.qrcode,
        qrcode_img_content: raw.qrcode_img_content,
    })
}

/// 轮询二维码状态（长轮询单次调用；服务端无新事件时 hold 至超时返回 Wait）
pub async fn poll_qrcode_status(qrcode: &str) -> Result<IlinkQrStatus> {
    let url = format!("{ILINK_DEFAULT_BASE_URL}/ilink/bot/get_qrcode_status");
    let resp = client()
        .get(&url)
        .query(&[("qrcode", qrcode)])
        .header("iLink-App-ClientVersion", "1")
        .send()
        .await;

    let body = match resp {
        Ok(r) => match r.error_for_status() {
            Ok(r) => r.text().await.map_err(|e| from_http("qr_status", e))?,
            Err(e) if e.is_timeout() => {
                // 服务端 hold 到期无事件：客户端先超时是长轮询常态，等价 Wait
                return Ok(wait());
            }
            Err(e) => return Err(from_http("qr_status", e)),
        },
        // 连接层超时同样视为本轮无事件
        Err(e) if e.is_timeout() => return Ok(wait()),
        Err(e) => return Err(from_http("qr_status", e)),
    };

    parse_qr_status(&body)
}

fn wait() -> IlinkQrStatus {
    IlinkQrStatus {
        status: IlinkQrStatusKind::Wait,
        confirmed: None,
    }
}

/// 解析状态长轮询响应（抽纯函数便于单测）
fn parse_qr_status(body: &str) -> Result<IlinkQrStatus> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        status: String,
        #[serde(default)]
        bot_token: Option<String>,
        #[serde(default)]
        ilink_bot_id: Option<String>,
        #[serde(default)]
        ilink_user_id: Option<String>,
        #[serde(default)]
        baseurl: Option<String>,
    }
    let raw: Raw = serde_json::from_str(body)?;

    let kind = match raw.status.as_str() {
        "wait" => IlinkQrStatusKind::Wait,
        "scaned" => IlinkQrStatusKind::Scaned,
        "expired" => IlinkQrStatusKind::Expired,
        "confirmed" => IlinkQrStatusKind::Confirmed,
        // 协议较新：未知状态宽容为 Wait（调用方按既定节奏继续轮询），避免死循环式报错
        _ => IlinkQrStatusKind::Wait,
    };

    let confirmed = if kind == IlinkQrStatusKind::Confirmed {
        let bot_token = raw.bot_token.unwrap_or_default();
        let bot_id = raw.ilink_bot_id.unwrap_or_default();
        if bot_token.is_empty() || bot_id.is_empty() {
            return Err(err!(
                ThirdPartyError,
                "iLink 登录确认但未返回 bot_token / ilink_bot_id"
            ));
        }
        Some(IlinkQrConfirmed {
            bot_token,
            bot_id,
            user_id: raw.ilink_user_id.filter(|s| !s.is_empty()),
            base_url: raw
                .baseurl
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| ILINK_DEFAULT_BASE_URL.to_string()),
        })
    } else {
        None
    };

    Ok(IlinkQrStatus {
        status: kind,
        confirmed,
    })
}

// ==================== 单测 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qr_status_wait_scaned_expired() {
        for (body, expected) in [
            (r#"{"status":"wait"}"#, IlinkQrStatusKind::Wait),
            (r#"{"status":"scaned"}"#, IlinkQrStatusKind::Scaned),
            (r#"{"status":"expired"}"#, IlinkQrStatusKind::Expired),
        ] {
            let parsed = parse_qr_status(body).unwrap();
            assert_eq!(parsed.status, expected);
            assert!(parsed.confirmed.is_none());
        }
    }

    #[test]
    fn test_parse_qr_status_confirmed_full() {
        let parsed = parse_qr_status(
            r#"{"status":"confirmed","bot_token":"tk","ilink_bot_id":"b1","ilink_user_id":"u1","baseurl":"https://alt.example.com"}"#,
        )
        .unwrap();
        let c = parsed.confirmed.expect("confirmed 应携带凭据");
        assert_eq!(c.bot_token, "tk");
        assert_eq!(c.bot_id, "b1");
        assert_eq!(c.user_id.as_deref(), Some("u1"));
        assert_eq!(c.base_url, "https://alt.example.com");
    }

    #[test]
    fn test_parse_qr_status_confirmed_defaults() {
        // user_id / baseurl 缺省：user_id=None，base_url 回落默认接入域
        let parsed =
            parse_qr_status(r#"{"status":"confirmed","bot_token":"tk","ilink_bot_id":"b1"}"#)
                .unwrap();
        let c = parsed.confirmed.expect("confirmed 应携带凭据");
        assert_eq!(c.user_id, None);
        assert_eq!(c.base_url, ILINK_DEFAULT_BASE_URL);
    }

    #[test]
    fn test_parse_qr_status_confirmed_missing_credentials() {
        // confirmed 但缺 token / bot_id：报错而非静默
        assert!(parse_qr_status(r#"{"status":"confirmed"}"#).is_err());
        assert!(parse_qr_status(r#"{"status":"confirmed","bot_token":"tk"}"#).is_err());
    }

    #[test]
    fn test_parse_qr_status_unknown_is_wait() {
        // 未知状态宽容为 Wait（协议较新，避免死循环式报错）
        let parsed = parse_qr_status(r#"{"status":"some_new_state"}"#).unwrap();
        assert_eq!(parsed.status, IlinkQrStatusKind::Wait);
    }
}
