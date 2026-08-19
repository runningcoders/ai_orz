//! 用户身份凭证子模块实现
//!
//! 身份凭证是驱动下游关键环节（渠道建联、lark_cli 工具身份）的资产，
//! 归属 finance domain 统一管理（含生命周期联动与飞书集成授权/绑定）。
//! 存储为独立表 `user_credentials`（一凭证一行），Domain 经 UserDal 行级读写，
//! 默认标记作用域由凭据 visibility 派生（private=个人默认 / public=组织默认）。

use crate::models::user_credential::{UserCredential, UserCredentialPo};
use crate::pkg::RequestContext;
use crate::service::dal::user::UserDal;
use crate::service::dao::user_credential::UserCredentialQuery;
use crate::service::domain::finance::FinanceDomainImpl;
use async_trait::async_trait;
use common::api::PaginationParams;
use common::enums::UserRole;
use common::error::{Result, bail_err, err};
use common::models::{CredentialKind, CredentialVisibility};

/// 凭证列表分页上限（单用户凭证为个位数量级，1000 覆盖全量场景）
const CREDENTIAL_PAGE_LIMIT: usize = 1000;

impl FinanceDomainImpl {
    /// 用户 DAL 引用（测试实例未注入时报内部错误）
    fn user_dal(&self) -> Result<&std::sync::Arc<dyn UserDal + Send + Sync>> {
        self.user_dal
            .as_ref()
            .ok_or_else(|| err!(Internal, "FinanceDomain user_dal 未注入"))
    }

    /// 用户存在性检查（凭证资产防呆：目标用户不存在时报 NotFound）
    async fn ensure_user_exists(&self, ctx: RequestContext, user_id: &str) -> Result<()> {
        if self.user_dal()?.find_by_id(ctx, user_id).await?.is_none() {
            bail_err!(NotFound, "用户不存在 user_id={}", user_id);
        }
        Ok(())
    }

    /// 加载目标凭证并校验归属（不属于该用户按不存在处理，不泄露存在性）
    async fn load_owned_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        credential_id: &str,
    ) -> Result<UserCredential> {
        let credential = self
            .user_dal()?
            .find_credential_by_id(ctx, credential_id)
            .await?
            .ok_or_else(|| err!(NotFound, "凭证不存在 credential_id={}", credential_id))?;
        if credential.user_id() != user_id {
            bail_err!(NotFound, "凭证不存在 credential_id={}", credential_id);
        }
        Ok(credential)
    }

    /// 构造该用户的凭证查询（活跃凭证全量，按创建序）
    fn owned_credential_query(user_id: &str) -> UserCredentialQuery {
        UserCredentialQuery {
            user_id: Some(user_id.to_string()),
            pagination: PaginationParams {
                limit: Some(CREDENTIAL_PAGE_LIMIT),
                offset: Some(0),
            },
            ..Default::default()
        }
    }
}

