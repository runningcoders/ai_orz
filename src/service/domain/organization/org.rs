//! 组织管理 trait 实现
//!
//! 定义组织相关业务接口实现

use crate::models::federation_contract::FederationContractPo;
use crate::models::organization::OrganizationPo;
use crate::models::organization_link::OrganizationLinkPo;
use crate::models::organization_pairing_code::OrganizationPairingCodePo;
use crate::models::user::UserPo;
use crate::pkg::RequestContext;
use crate::service::dao::organization::PeerOrgUpsert;
use crate::service::dao::organization_link::OrganizationLinkQuery;
use crate::service::domain::organization::FederationRequestProof;
use async_trait::async_trait;
use chrono::Utc;
use common::api::{
    CreateLinkRequest, CreateLinkResponse, IssuePairingCodeResponse, LinkItem, ListLinksResponse,
    OrganizationConfig, PAIRING_CODE_LEN, PAIRING_CODE_TTL_MS, PeerOrgDirectoryEntry,
    VerifyPairingCodeRequest, VerifyPairingCodeResponse,
};
use common::enums::organization::OrganizationLinkStatus;
use common::enums::{OrganizationStatus, UserRole};
use common::error::{Error, Result, err};
use rand::Rng;
use uuid::Uuid;

/// 联邦签名时间窗（秒，§四：两端 NTP 基本同步的容错上限）
const SIGNATURE_TIMESTAMP_WINDOW_SECS: i64 = 300;

/// 本端联邦签名身份（出站签名 / 建联交叉签名用；signing_key 已解密为明文 base64）
#[derive(Debug, Clone)]
pub(super) struct LocalFederationIdentity {
    pub did: String,
    pub signing_key: String,
}

/// 从组织行提取本端联邦签名身份（S1 密钥底座保证 Local 组织必有；缺失 = 内部错误）
fn local_federation_identity_from_org(org: &OrganizationPo) -> Result<LocalFederationIdentity> {
    let did = org
        .did
        .clone()
        .ok_or_else(|| err!(Internal, "组织 {} 缺少联邦 DID（密钥底座未就绪）", org.id))?;
    let encrypted = org
        .signing_key
        .clone()
        .ok_or_else(|| err!(Internal, "组织 {} 缺少联邦私钥（密钥底座未就绪）", org.id))?;
    let signing_key = crate::pkg::crypto::decrypt_channel_secret(&encrypted)?;
    Ok(LocalFederationIdentity { did, signing_key })
}

/// 验证建联交叉签名 + DID/公钥自洽（建联双方共用的校验，§六 S2）
fn verify_handshake_signature(
    announced_did: &str,
    announced_verification_key: &str,
    signature: &str,
    counterparty_did: &str,
    pairing_code: &str,
) -> Result<()> {
    use crate::pkg::crypto::did::{
        did_matches_verification_key, pairing_handshake_message, verify_request,
    };
    if !did_matches_verification_key(announced_did, announced_verification_key) {
        return Err(Error::unauthorized("联邦 DID 与公钥不一致"));
    }
    verify_request(
        announced_verification_key,
        pairing_handshake_message(counterparty_did, pairing_code).as_bytes(),
        signature,
    )
    .map_err(|e| Error::unauthorized(format!("建联交叉签名验证失败: {}", e)))
}

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

/// 生成本端联邦身份三列值（did / 公钥 / `encrypt_channel_secret` 加密后的私钥种子）
///
/// 主密钥不可用（测试环境未初始化配置）时返回 None，调用方保持 NULL，
/// 由启动自检（`ensure_local_federation_identity`）兜底补齐。
fn generate_federation_identity_columns() -> Option<(String, String, String)> {
    let kp = crate::pkg::crypto::did::generate_keypair();
    let signing_key = crate::pkg::crypto::encrypt_channel_secret(&kp.signing_key).ok()?;
    Some((kp.did, kp.verification_key, signing_key))
}

