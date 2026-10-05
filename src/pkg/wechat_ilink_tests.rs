//! tests 单元测试（拆分自 wechat_ilink.rs）
//!
//! 文件瘦身：原 684 行 → 524 行，测试体 161 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn test_parse_qr_status_all_eight_kinds() {
    for (wire, expected) in [
        (QR_STATUS_WAIT, IlinkQrStatusKind::Wait),
        (QR_STATUS_SCANED, IlinkQrStatusKind::Scaned),
        (QR_STATUS_EXPIRED, IlinkQrStatusKind::Expired),
        (QR_STATUS_CONFIRMED, IlinkQrStatusKind::Confirmed),
        (
            QR_STATUS_SCANED_BUT_REDIRECT,
            IlinkQrStatusKind::ScanedButRedirect,
        ),
        (QR_STATUS_NEED_VERIFYCODE, IlinkQrStatusKind::NeedVerifyCode),
        (
            QR_STATUS_VERIFY_CODE_BLOCKED,
            IlinkQrStatusKind::VerifyCodeBlocked,
        ),
        (QR_STATUS_BINDED_REDIRECT, IlinkQrStatusKind::BindedRedirect),
    ] {
        // confirmed 需带凭据，单独构造
        let body = if expected == IlinkQrStatusKind::Confirmed {
            format!(r#"{{"status":"{wire}","bot_token":"tk","ilink_bot_id":"b1"}}"#)
        } else {
            format!(r#"{{"status":"{wire}"}}"#)
        };
        let parsed = parse_qr_status(&body).unwrap();
        assert_eq!(parsed.status, expected, "wire={wire}");
        // 状态字面量往返一致（出站 DTO / 日志口径 SSOT）
        assert_eq!(parsed.status.as_str(), wire);
        if expected != IlinkQrStatusKind::Confirmed {
            assert!(parsed.confirmed.is_none(), "wire={wire}");
        }
    }
}

#[test]
fn test_parse_qr_status_terminal_kinds() {
    assert!(IlinkQrStatusKind::Confirmed.is_terminal());
    assert!(IlinkQrStatusKind::BindedRedirect.is_terminal());
    for kind in [
        IlinkQrStatusKind::Wait,
        IlinkQrStatusKind::Scaned,
        IlinkQrStatusKind::Expired,
        IlinkQrStatusKind::ScanedButRedirect,
        IlinkQrStatusKind::NeedVerifyCode,
        IlinkQrStatusKind::VerifyCodeBlocked,
    ] {
        assert!(!kind.is_terminal(), "{kind}");
    }
}

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
        assert!(parsed.redirect_host.is_none());
    }
}

#[test]
fn test_parse_qr_status_redirect_host_only_on_redirect() {
    // scaned_but_redirect：带出 redirect_host
    let parsed =
        parse_qr_status(r#"{"status":"scaned_but_redirect","redirect_host":"idc2.weixin.qq.com"}"#)
            .unwrap();
    assert_eq!(parsed.status, IlinkQrStatusKind::ScanedButRedirect);
    assert_eq!(parsed.redirect_host.as_deref(), Some("idc2.weixin.qq.com"));

    // 非重定向态即使响应里带了 redirect_host 也不透出（避免调用方误切换接入点）
    let parsed =
        parse_qr_status(r#"{"status":"scaned","redirect_host":"idc2.weixin.qq.com"}"#).unwrap();
    assert!(parsed.redirect_host.is_none());

    // 重定向态但缺字段：None（调用方沿用当前接入点）
    let parsed = parse_qr_status(r#"{"status":"scaned_but_redirect"}"#).unwrap();
    assert!(parsed.redirect_host.is_none());
}

#[test]
fn test_sanitize_redirect_host_accepts_bare_tencent_host() {
    assert_eq!(
        sanitize_redirect_host("  IDC2.Weixin.QQ.com ").as_deref(),
        Some("idc2.weixin.qq.com")
    );
    assert_eq!(
        sanitize_redirect_host("ilink-bj.qq.com").as_deref(),
        Some("ilink-bj.qq.com")
    );
}

#[test]
fn test_sanitize_redirect_host_rejects_non_tencent_or_dirty() {
    // 这四种都是"把出站目标交出去"的形态，必须拒绝
    for bad in [
        "",                               // 空
        "evil.example.com",               // 非腾讯域
        "weixin.qq.com.evil.example.com", // 后缀伪装
        "https://idc2.weixin.qq.com",     // 带 scheme
        "idc2.weixin.qq.com:8443",        // 带端口
        "idc2.weixin.qq.com/ilink",       // 带路径
        "user@idc2.weixin.qq.com",        // 带 userinfo
        "127.0.0.1",                      // 直连 IP
        "qq.com",                         // 裸后缀不算子域
    ] {
        assert!(sanitize_redirect_host(bad).is_none(), "bad={bad}");
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
fn test_parse_qr_status_binded_redirect_has_no_credentials() {
    // binded_redirect 是幂等成功，不携带任何凭据
    let parsed = parse_qr_status(r#"{"status":"binded_redirect"}"#).unwrap();
    assert_eq!(parsed.status, IlinkQrStatusKind::BindedRedirect);
    assert!(parsed.confirmed.is_none());
}

#[test]
fn test_parse_qr_status_confirmed_defaults() {
    // user_id / baseurl 缺省：user_id=None，base_url 回落默认接入域
    let parsed =
        parse_qr_status(r#"{"status":"confirmed","bot_token":"tk","ilink_bot_id":"b1"}"#).unwrap();
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
