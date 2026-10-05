//! tests 单元测试（拆分自 identity_credentials.rs）
//!
//! 文件瘦身：原 2051 行 → 1055 行，测试体 997 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

#[test]
fn test_credential_kind_serde_snake_case() {
    assert_eq!(
        serde_json::to_value(CredentialKind::LarkApp).unwrap(),
        "lark_app"
    );
    assert_eq!(
        serde_json::to_value(CredentialKind::GithubToken).unwrap(),
        "github_token"
    );
    // 往返一致（DB TEXT 值 = serde 值）
    assert_eq!(
        serde_json::from_value::<CredentialKind>(serde_json::json!("lark_app")).unwrap(),
        CredentialKind::LarkApp
    );
}

#[test]
fn test_credential_visibility_serde_snake_case() {
    assert_eq!(
        serde_json::to_value(CredentialVisibility::Private).unwrap(),
        "private"
    );
    assert_eq!(
        serde_json::to_value(CredentialVisibility::Public).unwrap(),
        "public"
    );
    assert_eq!(
        CredentialVisibility::default(),
        CredentialVisibility::Private
    );
}

#[test]
fn test_detail_serde_tag() {
    let detail = CredentialDetail::LarkApp {
        app_id: "cli_a1b2c3".to_string(),
        app_secret: "enc:v1:secret".to_string(),
        encrypt_key: Some("enc:v1:key".to_string()),
        verification_token: None,
    };
    let json = serde_json::to_value(&detail).unwrap();
    assert_eq!(json["type"], "lark_app");
    assert_eq!(json["app_id"], "cli_a1b2c3");
    // 往返一致
    let parsed: CredentialDetail = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, detail);
}

#[test]
fn test_github_credential_serde_tag() {
    let detail = CredentialDetail::GithubToken {
        token: "enc:v1:gh-token".to_string(),
    };
    let json = serde_json::to_value(&detail).unwrap();
    assert_eq!(json["type"], "github_token");
    assert_eq!(json["token"], "enc:v1:gh-token");
    let parsed: CredentialDetail = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, detail);
}

// ==================== CredentialDetail 行为 ====================

#[test]
fn test_detail_kind_and_primary_id() {
    let lark = CredentialDetail::LarkApp {
        app_id: "cli_a1b2c3".to_string(),
        app_secret: "s".to_string(),
        encrypt_key: None,
        verification_token: None,
    };
    assert_eq!(lark.kind(), CredentialKind::LarkApp);
    assert_eq!(lark.primary_id(), Some("cli_a1b2c3"));

    let gh = CredentialDetail::GithubToken {
        token: "t".to_string(),
    };
    assert_eq!(gh.kind(), CredentialKind::GithubToken);
    assert_eq!(gh.primary_id(), None);
}

#[test]
fn test_detail_normalized_trims_and_drops_empty_optionals() {
    let plain = CredentialDetail::LarkApp {
        app_id: "  cli_x  ".to_string(),
        app_secret: " s1 ".to_string(),
        encrypt_key: Some("   ".to_string()),
        verification_token: Some(" vt ".to_string()),
    };
    let normalized = plain.normalized();
    let CredentialDetail::LarkApp {
        app_id,
        app_secret,
        encrypt_key,
        verification_token,
    } = normalized
    else {
        panic!("kind 不变");
    };
    assert_eq!(app_id, "cli_x");
    assert_eq!(app_secret, "s1");
    assert_eq!(encrypt_key, None, "空白可选字段视为未提供");
    assert_eq!(verification_token.as_deref(), Some("vt"));

    let gh = CredentialDetail::GithubToken {
        token: " ghp_x\n ".to_string(),
    }
    .normalized();
    assert!(matches!(gh, CredentialDetail::GithubToken { ref token } if token == "ghp_x"));
}

