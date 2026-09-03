//! 场景化脱敏策略预设
//!
//! 不同出口对「信息完整度」与「安全强度」的诉求不同，用一份策略描述差异，
//! 避免调用方散落魔法参数。

use super::mask::MaskStyle;

/// JSON 遍历的默认最大深度（超出部分原样保留，防深嵌套栈溢出）
const DEFAULT_MAX_DEPTH: usize = 16;
/// 单个字符串值参与文本扫描的默认最大字节数
const DEFAULT_MAX_TEXT_BYTES: usize = 1024 * 1024;

/// 脱敏策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RedactPolicy {
    /// 命中后的值呈现形态
    pub style: MaskStyle,
    /// 是否对非敏感键下的字符串值做文本级扫描
    ///
    /// 文本级扫描能捕获命令参数、错误文本里的 `key=value` / `--flag value` 等
    /// 自由文本凭证，但会遍历每个字符串值，成本更高。
    pub scan_free_text: bool,
    /// JSON 遍历最大深度
    pub max_depth: usize,
    /// 单个字符串值超过此字节数时跳过文本扫描（防超大 payload 拖慢响应）
    pub max_text_bytes: usize,
}

impl Default for RedactPolicy {
    fn default() -> Self {
        EXPORT
    }
}

/// 对外接口输出（默认策略）
///
/// 用于所有经 HTTP 接口向外部返回的响应体。保留首尾便于运维定位凭证来源，
/// 同时开启自由文本扫描以覆盖内嵌在命令参数与错误文本里的凭证。
pub const EXPORT: RedactPolicy = RedactPolicy {
    style: MaskStyle::Partial,
    scan_free_text: true,
    max_depth: DEFAULT_MAX_DEPTH,
    max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
};

/// 内部持久化（当前未启用，保留为扩展点）
///
/// 项目边界决策：系统内部存储（JSONL trace、SQLite）保持原文不动，风险由
/// 访问控制承担。若后续需要为落库单独收紧，在此调整即可，无需改调用点。
pub const PERSIST: RedactPolicy = RedactPolicy {
    style: MaskStyle::Partial,
    scan_free_text: true,
    max_depth: DEFAULT_MAX_DEPTH,
    max_text_bytes: DEFAULT_MAX_TEXT_BYTES,
};

/// 日志输出（当前未接入，保留为扩展点）
///
/// 日志不直接对外展示，且写入路径性能敏感，故关闭自由文本扫描、压低深度上限。
pub const LOG: RedactPolicy = RedactPolicy {
    style: MaskStyle::Full,
    scan_free_text: false,
    max_depth: 8,
    max_text_bytes: 4 * 1024,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_export() {
        assert_eq!(RedactPolicy::default(), EXPORT);
    }

    #[test]
    fn export_keeps_partial_style() {
        // 对外输出保留首尾，便于定位是哪个凭证出问题
        assert_eq!(EXPORT.style, MaskStyle::Partial);
        const { assert!(EXPORT.scan_free_text) };
    }

    #[test]
    fn log_policy_is_cheaper() {
        const {
            assert!(!LOG.scan_free_text);
            assert!(LOG.max_depth < EXPORT.max_depth);
            assert!(LOG.max_text_bytes < EXPORT.max_text_bytes);
        }
    }
}
