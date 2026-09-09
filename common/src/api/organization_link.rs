//! 组织组网（federation / links）相关 API 请求/响应 DTO
//!
//! 统一前缀 `/api/v1/organization/links/*`：
//! - 用户侧端点（JWT）挂 `organization_protected_routes`
//! - 机器侧端点（配对码 / 契约凭证）同前缀 root 层直挂（见评审稿 D7）
//!
//! DTO 单一事实源：后端 handler 与前端共用，结构体化参数（见 AGENTS.md §14）。

use ai_orz_macros::Params;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 配对码 TTL（毫秒）：10 分钟
pub const PAIRING_CODE_TTL_MS: i64 = 10 * 60 * 1000;

/// 配对码长度（字符）
pub const PAIRING_CODE_LEN: usize = 24;

// ============ 配对码签发（用户侧，JWT） ============

/// 签发配对码请求
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct IssuePairingCodeRequest {
    /// 可选：钉住预期对端 DID（`did:key:z...`，§2.1）
    ///
    /// 填了则建联时对端出示的 DID 与之不符即拒绝——关闭「首达者即身份」窗口的
    /// 唯一手段；互信组织走 IM 传递配对码时 TOFU 已够，可不填。
    #[serde(default)]
    pub expected_peer_did: Option<String>,
}

/// 签发配对码响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IssuePairingCodeResponse {
    /// 24 字符配对码（字符集同邀请码：去 0/O/1/I），一次性、短时效
    pub pairing_code: String,
    /// 过期绝对时间（毫秒时间戳）
    pub expires_at: i64,
    /// TTL（秒），便于前端倒计时展示
    pub ttl_seconds: i64,
}

// ============ 配对码验证 + 凭证交换（机器侧，配对码鉴权） ============

/// 联邦地址作用域：内网
pub const ADDRESS_SCOPE_PRIVATE: &str = "private";
/// 联邦地址作用域：公网
pub const ADDRESS_SCOPE_PUBLIC: &str = "public";

/// 联邦地址条目（P7 多地址模型，三层之①层：自报全集）
///
/// 节点自报的一个可达地址及其作用域。自报 ≠ 信任：内网地址对调用方未必
/// 有意义（网段撞车 / 不在同一内网），仅作探测候选池，可达性由调用方
/// 探测裁决（见 `dao/organization_link/resolver.rs`）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct FederationAddress {
    /// 地址基址（如 `http://10.0.0.5:8080` 或 `https://peer.example.com`）
    pub url: String,
    /// 作用域：`private`（内网，探测优先）/ `public`（公网，回退）
    pub scope: String,
}

impl FederationAddress {
    /// 是否内网地址（探测优先级更高）
    pub fn is_private(&self) -> bool {
        self.scope == ADDRESS_SCOPE_PRIVATE
    }
}

/// 对端组织目录条目（白名单字段，评审稿 §5.1）
///
/// 仅目录元信息，绝不携带用户 / Agent / 任务 / 消息 / 记忆 / 凭证等业务数据。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct PeerOrgDirectoryEntry {
    /// 组织 ID
    pub id: String,
    /// 组织名称
    pub name: String,
    /// 组织描述
    pub description: String,
    /// 外网访问 Base URL（展示用；联邦通信地址以 link.endpoint 为准）
    pub base_url: String,
    /// 集团名（纯展示标签），可空
    #[serde(default)]
    pub group_name: Option<String>,
    /// 组织状态（1=Active, 0=Disabled）
    pub status: i32,
    /// 组织 DID（`did:key:z...`，S1 密钥底座随目录同步上报）
    ///
    /// S2 起验签必需；缺失/为空时对端按「无身份」处理（fail-closed）。
    /// Option 而非必填的原因：展示场景（LinkItem）与迁移前历史影子行允许缺失，
    /// 必填性由 S2 验签强制而非 DTO 类型。
    #[serde(default)]
    pub did: Option<String>,
    /// 联邦公钥（Ed25519 32B base64，与 did 一一对应）
    #[serde(default)]
    pub verification_key: Option<String>,
    /// 数据版本（毫秒时间戳）：新者胜比较基准
    pub updated_at: i64,
    /// 自报联邦地址列表（P7 多地址模型）
    ///
    /// Local 组织 = 配置动态推导（即时生效不入库）；影子组织 = 目录同步复制。
    /// None = 对端旧版本未上报（探测退化为仅主地址，向后兼容）。
    #[serde(default)]
    pub addresses: Option<Vec<FederationAddress>>,
}

/// 验证配对码 + 交换身份（机器侧，配对码鉴权）
///
/// 调用方（本地节点）凭配对码调对端 `POST /links/pairing/verify`。
/// 请求携带本地组织的目录条目（含 DID + 公钥）+ 交叉签名证明
/// 「调用方拥有其声明 DID 的私钥」；响应回带对端目录条目 + 对端交叉签名。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct VerifyPairingCodeRequest {
    /// 配对码（明文）
    pub pairing_code: String,
    /// 本地节点组织目录条目（供对端写 scope=Linked 影子；did / verification_key
    /// S2 起必填，缺失即拒绝）
    pub local_org: PeerOrgDirectoryEntry,
    /// 本地节点联邦地址（对端将来调用本地时用）
    pub local_endpoint: String,
    /// 交叉签名：Ed25519(local_org 私钥) over
    /// `pairing_handshake_message(local_org.did, pairing_code)`，base64url 无 padding
    pub local_signature: String,
}

