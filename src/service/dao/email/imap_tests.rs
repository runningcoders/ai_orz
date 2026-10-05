//! tests 单元测试（拆分自 imap.rs）
//!
//! 文件瘦身：原 932 行 → 709 行，测试体 224 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use common::models::CredentialDetail;

fn credential_row(
    kind: common::models::CredentialKind,
    detail: CredentialDetail,
) -> UserCredentialPo {
    UserCredentialPo::new(
        "cred_1".to_string(),
        "org_1".to_string(),
        "user_1".to_string(),
        kind,
        "邮箱机器人".to_string(),
        detail,
        common::models::CredentialVisibility::Private,
        "user_1".to_string(),
    )
}

fn email_detail() -> CredentialDetail {
    // 明文直存：decrypt_channel_secret 对无 enc:v1: 前缀的值透传（不依赖 master_key 配置）
    CredentialDetail::EmailBot {
        email_address: "bot@qq.com".to_string(),
        smtp_host: "smtp.qq.com".to_string(),
        smtp_port: 465,
        imap_host: "imap.qq.com".to_string(),
        imap_port: 993,
        username: "bot@qq.com".to_string(),
        password: "authcode123".to_string(),
    }
}

/// 凭证解析：kind 校验 + 授权码解密透传 + IMAP 参数完整性
#[test]
fn test_resolve_imap_credentials() {
    let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
    let resolved = resolve_imap_credentials(&row).unwrap();
    assert_eq!(resolved.credential_id, "cred_1");
    assert_eq!(resolved.email_address, "bot@qq.com");
    assert_eq!(resolved.imap_host, "imap.qq.com");
    assert_eq!(resolved.imap_port, 993);
    assert_eq!(resolved.username, "bot@qq.com");
    assert_eq!(resolved.password, "authcode123");

    // kind 不匹配：报错
    let row = credential_row(
        common::models::CredentialKind::GithubToken,
        CredentialDetail::GithubToken {
            token: "tok".to_string(),
        },
    );
    assert!(resolve_imap_credentials(&row).is_err());

    // IMAP 主机缺失：报错
    let bad = CredentialDetail::EmailBot {
        email_address: "bot@qq.com".to_string(),
        smtp_host: "smtp.qq.com".to_string(),
        smtp_port: 465,
        imap_host: String::new(),
        imap_port: 993,
        username: "bot@qq.com".to_string(),
        password: "authcode123".to_string(),
    };
    let row = credential_row(common::models::CredentialKind::EmailBot, bad);
    assert!(resolve_imap_credentials(&row).is_err());
}

/// Debug 掩码：授权码不得以明文出现在日志形态中
#[test]
fn test_imap_credentials_debug_masks_password() {
    let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
    let resolved = resolve_imap_credentials(&row).unwrap();
    let debug = format!("{:?}", resolved);
    assert!(!debug.contains("authcode123"));
    assert!(debug.contains("***"));
}

/// 凭证指纹：同参相等，任一参数变化 → 不同（ensure 停旧重建依据）
#[test]
fn test_fingerprint_changes_on_any_credential_field() {
    let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
    let base = resolve_imap_credentials(&row).unwrap();
    let same = base.clone();
    assert_eq!(base.fingerprint(), same.fingerprint());

    let rotated = EmailImapCredentials {
        password: "new_authcode".to_string(),
        ..base.clone()
    };
    assert_ne!(base.fingerprint(), rotated.fingerprint());

    let moved = EmailImapCredentials {
        imap_port: 143,
        ..base.clone()
    };
    assert_ne!(base.fingerprint(), moved.fingerprint());
}

/// MIME 解析：text/plain 单部件邮件（Message-ID / From 规范化 / 主题 / QP 解码正文 + CRLF 规范化）
#[test]
fn test_parse_simple_plain_mail() {
    let raw = b"Message-ID: <abc@example.com>\r\n\
                From: Zhang San <Zhang.San@Example.COM>\r\n\
                To: bot@qq.com\r\n\
                Subject: =?UTF-8?B?5L2g5aW9?=\r\n\
                Content-Type: text/plain; charset=utf-8\r\n\
                Content-Transfer-Encoding: quoted-printable\r\n\
                \r\n\
    ";
    // 合法 QP 体：你好\r\n正文第二行\r\n（注意：QP Robust 解码会静默剥离裸非 ASCII 字节，体必须真编码）
    let qp_body = "=E4=BD=A0=E5=A5=BD\r\n=E6=AD=A3=E6=96=87=E7=AC=AC=E4=BA=8C=E8=A1=8C\r\n";
    let with_body = [raw.as_slice(), qp_body.as_bytes()].concat();
    let parsed = parse_mail(&with_body).unwrap();
    assert_eq!(
        extract_message_key(&parsed, &with_body),
        "<abc@example.com>"
    );
    assert_eq!(extract_from(&parsed), "zhang.san@example.com");
    assert_eq!(extract_subject(&parsed), "你好");
    assert_eq!(extract_text_content(&parsed), "你好\n正文第二行");
}

