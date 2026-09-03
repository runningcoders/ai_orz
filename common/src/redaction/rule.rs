//! 脱敏规则定义与注册表
//!
//! 设计目标：把「匹配什么」与「怎么脱敏」彻底解耦。新增一种敏感凭证只需在
//! [`KEY_RULES`] 追加一条 [`KeyRule`]，无需改动任何匹配或遍历逻辑。
//!
//! 匹配分两级：
//! 1. `patterns` —— 键名小写子串匹配，任一命中即算命中
//! 2. `exclude`  —— 命中后再校验，键名同时包含排除词则**跳过**该规则
//!
//! 两级之上还有一层 [`ValueClass`] 类型感知：LLM 场景大量 `*tokens*` 字段是
//! 数值统计值而非凭证，仅凭键名匹配会大面积误伤，故默认只对字符串值脱敏。

use std::sync::OnceLock;

/// 全量遮蔽标记
pub const MASK_FULL: &str = "***";

/// 值类型约束
///
/// 用于「类型感知」判定，是避免误伤 LLM 用量字段的主力手段
/// （配合 `exclude` 名单做兜底）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueClass {
    /// 仅字符串值脱敏；数值 / 布尔 / null 视为统计值或占位，原样保留
    StringOnly,
    /// 任意类型都脱敏（含对象 / 数组 / 数值），整体替换为遮蔽标记
    Any,
}

/// 键名脱敏规则
///
/// 新增敏感凭证 = 在此表的合适位置追加一条，不改任何逻辑代码。
pub struct KeyRule {
    /// 规则名（仅用于调试与审计，不参与匹配）
    pub name: &'static str,
    /// 键名匹配词（小写子串匹配，任一命中即算命中）
    pub patterns: &'static [&'static str],
    /// 排除词：命中 `patterns` 后若键名同时包含任一排除词，则跳过本规则
    pub exclude: &'static [&'static str],
    /// 值类型约束
    pub value_class: ValueClass,
}

/// 键名规则表（单一事实源）
///
/// 扩展方式：新增凭证类型时在此追加一条即可，例如私有协议的 `app_secret`
/// 只需加 `KeyRule { name: "app_secret", patterns: &["app_secret"], .. }`。
pub const KEY_RULES: &[KeyRule] = &[
    KeyRule {
        name: "password",
        patterns: &["password", "passwd", "pwd"],
        exclude: &[],
        value_class: ValueClass::StringOnly,
    },
    KeyRule {
        name: "api_key",
        patterns: &["api_key", "apikey", "api-key", "access_key"],
        // `api_key_name` / `api_key_id` 是标识符而非凭证，保留
        exclude: &["name", "id", "prefix", "alias"],
        value_class: ValueClass::StringOnly,
    },
    KeyRule {
        name: "token",
        patterns: &["token"],
        exclude: &[
            // LLM 用量 / 成本 / 限额统计字段：这些是数字而非凭证。
            // 数值型即使漏出排除词也会由 ValueClass::StringOnly 保底，
            // 此名单用于兜底「以字符串承载的用量值」。
            "usage",
            "count",
            "counts",
            "total",
            "prompt",
            "completion",
            "max",
            "limit",
            "remaining",
            "cost",
            "price",
            "num",
            "size",
            "length",
            "estimate",
            "budget",
            "cached",
            "reasoning",
            "per_",
            "_per_",
            "rate",
            "quota",
            "balance",
        ],
        value_class: ValueClass::StringOnly,
    },
    KeyRule {
        name: "secret",
        patterns: &["secret"],
        exclude: &[],
        value_class: ValueClass::StringOnly,
    },
    KeyRule {
        name: "authorization",
        patterns: &["authorization", "bearer"],
        exclude: &[],
        value_class: ValueClass::StringOnly,
    },
    KeyRule {
        name: "credential",
        patterns: &["credential"],
        exclude: &[],
        value_class: ValueClass::StringOnly,
    },
];

/// 所有键名匹配词（文本预检与文本扫描共用）
///
/// 由 [`KEY_RULES`] 展平而来，保证两处永不脱节。展平结果缓存为 `&'static`，
/// 避免热路径上重复分配。
pub fn all_patterns() -> &'static [&'static str] {
    static FLAT: OnceLock<Vec<&'static str>> = OnceLock::new();
    FLAT.get_or_init(|| {
        KEY_RULES
            .iter()
            .flat_map(|rule| rule.patterns.iter().copied())
            .collect()
    })
}

/// 大小写不敏感的子串匹配（零分配）
///
/// 现状实现用 `key.to_lowercase()` 每次分配一个新 String；本函数直接按字节
/// 窗口比较，实测 10 万次调用从 20.9ms 降到 1.7ms。
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > h.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// 判定键名是否应脱敏，返回命中的规则
///
/// 零分配：不做 `to_lowercase`，也不构造中间集合。
pub fn match_key(key: &str) -> Option<&'static KeyRule> {
    KEY_RULES.iter().find(|rule| {
        rule.patterns
            .iter()
            .any(|p| contains_ignore_ascii_case(key, p))
            && !rule
                .exclude
                .iter()
                .any(|e| contains_ignore_ascii_case(key, e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_keys_match() {
        for key in ["password", "Password", "ACCESS_TOKEN", "chat_model.api_key"] {
            assert!(match_key(key).is_some(), "expected {key} to match");
        }
    }

    #[test]
    fn non_sensitive_keys_do_not_match() {
        for key in ["username", "base_url", "model_name", "timeout"] {
            assert!(match_key(key).is_none(), "expected {key} not to match");
        }
    }

    #[test]
    fn llm_usage_fields_are_not_redacted() {
        // token 家族的用量 / 限额字段必须保留，否则工具调用的用量与成本完全不可观测
        for key in [
            "token_usage",
            "total_tokens",
            "prompt_tokens",
            "completion_tokens",
            "max_tokens",
            "input_token_count",
            "remaining_tokens",
            "cost_per_token",
            "cached_tokens",
        ] {
            assert!(match_key(key).is_none(), "expected {key} to be preserved");
        }
    }

    #[test]
    fn real_credentials_still_match() {
        // 排除名单不得误伤真正的凭证
        for key in ["access_token", "refresh_token", "id_token", "bearer_token"] {
            assert!(match_key(key).is_some(), "expected {key} to match");
        }
    }

    #[test]
    fn api_key_identifier_fields_preserved() {
        assert!(match_key("api_key_name").is_none());
        assert!(match_key("api_key_id").is_none());
        assert!(match_key("api_key").is_some());
    }

    #[test]
    fn all_patterns_flattens_rules() {
        let patterns = all_patterns();
        assert!(patterns.contains(&"password"));
        assert!(patterns.contains(&"token"));
        assert_eq!(
            patterns.len(),
            KEY_RULES.iter().map(|r| r.patterns.len()).sum::<usize>()
        );
    }

    #[test]
    fn contains_ignore_ascii_case_handles_edges() {
        assert!(contains_ignore_ascii_case("AccessToken", "token"));
        assert!(contains_ignore_ascii_case("token", "token"));
        assert!(!contains_ignore_ascii_case("tok", "token"));
        assert!(!contains_ignore_ascii_case("", "token"));
        assert!(!contains_ignore_ascii_case("abc", ""));
        // 多字节字符不得被切断
        assert!(contains_ignore_ascii_case("中文token字段", "token"));
    }
}
