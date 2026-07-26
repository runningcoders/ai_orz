//! Diff 算法 + 敏感字段解析（纯函数）

use std::collections::HashMap;
use super::defs::*;

/// 对比两个快照（纯函数）
pub fn diff_snapshots(_base: &SeedSnapshot, _target: &SeedSnapshot) -> SeedDiff {
    unimplemented!("将在 Task 3 实现")
}

/// 校验敏感字段是否齐备（纯函数）
pub fn validate_sensitive_fields(
    _snapshot: &SeedSnapshot,
    _sensitive_values: &HashMap<String, String>,
) -> Result<(), String> {
    unimplemented!("将在 Task 3 实现")
}

/// 解析密码占位符（纯函数，current_password 由 handler 查 DB 后传入）
pub fn resolve_password(
    _ref_value: &str,
    _user_id: &str,
    _sensitive_values: &HashMap<String, String>,
    _current_password_hash: Option<&str>,
) -> Result<String, String> {
    unimplemented!("将在 Task 3 实现")
}

/// 解析 API Key 占位符
pub fn resolve_api_key(
    _ref_value: &str,
    _provider_id: &str,
    _sensitive_values: &HashMap<String, String>,
    _current_api_key: Option<&str>,
) -> Result<String, String> {
    unimplemented!("将在 Task 3 实现")
}
