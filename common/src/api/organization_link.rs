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

/// 签发配对码请求（无参数）
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema, Params)]
pub struct IssuePairingCodeRequest {}

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
    /// 数据版本（毫秒时间戳）：新者胜比较基准
    pub updated_at: i64,
}

/// 验证配对码 + 交换凭证请求
///
/// 调用方（本地节点）凭配对码调对端 `POST /links/pairing/verify`。
/// 请求携带本地组织的目录条目 + 联邦地址 + 为对端生成的出站凭证（对端只存哈希）。
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Params)]
pub struct VerifyPairingCodeRequest {
    /// 配对码（明文）
    pub pairing_code: String,
    /// 本地节点组织目录条目（供对端写 scope=Linked 影子）
    pub local_org: PeerOrgDirectoryEntry,
    /// 本地节点联邦地址（对端将来调用本地时用）
    pub local_endpoint: String,
    /// 本地节点为对端生成的凭证（对端调用本地时携带）；对端仅存其 SHA-256 哈希
    pub local_token: String,
}

/// 验证配对码 + 交换凭证响应
///
/// 返回对端（本节点）组织目录条目 + 对端为调用方生成的出站凭证。
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct VerifyPairingCodeResponse {
    /// 对端组织目录条目
    pub peer_org: PeerOrgDirectoryEntry,
    /// 对端为调用方生成的出站凭证（调用方存为 access_token，调用对端时携带）
    pub peer_token: String,
}
