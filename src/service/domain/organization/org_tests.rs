//! directory_sync_tests 单元测试（拆分自 org.rs）
//!
//! 文件瘦身：原 1900 行 → 1541 行，测试体 360 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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