#[test]
fn test_detail_validate_required_fields() {
    // 合法
    assert!(
        CredentialDetail::LarkApp {
            app_id: "cli_x".to_string(),
            app_secret: "s".to_string(),
            encrypt_key: None,
            verification_token: None,
        }
        .validate()
        .is_ok()
    );
    assert!(
        CredentialDetail::GithubToken {
            token: "t".to_string()
        }
        .validate()
        .is_ok()
    );
    // lark 缺 app_id
    let bad = CredentialDetail::LarkApp {
        app_id: " ".to_string(),
        app_secret: "s".to_string(),
        encrypt_key: None,
        verification_token: None,
    }
    .normalized();
    assert!(bad.validate().is_err());
    // github 缺 token
    let bad_gh = CredentialDetail::GithubToken {
        token: String::new(),
    };
    assert!(bad_gh.validate().is_err());
}

#[test]
fn test_encrypt_sensitive_only_secret_fields() {
    let plain = CredentialDetail::LarkApp {
        app_id: "cli_x".to_string(),
        app_secret: "s1".to_string(),
        encrypt_key: Some("k1".to_string()),
        verification_token: Some("vt".to_string()),
    };
    let enc = plain
        .clone()
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
        .unwrap();
    let CredentialDetail::LarkApp {
        app_id,
        app_secret,
        encrypt_key,
        verification_token,
    } = enc
    else {
        panic!("kind 不变");
    };
    assert_eq!(app_id, "cli_x", "app_id 非敏感原样保留");
    assert_eq!(app_secret, "enc:s1");
    assert_eq!(encrypt_key.as_deref(), Some("enc:k1"));
    assert_eq!(
        verification_token.as_deref(),
        Some("vt"),
        "verification_token 非加密字段"
    );

    // encrypt_key=None 不调用加密器
    let no_key = CredentialDetail::LarkApp {
        app_id: "a".to_string(),
        app_secret: "s".to_string(),
        encrypt_key: None,
        verification_token: None,
    };
    assert!(no_key.encrypt_sensitive(|_| Err(bail_err_test())).is_err());

    // github token 加密
    let gh_enc = CredentialDetail::GithubToken {
        token: "ghp_x".to_string(),
    }
    .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
    .unwrap();
    assert!(matches!(gh_enc, CredentialDetail::GithubToken { ref token } if token == "enc:ghp_x"));
}

/// 测试用错误构造（避免测试依赖具体 error 变体）
fn bail_err_test() -> crate::error::Error {
    crate::error::Error::internal("test")
}

// ==================== 补丁应用 ====================

fn plain_lark_detail() -> CredentialDetail {
    CredentialDetail::LarkApp {
        app_id: "cli_old".to_string(),
        app_secret: "enc:v1:old-secret".to_string(),
        encrypt_key: None,
        verification_token: Some("vt-old".to_string()),
    }
}

#[test]
fn test_apply_patch_unchanged_is_noop() {
    let mut detail = plain_lark_detail();
    let before = detail.clone();
    let impact = detail
        .apply_patch(CredentialDetailPatch::Unchanged, |s| {
            Ok(format!("enc:{}", s))
        })
        .unwrap();
    assert_eq!(detail, before);
    assert!(!impact.secret_changed);
}

#[test]
fn test_apply_patch_lark_fields() {
    let mut detail = plain_lark_detail();
    let impact = detail
        .apply_patch(
            CredentialDetailPatch::LarkApp {
                app_id: Some("cli_new".to_string()),
                app_secret: Some("new-secret".to_string()),
                encrypt_key: Some("k1".to_string()),
                verification_token: Some("  ".to_string()), // 空白清除
            },
            |s| Ok(format!("enc:{}", s)),
        )
        .unwrap();
    let CredentialDetail::LarkApp {
        app_id,
        app_secret,
        encrypt_key,
        verification_token,
    } = &detail
    else {
        panic!("kind 不变");
    };
    assert_eq!(app_id, "cli_new");
    assert_eq!(app_secret, "enc:new-secret");
    assert_eq!(encrypt_key.as_deref(), Some("enc:k1"));
    assert_eq!(*verification_token, None, "空白 verification_token 清除");
    assert!(impact.secret_changed, "secret/encrypt_key 任一变更即轮换");
}