/// Message-ID 缺失：以报文 sha256 兜底（跨重启稳定）
#[test]
fn test_message_key_fallback_hash() {
    let raw = b"From: a@b.com\r\nSubject: hi\r\n\r\nbody";
    let parsed = parse_mail(raw).unwrap();
    let key = extract_message_key(&parsed, raw);
    assert!(key.starts_with("no-id-"));
    let again = extract_message_key(&parsed, raw);
    assert_eq!(key, again);
}

/// From 缺失 / 不可解析：回落安全值（不可能命中渠道，消费侧丢弃）
#[test]
fn test_from_fallback() {
    let raw = b"Subject: no from\r\n\r\nbody";
    let parsed = parse_mail(raw).unwrap();
    assert_eq!(extract_from(&parsed), "unknown@unknown.invalid");
}

/// multipart 混合邮件：text/plain 优先，text/html 不参与
#[test]
fn test_multipart_prefers_plain() {
    let raw = b"From: a@b.com\r\n\
                Subject: mixed\r\n\
                MIME-Version: 1.0\r\n\
                Content-Type: multipart/alternative; boundary=BOUND\r\n\
                \r\n\
                --BOUND\r\n\
                Content-Type: text/html; charset=utf-8\r\n\
                \r\n\
                <p>HTML <b>version</b></p>\r\n\
                --BOUND\r\n\
                Content-Type: text/plain; charset=utf-8\r\n\
                \r\n\
                PLAIN text\r\n\
                --BOUND--\r\n\
            ";
    let parsed = parse_mail(raw).unwrap();
    assert_eq!(extract_text_content(&parsed), "PLAIN text");
}

/// 纯 HTML 邮件：剥标签降级（script/style 去除、块级标签转换行、实体解码）
#[test]
fn test_html_fallback_stripped() {
    let raw = b"From: a@b.com\r\n\
                Subject: html only\r\n\
                Content-Type: text/html; charset=utf-8\r\n\
                \r\n\
                <html><head><style>p{}</style></head>\
                <body><p>Hello &amp; <b>World</b></p><p>Line2</p></body></html>\r\n\
            ";
    let parsed = parse_mail(raw).unwrap();
    let content = extract_text_content(&parsed);
    assert_eq!(content, "Hello & World\nLine2");
}

/// 正文组装：主题 + 空行 + 正文；仅主题 / 仅正文的边界
#[test]
fn test_parse_inbound_event_content_composition() {
    let credentials = EmailImapCredentials {
        credential_id: "cred_1".to_string(),
        email_address: "bot@qq.com".to_string(),
        imap_host: "imap.qq.com".to_string(),
        imap_port: 993,
        username: "bot@qq.com".to_string(),
        password: "authcode123".to_string(),
    };

    // 主题 + 正文
    let raw = b"Message-ID: <m1@x>\r\nFrom: peer@x.com\r\nSubject: Title\r\n\r\nBody text\r\n";
    let event = parse_inbound_event(&credentials, 7, raw).unwrap();
    assert_eq!(event.credential_id, "cred_1");
    assert_eq!(event.from, "peer@x.com");
    assert_eq!(event.message_key, "<m1@x>");
    assert_eq!(event.subject, "Title");
    assert_eq!(event.content, "Title\n\nBody text");
    assert_eq!(event.uid, 7);

    // 仅正文（无主题头）
    let raw = b"From: peer@x.com\r\n\r\nJust body\r\n";
    let event = parse_inbound_event(&credentials, 8, raw).unwrap();
    assert_eq!(event.content, "Just body");

    // 仅主题（空正文）
    let raw = b"From: peer@x.com\r\nSubject: Only title\r\n\r\n   \r\n";
    let event = parse_inbound_event(&credentials, 9, raw).unwrap();
    assert_eq!(event.content, "Only title");
}

/// registry：空态查询 / 停止不存在的循环（幂等 false）
#[tokio::test]
async fn test_registry_empty_state() {
    let registry = ImapPollRegistry::new();
    assert!(!registry.is_running("cred_x").await);
    assert!(!registry.stop("cred_x").await);
}

/// strip_html 边界：无标签纯文本透传
#[test]
fn test_strip_html_plain_passthrough() {
    assert_eq!(strip_html("plain text"), "plain text");
}