/// S1 密钥底座启动自检：确保所有 Local 组织都有联邦身份密钥对（幂等）
///
/// 覆盖两类场景：历史组织（迁移前创建，did 为空）、创建时主密钥不可用被跳过的组织。
/// 仅动 did / verification_key / signing_key 三列；加密能力不可用时整批跳过（fail-soft，
/// 下次启动重试），不阻断启动流程。调用约定：仅 init（`init_base_data`），非业务方法。
pub(super) async fn ensure_local_federation_identity(
    ctx: &RequestContext,
    org_dal: &dyn crate::service::dao::organization::OrganizationDao,
) -> Result<()> {
    use crate::service::dao::organization::OrganizationQuery;
    use common::enums::OrganizationScope;

    let locals = org_dal
        .query(
            ctx.clone(),
            OrganizationQuery {
                scope: Some(OrganizationScope::Local),
                ..Default::default()
            },
        )
        .await?;

    for org in locals.iter().filter(|o| o.did.is_none()) {
        let Some((did, verification_key, signing_key)) = generate_federation_identity_columns()
        else {
            log_warn!(
                ctx,
                "federation_identity",
                "主密钥不可用，跳过组织联邦密钥生成 org_id={}（下次启动重试）",
                org.id
            );
            continue;
        };
        org_dal
            .update_federation_identity(
                ctx.clone(),
                &org.id,
                Some(&did),
                Some(&verification_key),
                Some(&signing_key),
            )
            .await?;
        log_info!(
            ctx,
            "federation_identity",
            "生成本地组织联邦身份密钥对 org_id={}",
            org.id
        );
    }
    Ok(())
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
        // 1. 创建组织（S1 密钥底座：Local 组织创建即生成联邦身份密钥对，私钥加密落库；
        //    主密钥不可用的环境保持 NULL，由启动自检兜底补齐）
        let org_id = generate_org_id();
        let mut org = OrganizationPo::new(
            org_id.clone(),
            params.organization_name,
            params.description.unwrap_or_default(),
            None,
            org_id.clone(), // 系统初始化时由组织自己创建
        );
        if let Some((did, verification_key, signing_key)) = generate_federation_identity_columns() {
            org.did = Some(did);
            org.verification_key = Some(verification_key);
            org.signing_key = Some(signing_key);
        }
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
    /// 返回明文 + 过期绝对时间。可选钉住预期对端 DID（§2.1）。签发记审计。
    async fn issue_pairing_code(
        &self,
        ctx: RequestContext,
        expected_peer_did: Option<String>,
    ) -> Result<IssuePairingCodeResponse> {
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

        // 3) 生成 24 字符配对码（去 0/O/1/I），仅存哈希；可选钉住预期对端 DID
        let code = generate_pairing_code();
        let code_hash = sha256::digest(code.as_bytes());
        let expected_peer_did = expected_peer_did
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(did) = &expected_peer_did {
            crate::pkg::crypto::did::decode_did_key(did)
                .map_err(|e| Error::bad_request(format!("钉住的预期对端 DID 非法: {}", e)))?;
        }
        let now = Utc::now().timestamp_millis();
        let expires_at = now + PAIRING_CODE_TTL_MS;
        let created_by = ctx.caller_id().unwrap_or_else(|| org_id.clone());

        self.pairing_dal
            .insert(
                ctx.clone(),
                &OrganizationPairingCodePo {
                    id: Uuid::now_v7().to_string(),
                    org_id: org_id.clone(),
                    code_hash,
                    expires_at,
                    consumed_at: None,
                    expected_peer_did: expected_peer_did.clone(),
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

    /// 验证配对码 + 交换身份（机器侧，配对码鉴权）
    ///
    /// 消费配对码（单用途 + TTL），验证调用方交叉签名（证明其拥有声明 DID 的
    /// 私钥），落对端 link（含对端 DID / 公钥）+ Linked 影子，返回本端目录条目
    /// + 本端交叉签名。无效 / 过期 / 已用 / 签名不符统一 unauthorized（防枚举）。
    async fn verify_pairing_code(
        &self,
        ctx: RequestContext,
        req: VerifyPairingCodeRequest,
    ) -> Result<VerifyPairingCodeResponse> {
        // 1) 原子消费配对码（单用途 + TTL 合一）；无效/过期/已用统一 None → 不区分（防枚举）
        let code_hash = sha256::digest(req.pairing_code.as_bytes());
        let now = Utc::now().timestamp_millis();
        let consumed = self
            .pairing_dal
            .consume(ctx.clone(), &code_hash, now)
            .await?
            .ok_or_else(|| Error::unauthorized("配对码无效或已失效"))?;
        let org_id = consumed.org_id;

        // 2) DID 钉住校验（§2.1：签发时钉住预期对端 DID 则强校验，关闭首达者窗口）
        if let Some(pinned) = consumed.expected_peer_did.as_deref() {
            let announced = req.local_org.did.as_deref().unwrap_or_default();
            if announced != pinned {
                return Err(Error::unauthorized("对端 DID 与签发时钉住的不一致"));
            }
        }

        // 3) 调用方身份声明校验：DID + 公钥必填、二者自洽、交叉签名证明私钥持有
        let caller_did = req
            .local_org
            .did
            .as_deref()
            .ok_or_else(|| Error::unauthorized("建联请求缺少联邦 DID"))?
            .to_string();
        let caller_vk = req
            .local_org
            .verification_key
            .as_deref()
            .ok_or_else(|| Error::unauthorized("建联请求缺少联邦公钥"))?;
        verify_handshake_signature(
            &caller_did,
            caller_vk,
            &req.local_signature,
            &caller_did,
            &req.pairing_code,
        )?;

        // 4) 取签发方（本节点）组织信息 + 本端签名身份
        let issuer = self
            .org_dal
            .get_by_id(ctx.clone(), &org_id)
            .await?
            .ok_or_else(|| Error::unauthorized("配对码关联组织不存在"))?;
        let issuer_identity = local_federation_identity_from_org(&issuer)?;

        // 5) 落本端 link（幂等：已存在则续联更新 endpoint / 对端身份）
        //
        // S2 身份流向：peer_did / peer_verification_key = 调用方建联时出示的
        // DID / 公钥（入站验签依据）；共享密钥已删除，鉴权 = 每请求 Ed25519 验签。
        let existing = self
            .link_dal
            .find_by_pair(ctx.clone(), &org_id, &req.local_org.id)
            .await?;
        let mut link = OrganizationLinkPo::new(
            existing
                .as_ref()
                .map(|l| l.id.clone())
                .unwrap_or_else(|| Uuid::now_v7().to_string()),
            org_id.clone(),
            req.local_org.id.clone(),
            req.local_endpoint.clone(),
            org_id.clone(),
        );
        link.peer_did = Some(caller_did.clone());
        link.peer_verification_key = Some(caller_vk.to_string());
        if existing.is_some() {
            self.link_dal.update(ctx.clone(), &link).await?;
        } else {
            self.link_dal.insert(ctx.clone(), &link).await?;
        }
        // S3：建联成功即确保存在 basic 合约（幂等；续联不动已编辑的能力集）
        self.contract_dal
            .ensure_basic(ctx.clone(), &org_id, &req.local_org.id)
            .await?;

        // 6) 写对端（本地节点）影子：直接建联必为 Linked（R5 保护本节点 Local 组织）
        //    走 org DAL 静默方法：影子是复制不是业务变更，不发布 organization.changed
        let shadow = PeerOrgUpsert {
            id: req.local_org.id.clone(),
            name: req.local_org.name.clone(),
            description: req.local_org.description.clone(),
            base_url: req.local_org.base_url.clone(),
            group_name: req.local_org.group_name.clone(),
            addresses: req.local_org.addresses.clone(),
            status: OrganizationStatus::from_i32(req.local_org.status),
            did: req.local_org.did.clone(),
            verification_key: req.local_org.verification_key.clone(),
            updated_at: req.local_org.updated_at,
        };
        self.org_dal
            .upsert_linked_shadow(ctx.clone(), &shadow)
            .await?;

        // 7) 本端交叉签名：对（调用方 DID + 配对码哈希）签名——证明本端拥有其
        //    声明 DID 的私钥，且把这次建联钉在调用方身份上
        let peer_signature = crate::pkg::crypto::did::sign_request(
            &issuer_identity.signing_key,
            crate::pkg::crypto::did::pairing_handshake_message(&caller_did, &req.pairing_code)
                .as_bytes(),
        )?;

        log_info!(
            &ctx,
            "verify_pairing_code",
            "配对码验证成功，建立连接 peer_did={} local_org_id={}",
            caller_did,
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
                did: Some(issuer_identity.did),
                verification_key: issuer.verification_key.clone(),
                updated_at: issuer.updated_at,
                // issuer 是本节点组织：自报地址 = 配置动态推导（即时生效）
                addresses: Some(self.self_addresses.clone()),
            },
            peer_signature,
        })
    }

    /// 发起建联（用户侧，JWT）
    ///
    /// 凭对端配对码出站调对端 verify 完成身份交换（双向交叉签名验证），落本地
    /// link（含对端 DID / 公钥）+ Linked 影子。
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

        // 1) 本端组织（JWT 绑定）+ 本端签名身份（S1 密钥底座保证存在）
        let org_id = ctx
            .organization_id()
            .ok_or_else(|| Error::unauthorized("未识别的组织上下文"))?
            .to_string();
        let local_org = self
            .org_dal
            .get_by_id(ctx.clone(), &org_id)
            .await?
            .ok_or_else(|| Error::unauthorized("当前登录身份已失效（组织不存在）"))?;
        let local_identity = local_federation_identity_from_org(&local_org)?;

        // 2) 本端目录条目（did / verification_key S2 起必填）+ 交叉签名
        //    （证明本端拥有其声明 DID 的私钥）
        let local_signature = crate::pkg::crypto::did::sign_request(
            &local_identity.signing_key,
            crate::pkg::crypto::did::pairing_handshake_message(&local_identity.did, &pairing_code)
                .as_bytes(),
        )?;

        // 3) 出站调对端 verify：验证配对码 + 交换身份
        let verify_req = VerifyPairingCodeRequest {
            pairing_code,
            local_org: PeerOrgDirectoryEntry {
                id: local_org.id.clone(),
                name: local_org.name.clone(),
                description: local_org.description.clone(),
                base_url: local_org.base_url.clone(),
                group_name: local_org.group_name.clone(),
                status: local_org.status.to_i32(),
                did: Some(local_identity.did.clone()),
                verification_key: local_org.verification_key.clone(),
                updated_at: local_org.updated_at,
                // 本端自报地址 = 配置动态推导（对端拿去做探测候选池）
                addresses: Some(self.self_addresses.clone()),
            },
            local_endpoint: local_endpoint.clone(),
            local_signature,
        };
        let resp = self
            .link_dal
            .verify_pairing_code(&peer_endpoint, &verify_req)
            .await?;

        // 4) 防自联 + 对端身份校验：DID/公钥必填、自洽、交叉签名证明对端私钥持有
        if resp.peer_org.id == org_id {
            return Err(Error::bad_request("对端组织与本端组织相同，拒绝建联"));
        }
        let peer_did = resp
            .peer_org
            .did
            .as_deref()
            .ok_or_else(|| Error::unauthorized("对端建联响应缺少联邦 DID"))?
            .to_string();
        let peer_vk = resp
            .peer_org
            .verification_key
            .as_deref()
            .ok_or_else(|| Error::unauthorized("对端建联响应缺少联邦公钥"))?;
        verify_handshake_signature(
            &peer_did,
            peer_vk,
            &resp.peer_signature,
            &local_identity.did,
            &verify_req.pairing_code,
        )?;

        // 5) 落本端 link（幂等续联）：对端 DID / 公钥入站验签依据
        let existing = self
            .link_dal
            .find_by_pair(ctx.clone(), &org_id, &resp.peer_org.id)
            .await?;
        let mut link = OrganizationLinkPo::new(
            existing
                .as_ref()
                .map(|l| l.id.clone())
                .unwrap_or_else(|| Uuid::now_v7().to_string()),
            org_id.clone(),
            resp.peer_org.id.clone(),
            peer_endpoint.clone(),
            ctx.caller_id().unwrap_or_else(|| org_id.clone()),
        );
        link.peer_did = Some(peer_did.clone());
        link.peer_verification_key = Some(peer_vk.to_string());
        if existing.is_some() {
            self.link_dal.update(ctx.clone(), &link).await?;
        } else {
            self.link_dal.insert(ctx.clone(), &link).await?;
        }
        // S3：建联成功即确保存在 basic 合约（幂等；续联不动已编辑的能力集）
        self.contract_dal
            .ensure_basic(ctx.clone(), &org_id, &resp.peer_org.id)
            .await?;

        // 6) 写对端影子：直接建联必为 Linked（R5 保护本端 Local 组织）
        let shadow = PeerOrgUpsert {
            id: resp.peer_org.id.clone(),
            name: resp.peer_org.name.clone(),
            description: resp.peer_org.description.clone(),
            base_url: resp.peer_org.base_url.clone(),
            group_name: resp.peer_org.group_name.clone(),
            addresses: resp.peer_org.addresses.clone(),
            status: OrganizationStatus::from_i32(resp.peer_org.status),
            did: resp.peer_org.did.clone(),
            verification_key: resp.peer_org.verification_key.clone(),
            updated_at: resp.peer_org.updated_at,
        };
        self.org_dal
            .upsert_linked_shadow(ctx.clone(), &shadow)
            .await?;

        // 7) 目录双向同步（评审稿 §4.1 步骤 5 / §5.2）：拉对端全量目录 + 推本地目录。
        //    best-effort：目录同步失败不回滚建联（契约已落库，可由下次同步补齐），仅记审计。
        self.sync_directories_after_link(&ctx, &link).await;

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
            .link_dal
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
                    did: peer.did.clone(),
                    verification_key: peer.verification_key.clone(),
                    updated_at: peer.updated_at,
                    // 前端展示页无需地址明细（联通地址以 link.endpoint 为准）
                    addresses: None,
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

    /// 联邦入站请求验签（S2，机器侧端点统一鉴权入口）
    ///
    /// 任一环节失败统一 unauthorized（不区分不存在/已断联/签名不符，防枚举）：
    /// ① 按 key_id 查 Active 连接（未命中即 401，跨连接冒充天然拦下）；
    /// ② 时间窗 ±300s；③ nonce 进程内去重（防重放）；④ 用连接上对端公钥验签。
    async fn authenticate_federation_request(
        &self,
        ctx: RequestContext,
        proof: &FederationRequestProof,
        method: &str,
        path_with_query: &str,
        body_hash: &str,
    ) -> Result<OrganizationLinkPo> {
        use crate::pkg::crypto::did::{canonical_request_string, verify_request};
        use crate::pkg::nonce;

        // ① 连接定位：key_id = 对端 DID（建联时交换落库）
        let link = self
            .link_dal
            .find_active_by_peer_did(ctx.clone(), &proof.key_id)
            .await?
            .ok_or_else(|| Error::unauthorized("联邦请求无法归属到任何 Active 连接"))?;

        // ② 时间窗：±300s（两端 NTP 基本同步）
        let now = Utc::now().timestamp();
        if (now - proof.timestamp).abs() > SIGNATURE_TIMESTAMP_WINDOW_SECS {
            return Err(Error::unauthorized("联邦请求时间戳超出允许窗口"));
        }

        // ③ nonce 去重：原子查重 + 登记，重复即重放
        if !nonce::check_and_insert(&proof.nonce) {
            return Err(Error::unauthorized("联邦请求 nonce 重复（疑似重放）"));
        }

        // ④ 验签：对端公钥必填（S2 建联交换，缺失 = 旧数据，fail-closed）
        let peer_vk = link
            .peer_verification_key
            .as_deref()
            .ok_or_else(|| Error::unauthorized("连接缺少对端联邦公钥，无法验签"))?;
        let msg = canonical_request_string(
            method,
            path_with_query,
            proof.timestamp,
            &proof.nonce,
            body_hash,
        );
        verify_request(peer_vk, msg.as_bytes(), &proof.signature).map_err(|e| {
            sys_debug!("federation signature rejected: {}", e);
            Error::unauthorized("联邦签名验证失败")
        })?;
        Ok(link)
    }

    /// 验证异步回调任务令牌（S2 自签发，`/a2a/callback` 唯一鉴权手段）
    ///
    /// 签发方 = 本端 = 回调接收方：用本端公钥验签，零状态（不查库不查对端）。
    /// exp / task_id 绑定 / aud 绑定逐项校验，任一不符即 401。
    async fn verify_callback_task_token(
        &self,
        ctx: RequestContext,
        token: &str,
        task_id: &str,
    ) -> Result<()> {
        use crate::pkg::crypto::task_token::verify_task_token_signature;

        // 本端组织（唯一 Local）签名身份
        let local = self
            .org_dal
            .query(
                ctx,
                crate::service::dao::organization::OrganizationQuery {
                    scope: Some(common::enums::OrganizationScope::Local),
                    ..Default::default()
                },
            )
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::internal("本节点无 Local 组织，无法验证回调令牌"))?;
        let identity = local_federation_identity_from_org(&local)?;

        // 验签（公钥 = 本端 verification_key）+ claims 逐项比对
        let claims = verify_task_token_signature(
            &local
                .verification_key
                .clone()
                .ok_or_else(|| Error::internal("本端组织缺少联邦公钥，无法验证回调令牌"))?,
            token,
        )?;
        let now = Utc::now().timestamp();
        if claims.exp <= now {
            return Err(Error::unauthorized("任务令牌已过期"));
        }
        if claims.task_id != task_id {
            return Err(Error::unauthorized("任务令牌与回调路径的任务不匹配"));
        }
        if claims.aud != identity.did {
            return Err(Error::unauthorized("任务令牌受众与本端 DID 不符"));
        }
        Ok(())
    }

    /// 本节点组织目录（白名单字段）
    async fn get_directory(&self, ctx: RequestContext) -> Result<Vec<PeerOrgDirectoryEntry>> {
        use common::enums::OrganizationScope;

        let orgs = self.org_dal.list_all(ctx.clone()).await?;
        // 影子组织的自报地址（P7 ①层）：目录同步复制的原文；Local 组织动态
        // 注入配置推导地址（即时生效，无漂移）
        let addr_map: std::collections::HashMap<
            String,
            Vec<common::api::organization_link::FederationAddress>,
        > = self
            .org_dal
            .list_addresses(ctx)
            .await?
            .into_iter()
            .filter_map(|(id, json)| {
                serde_json::from_str(&json)
                    .ok()
                    .map(|v: Vec<common::api::organization_link::FederationAddress>| (id, v))
            })
            .collect();

        Ok(orgs
            .into_iter()
            .map(|org| {
                let addresses = if org.scope == OrganizationScope::Local {
                    Some(self.self_addresses.clone())
                } else {
                    addr_map.get(&org.id).cloned()
                };
                PeerOrgDirectoryEntry {
                    id: org.id,
                    name: org.name,
                    description: org.description,
                    base_url: org.base_url,
                    group_name: org.group_name,
                    status: org.status.to_i32(),
                    did: org.did.clone(),
                    verification_key: org.verification_key.clone(),
                    updated_at: org.updated_at,
                    addresses,
                }
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
                addresses: entry.addresses.clone(),
                status: OrganizationStatus::from_i32(entry.status),
                did: entry.did.clone(),
                verification_key: entry.verification_key.clone(),
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
            .link_dal
            .find_by_pair(ctx.clone(), &org_id, peer_org_id)
            .await?
            .ok_or_else(|| Error::not_found(format!("未找到与组织 {} 的连接", peer_org_id)))?;

        // 3) 断联（org DAL 组合：link → Revoked + 对端影子 Linked → Remote，不删除记录；
        //    两步各自幂等，第二步失败重试本方法即可修复）
        self.org_dal
            .revoke_link(ctx.clone(), &link.id, peer_org_id)
            .await?;
        // S3：断联同时终止合约（幂等；合约记录保留审计线索）
        self.contract_dal
            .terminate(ctx.clone(), &org_id, peer_org_id)
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

    // ==================== 联邦合约（S3，用户侧管理员接口） ====================

    async fn list_contracts(
        &self,
        ctx: RequestContext,
    ) -> Result<common::api::ListContractsResponse> {
        self.ensure_admin(&ctx)?;
        let org_id = self.require_org_id(&ctx)?;
        let contracts = self
            .contract_dal
            .list_by_org(ctx, &org_id)
            .await?
            .iter()
            .map(Self::contract_item)
            .collect();
        Ok(common::api::ListContractsResponse { contracts })
    }

    async fn update_contract_capabilities(
        &self,
        ctx: RequestContext,
        req: common::api::UpdateContractCapabilitiesRequest,
    ) -> Result<common::api::UpdateContractCapabilitiesResponse> {
        self.ensure_admin(&ctx)?;
        let org_id = self.require_org_id(&ctx)?;
        Self::validate_capabilities(&req.capabilities)?;
        let po = self
            .contract_dal
            .update_capabilities(ctx, &org_id, &req.contract_id, req.capabilities)
            .await?;
        Ok(common::api::UpdateContractCapabilitiesResponse {
            contract: Self::contract_item(&po),
        })
    }

    async fn terminate_contract(
        &self,
        ctx: RequestContext,
        req: common::api::TerminateContractRequest,
    ) -> Result<common::api::TerminateContractResponse> {
        self.ensure_admin(&ctx)?;
        let org_id = self.require_org_id(&ctx)?;
        self.contract_dal
            .terminate(ctx, &org_id, &req.peer_org_id)
            .await?;
        Ok(common::api::TerminateContractResponse { success: true })
    }

    async fn contract_capabilities(
        &self,
        ctx: RequestContext,
        local_org_id: &str,
        peer_org_id: &str,
    ) -> Result<Vec<String>> {
        self.contract_dal
            .active_capabilities(ctx, local_org_id, peer_org_id)
            .await
    }

    async fn delegate_agent_task(
        &self,
        ctx: RequestContext,
        peer_org_id: &str,
        peer_agent_id: &str,
        prompt: &str,
        caller_user: Option<String>,
    ) -> Result<Option<String>> {
        use common::api::organization_link::{CAPABILITY_A2A_TASK, FederationCallerDeclaration};
        use common::enums::OrganizationScope;

        // 1) 本端组织：优先 ctx（JWT 上下文），消费链路（system ctx）回退查唯一 Local 组织
        let local_org_id = match ctx.organization_id() {
            Some(id) => id.to_string(),
            None => {
                let locals = self
                    .org_dal
                    .query(
                        ctx.clone(),
                        crate::service::dao::organization::OrganizationQuery {
                            scope: Some(OrganizationScope::Local),
                            ..Default::default()
                        },
                    )
                    .await?;
                match locals.first() {
                    Some(org) => org.id.clone(),
                    None => {
                        log_warn!(&ctx, "delegate_agent_task", "无 Local 组织，跳过联邦委派");
                        return Ok(None);
                    }
                }
            }
        };

        // 2) 路由决策：对端需有 Active 连接且开放 a2a_task 能力，否则降级
        let Some(link) = self
            .link_dal
            .find_by_pair(ctx.clone(), &local_org_id, peer_org_id)
            .await?
            .filter(|l| l.status == OrganizationLinkStatus::Active)
        else {
            return Ok(None);
        };
        // S3：能力门禁从合约读（无 active 合约 = 空集，降级）
        let capabilities = self
            .contract_dal
            .active_capabilities(ctx.clone(), &link.local_org_id, &link.peer_org_id)
            .await?;
        if !capabilities.iter().any(|c| c == CAPABILITY_A2A_TASK) {
            log_info!(
                &ctx,
                "delegate_agent_task",
                "连接未开放 a2a_task 能力，降级 peer_org_id={}",
                peer_org_id
            );
            return Ok(None);
        }

        // 3) 声明头：caller_org 必填；caller_user 由消息发送方透传（对端 R3 计量用）
        let declaration = FederationCallerDeclaration {
            caller_org: Some(local_org_id),
            caller_user,
            caller_agent: None,
        };

        // 4) 传输层：org DAL 组装 A2aRuntimeConfig 走联邦出站（send → 轮询到终态）
        //    本端签名身份：出站每请求签名 + 任务令牌签发（S2）
        let local_identity = local_federation_identity_from_org(&self.load_local_org(&ctx).await?);
        let reply = self
            .org_dal
            .send_federated_agent_task(
                ctx.clone(),
                &link,
                peer_agent_id,
                prompt,
                Some(serde_json::to_string(&declaration).map_err(|e| {
                    Error::internal(format!("failed to serialize caller declaration: {}", e))
                })?),
                &local_identity?.signing_key,
            )
            .await?;

        log_info!(
            &ctx,
            "delegate_agent_task",
            "跨组织委派完成 peer_org_id={} peer_agent_id={}",
            peer_org_id,
            peer_agent_id
        );
        Ok(Some(reply))
    }

    async fn list_federation_agents(
        &self,
        ctx: RequestContext,
    ) -> Result<common::api::ListFederationAgentsResponse> {
        use common::api::{FederationAgentGroup, ListFederationAgentsResponse};

        let links = self
            .link_dal
            .query(
                ctx.clone(),
                OrganizationLinkQuery {
                    status: Some(OrganizationLinkStatus::Active),
                    ..Default::default()
                },
            )
            .await?;

        let mut groups = Vec::new();
        let signing_key = self.load_local_org(&ctx).await?;
        let local_identity = local_federation_identity_from_org(&signing_key)?;
        for link in links {
            let org_name = self
                .org_dal
                .get_by_id(ctx.clone(), &link.peer_org_id)
                .await
                .ok()
                .flatten()
                .map(|o| o.name)
                .unwrap_or_else(|| link.peer_org_id.clone());
            // P7：出站前解析首选可达地址（内网优先探测 + TTL 缓存）
            let endpoint = self.org_dal.resolve_peer_endpoint(ctx.clone(), &link).await;
            match self
                .link_dal
                .fetch_capabilities(&endpoint, &local_identity.signing_key)
                .await
            {
                Ok(caps) => groups.push(FederationAgentGroup {
                    org_id: link.peer_org_id.clone(),
                    org_name,
                    agents: caps.agents,
                    capabilities: caps.capabilities,
                }),
                Err(e) => {
                    // 部分可用优于整体为空：单个对端失败仅跳过
                    log_warn!(
                        &ctx,
                        "federation_agents",
                        "拉取对端能力清单失败 peer_org_id={} error={}",
                        link.peer_org_id,
                        e
                    );
                }
            }
        }

        Ok(ListFederationAgentsResponse { groups })
    }

    async fn push_directory_to_peers(&self, ctx: RequestContext) -> Result<usize> {
        let peers = self.active_peer_endpoints(&ctx).await?;
        if peers.is_empty() {
            return Ok(0); // 无连接：不触签名身份（新建节点可能尚未建密钥底座）
        }
        let dir = self.get_directory(ctx.clone()).await?;
        let local_identity = local_federation_identity_from_org(&self.load_local_org(&ctx).await?)?;

        let mut pushed = 0usize;
        for (endpoint, peer_org_id) in &peers {
            match self
                .link_dal
                .push_directory(endpoint, &local_identity.signing_key, dir.clone())
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
        let mut report = super::DirectoryReconcileReport {
            peers: peers.len(),
            pushed: 0,
            pulled_written: 0,
        };
        if peers.is_empty() {
            return Ok(report); // 无连接：no-op（不触签名身份）
        }
        let dir = self.get_directory(ctx.clone()).await?;
        let local_identity = local_federation_identity_from_org(&self.load_local_org(&ctx).await?)?;

        for (endpoint, peer_org_id) in &peers {
            // 推：本地目录 → 对端（对端按其影子语义 upsert）
            match self
                .link_dal
                .push_directory(endpoint, &local_identity.signing_key, dir.clone())
                .await
            {
                Ok(()) => report.pushed += 1,
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
                .link_dal
                .fetch_directory(endpoint, &local_identity.signing_key)
                .await
            {
                Ok(entries) => {
                    let count = entries.len();
                    let req = common::api::DirectorySyncRequest { orgs: entries };
                    match self.handle_directory_sync(ctx.clone(), req).await {
                        Ok(written) => report.pulled_written += written,
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
            report.peers,
            report.pushed,
            report.pulled_written
        );
        Ok(report)
    }
}

impl super::OrganizationDomainImpl {
    /// 加载唯一 Local 组织（出站签名身份来源；S1 密钥底座保证其有密钥对）
    async fn load_local_org(&self, ctx: &RequestContext) -> Result<OrganizationPo> {
        // 优先 ctx 绑定的组织（JWT/消息上下文自带 org，多节点共享库的测试环境
        // 亦依赖此语义——Local 组织不唯一时必须按 ctx 定位）
        if let Some(org_id) = ctx.organization_id()
            && let Some(org) = self.org_dal.get_by_id(ctx.clone(), org_id).await?
        {
            return Ok(org);
        }
        // 回退：唯一 Local 组织（单节点常态）
        let locals = self
            .org_dal
            .query(
                ctx.clone(),
                crate::service::dao::organization::OrganizationQuery {
                    scope: Some(common::enums::OrganizationScope::Local),
                    ..Default::default()
                },
            )
            .await?;
        locals
            .into_iter()
            .next()
            .ok_or_else(|| err!(Internal, "本节点无 Local 组织"))
    }

    /// 收集所有 Active 连接的去重对端列表（endpoint 去重，同一节点多组织建联只推一次）
    ///
    /// 返回 `(endpoint, peer_org_id)`；出站签名统一用本端联邦私钥（每连接一份），
    /// 由调用方一次性加载。
    async fn active_peer_endpoints(&self, ctx: &RequestContext) -> Result<Vec<(String, String)>> {
        let links = self
            .link_dal
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
                peers.push((link.endpoint, link.peer_org_id));
            }
        }
        Ok(peers)
    }

    /// 建联完成后的目录双向同步（评审稿 §4.1 步骤 5 / §5.2）
    ///
    /// 拉取对端全量目录（→ 本地 Remote 影子 upsert）+ 推送本地目录（→ 对端
    /// 按其对端侧语义 upsert）。一次建联由发起方完成双向交换，对端无需补动作。
    /// 失败仅记 WARN 审计，不向调用方报错（契约已落库，目录可由下次同步补齐）。
    async fn sync_directories_after_link(&self, ctx: &RequestContext, link: &OrganizationLinkPo) {
        use super::OrganizationManage as _;

        // P7：出站前解析首选可达地址（建联时对端影子已写入，含自报地址）
        let peer_endpoint = self.org_dal.resolve_peer_endpoint(ctx.clone(), link).await;
        // 出站签名身份：本端联邦私钥（create_link 已确认密钥底座就绪）
        let local_org = self
            .org_dal
            .get_by_id(ctx.clone(), &link.local_org_id)
            .await
            .ok()
            .flatten();
        let signing_key = local_org
            .as_ref()
            .and_then(|o| local_federation_identity_from_org(o).ok())
            .map(|i| i.signing_key);

        // 拉：对端目录 → 本地 Remote 影子 upsert（新者胜 / 不动 scope / 保护 Local）
        let pulled = async {
            match signing_key.as_deref() {
                Some(key) => self.link_dal.fetch_directory(&peer_endpoint, key).await,
                None => Err(Error::internal("本端联邦签名身份缺失，跳过建联后目录同步")),
            }
        }
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
                let push = async {
                    match signing_key.as_deref() {
                        Some(key) => {
                            self.link_dal
                                .push_directory(&peer_endpoint, key, orgs)
                                .await
                        }
                        None => Err(Error::internal("本端联邦签名身份缺失")),
                    }
                };
                if let Err(e) = push.await {
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

    // ==================== 联邦合约（S3 合约授权） ====================

    /// 管理员权限预检（与 revoke_link 同口径：仅组织管理员可操作合约）
    fn ensure_admin(&self, ctx: &RequestContext) -> Result<()> {
        let role = ctx
            .user_role()
            .map(UserRole::from_i32)
            .unwrap_or(UserRole::Member);
        if !UserRole::has_permission(role, UserRole::Admin) {
            return Err(Error::forbidden("仅组织管理员可管理联邦合约"));
        }
        Ok(())
    }

    /// 取本组织 ID（JWT 上下文）
    fn require_org_id(&self, ctx: &RequestContext) -> Result<String> {
        ctx.organization_id()
            .ok_or_else(|| Error::unauthorized("未识别的组织上下文"))
            .map(|v| v.to_string())
    }

    /// PO → API 条目（kind/state 序列化为小写字符串，前端友好）
    fn contract_item(po: &FederationContractPo) -> common::api::ContractItem {
        common::api::ContractItem {
            id: po.id.clone(),
            peer_org_id: po.peer_org_id.clone(),
            kind: format!("{:?}", po.kind).to_lowercase(),
            state: format!("{:?}", po.state).to_lowercase(),
            capabilities: po.capabilities_list(),
            created_at: po.created_at,
            updated_at: po.updated_at,
        }
    }

    /// 校验能力集：非空元素且必须在已知能力集合内（fail-closed，防拼写错误静默扩权）
    fn validate_capabilities(caps: &[String]) -> Result<()> {
        use common::api::organization_link::CAPABILITY_A2A_TASK;
        const KNOWN: [&str; 1] = [CAPABILITY_A2A_TASK];
        for c in caps {
            let c = c.trim();
            if c.is_empty() {
                return Err(Error::bad_request("能力项不能为空"));
            }
            if !KNOWN.contains(&c) {
                return Err(Error::bad_request(format!("未知能力: {}", c)));
            }
        }
        Ok(())
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
            signing_key: &str,
        ) -> Result<Vec<PeerOrgDirectoryEntry>> {
            self.pulls
                .lock()
                .unwrap()
                .push((peer_endpoint.to_string(), signing_key.to_string()));
            Ok(self.fetched_dir.clone())
        }

        async fn push_directory(
            &self,
            peer_endpoint: &str,
            signing_key: &str,
            orgs: Vec<PeerOrgDirectoryEntry>,
        ) -> Result<()> {
            let names = orgs.iter().map(|o| o.name.clone()).collect();
            self.pushes.lock().unwrap().push((
                peer_endpoint.to_string(),
                signing_key.to_string(),
                names,
            ));
            Ok(())
        }

        async fn fetch_capabilities(
            &self,
            _peer_endpoint: &str,
            _signing_key: &str,
        ) -> Result<common::api::CapabilitiesResponse> {
            Err(Error::internal("mock: capabilities 未在本测试使用"))
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
            crate::service::dal::organization::link::new(link_dao_mod::new(), mock),
            crate::service::dal::organization::pairing::new(pairing_dao_mod::new()),
            crate::service::dal::organization::contract::new(
                crate::service::dao::federation_contract::new(),
            ),
            Vec::new(), // 测试不注入本端自报地址（get_directory 对 Local 组织即不报 addresses）
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
        // Local 组织补联邦身份（signing_key 存明文：decrypt_channel_secret 明文直通）
        if org.scope == common::enums::OrganizationScope::Local {
            let kp = crate::pkg::crypto::did::generate_keypair();
            org.did = Some(kp.did);
            org.verification_key = Some(kp.verification_key);
            org.signing_key = Some(kp.signing_key);
        }
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
        let mut link = OrganizationLinkPo::new(
            Uuid::now_v7().to_string(),
            local_org_id.to_string(),
            peer_org_id.to_string(),
            endpoint.to_string(),
            "test".to_string(),
        );
        link.peer_did = Some("did:key:z6MkPeerSeed".to_string());
        link.peer_verification_key = Some("cGVlci1zZWVkLWtleQ".to_string());
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
            did: Some("did:key:z6MkTest".to_string()),
            verification_key: Some("dGVzdC1rZXk".to_string()),
            updated_at: 0,
            addresses: None,
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
        let (endpoint, signing_key, names) = &pushes[0];
        assert_eq!(endpoint, "https://peer.example.com");
        assert!(!signing_key.is_empty(), "出站签名用本端私钥");
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

    /// S1 密钥底座：目录同步条目携带对端 DID / 公钥 → 落进影子组织行（私钥恒空）
    #[sqlx::test]
    async fn test_directory_sync_persists_peer_identity(pool: SqlitePool) {
        let ctx = new_test_ctx("tester", pool.clone());
        let entry = dir_entry("peer-did-x", "带身份的对端");
        let domain = build_domain(Arc::new(MockFederationClient::new(vec![])));

        domain
            .handle_directory_sync(
                ctx.clone(),
                common::api::DirectorySyncRequest {
                    orgs: vec![entry.clone()],
                },
            )
            .await
            .expect("sync failed");

        let shadow = crate::service::dao::organization::new()
            .find_by_id(ctx, &entry.id)
            .await
            .expect("query shadow failed")
            .expect("shadow should exist");
        assert_eq!(shadow.did.as_deref(), Some("did:key:z6MkTest"));
        assert_eq!(shadow.verification_key.as_deref(), Some("dGVzdC1rZXk"));
        // 影子组织不持私钥（私钥不出组织）
        assert!(shadow.signing_key.is_none());
    }

    /// S1 密钥底座：启动自检为 did 为空的 Local 组织补齐密钥对（幂等，私钥加密）
    #[sqlx::test]
    async fn test_ensure_local_federation_identity_fills_missing_keys(pool: SqlitePool) {
        let ctx = new_test_ctx("tester", pool.clone());
        // 主密钥就绪（与 memory DAO 测试同口径；配置进程级幂等）
        crate::config::init().expect("test config init failed");

        let org = seed_org(
            &ctx,
            &pool,
            "无密钥组织",
            common::enums::OrganizationScope::Local,
        )
        .await;
        // seed_org 已为 Local 组织补身份；此用例模拟「旧数据缺密钥」，先清空
        sqlx::query!(
            "UPDATE organizations SET did = NULL, verification_key = NULL, signing_key = NULL WHERE id = ?",
            org.id
        )
        .execute(&pool)
        .await
        .expect("clear identity failed");
        let cleared = crate::service::dao::organization::new()
            .find_by_id(ctx.clone(), &org.id)
            .await
            .expect("query cleared org failed")
            .expect("org should exist");
        assert!(cleared.did.is_none(), "前置：seed 组织无联邦身份");

        super::ensure_local_federation_identity(
            &ctx,
            crate::service::dao::organization::new().as_ref(),
        )
        .await
        .expect("ensure failed");

        let filled = crate::service::dao::organization::new()
            .find_by_id(ctx.clone(), &org.id)
            .await
            .expect("query failed")
            .expect("org should exist");
        let did = filled.did.expect("启动自检应补齐 did");
        let verification_key = filled.verification_key.expect("启动自检应补齐公钥");
        let signing_key = filled.signing_key.expect("启动自检应补齐私钥（加密态）");

        // did 与公钥一一对应；私钥为 enc:v1: 加密形态
        let decoded = crate::pkg::crypto::did::decode_did_key(&did).expect("did 应可解码");
        use base64::Engine as _;
        assert_eq!(
            base64::engine::general_purpose::STANDARD.encode(decoded),
            verification_key
        );
        assert!(signing_key.starts_with("enc:v1:"));

        // 幂等：第二次自检不改动已存在的密钥
        super::ensure_local_federation_identity(
            &ctx,
            crate::service::dao::organization::new().as_ref(),
        )
        .await
        .expect("ensure again failed");
        let again = crate::service::dao::organization::new()
            .find_by_id(ctx, &org.id)
            .await
            .expect("query failed")
            .expect("org should exist");
        assert_eq!(again.did.as_deref(), Some(did.as_str()));
    }
}