#[test]
fn test_apply_patch_empty_optional_keeps_value() {
    // 全 None（含空串）→ detail 不变、无 secret 轮换
    let mut detail = plain_lark_detail();
    let before = detail.clone();
    let impact = detail
        .apply_patch(
            CredentialDetailPatch::LarkApp {
                app_id: None,
                app_secret: Some("   ".to_string()),
                encrypt_key: None,
                verification_token: None,
            },
            |s| Ok(format!("enc:{}", s)),
        )
        .unwrap();
    assert_eq!(detail, before);
    assert!(!impact.secret_changed);
}

#[test]
fn test_apply_patch_kind_mismatch_rejected() {
    // github 补丁打到 lark 凭证 → 报错
    let mut detail = plain_lark_detail();
    assert!(
        detail
            .apply_patch(
                CredentialDetailPatch::GithubToken {
                    token: Some("t".to_string()),
                },
                |s| Ok(s.to_string()),
            )
            .is_err()
    );
    // lark 补丁打到 github 凭证 → 报错
    let mut gh = CredentialDetail::GithubToken {
        token: "enc:v1:t".to_string(),
    };
    assert!(
        gh.apply_patch(
            CredentialDetailPatch::LarkApp {
                app_id: None,
                app_secret: None,
                encrypt_key: None,
                verification_token: None,
            },
            |s| Ok(s.to_string()),
        )
        .is_err()
    );
}

#[test]
fn test_apply_patch_github_token() {
    let mut gh = CredentialDetail::GithubToken {
        token: "enc:v1:old".to_string(),
    };
    let impact = gh
        .apply_patch(
            CredentialDetailPatch::GithubToken {
                token: Some("ghp_new".to_string()),
            },
            |s| Ok(format!("enc:{}", s)),
        )
        .unwrap();
    assert!(matches!(&gh, CredentialDetail::GithubToken { token } if token == "enc:ghp_new"));
    assert!(impact.secret_changed);
}

// ==================== GenericToken / OAuth / UserPassword ====================

#[test]
fn test_new_kinds_serde_snake_case() {
    assert_eq!(
        serde_json::to_value(CredentialKind::GenericToken).unwrap(),
        "generic_token"
    );
    assert_eq!(
        serde_json::to_value(CredentialKind::OAuth).unwrap(),
        "oauth"
    );
    assert_eq!(
        serde_json::to_value(CredentialKind::UserPassword).unwrap(),
        "user_password"
    );
}

#[test]
fn test_generic_token_detail_lifecycle() {
    let plain = CredentialDetail::GenericToken {
        token: " ntn_xxx ".to_string(),
    }
    .normalized();
    assert!(matches!(&plain, CredentialDetail::GenericToken { token } if token == "ntn_xxx"));
    assert!(plain.validate().is_ok());
    let enc = CredentialDetail::GenericToken {
        token: "plain".to_string(),
    }
    .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
    .unwrap();
    assert!(matches!(&enc, CredentialDetail::GenericToken { token } if token == "enc:plain"));
    let mut detail = CredentialDetail::GenericToken {
        token: "enc:v1:old".to_string(),
    };
    detail
        .apply_patch(
            CredentialDetailPatch::GenericToken {
                token: Some("new".to_string()),
            },
            |s| Ok(format!("enc:{}", s)),
        )
        .unwrap();
    assert!(matches!(&detail, CredentialDetail::GenericToken { token } if token == "enc:new"));
}

#[test]
fn test_oauth_detail_validate_and_secret() {
    let detail = CredentialDetail::OAuth {
        token_endpoint: "https://example.invalid/oauth/token".to_string(),
        client_id: "cid".to_string(),
        client_secret: "csec".to_string(),
        refresh_token: "rt".to_string(),
        scope: Some("read".to_string()),
    };
    assert!(detail.clone().normalized().validate().is_ok());
    // 缺 token_endpoint → 校验失败
    let bad = CredentialDetail::OAuth {
        token_endpoint: String::new(),
        client_id: "c".into(),
        client_secret: "s".into(),
        refresh_token: "r".into(),
        scope: None,
    };
    assert!(bad.normalized().validate().is_err());
    assert_eq!(detail.primary_secret(), "rt");
    // client_secret / refresh_token 加密，client_id / token_endpoint 不加密
    let enc = detail
        .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
        .unwrap();
    let CredentialDetail::OAuth {
        client_id,
        client_secret,
        refresh_token,
        ..
    } = enc
    else {
        panic!("kind 不变");
    };
    assert_eq!(client_id, "cid");
    assert_eq!(client_secret, "enc:csec");
    assert_eq!(refresh_token, "enc:rt");
}

