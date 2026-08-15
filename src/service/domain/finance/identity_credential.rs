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

    // ==================== 统一凭证 CRUD（类型差异经 detail 行为 + match kind 分发） ====================

    /// 创建凭证（明文 detail → 规范化/校验/加密落库）
    async fn create_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        cmd: super::CreateCredentialCmd,
    ) -> Result<String> {
        let name = cmd.name.trim().to_string();
        if name.is_empty() {
            bail_err!(InvalidRequest, "凭证名称不能为空");
        }
        let detail = cmd.detail.normalized();
        detail.validate()?;
        let kind = detail.kind();
        let detail =
            detail.encrypt_sensitive(|s: &str| crate::pkg::crypto::encrypt_channel_secret(s))?;

        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let credential_id = uuid::Uuid::now_v7().to_string();
        library.items.push(UserIdentityCredential {
            id: credential_id.clone(),
            kind,
            name,
            created_at: now.clone(),
            updated_at: now,
            detail,
        });
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;
        Ok(credential_id)
    }

    /// 更新凭证（补丁语义 + 类型分发联动）
    async fn update_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        cmd: super::UpdateCredentialCmd,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let credential = library
            .find_by_id_mut(&cmd.credential_id)
            .ok_or_else(|| err!(NotFound, "凭证不存在 credential_id={}", cmd.credential_id))?;
        if let Some(n) = cmd.name.as_deref().filter(|s| !s.trim().is_empty()) {
            credential.name = n.trim().to_string();
        }
        let old_primary_id = credential
            .detail
            .primary_id()
            .unwrap_or_default()
            .to_string();
        let kind = credential.kind;
        let impact = credential.detail.apply_patch(
            cmd.patch,
            |s: &str| crate::pkg::crypto::encrypt_channel_secret(s),
        )?;
        credential.updated_at = chrono::Utc::now().to_rfc3339();
        let new_primary_id = credential
            .detail
            .primary_id()
            .unwrap_or_default()
            .to_string();

        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;

        // 类型分发：更新后联动（失败仅告警）
        if kind == CredentialKind::LarkApp
            && let Some(lark_dal) = &self.lark_channel_dal
        {
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
                    &old_primary_id,
                    &new_primary_id,
                    impact.secret_changed,
                )
                .await;
        }
        // GithubToken：token 轮换无需显式清登录态（gh_cli marker 指纹机制自动重登录）
        Ok(())
    }

    /// 删除凭证（前置检查 + 后置联动均按类型分发）
    async fn delete_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        credential_id: &str,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        let Some(credential) = library.find_by_id(credential_id).cloned() else {
            bail_err!(NotFound, "凭证不存在 credential_id={}", credential_id);
        };

        // 类型分发：前置检查（Lark 渠道引用 / GitHub 生效凭证快照）
        let github_was_active = match credential.kind {
            CredentialKind::LarkApp => {
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
                false
            }
            CredentialKind::GithubToken => library
                .resolve_github_credential()
                .is_some_and(|c| c.id == credential_id),
        };

        library.remove_by_id(credential_id);
        // 删掉的凭证若恰为该类型默认，联动清除对应默认槽位
        library.clear_default_for(credential.kind, credential_id);
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await?;

        // 类型分发：后置联动（失败仅告警；剩余凭证存在时下次调用自动重建）
        if credential.kind == CredentialKind::GithubToken && github_was_active {
            let home = crate::pkg::tool_registry::gh_cli::gh_home(
                &crate::config::get().base_data_path(),
                user_id,
            );
            if let Err(e) = crate::pkg::tool_registry::gh_cli::clear_gh_auth(&home).await {
                log_warn!(
                    "github credential delete: clear gh auth failed (ignored): user_id={} err={}",
                    user_id,
                    e
                );
            }
        }
        // LarkApp：不联动删 HOME config（保留用户授权 token）
        Ok(())
    }

    /// 设置默认凭证（各类型默认槽位独立）
    async fn set_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
        credential_id: Option<&str>,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        let mut library = self.load_credential_library(ctx.clone(), user_id).await?;
        library.set_default_for(kind, credential_id.map(|s| s.to_string()))?;
        user_dal
            .save_identity_credentials(ctx, user_id, &library)
            .await
    }

    /// GitHub 集成状态聚合（凭证快照 + gh 登录态实测）
    async fn github_integration_status(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<common::api::GithubIntegrationStatusResponse> {
        let library = self.load_credential_library(ctx, user_id).await?;
        let default_id = library.default_github_credential_id.clone();
        let mut credentials = Vec::new();
        for credential in library.items.iter().filter(|c| matches!(c.kind, CredentialKind::GithubToken)) {
            let CredentialDetail::GithubToken { token } = &credential.detail else {
                continue;
            };
            // token 尾号（解密失败按空串处理，不阻断状态聚合）
            let token_tail = crate::pkg::crypto::decrypt_channel_secret(token)
                .map(|plain| plain.chars().rev().take(4).collect::<String>().chars().rev().collect())
                .unwrap_or_default();
            credentials.push(common::api::GithubCredentialSnapshot {
                credential_id: credential.id.clone(),
                name: credential.name.clone(),
                token_tail,
                is_default: default_id.as_deref() == Some(credential.id.as_str()),
            });
        }
        let home = crate::pkg::tool_registry::gh_cli::gh_home(
            &crate::config::get().base_data_path(),
            user_id,
        );
        let auth = crate::pkg::tool_registry::gh_cli::gh_auth_status(&home).await;
        Ok(common::api::GithubIntegrationStatusResponse {
            credentials,
            auth: common::api::GithubAuthSnapshot {
                logged_in: auth.logged_in,
                user_name: auth.user_name,
                hint: auth.hint,
            },
        })
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
