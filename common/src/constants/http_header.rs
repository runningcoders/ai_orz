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