/// 验证配对码 + 交换身份响应
///
/// 返回对端（本节点）组织目录条目 + 对端交叉签名（证明对端拥有其声明 DID 的
/// 私钥，且把这次建联钉在调用方 DID 上）。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyPairingCodeResponse {
    /// 对端组织目录条目（did / verification_key S2 起必填）
    pub peer_org: PeerOrgDirectoryEntry,
    /// 交叉签名：Ed25519(peer_org 私钥) over
    /// `pairing_handshake_message(local_org.did, pairing_code)`，base64url 无 padding
    pub peer_signature: String,
}

// ============ 建联（用户侧，JWT） ============

/// 发起建联请求（用户在本地输入配对码 + 对端地址）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct CreateLinkRequest {
    /// 对端签发的配对码（明文）
    pub pairing_code: String,
    /// 对端联邦通信基址（如 `https://peer.example.com`），服务端出站完成验证
    pub peer_endpoint: String,
}

/// 已建联条目（前端"关联组织"页数据源）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LinkItem {
    /// 对端组织目录条目
    pub peer_org: PeerOrgDirectoryEntry,
    /// 对端联邦通信地址（本端出站调用目标）
    pub endpoint: String,
    /// 连接状态（1=Active, 0=Revoked）
    pub status: i32,
    /// 建联时间（毫秒时间戳）
    pub created_at: i64,
}

/// 发起建联响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateLinkResponse {
    /// 建联结果条目
    pub link: LinkItem,
}

/// 已建联列表请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListLinksRequest {}

/// 已建联列表响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListLinksResponse {
    /// 已建联条目（Active 在前，按建联时间倒序）
    pub links: Vec<LinkItem>,
}

// ============ 目录同步（机器侧，联邦签名鉴权） ============

/// 组织目录响应（白名单字段，评审稿 §5.1）
///
/// 返回本节点全部组织（Local 组织 + 已知影子）；仅目录元信息，
/// 绝不携带用户 / Agent / 任务 / 消息 / 记忆 / 凭证等业务数据。
/// 出站响应统一过 `redact!`（EXPORT policy）——本结构无凭证类字段，
/// 脱敏不会破坏协议（verify 响应携带交叉签名，**禁止** redact!，
/// 避免脱敏规则误命中签名字段导致建联损坏）。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DirectoryResponse {
    /// 组织目录条目
    pub orgs: Vec<PeerOrgDirectoryEntry>,
}

/// 目录同步推送请求（对端建联完成后主动推送一次本地目录）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct DirectorySyncRequest {
    /// 对端组织目录条目（白名单字段）
    pub orgs: Vec<PeerOrgDirectoryEntry>,
}

/// 目录同步推送响应（无数据）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct DirectorySyncResponse {}

// ============ 断联（用户侧，JWT，管理员） ============

/// 断联请求（路径参数：对端组织 ID）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct RevokeLinkRequest {
    /// 对端组织 ID
    #[param(source = "path")]
    pub peer_org_id: String,
}

/// 断联响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RevokeLinkResponse {
    /// 是否成功断联
    pub success: bool,
}

// ============ 能力发现（机器侧，联邦签名鉴权，P3） ============

/// 跨组织 Agent 委派能力（连接级白名单默认值，第一闭环能力）
pub const CAPABILITY_A2A_TASK: &str = "a2a_task";

/// 对端可调用的 Agent 条目（能力发现响应）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FederationAgentEntry {
    /// Agent ID
    pub id: String,
    /// Agent 名称
    pub name: String,
    /// 角色描述
    pub description: String,
}

/// 能力发现响应：本节点开放给**调用方这条连接**的能力清单 + 可调用 Agent 列表
///
/// 出站响应统一过 `redact!`（仅 id/name/description 白名单字段，无凭证类字段，
/// 脱敏不会破坏协议）。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapabilitiesResponse {
    /// 可调用的 Agent 列表（Onboarded 非 Remote）
    pub agents: Vec<FederationAgentEntry>,
    /// 这条连接开放的能力白名单
    pub capabilities: Vec<String>,
}

// ============ 联邦 Agent 目录（用户侧，JWT，P5） ============

/// 联邦 Agent 分组：一个 Active 对端开放的可调用 Agent 列表
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FederationAgentGroup {
    /// 对端组织 ID（`agent:<id>@<org_id>` 寻址的 org 段）
    pub org_id: String,
    /// 对端组织名（本地影子记录，展示用）
    pub org_name: String,
    /// 对端开放给本连接的 Agent 列表
    pub agents: Vec<FederationAgentEntry>,
    /// 对端连接的能力白名单（含 `a2a_task` 才可委派）
    pub capabilities: Vec<String>,
}