#[test]
fn test_user_password_detail() {
    let detail = CredentialDetail::UserPassword {
        username: "alice".to_string(),
        password: " pw ".to_string(),
    };
    let normalized = detail.normalized();
    assert!(
        matches!(&normalized, CredentialDetail::UserPassword { username, password } if username == "alice" && password == "pw")
    );
    assert!(normalized.validate().is_ok());
    let enc = CredentialDetail::UserPassword {
        username: "alice".into(),
        password: "p".into(),
    }
    .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
    .unwrap();
    // password 加密、username 不加密
    assert!(
        matches!(&enc, CredentialDetail::UserPassword { username, password } if username == "alice" && password == "enc:p")
    );
}

#[test]
fn test_wechat_ilink_serde_and_accessors() {
    let detail = CredentialDetail::WechatIlink {
        bot_token: "enc:v1:bot".to_string(),
        bot_id: "bot_123".to_string(),
        user_id: Some("u_abc".to_string()),
        base_url: "https://ilinkai.weixin.qq.com".to_string(),
    };
    let json = serde_json::to_value(&detail).unwrap();
    assert_eq!(json["type"], "wechat_ilink");
    assert_eq!(json["bot_id"], "bot_123");
    // 往返一致
    let parsed: CredentialDetail = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, detail);
    // kind / as_str / serde 值空间一致
    assert_eq!(detail.kind(), CredentialKind::WechatIlink);
    assert_eq!(CredentialKind::WechatIlink.as_str(), "wechat_ilink");
    assert_eq!(
        serde_json::to_value(CredentialKind::WechatIlink).unwrap(),
        "wechat_ilink"
    );
    // primary_id = bot_id（同 lark app_id 地位）；primary_secret = bot_token
    assert_eq!(detail.primary_id(), Some("bot_123"));
    assert_eq!(detail.primary_secret(), "enc:v1:bot");
    // 专用 kind 不需要 platform
    assert!(!CredentialKind::WechatIlink.requires_platform());
}

#[test]
fn test_wechat_ilink_normalized() {
    let detail = CredentialDetail::WechatIlink {
        bot_token: " tok ".to_string(),
        bot_id: " bot_1 ".to_string(),
        user_id: Some("  ".to_string()),
        base_url: "https://ilinkai.weixin.qq.com/".to_string(),
    };
    let normalized = detail.normalized();
    assert!(
        matches!(&normalized, CredentialDetail::WechatIlink { bot_token, bot_id, user_id, base_url }
            if bot_token == "tok" && bot_id == "bot_1" && user_id.is_none() && base_url == "https://ilinkai.weixin.qq.com")
    );
}

#[test]
fn test_wechat_ilink_validate() {
    let ok = CredentialDetail::WechatIlink {
        bot_token: "t".into(),
        bot_id: "b".into(),
        user_id: None,
        base_url: "https://ilinkai.weixin.qq.com".into(),
    };
    assert!(ok.validate().is_ok());
    // 必填字段为空
    let empty_token = CredentialDetail::WechatIlink {
        bot_token: "".into(),
        bot_id: "b".into(),
        user_id: None,
        base_url: "https://x".into(),
    };
    assert!(empty_token.validate().is_err());
    // base_url 必须是 https
    let http_url = CredentialDetail::WechatIlink {
        bot_token: "t".into(),
        bot_id: "b".into(),
        user_id: None,
        base_url: "http://ilinkai.weixin.qq.com".into(),
    };
    assert!(http_url.validate().is_err());
}

