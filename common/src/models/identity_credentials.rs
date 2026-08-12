//! 用户身份凭证库（users.identity_credentials JSON 列）
//!
//! 类型化结构体约束凭证类型/详情/关键 ID，前后端共享。
//! secret 类字段在落库前经 `pkg::crypto::encrypt_channel_secret` 加密，
//! 本结构体本身不含加密逻辑（加密发生在 DAL 读写时）。

use serde::{Deserialize, Serialize};

use crate::error::{Result, bail_err};

/// 用户身份凭证库（users.identity_credentials JSON 列的顶层结构）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentityCredentials {
    /// 凭证列表（多类型凭据共存）
    pub items: Vec<UserIdentityCredential>,
    /// 默认凭证 ID（用户显式选择；lark_cli 工具身份优先取引用该凭证的渠道）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_credential_id: Option<String>,
}

impl UserIdentityCredentials {
    /// 从 users.identity_credentials 列值解析（空串视为无凭证）
    pub fn parse(column: &str) -> Self {
        if column.trim().is_empty() {
            return Self::default();
        }
        serde_json::from_str(column).unwrap_or_default()
    }

    /// 序列化为落库 JSON 字符串（无凭证时返回空串，保持列默认值语义）
    pub fn to_column_value(&self) -> String {
        if self.items.is_empty() {
            return String::new();
        }
        serde_json::to_string(self).unwrap_or_default()
    }

    /// 按关键 ID 查找凭证
    pub fn find_by_id(&self, id: &str) -> Option<&UserIdentityCredential> {
        self.items.iter().find(|c| c.id == id)
    }

    /// 按关键 ID 查找可变凭证
    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut UserIdentityCredential> {
        self.items.iter_mut().find(|c| c.id == id)
    }

    /// 按关键 ID 删除凭证，返回被删除的凭证
    pub fn remove_by_id(&mut self, id: &str) -> Option<UserIdentityCredential> {
        let pos = self.items.iter().position(|c| c.id == id)?;
        Some(self.items.remove(pos))
    }

    /// 飞书渠道凭证引用统一校验（存在 + kind=LarkApp）
    ///
    /// 渠道创建/更新与快照聚合共用的单一校验入口；归属校验天然成立：
    /// 凭证库本身即按渠道/请求归属用户加载。
    pub fn resolve_lark_credential_ref(
        &self,
        lark_credential_id: Option<&str>,
    ) -> Result<&UserIdentityCredential> {
        let Some(credential_id) = lark_credential_id.map(str::trim).filter(|s| !s.is_empty())
        else {
            bail_err!(InvalidRequest, "飞书渠道必须选择已绑定的应用凭证");
        };
        let Some(credential) = self.find_by_id(credential_id) else {
            bail_err!(
                InvalidRequest,
                "所选飞书凭证不存在，请先在飞书集成中绑定应用"
            );
        };
        if !matches!(credential.kind, CredentialKind::LarkApp) {
            bail_err!(InvalidRequest, "所选凭证不是飞书应用凭证，无法用于飞书渠道");
        }
        Ok(credential)
    }
}

/// 单条用户身份凭证
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserIdentityCredential {
    /// 凭证关键 ID（渠道引用键，uuid）
    pub id: String,
    /// 凭证类型
    pub kind: CredentialKind,
    /// 用户自命名
    pub name: String,
    /// 创建时间（RFC3339）
    pub created_at: String,
    /// 更新时间（RFC3339）
    pub updated_at: String,
    /// 按 kind 区分的详情
    pub detail: CredentialDetail,
}

/// 凭证类型（可扩展，如后续 WechatApp）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// 飞书自建应用凭证
    LarkApp,
}

