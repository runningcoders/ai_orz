//! 脱敏样式：决定命中后的值以何种形态呈现

use super::rule::MASK_FULL;

/// 脱敏样式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskStyle {
    /// 保留首尾、遮蔽中段（默认）
    ///
    /// 例：`sk-proj-abcdef123456` → `sk-p***3456`
    ///
    /// 目的：既防止凭证被完整读取，又保留足够线索让运维定位「是哪个 key 出问题」。
    /// 过短的值（不足 [`MIN_PARTIAL_LEN`] 字符）保留首尾已无辨识价值，退化为全量遮蔽。
    #[default]
    Partial,
    /// 全量遮蔽为 `***`
    Full,
}

/// Partial 样式保留的首部字符数
const HEAD_CHARS: usize = 4;
/// Partial 样式保留的尾部字符数
const TAIL_CHARS: usize = 4;
/// 启用 Partial 的最小字符数：短于此值首尾各 4 字符已遮蔽不了多少，直接全量
const MIN_PARTIAL_LEN: usize = HEAD_CHARS + TAIL_CHARS + MASK_FULL.len();

/// 按指定样式生成脱敏值
///
/// 按 char（而非字节）切分，避免切断多字节字符边界。
pub fn mask_value(value: &str, style: MaskStyle) -> String {
    if value.is_empty() {
        return String::new();
    }

    match style {
        MaskStyle::Full => MASK_FULL.to_string(),
        MaskStyle::Partial => {
            let len = value.chars().count();
            if len < MIN_PARTIAL_LEN {
                return MASK_FULL.to_string();
            }
            let mut out = String::with_capacity(HEAD_CHARS + MASK_FULL.len() + TAIL_CHARS);
            out.extend(value.chars().take(HEAD_CHARS));
            out.push_str(MASK_FULL);
            out.extend(value.chars().skip(len - TAIL_CHARS));
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_style_always_masks() {
        assert_eq!(mask_value("sk-abcdef123456", MaskStyle::Full), "***");
        assert_eq!(mask_value("ab", MaskStyle::Full), "***");
    }

    #[test]
    fn partial_style_keeps_head_and_tail() {
        assert_eq!(
            mask_value("sk-abcdef123456", MaskStyle::Partial),
            "sk-a***3456"
        );
        assert_eq!(
            mask_value("hunter2hunter2", MaskStyle::Partial),
            "hunt***ter2"
        );
    }

    #[test]
    fn short_values_degrade_to_full() {
        // 短值保留首尾辨识度低，且遮蔽比例不足，直接全量遮蔽
        assert_eq!(mask_value("hunter2", MaskStyle::Partial), "***");
        assert_eq!(mask_value("abcdefghij", MaskStyle::Partial), "***");
        // 恰好达到阈值时启用 Partial
        assert_eq!(mask_value("abcdefghijk", MaskStyle::Partial), "abcd***hijk");
    }

    #[test]
    fn empty_value_stays_empty() {
        assert_eq!(mask_value("", MaskStyle::Partial), "");
        assert_eq!(mask_value("", MaskStyle::Full), "");
    }

    #[test]
    fn multibyte_text_is_not_split() {
        // 中文字符占 3 字节，按字节切会 panic 或产生乱码
        assert_eq!(
            mask_value("这是很长的中文凭证内容", MaskStyle::Partial),
            "这是很长***凭证内容"
        );
    }

    #[test]
    fn partial_does_not_leak_middle() {
        let secret = "sk-proj-SUPERSECRETVALUE-1234567890";
        let masked = mask_value(secret, MaskStyle::Partial);
        assert!(!masked.contains("SUPERSECRETVALUE"));
        assert!(masked.starts_with("sk-p"));
        assert!(masked.ends_with("7890"));
    }
}