/// 为 FinanceDomainImpl 实现 IdentityCredentialManage
#[async_trait]
impl super::IdentityCredentialManage for FinanceDomainImpl {
    /// 读取用户身份凭证列表（用户不存在返回 None，无凭证返回空列表）
    async fn get_identity_credentials(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<Option<Vec<UserCredential>>> {
        let user_dal = self.user_dal()?.clone();
        if user_dal.find_by_id(ctx.clone(), user_id).await?.is_none() {
            return Ok(None);
        }
        let page = user_dal
            .query_credentials(ctx, Self::owned_credential_query(user_id))
            .await?;
        Ok(Some(page.items))
    }

    // ==================== 统一凭证 CRUD（类型差异经 detail 行为 + match kind 分发） ====================

    /// 创建凭证（明文 detail → 规范化/校验/加密落库，private 起步）
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
        self.ensure_user_exists(ctx.clone(), user_id).await?;
        let credential_id = uuid::Uuid::now_v7().to_string();
        let po = UserCredentialPo::new(
            credential_id.clone(),
            ctx.organization_id().cloned().unwrap_or_default(),
            user_id.to_string(),
            kind,
            name,
            detail,
            CredentialVisibility::Private,
            ctx.caller_id_or_system(),
        );
        user_dal
            .insert_credential(ctx, &UserCredential::from_po(po))
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
        let mut credential = self
            .load_owned_credential(ctx.clone(), user_id, &cmd.credential_id)
            .await?;
        if let Some(n) = cmd.name.as_deref().filter(|s| !s.trim().is_empty()) {
            credential.po.name = n.trim().to_string();
        }
        let old_primary_id = credential
            .detail()
            .primary_id()
            .unwrap_or_default()
            .to_string();
        let kind = credential.kind();
        let impact = credential.po.detail.0.apply_patch(cmd.patch, |s: &str| {
            crate::pkg::crypto::encrypt_channel_secret(s)
        })?;
        credential.po.modified_by = ctx.caller_id_or_system();
        let new_primary_id = credential
            .detail()
            .primary_id()
            .unwrap_or_default()
            .to_string();

        user_dal.update_credential(ctx.clone(), &credential).await?;

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
        let credential = self
            .load_owned_credential(ctx.clone(), user_id, credential_id)
            .await?;

        // 类型分发：前置检查（Lark 渠道引用 / GitHub 生效凭证快照）
        let github_was_active = match credential.kind() {
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
            CredentialKind::GithubToken => user_dal
                .find_default_credential(ctx.clone(), user_id, CredentialKind::GithubToken)
                .await?
                .is_some_and(|active| active.id() == credential_id),
            // TavilyKey：无渠道引用与本地运行态，直接删除
            CredentialKind::TavilyKey => false,
        };

        // 软删（DAO 联动清默认标记：删掉的凭证若为默认，对应作用域默认槽位自动空出）
        user_dal
            .soft_delete_credential(ctx.clone(), credential_id)
            .await?;

        // 类型分发：后置联动（失败仅告警；剩余凭证存在时下次调用自动重建）
        if credential.kind() == CredentialKind::GithubToken && github_was_active {
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

    /// 设置默认凭证（作用域由目标凭据 visibility 派生：private=个人默认 / public=组织默认）
    async fn set_default_credential(
        &self,
        ctx: RequestContext,
        user_id: &str,
        kind: CredentialKind,
        credential_id: Option<&str>,
    ) -> Result<()> {
        let user_dal = self.user_dal()?.clone();
        match credential_id.map(str::trim).filter(|s| !s.is_empty()) {
            // None/空白：取消该用户该类型个人默认（幂等）
            None => user_dal.clear_default_credential(ctx, user_id, kind).await,
            Some(credential_id) => {
                let credential = self
                    .load_owned_credential(ctx.clone(), user_id, credential_id)
                    .await?;
                if credential.kind() != kind {
                    bail_err!(
                        InvalidRequest,
                        "凭证类型不匹配：目标凭证为 {:?}，请求类型为 {:?}",
                        credential.kind(),
                        kind
                    );
                }
                // 权限门控：private=个人默认仅所有者本人可设；
                // public=组织默认需 org 管理权限（Admin+），防成员劫持组织默认
                match credential.visibility() {
                    CredentialVisibility::Private => {
                        if ctx.uid() != user_id {
                            bail_err!(Forbidden, "个人默认凭证仅所有者本人可设置");
                        }
                    }
                    CredentialVisibility::Public => {
                        let role = ctx
                            .user_role
                            .map(UserRole::from)
                            .unwrap_or(UserRole::Member);
                        if !UserRole::has_permission(role, UserRole::Admin) {
                            bail_err!(Forbidden, "设置组织默认凭证需要管理员权限");
                        }
                    }
                }
                // DAO 同事务「清同作用域旧默认 → 立新默认」（双部分唯一索引兜底并发）
                user_dal.set_default_credential(ctx, credential_id).await
            }
        }
    }

    /// GitHub 集成状态聚合（凭证快照 + gh 登录态实测）
    async fn github_integration_status(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<common::api::GithubIntegrationStatusResponse> {
        let user_dal = self.user_dal()?.clone();
        let mut query = Self::owned_credential_query(user_id);
        query.kind = Some(CredentialKind::GithubToken);
        let page = user_dal.query_credentials(ctx, query).await?;
        let mut credentials = Vec::new();
        for credential in page.items {
            let common::models::CredentialDetail::GithubToken { token } = credential.detail()
            else {
                continue;
            };
            // token 尾号（解密失败按空串处理，不阻断状态聚合）
            let token_tail = crate::pkg::crypto::decrypt_channel_secret(token)
                .map(|plain| {
                    plain
                        .chars()
                        .rev()
                        .take(4)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect()
                })
                .unwrap_or_default();
            credentials.push(common::api::GithubCredentialSnapshot {
                credential_id: credential.id().to_string(),
                name: credential.name().to_string(),
                token_tail,
                is_default: credential.po.is_default,
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

    /// Tavily 集成状态聚合（凭证快照 + 共享 key 配置状态，双轨授权的实例侧可见性）
    async fn tavily_integration_status(
        &self,
        ctx: RequestContext,
        user_id: &str,
    ) -> Result<common::api::TavilyIntegrationStatusResponse> {
        let user_dal = self.user_dal()?.clone();
        let mut query = Self::owned_credential_query(user_id);
        query.kind = Some(CredentialKind::TavilyKey);
        let page = user_dal.query_credentials(ctx, query).await?;
        let mut credentials = Vec::new();
        for credential in page.items {
            let common::models::CredentialDetail::TavilyKey { api_key } = credential.detail()
            else {
                continue;
            };
            // key 尾号（解密失败按空串处理，不阻断状态聚合）
            let api_key_tail = crate::pkg::crypto::decrypt_channel_secret(api_key)
                .map(|plain| {
                    plain
                        .chars()
                        .rev()
                        .take(4)
                        .collect::<String>()
                        .chars()
                        .rev()
                        .collect()
                })
                .unwrap_or_default();
            credentials.push(common::api::TavilyCredentialSnapshot {
                credential_id: credential.id().to_string(),
                name: credential.name().to_string(),
                api_key_tail,
                is_default: credential.po.is_default,
            });
        }
        Ok(common::api::TavilyIntegrationStatusResponse {
            credentials,
            shared_key_configured: !crate::config::get().tavily.api_key.trim().is_empty(),
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