/// 凭证详情（serde 内部 tag，按类型区分字段集）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CredentialDetail {
    /// 飞书自建应用：app_id + app_secret（落库前加密）+ 可选事件回调配置
    LarkApp {
        /// 应用 ID
        app_id: String,
        /// 应用 Secret（落库前经 encrypt_channel_secret 加密）
        app_secret: String,
        /// Encrypt Key（可选，同样加密存储）
        encrypt_key: Option<String>,
        /// Verification Token（可选，事件回调校验用）
        verification_token: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lark_credential(id: &str, name: &str) -> UserIdentityCredential {
        UserIdentityCredential {
            id: id.to_string(),
            kind: CredentialKind::LarkApp,
            name: name.to_string(),
            created_at: "2026-08-12T00:00:00Z".to_string(),
            updated_at: "2026-08-12T00:00:00Z".to_string(),
            detail: CredentialDetail::LarkApp {
                app_id: "cli_a1b2c3".to_string(),
                app_secret: "enc:v1:secret".to_string(),
                encrypt_key: Some("enc:v1:key".to_string()),
                verification_token: None,
            },
        }
    }

    #[test]
    fn test_parse_empty_column() {
        assert_eq!(
            UserIdentityCredentials::parse(""),
            UserIdentityCredentials::default()
        );
        assert_eq!(
            UserIdentityCredentials::parse("   "),
            UserIdentityCredentials::default()
        );
        // 非法 JSON 容错为空库
        assert_eq!(
            UserIdentityCredentials::parse("not-json"),
            UserIdentityCredentials::default()
        );
    }

    #[test]
    fn test_column_round_trip() {
        let creds = UserIdentityCredentials {
            items: vec![
                lark_credential("cred-1", "工作应用"),
                lark_credential("cred-2", "测试应用"),
            ],
            default_credential_id: Some("cred-1".to_string()),
        };
        let column = creds.to_column_value();
        assert!(!column.is_empty());
        let parsed = UserIdentityCredentials::parse(&column);
        assert_eq!(parsed, creds);
    }

    #[test]
    fn test_legacy_column_without_default_field_parses() {
        // 存量 JSON 无 default_credential_id 字段 → serde(default) 容错
        let legacy = r#"{"items":[]}"#;
        let parsed = UserIdentityCredentials::parse(legacy);
        assert_eq!(parsed.default_credential_id, None);
    }

    #[test]
    fn test_empty_library_serializes_to_empty_string() {
        assert_eq!(UserIdentityCredentials::default().to_column_value(), "");
    }

    #[test]
    fn test_find_and_remove_by_id() {
        let mut creds = UserIdentityCredentials {
            items: vec![
                lark_credential("cred-1", "A"),
                lark_credential("cred-2", "B"),
            ],
            ..Default::default()
        };
        assert!(creds.find_by_id("cred-1").is_some());
        assert!(creds.find_by_id("cred-x").is_none());

        let removed = creds.remove_by_id("cred-1");
        assert_eq!(removed.unwrap().name, "A");
        assert_eq!(creds.items.len(), 1);
        assert!(creds.remove_by_id("cred-1").is_none());
    }

    #[test]
    fn test_detail_serde_tag() {
        let cred = lark_credential("cred-1", "A");
        let json = serde_json::to_value(&cred).unwrap();
        assert_eq!(json["detail"]["type"], "lark_app");
        assert_eq!(json["kind"], "lark_app");
        assert_eq!(json["detail"]["app_id"], "cli_a1b2c3");
    }

    #[test]
    fn test_resolve_lark_credential_ref() {
        let mut creds = UserIdentityCredentials {
            items: vec![lark_credential("cred-1", "A")],
            ..Default::default()
        };
        // 缺失/空白引用拒绝
        assert!(creds.resolve_lark_credential_ref(None).is_err());
        assert!(creds.resolve_lark_credential_ref(Some("  ")).is_err());
        // 不存在的引用拒绝
        assert!(creds.resolve_lark_credential_ref(Some("missing")).is_err());
        // 存在的 LarkApp 引用通过
        let cred = creds.resolve_lark_credential_ref(Some("cred-1")).unwrap();
        assert_eq!(cred.name, "A");

        // 非 LarkApp 类型拒绝（当前枚举仅 LarkApp，以构造非法 kind 不现实，
        // 改为验证空库拒绝）
        creds.items.clear();
        assert!(creds.resolve_lark_credential_ref(Some("cred-1")).is_err());
    }
}
