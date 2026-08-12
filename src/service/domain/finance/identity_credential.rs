//! 用户身份凭证子模块实现
//!
//! 身份凭证是驱动下游关键环节（渠道建联、lark_cli 工具身份）的资产，
//! 归属 finance domain 统一管理（含生命周期联动与飞书集成授权/绑定）。

use crate::pkg::RequestContext;
use crate::service::dal::user::UserDal;
use crate::service::domain::finance::FinanceDomainImpl;
use async_trait::async_trait;
use common::error::{Result, bail_err, err};
use common::models::{CredentialDetail, CredentialKind, UserIdentityCredential};

impl FinanceDomainImpl {
    /// 用户 DAL 引用（测试实例未注入时报内部错误）
    fn user_dal(&self) -> Result<&std::sync::Arc<dyn UserDal + Send + Sync>> {
        self.user_dal
            .as_ref()
            .ok_or_else(|| err!(Internal, "FinanceDomain user_dal 未注入"))
    }

    /// 加载用户凭证库（用户不存在报错，无凭证返回空库）
    async fn load_credential_library(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<common::models::UserIdentityCredentials> {
        self.user_dal()?
            .get_identity_credentials(ctx, user_id)
            .await?
            .ok_or_else(|| err!(NotFound, "用户不存在 user_id={}", user_id))
    }
}

/// 为 FinanceDomainImpl 实现 IdentityCredentialManage
#[async_trait]
impl super::IdentityCredentialManage for FinanceDomainImpl {
    /// 读取用户身份凭证库（用户不存在返回 None，无凭证返回空库）
    async fn get_identity_credentials(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<Option<common::models::UserIdentityCredentials>> {
        self.user_dal()?
            .get_identity_credentials(ctx, user_id)
            .await
    }

    /// 创建飞书应用凭证（secret 加密落库），返回凭证唯一 ID
    async fn create_lark_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        name: &str,
        app_id: &str,
        app_secret: &str,
        encrypt_key: Option<&str>,
        verification_token: Option<&str>,
    ) -> Result<String> {
        if name.trim().is_empty() {
            bail_err!(InvalidRequest, "凭证名称不能为空");
        }
        if app_id.trim().is_empty() || app_secret.trim().is_empty() {
            bail_err!(InvalidRequest, "飞书应用 App ID / App Secret 不能为空");
        }
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let credential_id = uuid::Uuid::now_v7().to_string();
        library.items.push(UserIdentityCredential {
            id: credential_id.clone(),
            kind: CredentialKind::LarkApp,
            name: name.trim().to_string(),
            created_at: now.clone(),
            updated_at: now,
            detail: CredentialDetail::LarkApp {
                app_id: app_id.trim().to_string(),
                app_secret: crate::pkg::crypto::encrypt_channel_secret(app_secret)?,
                encrypt_key: match encrypt_key.filter(|s| !s.trim().is_empty()) {
                    Some(v) => Some(crate::pkg::crypto::encrypt_channel_secret(v)?),
                    None => None,
                },
                verification_token: verification_token
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.trim().to_string()),
            },
        });
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;
        Ok(credential_id)
    }

    /// 更新飞书凭证（secret 非空时重新加密覆盖）+ 变更联动
    async fn update_lark_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        credential_id: &str,
        name: Option<&str>,
        app_id: Option<&str>,
        app_secret: Option<&str>,
        encrypt_key: Option<&str>,
        verification_token: Option<&str>,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let credential = library
            .find_by_id_mut(credential_id)
            .ok_or_else(|| err!(NotFound, "凭证不存在 credential_id={}", credential_id))?;
        if !matches!(credential.kind, CredentialKind::LarkApp) {
            bail_err!(InvalidRequest, "该凭证不是飞书应用凭证，无法按飞书凭证更新");
        }
        let CredentialDetail::LarkApp {
            app_id: old_app_id, ..
        } = &credential.detail;
        let old_app_id = old_app_id.clone();

