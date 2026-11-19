//! token 估算与按预算截断（纯函数，跨层共用）
//!
//! 不引真实 tokenizer：估算只服务于「预算分级 / 是否该裁」这类策略判断，
//! 口径固定为「CJK 1 token/字、其余 4 字符/token 向上取整」。
//!
//! ## 为什么在 pkg
//!
//! 使用方横跨数层：运行时（压缩 / 思考强度 / 小脑 state 裁剪）、dal（prompt
//! 组装）、producer（入站上下文裁剪）。原先长在 `domain::runtime::compaction`
//! 里，导致 `dal → domain`、`producer → domain` 反向依赖。它们是**纯算法**，
//! 没有领域语义，唯一正确的位置是底层共享。
//!
//! 份额表（各消费点占上下文容量的比例）见 [`crate::pkg::text_budget`]。

/// 是否 CJK 字符（中日韩表意文字 / 假名 / 谚文 / 全角标点）
///
/// 只用于 token 估算：CJK 一字约等于 1 token，拉丁文约 4 字符 1 token。
fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{3400}'..='\u{4DBF}'
            | '\u{4E00}'..='\u{9FFF}'
            | '\u{3040}'..='\u{30FF}'
            | '\u{AC00}'..='\u{D7AF}'
            | '\u{FF00}'..='\u{FFEF}'
    )
}

/// 估算一段文本的 token 数（不引真实 tokenizer，够策略分级用）
///
/// 规则：CJK 按 1 token/字；其余（拉丁字母/数字/ASCII 标点）按 4 字符/token，
/// 向上取整。混合文本按字符逐类累加，避免同一段中英夹杂时估算失真。
pub(crate) fn estimate_tokens(text: &str) -> usize {
    let mut cjk = 0usize;
    let mut other = 0usize;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other.div_ceil(4)
}

/// 按 token 预算截断到最大可行前缀，并留下调用方指定的截断标注
///
/// 标注本身的 token 也计入 `budget`：否则「正好截到预算」再拼一段标注就会越界
/// （实测过 436 > 400）。返回值恒满足 `estimate_tokens(..) <= budget`。
///
/// **唯一实现**：链内正文裁剪（`chain_reply`）、小脑 state 裁剪（`dal::brain`）、
/// 入站上下文裁剪（`producer::inbound_filter`）共用本函数，只差标注文案——
/// 各调用点文案语义不同，故由调用方传入，不写死。
pub(crate) fn truncate_to_tokens(text: &str, budget: usize, suffix: &str) -> String {
    if estimate_tokens(text) <= budget {
        return text.to_string();
    }
    let body_budget = budget.saturating_sub(estimate_tokens(suffix));
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let prefix: String = chars[..mid].iter().collect();
        if estimate_tokens(&prefix) <= body_budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    // 预算连一个字符都装不下：只留标注，保证「有内容被截」这一事实可见
    if lo == 0 {
        return suffix.to_string();
    }
    let mut out: String = chars[..lo].iter().collect();
    out.push_str(suffix);
    out
}

#[cfg(test)]
mod tests {
    use super::{estimate_tokens, truncate_to_tokens};

    #[test]
    fn cjk_counts_one_per_char_latin_quarter() {
        assert_eq!(estimate_tokens("中文四字"), 4);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2, "向上取整");
        assert_eq!(estimate_tokens("中abcd"), 2, "混合按类累加");
        assert_eq!(estimate_tokens(""), 0);
    }

    #[test]
    fn truncate_keeps_within_budget_and_marks_cut() {
        let text = "中".repeat(100);
        let out = truncate_to_tokens(&text, 10, "…");
        assert!(estimate_tokens(&out) <= 10, "截断后必须满足预算");
        assert!(out.ends_with('…'), "必须留下截断标注");
    }

    #[test]
    fn short_text_is_returned_verbatim() {
        assert_eq!(truncate_to_tokens("短文本", 10, "…"), "短文本");
    }

    #[test]
    fn suffix_larger_than_budget_keeps_only_suffix() {
        // 预算连一个字符都装不下：仍要保留「被截」这一事实
        assert_eq!(truncate_to_tokens(&"中".repeat(50), 0, "…"), "…");
    }
}
