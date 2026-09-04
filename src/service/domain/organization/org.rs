//! 组织管理 trait 实现
//!
//! 定义组织相关业务接口实现

use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::models::organization_pairing_code::OrganizationPairingCodePo;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization_link::PeerOrgUpsert;
use async_trait::async_trait;
use chrono::Utc;
use common::api::{
    IssuePairingCodeResponse, OrganizationConfig, PAIRING_CODE_LEN, PAIRING_CODE_TTL_MS,
    PeerOrgDirectoryEntry, VerifyPairingCodeRequest, VerifyPairingCodeResponse,
};
use common::enums::{OrganizationStatus, UserRole};
use common::error::{Error, Result};
use rand::Rng;
use uuid::Uuid;

/// 生成组织 ID（12 位大写字母 + 数字）
fn generate_org_id() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const LEN: usize = 12;
    let mut rng = rand::thread_rng();
    (0..LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// 生成用户 ID（16 位大写字母 + 数字）
pub(super) fn generate_user_id() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    const LEN: usize = 16;
    let mut rng = rand::thread_rng();
    (0..LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[async_trait]
impl super::OrganizationManage for super::OrganizationDomainImpl {
    /// 检查系统是否已经初始化（即是否存在 Local 组织）
    ///
    /// 一台设备只允许有一个 Local 组织；Remote 组织可存在多个（记录交互信息）。
    async fn check_initialized(&self, ctx: RequestContext) -> Result<bool> {
        use crate::service::dao::organization::OrganizationQuery;
        use common::enums::OrganizationScope;

        let count = self
            .org_dal
            .count(
                ctx,
                OrganizationQuery {
                    scope: Some(OrganizationScope::Local),
                    ..Default::default()
                },
            )
            .await?;
        Ok(count > 0)
    }

    /// 创建组织 + Owner（超级管理员角色）
    ///
    /// 通用方法：可用于系统初始化，也可用于后续创建新组织。
    /// 返回 (organization_id, user_id)
    async fn create_org_and_owner(
        &self,
        ctx: RequestContext,
        params: common::api::InitializeSystemRequest,
    ) -> Result<(String, String)> {
        // 1. 创建组织
        let org_id = generate_org_id();
        let org = OrganizationPo::new(
            org_id.clone(),
            params.organization_name,
            params.description.unwrap_or_default(),
            None,
            org_id.clone(), // 系统初始化时由组织自己创建
        );
        self.org_dal.create(ctx.clone(), &org).await?;

        // 2. 创建超级管理员用户
        let user_id = generate_user_id();
        let user = UserPo::new(
            user_id.clone(),
            org_id.clone(),
            params.admin_username,
            params
                .admin_display_name
                .unwrap_or_else(|| "超级管理员".to_string()),
            params.admin_email.unwrap_or_default(),
            crate::pkg::password::hash_password(&params.admin_password)?,
            common::enums::UserRole::SuperAdmin,
            org_id.clone(), // 系统初始化时由组织创建
        );
        self.user_dal.create(ctx.clone(), &user).await?;

        Ok((org_id, user_id))
    }

    /// 获取组织信息
    async fn get_by_id(&self, ctx: RequestContext, org_id: &str) -> Result<Option<OrganizationPo>> {
        self.org_dal.get_by_id(ctx, org_id).await
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::organization::OrganizationQuery,
    ) -> Result<Vec<OrganizationPo>> {
        self.org_dal.query(ctx, query).await
    }

    /// 获取所有组织列表
    ///
    /// 调用 DAL 层 list_all 方法
    async fn list_all(&self, ctx: RequestContext) -> Result<Vec<OrganizationPo>> {
        self.org_dal.list_all(ctx).await
    }

    /// 根据邀请码获取组织（公开注册用，仅返回未删除的有效组织）
    ///
    /// 归一化规则：去首尾空白 + 统一大写（邀请码字符集本身全大写且不含易混淆的 I/O）
    async fn find_org_by_invite_code(
        &self,
        ctx: RequestContext,
        invite_code: &str,
    ) -> Result<Option<OrganizationPo>> {
        let code = invite_code.trim().to_ascii_uppercase();
        if code.is_empty() {
            return Ok(None);
        }
        self.org_dal.find_by_invite_code(ctx, &code).await
    }

    /// 更新组织信息
    async fn update(&self, ctx: RequestContext, org: &OrganizationPo) -> Result<()> {
        self.org_dal.update(ctx, org).await
    }

    /// 删除组织（软删除）
    async fn delete(&self, ctx: RequestContext, org_id: &str) -> Result<()> {
        self.org_dal.delete(ctx, org_id).await
    }

    /// 读取组织级配置（透传 DAL → DAO，带缓存）
    async fn get_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
    ) -> Result<OrganizationConfig> {
        self.org_dal.get_org_config(ctx, org_id).await
    }

    /// 写入组织级配置（透传 DAL → DAO，写穿缓存）
    async fn update_org_config(
        &self,
        ctx: RequestContext,
        org_id: &str,
        config: &OrganizationConfig,
    ) -> Result<()> {
        self.org_dal.update_org_config(ctx, org_id, config).await
    }

    /// 统计符合查询条件的组织数量（透传 DAL count）
    async fn count_organizations(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::organization::OrganizationQuery,
    ) -> Result<u64> {
        self.org_dal.count(ctx, query).await
    }

    /// 签发组网配对码（用户侧，需管理员权限）
    ///
    /// 生成 24 字符配对码（去 0/O/1/I）、10 分钟 TTL、单用途；仅存哈希，
    /// 返回明文 + 过期绝对时间。签发记审计（评审稿 §4.1 / §6.3）。
    async fn issue_pairing_code(&self, ctx: RequestContext) -> Result<IssuePairingCodeResponse> {
        // 1) 必须是本组织管理员（评审稿 §4.2：本端管理员 JWT）
        let role = ctx
            .user_role()
            .map(UserRole::from_i32)
            .unwrap_or(UserRole::Member);
        if !UserRole::has_permission(role, UserRole::Admin) {
            return Err(Error::forbidden("仅组织管理员可签发组网配对码"));
        }

        // 2) 取本组织 ID（JWT 绑定）
        let org_id = ctx
            .organization_id()
            .ok_or_else(|| Error::unauthorized("未识别的组织上下文"))?
            .to_string();

        // 3) 生成 24 字符配对码（去 0/O/1/I），仅存哈希
        let code = generate_pairing_code();
        let code_hash = sha256::digest(code.as_bytes());
        let now = Utc::now().timestamp_millis();
        let expires_at = now + PAIRING_CODE_TTL_MS;
        let created_by = ctx.caller_id().unwrap_or_else(|| org_id.clone());

        self.pairing_dao
            .insert(
                ctx.clone(),
                &OrganizationPairingCodePo {
                    id: Uuid::now_v7().to_string(),
                    org_id: org_id.clone(),
                    code_hash,
                    expires_at,
                    consumed_at: None,
                    created_by,
                    created_at: now,
                },
            )
            .await?;

        log_info!(
            &ctx,
            "issue_pairing_code",
            "组网配对码签发 org_id={} expires_at={}",
            org_id,
            expires_at
        );

        Ok(IssuePairingCodeResponse {
            pairing_code: code,
            expires_at,
            ttl_seconds: PAIRING_CODE_TTL_MS / 1000,
        })
    }

    /// 验证配对码 + 交换凭证（机器侧，配对码鉴权）
    ///
    /// 消费配对码（单用途 + TTL），生成对端出站 token，落对端 link + Linked 影子，
    /// 返回对端目录条目 + token。无效 / 过期 / 已用统一返回 unauthorized（防枚举）。
    async fn verify_pairing_code(
        &self,
        ctx: RequestContext,
        req: VerifyPairingCodeRequest,
    ) -> Result<VerifyPairingCodeResponse> {
        // 1) 原子消费配对码（单用途 + TTL 合一）；无效/过期/已用统一 None → 不区分（防枚举）
        let code_hash = sha256::digest(req.pairing_code.as_bytes());
        let now = Utc::now().timestamp_millis();
        let org_id = self
            .pairing_dao
            .consume(ctx.clone(), &code_hash, now)
            .await?
            .ok_or_else(|| Error::unauthorized("配对码无效或已失效"))?;

        // 2) 取签发方（本节点）组织信息用于返回
        let issuer = self
            .org_dal
            .get_by_id(ctx.clone(), &org_id)
            .await?
            .ok_or_else(|| Error::unauthorized("配对码关联组织不存在"))?;

        // 3) 生成对端出站 token（本地节点调用本节点时使用）
        let peer_token = generate_link_token();

        // 4) 落对端 link（幂等：已存在则续联更新凭证 / endpoint）
        let existing = self
            .link_dao
            .find_by_pair(ctx.clone(), &org_id, &req.local_org.id)
            .await?;
        let link = OrganizationLinkPo::new(
            existing
                .as_ref()
                .map(|l| l.id.clone())
                .unwrap_or_else(|| Uuid::now_v7().to_string()),
            org_id.clone(),
            req.local_org.id.clone(),
            req.local_endpoint.clone(),
            peer_token.clone(),
            sha256::digest(req.local_token.as_bytes()),
            org_id.clone(),
        );
        if existing.is_some() {
            self.link_dao.update(ctx.clone(), &link).await?;
        } else {
            self.link_dao.insert(ctx.clone(), &link).await?;
        }

        // 5) 写对端（本地节点）影子：直接建联必为 Linked（R5 保护本节点 Local 组织）
        let shadow = PeerOrgUpsert {
            id: req.local_org.id.clone(),
            name: req.local_org.name.clone(),
            description: req.local_org.description.clone(),
            base_url: req.local_org.base_url.clone(),
            group_name: req.local_org.group_name.clone(),
            status: OrganizationStatus::from_i32(req.local_org.status),
            updated_at: req.local_org.updated_at,
        };
        self.link_dao
            .upsert_linked_peer_org(ctx.clone(), &shadow)
            .await?;

        log_info!(
            &ctx,
            "verify_pairing_code",
            "配对码验证成功，建立连接 peer_org_id={} local_org_id={}",
            req.local_org.id,
            org_id
        );

        Ok(VerifyPairingCodeResponse {
            peer_org: PeerOrgDirectoryEntry {
                id: issuer.id,
                name: issuer.name,
                description: issuer.description,
                base_url: issuer.base_url,
                group_name: issuer.group_name,
                status: issuer.status.to_i32(),
                updated_at: issuer.updated_at,
            },
            peer_token,
        })
    }
}

/// 生成组网配对码（24 字符，去 0/O/1/I，字符集同邀请码）
fn generate_pairing_code() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..PAIRING_CODE_LEN)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// 生成链路凭证（64 字符 hex = 32 字节熵）
fn generate_link_token() -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut rng = rand::thread_rng();
    (0..64)
        .map(|_| {
            let idx = rng.gen_range(0..HEX.len());
            HEX[idx] as char
        })
        .collect()
}
