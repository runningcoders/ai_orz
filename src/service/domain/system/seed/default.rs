//! 内置默认快照
//!
//! 通过 include_str! 嵌入 default.json，无需文件系统即可使用

const DEFAULT_JSON: &str = include_str!("default.json");

/// 获取内置默认快照
pub fn embedded_default_snapshot() -> super::defs::SeedSnapshot {
    serde_json::from_str(DEFAULT_JSON)
        .expect("内置 default.json 解析失败（编译期检查）")
}