#[test]
fn test_wechat_ilink_encrypt_and_patch() {
    // 只加密 bot_token，其余字段不动
    let enc = CredentialDetail::WechatIlink {
        bot_token: "secret-token".into(),
        bot_id: "bot_1".into(),
        user_id: Some("u".into()),
        base_url: "https://ilinkai.weixin.qq.com".into(),
    }
    .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
    .unwrap();
    assert!(
        matches!(&enc, CredentialDetail::WechatIlink { bot_token, bot_id, base_url, .. }
            if bot_token == "enc:secret-token" && bot_id == "bot_1" && base_url == "https://ilinkai.weixin.qq.com")
    );

    // 重新扫码 = 整组轮换：bot_token 轮换计为 secret_changed，user_id 空白清除
    let mut mutable = enc;
    let impact = mutable
        .apply_patch(
            CredentialDetailPatch::WechatIlink {
                bot_token: Some("new-token".into()),
                bot_id: Some("bot_2".into()),
                user_id: Some("  ".into()),
                base_url: Some("https://new.example.com/".into()),
            },
            |s| Ok(format!("enc:{}", s)),
        )
        .unwrap();
    assert!(impact.secret_changed);
    assert!(
        matches!(&mutable, CredentialDetail::WechatIlink { bot_token, bot_id, user_id, base_url }
            if bot_token == "enc:new-token" && bot_id == "bot_2" && user_id.is_none() && base_url == "https://new.example.com")
    );

    // 补丁类型不匹配被拒
    let mut lark = CredentialDetail::LarkApp {
        app_id: "a".into(),
        app_secret: "s".into(),
        encrypt_key: None,
        verification_token: None,
    };
    assert!(
        lark.apply_patch(
            CredentialDetailPatch::WechatIlink {
                bot_token: None,
                bot_id: None,
                user_id: None,
                base_url: None,
            },
            |s| Ok(s.to_string()),
        )
        .is_err()
    );
}

// ==================== EmailBot ====================

#[test]
fn test_email_bot_serde_and_accessors() {
    let detail = CredentialDetail::EmailBot {
        email_address: "bot@qq.com".to_string(),
        smtp_host: "smtp.qq.com".to_string(),
        smtp_port: 465,
        imap_host: "imap.qq.com".to_string(),
        imap_port: 993,
        username: "bot@qq.com".to_string(),
        password: "enc:v1:authcode".to_string(),
    };
    let json = serde_json::to_value(&detail).unwrap();
    assert_eq!(json["type"], "email_bot");
    assert_eq!(json["email_address"], "bot@qq.com");
    assert_eq!(json["smtp_port"], 465);
    // 往返一致
    let parsed: CredentialDetail = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, detail);
    // kind / as_str / serde 值空间一致
    assert_eq!(detail.kind(), CredentialKind::EmailBot);
    assert_eq!(CredentialKind::EmailBot.as_str(), "email_bot");
    assert_eq!(
        serde_json::to_value(CredentialKind::EmailBot).unwrap(),
        "email_bot"
    );
    // primary_id = 邮箱地址（同 lark app_id 地位）；primary_secret = 密码/授权码
    assert_eq!(detail.primary_id(), Some("bot@qq.com"));
    assert_eq!(detail.primary_secret(), "enc:v1:authcode");
    // 多提供商专用 kind：platform 必填（提供商维度）
    assert!(CredentialKind::EmailBot.requires_platform());
}

#[test]
fn test_email_bot_normalized_and_validate() {
    let detail = CredentialDetail::EmailBot {
        email_address: " bot@qq.com ".to_string(),
        smtp_host: " smtp.qq.com ".to_string(),
        smtp_port: 465,
        imap_host: " imap.qq.com ".to_string(),
        imap_port: 993,
        username: " bot@qq.com ".to_string(),
        password: " authcode ".to_string(),
    };
    let normalized = detail.normalized();
    assert!(
        matches!(&normalized, CredentialDetail::EmailBot { email_address, smtp_host, imap_host, username, password, .. }
            if email_address == "bot@qq.com" && smtp_host == "smtp.qq.com" && imap_host == "imap.qq.com" && username == "bot@qq.com" && password == "authcode")
    );
    assert!(normalized.validate().is_ok());
    // 缺邮箱地址
    let bad = CredentialDetail::EmailBot {
        email_address: String::new(),
        smtp_host: "smtp.qq.com".into(),
        smtp_port: 465,
        imap_host: "imap.qq.com".into(),
        imap_port: 993,
        username: "bot@qq.com".into(),
        password: "authcode".into(),
    };
    assert!(bad.validate().is_err());
    // 邮箱地址缺 @
    let no_at = CredentialDetail::EmailBot {
        email_address: "bot.qq.com".into(),
        smtp_host: "smtp.qq.com".into(),
        smtp_port: 465,
        imap_host: "imap.qq.com".into(),
        imap_port: 993,
        username: "bot@qq.com".into(),
        password: "authcode".into(),
    };
    assert!(no_at.validate().is_err());
    // 端口为 0
    let zero_port = CredentialDetail::EmailBot {
        email_address: "bot@qq.com".into(),
        smtp_host: "smtp.qq.com".into(),
        smtp_port: 0,
        imap_host: "imap.qq.com".into(),
        imap_port: 993,
        username: "bot@qq.com".into(),
        password: "authcode".into(),
    };
    assert!(zero_port.validate().is_err());
}

