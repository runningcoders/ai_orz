//! tests 单元测试（拆分自 create_message_channel.rs）
//!
//! 文件瘦身：原 484 行 → 225 行，测试体 260 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use crate::models::user_credential::UserCredentialPo;
use common::api::CreateMessageChannelConfig;
use common::enums::ChannelType;
use common::models::{CredentialDetail, CredentialVisibility};

fn lark_credentials(credential_id: &str) -> Vec<UserCredential> {
    vec![UserCredential::from_po(UserCredentialPo::new(
        credential_id.to_string(),
        "org-1".to_string(),
        "user-1".to_string(),
        CredentialKind::LarkApp,
        "测试凭证".to_string(),
        CredentialDetail::LarkApp {
            app_id: "cli_x".to_string(),
            app_secret: "enc:v1:secret".to_string(),
            encrypt_key: None,
            verification_token: None,
        },
        CredentialVisibility::Private,
        "user-1".to_string(),
    ))]
}

#[test]
fn lark_credential_ref_required_for_lark_type() {
    let credentials = lark_credentials("cred-1");
    assert!(validate_lark_credential_ref(ChannelType::Lark, None, &credentials).is_err());
    assert!(validate_lark_credential_ref(ChannelType::Lark, Some("  "), &credentials).is_err());
    // 引用不存在的凭证
    assert!(
        validate_lark_credential_ref(ChannelType::Lark, Some("missing"), &credentials).is_err()
    );
    // 引用存在的 LarkApp 凭证
    assert!(validate_lark_credential_ref(ChannelType::Lark, Some("cred-1"), &credentials).is_ok());
}

#[test]
fn empty_credentials_rejects_any_ref() {
    let credentials: Vec<UserCredential> = Vec::new();
    assert!(validate_lark_credential_ref(ChannelType::Lark, Some("cred-1"), &credentials).is_err());
}

#[test]
fn non_lark_type_skips_credential_validation() {
    let credentials: Vec<UserCredential> = Vec::new();
    assert!(validate_lark_credential_ref(ChannelType::Webhook, None, &credentials).is_ok());
    assert!(validate_lark_credential_ref(ChannelType::Email, None, &credentials).is_ok());
}

#[test]
fn extract_config_handles_none() {
    let req = CreateMessageChannelRequest {
        user_id: None,
        agent_id: None,
        channel_type: ChannelType::Lark,
        channel_name: "test".to_string(),
        webhook_url: None,
        access_token: None,
        secret: None,
        config: None,
    };
    let config = extract_channel_config(&req);
    assert!(config.lark_credential_id.is_none());
    assert!(config.wechat_credential_id.is_none());
    assert!(config.email_credential_id.is_none());
}

#[test]
fn extract_config_extracts_lark_fields() {
    let req = CreateMessageChannelRequest {
        user_id: None,
        agent_id: None,
        channel_type: ChannelType::Lark,
        channel_name: "test".to_string(),
        webhook_url: None,
        access_token: None,
        secret: None,
        config: Some(CreateMessageChannelConfig {
            lark: Some(common::api::CreateLarkChannelConfig {
                credential_id: Some("cred-1".to_string()),
                identity_mode: Some("bot".to_string()),
                open_id: Some("ou_xxx".to_string()),
                user_name: Some("Test".to_string()),
                listen_inbound: Some(true),
            }),
            wechat: None,
            email: None,
            slack: None,
            webhook: None,
        }),
    };
    let config = extract_channel_config(&req);
    assert_eq!(config.lark_credential_id.as_deref(), Some("cred-1"));
    assert_eq!(config.lark_identity_mode.as_deref(), Some("bot"));
    assert_eq!(config.lark_open_id.as_deref(), Some("ou_xxx"));
    assert_eq!(config.lark_user_name.as_deref(), Some("Test"));
    assert_eq!(config.lark_listen_inbound, Some(true));
}

fn wechat_credentials(credential_id: &str) -> Vec<UserCredential> {
    vec![UserCredential::from_po(UserCredentialPo::new(
        credential_id.to_string(),
        "org-1".to_string(),
        "user-1".to_string(),
        CredentialKind::WechatIlink,
        "微信 iLink".to_string(),
        CredentialDetail::WechatIlink {
            bot_token: "enc:v1:token".to_string(),
            bot_id: "bot_x".to_string(),
            user_id: None,
            base_url: "https://ilinkai.weixin.qq.com".to_string(),
        },
        CredentialVisibility::Private,
        "user-1".to_string(),
    ))]
}