        if let Some(n) = name.filter(|s| !s.trim().is_empty()) {
            credential.name = n.trim().to_string();
        }
        let app_secret_new = match app_secret.filter(|s| !s.trim().is_empty()) {
            Some(v) => Some(crate::pkg::crypto::encrypt_channel_secret(v)?),
            None => None,
        };
        let encrypt_key_new = match encrypt_key.filter(|s| !s.trim().is_empty()) {
            Some(v) => Some(crate::pkg::crypto::encrypt_channel_secret(v)?),
            None => None,
        };
        let app_id_new = app_id
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string());
        // secret 轮换标记（app_secret / encrypt_key 任一变更即需强制重建 WS 连接）
        let secret_changed = app_secret_new.is_some() || encrypt_key_new.is_some();
        {
            let CredentialDetail::LarkApp {
                app_id,
                app_secret: secret_slot,
                encrypt_key: encrypt_slot,
                verification_token: token_slot,
                ..
            } = &mut credential.detail;
            if let Some(v) = app_id_new {
                *app_id = v;
            }
            if let Some(v) = app_secret_new {
                *secret_slot = v;
            }
            if let Some(v) = encrypt_key_new {
                *encrypt_slot = Some(v);
            }
            if let Some(v) = verification_token {
                // 空白视为清除（None），非空则 trim 后覆盖
                *token_slot = Some(v.trim().to_string()).filter(|s| !s.is_empty());
            }
        }
        credential.updated_at = chrono::Utc::now().to_rfc3339();
        let CredentialDetail::LarkApp {
            app_id: new_app_id, ..
        } = &credential.detail;
        let new_app_id = new_app_id.clone();

        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;

        // 变更联动：清该用户 HOME 的 lark-cli config + WS 监听移交（失败仅告警）
        if let Some(lark_dal) = &self.lark_channel_dal {
            let home = crate::pkg::tool_registry::lark_cli::lark_home(
                &crate::config::get().base_data_path(),
                user_id,
            );
            if let Err(e) = crate::pkg::tool_registry::lark_cli::clear_cli_config(&home).await {
                log_warn!(
                    "lark credential update: clear cli config failed (ignored): user_id={} err={}",
                    user_id,
                    e
                );
            }
            lark_dal
                .handover_listeners_after_credential_change(
                    &old_app_id,
                    &new_app_id,
                    secret_changed,
                )
                .await;
        }
        Ok(())
    }

    /// 删除凭证（有渠道引用时报 Conflict；不联动删 HOME config，保留用户授权 token）
    async fn delete_lark_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        credential_id: &str,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        if library.find_by_id(credential_id).is_none() {
            bail_err!(NotFound, "凭证不存在 credential_id={}", credential_id);
        }
        if let Some(lark_dal) = &self.lark_channel_dal {
            let channels = lark_dal
                .find_channels_by_credential_id(credential_id)
                .await?;
            if !channels.is_empty() {
                bail_err!(
                    Conflict,
                    "凭证被 {} 个渠道引用，请先删除或更换引用渠道",
                    channels.len()
                );
            }
        }
        library.remove_by_id(credential_id);
        // 删掉的凭证若恰为默认，联动清除默认标记
        if library.default_credential_id.as_deref() == Some(credential_id) {
            library.default_credential_id = None;
        }
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;
        Ok(())
    }

    /// 设置默认飞书凭证（lark_cli 工具身份优先取引用该凭证的渠道）
    ///
    /// 空凭证 ID 表示取消默认；非空校验凭证存在且为 LarkApp 类型。
    async fn set_default_lark_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        credential_id: &str,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let trimmed = credential_id.trim();
        if trimmed.is_empty() {
            library.default_credential_id = None;
        } else {
            library.resolve_lark_credential_ref(Some(trimmed))?;
            library.default_credential_id = Some(trimmed.to_string());
        }
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await
    }

    // ==================== 飞书集成授权/绑定（handler 禁直调 pkg，经 Domain 包装） ====================

    /// 发起飞书用户授权 device flow（返回设备码 + 验证 URL）
    async fn lark_auth_start(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        domains: &[String],
    ) -> Result<crate::pkg::lark_integration::DeviceLoginStart> {
        crate::pkg::lark_integration::start_device_login(user_id, domains).await
    }

    /// 完成飞书用户授权 device flow
    async fn lark_auth_complete(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        device_code: &str,
    ) -> Result<crate::pkg::lark_integration::LarkAuthOutcome> {
        crate::pkg::lark_integration::complete_device_login(user_id, device_code).await
    }

    /// 查询飞书用户授权现状
    async fn lark_auth_status(
        &self,
        _ctx: RequestContext,
        user_id: &str,
    ) -> Result<crate::pkg::lark_integration::LarkAuthStatus> {
        crate::pkg::lark_integration::auth_status(user_id).await
    }

    /// 取消飞书用户授权（清本机登录态）
    async fn lark_auth_logout(
        &self,
        _ctx: RequestContext,
        user_id: &str,
    ) -> Result<crate::pkg::lark_integration::LarkAuthOutcome> {
        crate::pkg::lark_integration::auth_logout(user_id).await
    }

    /// 发起飞书应用自动绑定会话（返回 session_id + 验证 URL）
    async fn lark_bind_start(
        &self,
        _ctx: RequestContext,
        user_id: &str,
    ) -> Result<(String, String)> {
        crate::pkg::lark_integration::start_bind_session(user_id).await
    }

    /// 查询飞书应用绑定会话状态（会话不存在/非本人返回 None）
    async fn lark_bind_status(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        session_id: &str,
    ) -> Result<Option<crate::pkg::lark_integration::BindSessionSnapshot>> {
        crate::pkg::lark_integration::bind_session_status(user_id, session_id).await
    }

    /// 取消飞书应用绑定会话
    async fn lark_bind_cancel(
        &self,
        _ctx: RequestContext,
        user_id: &str,
        session_id: &str,
    ) -> Result<bool> {
        crate::pkg::lark_integration::cancel_bind_session(user_id, session_id).await
    }
}