#[test]
fn test_email_bot_encrypt_and_patch() {
    // 只加密 password，其余字段不动
    let enc = CredentialDetail::EmailBot {
        email_address: "bot@qq.com".into(),
        smtp_host: "smtp.qq.com".into(),
        smtp_port: 465,
        imap_host: "imap.qq.com".into(),
        imap_port: 993,
        username: "bot@qq.com".into(),
        password: "authcode".into(),
    }
    .encrypt_sensitive(|s| Ok(format!("enc:{}", s)))
    .unwrap();
    assert!(
        matches!(&enc, CredentialDetail::EmailBot { email_address, password, .. }
            if email_address == "bot@qq.com" && password == "enc:authcode")
    );

    // 补丁：授权码轮换计为 secret_changed；端口 0 视为未提供
    let mut mutable = enc;
    let impact = mutable
        .apply_patch(
            CredentialDetailPatch::EmailBot {
                email_address: None,
                smtp_host: Some("smtp.163.com".into()),
                smtp_port: Some(0),
                imap_host: None,
                imap_port: Some(994),
                username: None,
                password: Some("new-code".into()),
            },
            |s| Ok(format!("enc:{}", s)),
        )
        .unwrap();
    assert!(impact.secret_changed);
    assert!(
        matches!(&mutable, CredentialDetail::EmailBot { smtp_host, smtp_port, imap_port, password, .. }
            if smtp_host == "smtp.163.com" && *smtp_port == 465 && *imap_port == 994 && password == "enc:new-code")
    );

    // 补丁类型不匹配被拒
    let mut lark = CredentialDetail::LarkApp {
        app_id: "a".into(),
        app_secret: "s".into(),
        encrypt_key: None,
        verification_token: None,
    };
    assert!(
        lark.apply_patch(
            CredentialDetailPatch::EmailBot {
                email_address: None,
                smtp_host: None,
                smtp_port: None,
                imap_host: None,
                imap_port: None,
                username: None,
                password: None,
            },
            |s| Ok(s.to_string()),
        )
        .is_err()
    );
}

#[test]
fn test_new_kinds_primary_id_none_and_requires_platform() {
    let kinds = [
        CredentialKind::GenericToken,
        CredentialKind::OAuth,
        CredentialKind::UserPassword,
    ];
    for kind in kinds {
        assert!(kind.requires_platform(), "generic 类 kind platform 必填");
    }
    for kind in [CredentialKind::LarkApp, CredentialKind::GithubToken] {
        assert!(!kind.requires_platform(), "专用 kind platform 必空");
    }
    // 三新变体 primary_id 均 None
    assert_eq!(
        CredentialDetail::GenericToken { token: "t".into() }.primary_id(),
        None
    );
    assert_eq!(
        CredentialDetail::UserPassword {
            username: "u".into(),
            password: "p".into()
        }
        .primary_id(),
        None
    );
}

// ==================== CredentialRequirement 契约 ====================