/// 联邦 Agent 目录响应（mention picker 候选数据源）
///
/// 聚合所有 Active 连接的对端 capabilities（实时拉取，单个对端失败仅跳过）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListFederationAgentsResponse {
    /// 联邦 Agent 分组（按 Active 对端组织聚合）
    pub groups: Vec<FederationAgentGroup>,
}

/// 联邦 Agent 目录请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListFederationAgentsRequest {}

// ============ 联邦调用方声明（方案②：凭证直传 + 身份声明） ============

/// 联邦调用方声明（`X-Federation-Caller` header 载荷）
///
/// 发起方（A 侧）随连接凭证一起携带的**明文**身份声明：声明这次调用是以哪个
/// 组织/用户/Agent 身份发起的。B 侧在连接凭证（`authenticate_link_call`）校验
/// 通过后解析，注入 RequestContext 的 `caller_organization_id` 等（R3 计量维度）。
///
/// 信任模型：对端节点可信（与配对建联同级），声明无密码学保护；缺失 =
/// 连接级匿名调用（只有 link 的 peer_org_id 可作为调用方组织）。
/// 字段语义与 R2 Claims iss/aud 对齐，未来升级 token 交换时可平替。
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct FederationCallerDeclaration {
    /// 发起方组织 ID（应与连接的 peer_org_id 一致；不一致以凭证为准并告警）
    #[serde(default)]
    pub caller_org: Option<String>,
    /// 发起方用户 ID（A 侧语义，仅透传展示/计量）
    #[serde(default)]
    pub caller_user: Option<String>,
    /// 发起方 Agent ID（A 侧语义，Agent 委派场景携带）
    #[serde(default)]
    pub caller_agent: Option<String>,
}

impl FederationCallerDeclaration {
    /// 从 header 值解析（明文 JSON）；非法 JSON 返回 None（调用方应 fail-closed 401）
    pub fn from_header_value(value: &str) -> Option<Self> {
        serde_json::from_str(value).ok()
    }

    /// 序列化为 header 值
    pub fn to_header_value(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

// ============ 联邦合约（用户侧，JWT，S3 合约授权） ============

/// 合约条目（管理员"联邦合约"数据源）
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ContractItem {
    /// 合约 ID
    pub id: String,
    /// 对端组织 ID
    pub peer_org_id: String,
    /// 合约类型（`basic`；未来 `task_sla`）
    pub kind: String,
    /// 合约状态（`active` / `terminated`）
    pub state: String,
    /// 能力白名单
    pub capabilities: Vec<String>,
    /// 创建时间戳（毫秒）
    pub created_at: i64,
    /// 更新时间戳（毫秒）
    pub updated_at: i64,
}

/// 合约列表请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct ListContractsRequest {}

/// 合约列表响应
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ListContractsResponse {
    /// 本组织的合约（按创建时间倒序）
    pub contracts: Vec<ContractItem>,
}

/// 更新合约能力集请求（仅 active 合约可编辑）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct UpdateContractCapabilitiesRequest {
    /// 合约 ID
    pub contract_id: String,
    /// 新能力白名单（元素须在已知能力集合内，未知能力拒绝）
    pub capabilities: Vec<String>,
}

/// 更新合约能力集响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateContractCapabilitiesResponse {
    /// 更新后的合约
    pub contract: ContractItem,
}

/// 终止合约请求（fail-closed 熔断开关；不删记录，保留审计线索）
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct TerminateContractRequest {
    /// 对端组织 ID
    pub peer_org_id: String,
}

/// 终止合约响应
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TerminateContractResponse {
    /// 是否成功（幂等操作恒为 true）
    pub success: bool,
}

#[cfg(test)]
mod federation_caller_tests {
    use super::*;

    /// 声明往返：序列化 → 解析保持字段
    #[test]
    fn test_declaration_roundtrip() {
        let decl = FederationCallerDeclaration {
            caller_org: Some("org-a".to_string()),
            caller_user: Some("user-1".to_string()),
            caller_agent: Some("agent-9".to_string()),
        };
        let parsed = FederationCallerDeclaration::from_header_value(&decl.to_header_value())
            .expect("should parse");
        assert_eq!(parsed.caller_org.as_deref(), Some("org-a"));
        assert_eq!(parsed.caller_user.as_deref(), Some("user-1"));
        assert_eq!(parsed.caller_agent.as_deref(), Some("agent-9"));
    }

    /// 最小声明：只有 caller_org 也能解析（字段全部 optional）
    #[test]
    fn test_declaration_minimal() {
        let parsed = FederationCallerDeclaration::from_header_value(r#"{"caller_org":"org-a"}"#)
            .expect("minimal declaration should parse");
        assert_eq!(parsed.caller_org.as_deref(), Some("org-a"));
        assert!(parsed.caller_user.is_none());
        assert!(parsed.caller_agent.is_none());
    }

    /// 非法 JSON 返回 None（调用方 fail-closed 401）
    #[test]
    fn test_declaration_invalid_json_returns_none() {
        assert!(FederationCallerDeclaration::from_header_value("not-json").is_none());
    }
}
