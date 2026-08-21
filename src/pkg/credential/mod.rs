//! 凭据域纯值加工模块（设计：docs/design/tool_credential_requirement_design.md）
//!
//! 纯值加工模块（D17）：解密 / 增强器 / canonical / OAuth 刷新 / 配置校验。
//! 零数据访问——凭据由 service 编排层（domain resolve_tool_credentials）
//! 从 user dal 取回后传入；本模块不持有 ctx、不定义数据端口、无注入注册。
//!
//! 不隶属 tool_registry（依赖方向 tool_registry → credential 单向）：
//! 凭据加工是凭据域通用能力，未来非工具消费方（渠道出站认证等）可直接引用。

use common::error::Result;
use common::models::CredentialDetail;

// ==================== 解密单点 ====================

/// 按 kind 解密 detail 敏感字段（与 `CredentialDetail::encrypt_sensitive` 规则对称）
///
/// 入参为 DAL 取回的加密态 detail；解密结果仅存于当次调用栈。
/// `decrypt_channel_secret` 明文兼容：无 `enc:v1:` 前缀的值原样返回（测试直通）。
pub(crate) fn decrypt_detail(detail: CredentialDetail) -> Result<CredentialDetail> {
    let decrypt = crate::pkg::crypto::decrypt_channel_secret;
    Ok(match detail {
        CredentialDetail::LarkApp {
            app_id,
            app_secret,
            encrypt_key,
            verification_token,
        } => CredentialDetail::LarkApp {
            app_id,
            app_secret: decrypt(app_secret.as_str())?,
            encrypt_key: match encrypt_key {
                Some(v) => Some(decrypt(v.as_str())?),
                None => None,
            },
            verification_token,
        },
        CredentialDetail::GithubToken { token } => CredentialDetail::GithubToken {
            token: decrypt(token.as_str())?,
        },
        CredentialDetail::TavilyKey { api_key } => CredentialDetail::TavilyKey {
            api_key: decrypt(api_key.as_str())?,
        },
        CredentialDetail::GenericToken { token } => CredentialDetail::GenericToken {
            token: decrypt(token.as_str())?,
        },
        CredentialDetail::OAuth {
            token_endpoint,
            client_id,
            client_secret,
            refresh_token,
            scope,
        } => CredentialDetail::OAuth {
            token_endpoint,
            client_id,
            client_secret: decrypt(client_secret.as_str())?,
            refresh_token: decrypt(refresh_token.as_str())?,
            scope,
        },
        CredentialDetail::UserPassword { username, password } => CredentialDetail::UserPassword {
            username,
            password: decrypt(password.as_str())?,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 明文兼容直通：无 enc:v1: 前缀的敏感字段原样返回（非敏感字段不经解密函数）
    #[test]
    fn decrypt_detail_passes_plaintext_through() {
        let detail = CredentialDetail::UserPassword {
            username: "alice".to_string(),
            password: "plain-pw".to_string(),
        };
        let decrypted = decrypt_detail(detail).unwrap();
        assert_eq!(
            decrypted,
            CredentialDetail::UserPassword {
                username: "alice".to_string(),
                password: "plain-pw".to_string(),
            }
        );
    }

    /// 加密态全字段对称：encrypt_sensitive → decrypt_detail 还原明文
    ///（config::init 落默认 secret_key，encrypt/decrypt 同钥可逆）
    #[test]
    fn decrypt_detail_roundtrips_encrypted_fields() {
        let _ = crate::config::init();
        let plain = CredentialDetail::OAuth {
            token_endpoint: "https://oauth.example.com/token".to_string(),
            client_id: "cid".to_string(),
            client_secret: "csec".to_string(),
            refresh_token: "rtok".to_string(),
            scope: Some("read".to_string()),
        };
        let encrypted = plain
            .clone()
            .encrypt_sensitive(|s| crate::pkg::crypto::encrypt_channel_secret(s))
            .unwrap();
        // 落库态确为密文
        let CredentialDetail::OAuth {
            client_secret, refresh_token, ..
        } = &encrypted
        else {
            panic!("kind 不变");
        };
        assert!(client_secret.starts_with("enc:v1:"));
        assert!(refresh_token.starts_with("enc:v1:"));

        let decrypted = decrypt_detail(encrypted).unwrap();
        assert_eq!(decrypted, plain);
    }

    /// 非敏感字段（token_endpoint/client_id/scope/username）不经加密函数
    #[test]
    fn decrypt_detail_keeps_non_sensitive_fields_untouched() {
        let _ = crate::config::init();
        let detail = CredentialDetail::LarkApp {
            app_id: "cli_a".to_string(),
            app_secret: "sec".to_string(),
            encrypt_key: None,
            verification_token: Some("vt".to_string()),
        };
        // 仅 app_secret 被加密，app_id / verification_token 保持明文
        let encrypted = detail
            .encrypt_sensitive(|s| crate::pkg::crypto::encrypt_channel_secret(s))
            .unwrap();
        let CredentialDetail::LarkApp {
            app_id, verification_token, ..
        } = &encrypted
        else {
            panic!("kind 不变");
        };
        assert_eq!(app_id, "cli_a");
        assert_eq!(verification_token.as_deref(), Some("vt"));
    }
}