#[test]
fn test_requirement_serde_roundtrip() {
    let req = CredentialRequirement {
        kind: CredentialKind::OAuth,
        platform: Some("linear".to_string()),
        field: None,
        enhancer: Some(CredentialEnhancerKind::BearerToken),
        binding: CredentialBinding::Header {
            name: "authorization".to_string(),
        },
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["kind"], "oauth");
    assert_eq!(json["platform"], "linear");
    assert_eq!(json["enhancer"], "bearer_token");
    assert_eq!(json["binding"]["type"], "header");
    assert_eq!(json["binding"]["name"], "authorization");
    let parsed: CredentialRequirement = serde_json::from_value(json).unwrap();
    assert_eq!(parsed, req);
}

#[test]
fn test_binding_serde_snake_case() {
    let env = CredentialBinding::Env {
        name: "API_TOKEN".to_string(),
    };
    assert_eq!(serde_json::to_value(&env).unwrap()["type"], "env");
    let query = CredentialBinding::Query {
        name: "api_key".to_string(),
    };
    assert_eq!(serde_json::to_value(&query).unwrap()["type"], "query");
    let internal = CredentialBinding::Internal {
        field: "app_id".to_string(),
    };
    assert_eq!(serde_json::to_value(&internal).unwrap()["type"], "internal");
}

#[test]
fn test_enhancer_supports_matrix() {
    use CredentialEnhancerKind as E;
    // 精确表
    assert!(enhancer_supports(
        CredentialKind::GenericToken,
        E::BearerToken
    ));
    assert!(enhancer_supports(CredentialKind::OAuth, E::BearerToken));
    assert!(enhancer_supports(CredentialKind::OAuth, E::AccessToken));
    assert!(enhancer_supports(
        CredentialKind::UserPassword,
        E::BasicAuth
    ));
    // 专用 kind 零支持
    for kind in [CredentialKind::LarkApp, CredentialKind::GithubToken] {
        assert!(!enhancer_supports(kind, E::BearerToken));
        assert!(!enhancer_supports(kind, E::BasicAuth));
        assert!(!enhancer_supports(kind, E::AccessToken));
    }
    // 反向组合拒绝
    assert!(!enhancer_supports(
        CredentialKind::UserPassword,
        E::BearerToken
    ));
    assert!(!enhancer_supports(
        CredentialKind::GenericToken,
        E::AccessToken
    ));
    assert!(!enhancer_supports(
        CredentialKind::GenericToken,
        E::BasicAuth
    ));
}

#[test]
fn test_default_enhancer_assembly() {
    use CredentialEnhancerKind as E;
    assert_eq!(
        default_enhancer(CredentialKind::OAuth),
        Some(E::AccessToken)
    );
    assert_eq!(
        default_enhancer(CredentialKind::UserPassword),
        Some(E::BasicAuth)
    );
    assert_eq!(default_enhancer(CredentialKind::GenericToken), None);
    assert_eq!(default_enhancer(CredentialKind::LarkApp), None);
}

#[test]
fn test_is_sensitive_credential_name() {
    // 精确匹配（忽略大小写）
    assert!(is_sensitive_credential_name("authorization"));
    assert!(is_sensitive_credential_name("Authorization"));
    assert!(is_sensitive_credential_name("cookie"));
    assert!(is_sensitive_credential_name("Set-Cookie"));
    // 子串匹配
    assert!(is_sensitive_credential_name("X-API-KEY"));
    assert!(is_sensitive_credential_name("x-api-key"));
    assert!(is_sensitive_credential_name("My-Token"));
    assert!(is_sensitive_credential_name("client_secret"));
    assert!(is_sensitive_credential_name("user_password"));
    // 连字符规则：api-key 命中、api_key 不命中（与后端 tool_security 历史规则一致）
    assert!(!is_sensitive_credential_name("api_key"));
    // 非敏感
    assert!(!is_sensitive_credential_name("Content-Type"));
    assert!(!is_sensitive_credential_name("Accept"));
    assert!(!is_sensitive_credential_name("X-Api-Version"));
    assert!(!is_sensitive_credential_name(""));
}

fn req(
    kind: CredentialKind,
    platform: Option<&str>,
    field: Option<&str>,
    enhancer: Option<CredentialEnhancerKind>,
    binding: CredentialBinding,
) -> CredentialRequirement {
    CredentialRequirement {
        kind,
        platform: platform.map(|s| s.to_string()),
        field: field.map(|s| s.to_string()),
        enhancer,
        binding,
    }
}

