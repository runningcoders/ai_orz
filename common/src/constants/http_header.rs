//! HTTP Header Keys（统一管理所有 header key）

/// 请求追踪 ID（用于日志串联）
pub const LOG_ID: &str = "X-Log-Id";

/// 当前用户 ID
pub const USER_ID: &str = "X-User-Id";

/// 当前用户名
pub const USERNAME: &str = "X-Username";

/// 当前组织 ID
pub const ORGANIZATION_ID: &str = "X-Organization-Id";

/// 调用方所属组织 ID（联邦计量维度，R3：iss 优先，存量 token 回退 organization_id）
pub const CALLER_ORGANIZATION_ID: &str = "X-Caller-Organization-Id";

/// 当前用户角色（UserRole 数值）
pub const USER_ROLE: &str = "X-User-Role";

/// 调用方类型（CallerType: user/agent/system 或 0/1/2）
pub const CALLER_TYPE: &str = "X-Caller-Type";

/// 联邦调用方声明（方案②：明文 JSON，连接凭证鉴权通过后作为身份补充声明；
/// 对端节点可信前提下接受，无密码学保护——升级路径见跨组织业务调用方案 F1）
pub const FEDERATION_CALLER: &str = "X-Federation-Caller";

/// 联邦签名四件套（S2 联邦鉴权升级，SSOT: docs/plan/联邦鉴权升级方案.md §四）：
/// 每请求 Ed25519 签名，替代共享密钥 Bearer。四个头缺一即 401（无回退路径）。
/// 发起方组织 DID（`did:key:z6Mk...`，入站与 link.peer_did 比对防跨连接冒充）
pub const FEDERATION_KEY_ID: &str = "X-Federation-Key-Id";
/// Unix 秒级时间戳（±300s 窗口）
pub const FEDERATION_TIMESTAMP: &str = "X-Federation-Timestamp";
/// 随机 16 字节 hex（进程内去重，防重放）
pub const FEDERATION_NONCE: &str = "X-Federation-Nonce";
/// Ed25519 签名，base64url 无 padding（规范串 = method\npath\ntimestamp\nnonce\nsha256(body)）
pub const FEDERATION_SIGNATURE: &str = "X-Federation-Signature";
