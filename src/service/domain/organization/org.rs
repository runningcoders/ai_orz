//! 组织管理 trait 实现
//!
//! 定义组织相关业务接口实现

use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::models::organization_pairing_code::OrganizationPairingCodePo;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::PeerOrgUpsert;
use crate::service::dao::organization_link::OrganizationLinkQuery;
use async_trait::async_trait;
use chrono::Utc;
use common::api::{
    CreateLinkRequest, CreateLinkResponse, IssuePairingCodeResponse, LinkItem, ListLinksResponse,
    OrganizationConfig, PAIRING_CODE_LEN, PAIRING_CODE_TTL_MS, PeerOrgDirectoryEntry,
    VerifyPairingCodeRequest, VerifyPairingCodeResponse,
};
use common::enums::organization::OrganizationLinkStatus;
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

        // 4) 落本端 link（幂等：已存在则续联更新凭证 / endpoint）
        //
        // 凭证流向（D6 双向独立凭证）：
        // - access_token = 调用方为对端生成的 local_token（本节点出站调用对端时携带，
        //   对端存其哈希校验入站）→ 存明文
        // - peer_token_hash = 本节点生成的 peer_token 的哈希（对端出站调用本节点时
        //   携带 peer_token，本节点据此校验入站）→ 存哈希
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
            req.local_token.clone(),
            sha256::digest(peer_token.as_bytes()),
            org_id.clone(),
        );
        if existing.is_some() {
            self.link_dao.update(ctx.clone(), &link).await?;
        } else {
            self.link_dao.insert(ctx.clone(), &link).await?;
        }

        // 5) 写对端（本地节点）影子：直接建联必为 Linked（R5 保护本节点 Local 组织）
        //    走 org DAL 静默方法：影子是复制不是业务变更，不发布 organization.changed
        let shadow = PeerOrgUpsert {
            id: req.local_org.id.clone(),
            name: req.local_org.name.clone(),
            description: req.local_org.description.clone(),
            base_url: req.local_org.base_url.clone(),
            group_name: req.local_org.group_name.clone(),
            status: OrganizationStatus::from_i32(req.local_org.status),
            updated_at: req.local_org.updated_at,
        };
        self.org_dal
            .upsert_linked_shadow(ctx.clone(), &shadow)
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

    /// 发起建联（用户侧，JWT）
    ///
    /// 凭对端配对码出站调对端 verify 完成双向凭证交换，落本地 link + Linked 影子。
    /// 本端联邦地址由 adapter 层从配置解析后传入（Domain 不读全局配置单例）。
    async fn create_link(
        &self,
        ctx: RequestContext,
        req: CreateLinkRequest,
        local_endpoint: String,
    ) -> Result<CreateLinkResponse> {
        let pairing_code = req.pairing_code.trim().to_string();
        let peer_endpoint = req.peer_endpoint.trim().trim_end_matches('/').to_string();
        if pairing_code.is_empty() {
            return Err(Error::bad_request("配对码不能为空"));
        }
        if peer_endpoint.is_empty() {
            return Err(Error::bad_request("对端地址不能为空"));
        }

        // 1) 本端组织（JWT 绑定）
        let org_id = ctx
            .organization_id()
            .ok_or_else(|| Error::unauthorized("未识别的组织上下文"))?
            .to_string();
        let local_org = self
            .org_dal
            .get_by_id(ctx.clone(), &org_id)
            .await?
            .ok_or_else(|| Error::unauthorized("当前登录身份已失效（组织不存在）"))?;

        // 2) 生成为对端准备的入站校验凭证（对端出站调用本端时携带，本端存哈希校验）
        let local_token = generate_link_token();

        // 3) 出站调对端 verify：验证配对码 + 交换凭证
        let verify_req = VerifyPairingCodeRequest {
            pairing_code,
            local_org: PeerOrgDirectoryEntry {
                id: local_org.id.clone(),
                name: local_org.name.clone(),
                description: local_org.description.clone(),
                base_url: local_org.base_url.clone(),
                group_name: local_org.group_name.clone(),
                status: local_org.status.to_i32(),
                updated_at: local_org.updated_at,
            },
            local_endpoint: local_endpoint.clone(),
            local_token: local_token.clone(),
        };
        let resp = self
            .http_client
            .verify_pairing_code(&peer_endpoint, &verify_req)
            .await?;

        // 4) 防自联（对端 id 与本端相同 = 配置错误或恶意对端）
        if resp.peer_org.id == org_id {
            return Err(Error::bad_request("对端组织与本端组织相同，拒绝建联"));
        }

        // 5) 落本端 link（幂等续联）：access_token = 对端为本端生成的 peer_token
        //    （本端出站调用对端时携带），peer_token_hash = 本端生成的 local_token 哈希
        //    （对端出站调用本端时携带，本端据此校验入站）
        let existing = self
            .link_dao
            .find_by_pair(ctx.clone(), &org_id, &resp.peer_org.id)
            .await?;
        let link = OrganizationLinkPo::new(
            existing
                .as_ref()
                .map(|l| l.id.clone())
                .unwrap_or_else(|| Uuid::now_v7().to_string()),
            org_id.clone(),
            resp.peer_org.id.clone(),
            peer_endpoint.clone(),
            resp.peer_token.clone(),
            sha256::digest(local_token.as_bytes()),
            ctx.caller_id().unwrap_or_else(|| org_id.clone()),
        );
        if existing.is_some() {
            self.link_dao.update(ctx.clone(), &link).await?;
        } else {
            self.link_dao.insert(ctx.clone(), &link).await?;
        }

        // 6) 写对端影子：直接建联必为 Linked（R5 保护本端 Local 组织）
        let shadow = PeerOrgUpsert {
            id: resp.peer_org.id.clone(),
            name: resp.peer_org.name.clone(),
            description: resp.peer_org.description.clone(),
            base_url: resp.peer_org.base_url.clone(),
            group_name: resp.peer_org.group_name.clone(),
            status: OrganizationStatus::from_i32(resp.peer_org.status),
            updated_at: resp.peer_org.updated_at,
        };
        self.org_dal
            .upsert_linked_shadow(ctx.clone(), &shadow)
            .await?;

        // 7) 目录双向同步（评审稿 §4.1 步骤 5 / §5.2）：拉对端全量目录 + 推本地目录。
        //    best-effort：目录同步失败不回滚建联（契约已落库，可由下次同步补齐），仅记审计。
        self.sync_directories_after_link(&ctx, &peer_endpoint, &resp.peer_token)
            .await;

        log_info!(
            &ctx,
            "create_link",
            "组织建联成功 local_org_id={} peer_org_id={} endpoint={}",
            org_id,
            resp.peer_org.id,
            peer_endpoint
        );

        Ok(CreateLinkResponse {
            link: LinkItem {
                peer_org: resp.peer_org,
                endpoint: link.endpoint,
                status: link.status.to_i32(),
                created_at: link.created_at,
            },
        })
    }

    /// 已建联列表（用户侧，JWT，前端"关联组织"页数据源）
    async fn list_links(&self, ctx: RequestContext) -> Result<ListLinksResponse> {
        let org_id = ctx
            .organization_id()
            .ok_or_else(|| Error::unauthorized("未识别的组织上下文"))?
            .to_string();

        let links = self
            .link_dao
            .query(
                ctx.clone(),
                OrganizationLinkQuery {
                    local_org_id: Some(org_id),
                    status: None,
                    limit: Some(200),
                },
            )
            .await?;

        let mut items = Vec::with_capacity(links.len());
        for link in links {
            // 对端目录条目读库内影子/本端组织行（links 与 organizations 的不变量：
            // scope == Linked ⇔ organization_links 存在记录）
            let Some(peer) = self
                .org_dal
                .get_by_id(ctx.clone(), &link.peer_org_id)
                .await?
            else {
                // 防御：影子行缺失时不渲染该条目（不变量被外力破坏的场景）
                continue;
            };
            items.push(LinkItem {
                peer_org: PeerOrgDirectoryEntry {
                    id: peer.id,
                    name: peer.name,
                    description: peer.description,
                    base_url: peer.base_url,
                    group_name: peer.group_name,
                    status: peer.status.to_i32(),
                    updated_at: peer.updated_at,
                },
                endpoint: link.endpoint,
                status: link.status.to_i32(),
                created_at: link.created_at,
            });
        }

        // Active 在前，同状态按建联时间倒序
        items.sort_by(|a, b| {
            b.status
                .cmp(&a.status)
                .then(b.created_at.cmp(&a.created_at))
        });

        Ok(ListLinksResponse { links: items })
    }

    /// 机器侧端点契约凭证鉴权
    ///
    /// 无效/吊销凭证统一 unauthorized，不区分「不存在/已断联」（防枚举）。
    async fn authenticate_link_call(
        &self,
        ctx: RequestContext,
        credential: &str,
    ) -> Result<OrganizationLinkPo> {
        if credential.trim().is_empty() {
            return Err(Error::unauthorized("缺少联邦契约凭证"));
        }
        let hash = sha256::digest(credential.trim().as_bytes());
        self.link_dao
            .find_active_by_peer_token_hash(ctx, &hash)
            .await?
            .ok_or_else(|| Error::unauthorized("联邦契约凭证无效"))
    }

    /// 本节点组织目录（白名单字段）
    async fn get_directory(&self, ctx: RequestContext) -> Result<Vec<PeerOrgDirectoryEntry>> {
        let orgs = self.org_dal.list_all(ctx).await?;
        Ok(orgs
            .into_iter()
            .map(|org| PeerOrgDirectoryEntry {
                id: org.id,
                name: org.name,
                description: org.description,
                base_url: org.base_url,
                group_name: org.group_name,
                status: org.status.to_i32(),
                updated_at: org.updated_at,
            })
            .collect())
    }

    /// 接收对端推送的目录（逐条 Remote 影子 upsert，评审稿 §5.2）
    async fn handle_directory_sync(
        &self,
        ctx: RequestContext,
        req: common::api::DirectorySyncRequest,
    ) -> Result<usize> {
        let mut written = 0usize;
        for entry in &req.orgs {
            let upsert = PeerOrgUpsert {
                id: entry.id.clone(),
                name: entry.name.clone(),
                description: entry.description.clone(),
                base_url: entry.base_url.clone(),
                group_name: entry.group_name.clone(),
                status: OrganizationStatus::from_i32(entry.status),
                updated_at: entry.updated_at,
            };
            if self
                .org_dal
                .upsert_remote_shadow(ctx.clone(), &upsert)
                .await?
            {
                written += 1;
            }
        }

        log_info!(
            &ctx,
            "directory_sync",
            "目录同步完成 received={} written={}",
            req.orgs.len(),
            written
        );
        Ok(written)
    }

    /// 断联（本端管理员）：连接 Revoked + 对端影子降级（org DAL 组合方法，不删除记录）
    async fn revoke_link(&self, ctx: RequestContext, peer_org_id: &str) -> Result<()> {
        // 1) 必须是本组织管理员（评审稿 §4.2：本端管理员 JWT）
        let role = ctx
            .user_role()
            .map(UserRole::from_i32)
            .unwrap_or(UserRole::Member);
        if !UserRole::has_permission(role, UserRole::Admin) {
            return Err(Error::forbidden("仅组织管理员可断联"));
        }

        // 2) 取本组织与连接（不存在 → 404）
        let org_id = ctx
            .organization_id()
            .ok_or_else(|| Error::unauthorized("未识别的组织上下文"))?
            .to_string();
        let link = self
            .link_dao
            .find_by_pair(ctx.clone(), &org_id, peer_org_id)
            .await?
            .ok_or_else(|| Error::not_found(format!("未找到与组织 {} 的连接", peer_org_id)))?;

        // 3) 断联（org DAL 组合：link → Revoked + 对端影子 Linked → Remote，不删除记录；
        //    两步各自幂等，第二步失败重试本方法即可修复）
        self.org_dal
            .revoke_link(ctx.clone(), &link.id, peer_org_id)
            .await?;

        log_info!(
            &ctx,
            "revoke_link",
            "组织断联成功 local_org_id={} peer_org_id={} link_id={}",
            org_id,
            peer_org_id,
            link.id
        );
        Ok(())
    }

    async fn push_directory_to_peers(&self, ctx: RequestContext) -> Result<usize> {
        let peers = self.active_peer_endpoints(&ctx).await?;
        let dir = self.get_directory(ctx.clone()).await?;

        let mut pushed = 0usize;
        for (endpoint, access_token, peer_org_id) in &peers {
            match self
                .http_client
                .push_directory(endpoint, access_token, dir.clone())
                .await
            {
                Ok(()) => pushed += 1,
                Err(e) => log_warn!(
                    ctx,
                    "directory_push",
                    "变更推送失败(不重试,由对账补齐) peer_org_id={} endpoint={} error={}",
                    peer_org_id,
                    endpoint,
                    e
                ),
            }
        }

        log_info!(
            ctx,
            "directory_push",
            "组织变更推送完成 peers={} pushed={}",
            peers.len(),
            pushed
        );
        Ok(pushed)
    }

    async fn reconcile_directories(
        &self,
        ctx: RequestContext,
    ) -> Result<super::DirectoryReconcileReport> {
        let peers = self.active_peer_endpoints(&ctx).await?;
        let dir = self.get_directory(ctx.clone()).await?;

        let mut pushed = 0usize;
        let mut pulled_written = 0usize;
        for (endpoint, access_token, peer_org_id) in &peers {
            // 推：本地目录 → 对端（对端按其影子语义 upsert）
            match self
                .http_client
                .push_directory(endpoint, access_token, dir.clone())
                .await
            {
                Ok(()) => pushed += 1,
                Err(e) => log_warn!(
                    ctx,
                    "directory_reconcile",
                    "对账推送失败 peer_org_id={} endpoint={} error={}",
                    peer_org_id,
                    endpoint,
                    e
                ),
            }

            // 拉：对端目录 → 本地影子 upsert（新者胜 / 不动 scope / 保护 Local）
            match self
                .http_client
                .fetch_directory(endpoint, access_token)
                .await
            {
                Ok(entries) => {
                    let count = entries.len();
                    let req = common::api::DirectorySyncRequest { orgs: entries };
                    match self.handle_directory_sync(ctx.clone(), req).await {
                        Ok(written) => pulled_written += written,
                        Err(e) => log_warn!(
                            ctx,
                            "directory_reconcile",
                            "对账拉取 upsert 失败 peer_org_id={} received={} error={}",
                            peer_org_id,
                            count,
                            e
                        ),
                    }
                }
                Err(e) => log_warn!(
                    ctx,
                    "directory_reconcile",
                    "对账拉取失败 peer_org_id={} endpoint={} error={}",
                    peer_org_id,
                    endpoint,
                    e
                ),
            }
        }

        log_info!(
            ctx,
            "directory_reconcile",
            "目录对账完成 peers={} pushed={} pulled_written={}",
            peers.len(),
            pushed,
            pulled_written
        );
        Ok(super::DirectoryReconcileReport {
            peers: peers.len(),
            pushed,
            pulled_written,
        })
    }
}