fn header(name: &str) -> CredentialBinding {
    CredentialBinding::Header {
        name: name.to_string(),
    }
}

#[test]
fn test_validate_requirements_six_rules() {
    // 合法组合（HttpTool scope：Header + platform 必填的 generic_token）
    let ok = req(
        CredentialKind::GenericToken,
        Some("linear"),
        None,
        None,
        header("authorization"),
    );
    assert!(
        validate_requirements(
            std::slice::from_ref(&ok),
            CredentialRequirementScope::HttpTool
        )
        .is_ok()
    );

    // 规则 1：Env binding 不适用 HttpTool scope
    let env_binding = req(
        CredentialKind::GenericToken,
        Some("linear"),
        None,
        None,
        CredentialBinding::Env {
            name: "LINEAR_TOKEN".to_string(),
        },
    );
    let err =
        validate_requirements(&[env_binding], CredentialRequirementScope::HttpTool).unwrap_err();
    assert!(err.contains("HTTP 工具仅支持请求头或查询参数注入"), "{err}");

    // 规则 2：注入名空白
    let blank = req(
        CredentialKind::GenericToken,
        Some("linear"),
        None,
        None,
        header("  "),
    );
    assert_eq!(
        validate_requirements(&[blank], CredentialRequirementScope::HttpTool).unwrap_err(),
        "凭据注入点名不能为空"
    );

    // 规则 3：generic 类缺 platform / 专用类多 platform
    let missing_platform = req(
        CredentialKind::GenericToken,
        None,
        None,
        None,
        header("authorization"),
    );
    assert_eq!(
        validate_requirements(&[missing_platform], CredentialRequirementScope::HttpTool)
            .unwrap_err(),
        "凭据类型 generic_token 必须填写平台标识"
    );
    let extra_platform = req(
        CredentialKind::GithubToken,
        Some("github"),
        None,
        None,
        header("authorization"),
    );
    assert_eq!(
        validate_requirements(&[extra_platform], CredentialRequirementScope::HttpTool).unwrap_err(),
        "凭据类型 github_token 不适用平台标识，请清空"
    );

    // 规则 4：field ↔ enhancer 互斥
    let both = req(
        CredentialKind::LarkApp,
        None,
        Some("app_id"),
        Some(CredentialEnhancerKind::AccessToken),
        header("authorization"),
    );
    let err = validate_requirements(&[both], CredentialRequirementScope::HttpTool).unwrap_err();
    assert!(err.contains("互斥"), "{err}");

    // 规则 5：supports 矩阵（github_token + BasicAuth 不支持）
    let unsupported = req(
        CredentialKind::GithubToken,
        None,
        None,
        Some(CredentialEnhancerKind::BasicAuth),
        header("authorization"),
    );
    let err =
        validate_requirements(&[unsupported], CredentialRequirementScope::HttpTool).unwrap_err();
    assert!(err.contains("不支持增强器 basic_auth"), "{err}");

    // 规则 6：三元组去重
    let dup = req(
        CredentialKind::GenericToken,
        Some("linear"),
        None,
        None,
        header("authorization"),
    );
    let err = validate_requirements(&[ok.clone(), dup], CredentialRequirementScope::HttpTool)
        .unwrap_err();
    assert!(err.contains("重复"), "{err}");
}

#[test]
fn test_mcp_transport_scope_mapping() {
    assert_eq!(
        mcp_transport_scope(crate::enums::McpTransport::Stdio),
        CredentialRequirementScope::McpStdio
    );
    assert_eq!(
        mcp_transport_scope(crate::enums::McpTransport::StreamableHttp),
        CredentialRequirementScope::McpHttp
    );
}

#[test]
fn test_enhancer_kind_as_str() {
    assert_eq!(CredentialEnhancerKind::BearerToken.as_str(), "bearer_token");
    assert_eq!(CredentialEnhancerKind::BasicAuth.as_str(), "basic_auth");
    assert_eq!(CredentialEnhancerKind::AccessToken.as_str(), "access_token");
}