#[test]
fn wechat_credential_ref_required_for_wechat_type() {
    let credentials = wechat_credentials("cred-wx");
    // 未选凭证 / 空白 / 引用不存在
    assert!(validate_wechat_credential_ref(ChannelType::Wechat, None, &credentials).is_err());
    assert!(validate_wechat_credential_ref(ChannelType::Wechat, Some(" "), &credentials).is_err());
    assert!(
        validate_wechat_credential_ref(ChannelType::Wechat, Some("missing"), &credentials).is_err()
    );
    // 引用存在的 WechatIlink 凭证
    assert!(
        validate_wechat_credential_ref(ChannelType::Wechat, Some("cred-wx"), &credentials).is_ok()
    );
}

#[test]
fn wechat_ref_must_match_kind() {
    // 拿飞书凭证当微信凭证用 → 拒绝
    let credentials = lark_credentials("cred-1");
    assert!(
        validate_wechat_credential_ref(ChannelType::Wechat, Some("cred-1"), &credentials).is_err()
    );
    // 反之：微信类型下飞书校验不生效
    let wechat = wechat_credentials("cred-wx");
    assert!(validate_lark_credential_ref(ChannelType::Wechat, None, &wechat).is_ok());
}

#[test]
fn extract_config_extracts_wechat_fields() {
    let req = CreateMessageChannelRequest {
        user_id: None,
        agent_id: None,
        channel_type: ChannelType::Wechat,
        channel_name: "test".to_string(),
        webhook_url: None,
        access_token: None,
        secret: None,
        config: Some(CreateMessageChannelConfig {
            lark: None,
            wechat: Some(common::api::CreateWechatChannelConfig {
                credential_id: Some("cred-wx".to_string()),
                peer_id: Some("wxid_abc".to_string()),
                listen_inbound: Some(false),
            }),
            email: None,
            slack: None,
            webhook: None,
        }),
    };
    let config = extract_channel_config(&req);
    assert_eq!(config.wechat_credential_id.as_deref(), Some("cred-wx"));
    assert_eq!(config.wechat_peer_id.as_deref(), Some("wxid_abc"));
    assert_eq!(config.wechat_listen_inbound, Some(false));
}

fn email_credentials(credential_id: &str) -> Vec<UserCredential> {
    vec![UserCredential::from_po(UserCredentialPo::new(
        credential_id.to_string(),
        "org-1".to_string(),
        "user-1".to_string(),
        CredentialKind::EmailBot,
        "QQ 代理邮箱".to_string(),
        CredentialDetail::EmailBot {
            email_address: "bot@qq.com".to_string(),
            smtp_host: "smtp.qq.com".to_string(),
            smtp_port: 465,
            imap_host: "imap.qq.com".to_string(),
            imap_port: 993,
            username: "bot@qq.com".to_string(),
            password: "enc:v1:code".to_string(),
        },
        CredentialVisibility::Private,
        "user-1".to_string(),
    ))]
}

#[test]
fn email_credential_ref_required_for_email_type() {
    let credentials = email_credentials("cred-em");
    // 未选凭证 / 空白 / 引用不存在
    assert!(validate_email_credential_ref(ChannelType::Email, None, &credentials).is_err());
    assert!(validate_email_credential_ref(ChannelType::Email, Some(" "), &credentials).is_err());
    assert!(
        validate_email_credential_ref(ChannelType::Email, Some("missing"), &credentials).is_err()
    );
    // 引用存在的 EmailBot 凭证
    assert!(
        validate_email_credential_ref(ChannelType::Email, Some("cred-em"), &credentials).is_ok()
    );
}

#[test]
fn email_ref_must_match_kind() {
    // 拿飞书凭证当邮箱凭证用 → 拒绝
    let credentials = lark_credentials("cred-1");
    assert!(
        validate_email_credential_ref(ChannelType::Email, Some("cred-1"), &credentials).is_err()
    );
    // 反之：邮箱类型下飞书校验不生效
    let email = email_credentials("cred-em");
    assert!(validate_lark_credential_ref(ChannelType::Email, None, &email).is_ok());
}

#[test]
fn extract_config_extracts_email_fields() {
    let req = CreateMessageChannelRequest {
        user_id: None,
        agent_id: None,
        channel_type: ChannelType::Email,
        channel_name: "test".to_string(),
        webhook_url: None,
        access_token: None,
        secret: None,
        config: Some(CreateMessageChannelConfig {
            lark: None,
            wechat: None,
            email: Some(common::api::CreateEmailChannelConfig {
                credential_id: Some("cred-em".to_string()),
                to_address: Some("me@163.com".to_string()),
            }),
            slack: None,
            webhook: None,
        }),
    };
    let config = extract_channel_config(&req);
    assert_eq!(config.email_credential_id.as_deref(), Some("cred-em"));
    assert_eq!(config.email_to_address.as_deref(), Some("me@163.com"));
}
