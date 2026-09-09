//! 公共工具函数

/// 组织连接默认能力白名单（JSON 数组；P3：连接级白名单，第一闭环能力 a2a_task）
pub const DEFAULT_LINK_CAPABILITIES: &str = "[\"a2a_task\"]";

/// basic 合约的条款哈希占位常量（S3：保证列存在；未来合同机制换成真实条款哈希）
pub const BASIC_CONTRACT_TERMS_HASH: &str = "basic";

/// 获取当前时间戳（秒）
pub fn current_timestamp() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// 获取当前时间戳（毫秒）
pub fn current_timestamp_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
