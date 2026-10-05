//! tests 单元测试（拆分自 smtp.rs）
//!
//! 文件瘦身：原 562 行 → 379 行，测试体 184 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::message_channel::{ChannelConfig, MessageChannelPo};
use common::models::CredentialDetail;

fn channel(to_address: Option<&str>) -> MessageChannel {
    MessageChannel::from_po(MessageChannelPo::new(
        "ch_email_1".to_string(),
        "org_1".to_string(),
        "user_1".to_string(),
        None,
        common::enums::ChannelType::Email,
        "我的邮箱".to_string(),
        None,
        None,
        None,
        ChannelConfig {
            email_to_address: to_address.map(|s| s.to_string()),
            ..Default::default()
        },
        "user_1".to_string(),
    ))
}

fn credential_row(
    kind: common::models::CredentialKind,
    detail: CredentialDetail,
) -> crate::models::user_credential::UserCredentialPo {
    crate::models::user_credential::UserCredentialPo::new(
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

/// 凭证解析：kind 校验 + 授权码解密透传 + SMTP 参数完整性
#[test]
fn test_resolve_email_credentials() {
    let ch = channel(Some("peer@example.com"));
    let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
    let resolved = resolve_email_credentials(&row, &ch).unwrap();
    assert_eq!(resolved.email_address, "bot@qq.com");
    assert_eq!(resolved.smtp_host, "smtp.qq.com");
    assert_eq!(resolved.smtp_port, 465);
    assert_eq!(resolved.username, "bot@qq.com");
    assert_eq!(resolved.password, "authcode123");

    // kind 不匹配：报错
    let row = credential_row(
        common::models::CredentialKind::GithubToken,
        CredentialDetail::GithubToken {
            token: "tok".to_string(),
        },
    );
    assert!(resolve_email_credentials(&row, &ch).is_err());

    // SMTP 必备参数缺失（端口 0）：报错
    let bad = CredentialDetail::EmailBot {
        email_address: "bot@qq.com".to_string(),
        smtp_host: "smtp.qq.com".to_string(),
        smtp_port: 0,
        imap_host: "imap.qq.com".to_string(),
        imap_port: 993,
        username: "bot@qq.com".to_string(),
        password: "authcode123".to_string(),
    };
    let row = credential_row(common::models::CredentialKind::EmailBot, bad);
    assert!(resolve_email_credentials(&row, &ch).is_err());
}

/// Debug 掩码：授权码不得以明文出现在日志形态中
#[test]
fn test_credentials_debug_masks_password() {
    let ch = channel(Some("peer@example.com"));
    let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
    let resolved = resolve_email_credentials(&row, &ch).unwrap();
    let debug = format!("{:?}", resolved);
    assert!(!debug.contains("authcode123"));
    assert!(debug.contains("***"));
}

/// 主题生成：取正文首个非空行 / 超长截断带省略号 / 全空白回落固定主题
#[test]
fn test_compose_subject() {
    assert_eq!(compose_subject("任务完成\n正文第二行"), "任务完成");
    assert_eq!(compose_subject("\n\n  缩进行  \n正文"), "缩进行");

    let long = "长".repeat(100);
    let subject = compose_subject(&long);
    assert_eq!(subject.chars().count(), 65); // 64 字符 + 省略号
    assert!(subject.ends_with('…'));

    assert_eq!(compose_subject(""), "AI Orz 消息推送");
    assert_eq!(compose_subject("  \n \n"), "AI Orz 消息推送");
}

/// 对端地址校验：缺失 / 空白报引导性错误，合法值透传
#[test]
fn test_require_to_address() {
    assert_eq!(
        require_to_address(&channel(Some("peer@example.com"))).unwrap(),
        "peer@example.com"
    );
    assert!(require_to_address(&channel(None)).is_err());
    assert!(require_to_address(&channel(Some("  "))).is_err());
}

/// transport 构建：465 走隐式 TLS / 587 走 STARTTLS（仅验证构建不报错，不发网络请求）
#[test]
fn test_build_transport() {
    let ch = channel(Some("peer@example.com"));
    let row = credential_row(common::models::CredentialKind::EmailBot, email_detail());
    let resolved = resolve_email_credentials(&row, &ch).unwrap();
    assert!(build_transport(&resolved).is_ok());

    let starttls = EmailSmtpCredentials {
        smtp_port: 587,
        ..resolved.clone()
    };
    assert!(build_transport(&starttls).is_ok());
}

/// P2：已确认游标 —— 单调不减 / `0` 是"未确认"哨兵（忽略）/ credential 隔离
///
/// 游标是**注入式** [`UidCursorStore`]：每个用例自建实例，天然隔离、无跨用例串扰。
#[test]
fn test_confirmed_uid_is_monotonic_and_scoped() {
    let store = UidCursorStore::new();

    assert_eq!(
        store.confirmed("cred_test_p2_cursor_a"),
        0,
        "未确认时起点为 0"
    );

    store.confirm("cred_test_p2_cursor_a", 10);
    assert_eq!(store.confirmed("cred_test_p2_cursor_a"), 10);

    // 单调不减：乱序回调（较小的 UID 后到）不得回退游标 —— 否则会重复拉取
    store.confirm("cred_test_p2_cursor_a", 3);
    assert_eq!(store.confirmed("cred_test_p2_cursor_a"), 10);

    // 0 = "未确认"哨兵（IMAP UID 从 1 起）→ 忽略
    store.confirm("cred_test_p2_cursor_a", 0);
    assert_eq!(store.confirmed("cred_test_p2_cursor_a"), 10);

    // credential 隔离：不同邮箱互不影响
    assert_eq!(store.confirmed("cred_test_p2_cursor_b"), 0);
}

/// P2：registry 与 DAO 共享**同一份**游标存储（轮询读 / 确认回调写同源）
#[test]
fn test_registry_and_dao_share_cursor_store() {
    let registry = ImapPollRegistry::new();
    let dao_cursors = registry.cursors();
    dao_cursors.confirm("cred_shared", 7);
    assert_eq!(
        dao_cursors.confirmed("cred_shared"),
        7,
        "同一 Arc 内的写入对方立即可见"
    );
    assert_eq!(
        registry.cursors().confirmed("cred_shared"),
        7,
        "registry 侧读取的是同一份游标"
    );
}