impl super::OrganizationDomainImpl {
    /// 收集所有 Active 连接的去重对端列表（endpoint 去重，同一节点多组织建联只推一次）
    ///
    /// 返回 `(endpoint, access_token, peer_org_id)`；同一 endpoint 取任一 Active
    /// 连接的出站凭证（每个连接的 token 均被对端单独签发且有效）。
    async fn active_peer_endpoints(
        &self,
        ctx: &RequestContext,
    ) -> Result<Vec<(String, String, String)>> {
        let links = self
            .link_dao
            .query(
                ctx.clone(),
                OrganizationLinkQuery {
                    local_org_id: None,
                    status: Some(OrganizationLinkStatus::Active),
                    limit: None,
                },
            )
            .await?;

        let mut seen = std::collections::HashSet::new();
        let mut peers = Vec::new();
        for link in links {
            if seen.insert(link.endpoint.clone()) {
                peers.push((link.endpoint, link.access_token, link.peer_org_id));
            }
        }
        Ok(peers)
    }

    /// 建联完成后的目录双向同步（评审稿 §4.1 步骤 5 / §5.2）
    ///
    /// 拉取对端全量目录（→ 本地 Remote 影子 upsert）+ 推送本地目录（→ 对端
    /// 按其对端侧语义 upsert）。一次建联由发起方完成双向交换，对端无需补动作。
    /// 失败仅记 WARN 审计，不向调用方报错（契约已落库，目录可由下次同步补齐）。
    async fn sync_directories_after_link(
        &self,
        ctx: &RequestContext,
        peer_endpoint: &str,
        access_token: &str,
    ) {
        use super::OrganizationManage as _;

        // 拉：对端目录 → 本地 Remote 影子 upsert（新者胜 / 不动 scope / 保护 Local）
        let pulled = self
            .http_client
            .fetch_directory(peer_endpoint, access_token)
            .await;
        match pulled {
            Ok(entries) => {
                let count = entries.len();
                let req = common::api::DirectorySyncRequest { orgs: entries };
                match self.handle_directory_sync(ctx.clone(), req).await {
                    Ok(written) => log_info!(
                        ctx,
                        "directory_pull",
                        "建联后拉取对端目录成功 endpoint={} received={} written={}",
                        peer_endpoint,
                        count,
                        written
                    ),
                    Err(e) => log_warn!(
                        ctx,
                        "directory_pull",
                        "建联后目录 upsert 失败(建联不受影响) endpoint={} error={}",
                        peer_endpoint,
                        e
                    ),
                }
            }
            Err(e) => log_warn!(
                ctx,
                "directory_pull",
                "建联后拉取对端目录失败(建联不受影响) endpoint={} error={}",
                peer_endpoint,
                e
            ),
        }

        // 推：本地目录 → 对端
        match self.get_directory(ctx.clone()).await {
            Ok(orgs) => {
                if let Err(e) = self
                    .http_client
                    .push_directory(peer_endpoint, access_token, orgs)
                    .await
                {
                    log_warn!(
                        ctx,
                        "directory_push",
                        "建联后推送本地目录失败(建联不受影响) endpoint={} error={}",
                        peer_endpoint,
                        e
                    );
                }
            }
            Err(e) => log_warn!(
                ctx,
                "directory_push",
                "建联后构建本地目录失败(建联不受影响) error={}",
                e
            ),
        }
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

#[cfg(test)]
mod directory_sync_tests {
    use super::*;
    use crate::pkg::request_context_test_support::new_test_ctx;
    use crate::service::dao::organization_link::http::FederationHttpClient;
    use sqlx::SqlitePool;
    use std::sync::{Arc, Mutex};

    use super::super::OrganizationManage as _;

    /// 出站客户端 mock：记录推送调用，拉取返回预设目录
    struct MockFederationClient {
        pushes: Mutex<Vec<(String, String, Vec<String>)>>,
        pulls: Mutex<Vec<(String, String)>>,
        fetched_dir: Vec<PeerOrgDirectoryEntry>,
    }

    impl MockFederationClient {
        fn new(fetched_dir: Vec<PeerOrgDirectoryEntry>) -> Self {
            Self {
                pushes: Mutex::new(Vec::new()),
                pulls: Mutex::new(Vec::new()),
                fetched_dir,
            }
        }
    }

    #[async_trait::async_trait]
    impl FederationHttpClient for MockFederationClient {
        async fn verify_pairing_code(
            &self,
            _peer_endpoint: &str,
            _req: &VerifyPairingCodeRequest,
        ) -> Result<VerifyPairingCodeResponse> {
            Err(Error::internal("mock: verify 未在本测试使用"))
        }

        async fn fetch_directory(
            &self,
            peer_endpoint: &str,
            access_token: &str,
        ) -> Result<Vec<PeerOrgDirectoryEntry>> {
            self.pulls
                .lock()
                .unwrap()
                .push((peer_endpoint.to_string(), access_token.to_string()));
            Ok(self.fetched_dir.clone())
        }

        async fn push_directory(
            &self,
            peer_endpoint: &str,
            access_token: &str,
            orgs: Vec<PeerOrgDirectoryEntry>,
        ) -> Result<()> {
            let names = orgs.iter().map(|o| o.name.clone()).collect();
            self.pushes.lock().unwrap().push((
                peer_endpoint.to_string(),
                access_token.to_string(),
                names,
            ));
            Ok(())
        }
    }

    fn build_domain(mock: Arc<MockFederationClient>) -> Arc<super::super::OrganizationDomainImpl> {
        use crate::service::dao::organization_link as link_dao_mod;
        use crate::service::dao::organization_pairing as pairing_dao_mod;
        use crate::service::dao::{organization as org_dao_mod, user as user_dao_mod};

        Arc::new(super::super::OrganizationDomainImpl::new(
            crate::service::dal::organization::new(org_dao_mod::new(), link_dao_mod::new()),
            crate::service::dal::user::new(
                user_dao_mod::new(),
                crate::service::dao::user_credential::new(),
            ),
            link_dao_mod::new(),
            pairing_dao_mod::new(),
            mock,
        ))
    }

    async fn seed_org(
        ctx: &RequestContext,
        pool: &SqlitePool,
        name: &str,
        scope: common::enums::OrganizationScope,
    ) -> OrganizationPo {
        let _ = pool;
        let mut org = OrganizationPo::new(
            Uuid::now_v7().to_string(),
            name.to_string(),
            String::new(),
            None,
            "test".to_string(),
        );
        org.scope = scope;
        crate::service::dao::organization::new()
            .insert(ctx.clone(), &org)
            .await
            .expect("seed org failed");
        org
    }

    async fn seed_link(
        ctx: &RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
        endpoint: &str,
    ) -> OrganizationLinkPo {
        let link = OrganizationLinkPo::new(
            Uuid::now_v7().to_string(),
            local_org_id.to_string(),
            peer_org_id.to_string(),
            endpoint.to_string(),
            "a".repeat(64),
            "b".repeat(64),
            "test".to_string(),
        );
        crate::service::dao::organization_link::new()
            .insert(ctx.clone(), &link)
            .await
            .expect("seed link failed");
        link
    }

    fn dir_entry(id: &str, name: &str) -> PeerOrgDirectoryEntry {
        PeerOrgDirectoryEntry {
            id: id.to_string(),
            name: name.to_string(),
            description: String::new(),
            base_url: String::new(),
            group_name: Some(String::new()),
            status: 1,
            updated_at: 0,
        }
    }

    /// 变更推送：全量本地目录推给去重后的 Active 对端
    #[sqlx::test]
    async fn test_push_directory_to_peers_dedups_by_endpoint(pool: SqlitePool) {
        let ctx = new_test_ctx("tester", pool.clone());
        let mock = Arc::new(MockFederationClient::new(vec![]));
        let domain = build_domain(mock.clone());

        let org_a = seed_org(
            &ctx,
            &pool,
            "节点A",
            common::enums::OrganizationScope::Local,
        )
        .await;
        let org_c = seed_org(
            &ctx,
            &pool,
            "节点C",
            common::enums::OrganizationScope::Local,
        )
        .await;
        let org_b = seed_org(
            &ctx,
            &pool,
            "节点B",
            common::enums::OrganizationScope::Remote,
        )
        .await;
        let org_d = seed_org(
            &ctx,
            &pool,
            "节点D",
            common::enums::OrganizationScope::Remote,
        )
        .await;

        // A、C 各自建联到同一对端节点（B、D 同 endpoint）→ endpoint 去重后只推一次
        seed_link(&ctx, &org_a.id, &org_b.id, "https://peer.example.com").await;
        seed_link(&ctx, &org_c.id, &org_d.id, "https://peer.example.com").await;

        let pushed = domain
            .push_directory_to_peers(ctx.clone())
            .await
            .expect("push failed");
        assert_eq!(pushed, 1, "同 endpoint 的多条 Active 连接应去重为一次推送");

        let pushes = mock.pushes.lock().unwrap();
        assert_eq!(pushes.len(), 1);
        let (endpoint, token, names) = &pushes[0];
        assert_eq!(endpoint, "https://peer.example.com");
        assert_eq!(token.len(), 64);
        assert!(names.contains(&"节点A".to_string()));
        assert!(names.contains(&"节点C".to_string()));
    }

    /// 定时对账：推本地 + 拉对端写影子；无 Active 连接时为 no-op
    #[sqlx::test]
    async fn test_reconcile_pulls_and_upserts_shadow(pool: SqlitePool) {
        let ctx = new_test_ctx("tester", pool.clone());
        let remote_entry = dir_entry(&Uuid::now_v7().to_string(), "对端新组织");
        let mock = Arc::new(MockFederationClient::new(vec![remote_entry.clone()]));
        let domain = build_domain(mock.clone());

        // 无连接：no-op
        let report = domain
            .reconcile_directories(ctx.clone())
            .await
            .expect("reconcile failed");
        assert_eq!(report.peers, 0);

        let org_a = seed_org(
            &ctx,
            &pool,
            "节点A",
            common::enums::OrganizationScope::Local,
        )
        .await;
        let org_b = seed_org(
            &ctx,
            &pool,
            "节点B",
            common::enums::OrganizationScope::Remote,
        )
        .await;
        seed_link(&ctx, &org_a.id, &org_b.id, "https://peer.example.com").await;

        let report = domain
            .reconcile_directories(ctx.clone())
            .await
            .expect("reconcile failed");
        assert_eq!(report.peers, 1);
        assert_eq!(report.pushed, 1);
        assert_eq!(report.pulled_written, 1, "对端新组织应写为 Remote 影子");

        // 拉到的对端目录条目已落库
        let org_dao = crate::service::dao::organization::new();
        let shadow = org_dao
            .find_by_id(ctx.clone(), &remote_entry.id)
            .await
            .expect("query shadow failed")
            .expect("shadow should exist");
        assert_eq!(shadow.name, "对端新组织");
        assert_eq!(shadow.scope, common::enums::OrganizationScope::Remote);

        assert_eq!(mock.pulls.lock().unwrap().len(), 1);
    }
}
