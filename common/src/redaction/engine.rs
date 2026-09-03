//! 脱敏引擎：JSON 遍历器 + 文本扫描器
//!
//! 性能要点
//! --------
//! 1. **键名判定零分配**：不做 `to_lowercase()`，改为字节窗口 + `eq_ignore_ascii_case`
//!    （实测 10 万次调用 20.9ms → 1.7ms）。
//! 2. **AC 预检快速路径**：文本扫描前先用 Aho-Corasick 判断全文是否含任一敏感词，
//!    无命中直接返回 `Cow::Borrowed`，零拷贝零分配。
//! 3. **禁止切片优化**：CLI flag 形态 `--token secret123` 的识别依赖左侧 `-` 上下文，
//!    一旦把扫描区间切掉前导安全区，`--token` 的 `-` 就会丢失导致漏脱敏。
//!    实测过该优化，收益为零且引入漏脱，故明令禁止。

use std::borrow::Cow;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use serde_json::Value;
use std::sync::OnceLock;

use super::mask::{MaskStyle, mask_value};
use super::policy::RedactPolicy;
use super::rule::{
    KeyRule, MASK_FULL, ValueClass, all_patterns, match_key, match_value_shape, value_prefixes,
};

/// 文本预检自动机（敏感词是否存在于全文）
static PRECHECK: OnceLock<AhoCorasick> = OnceLock::new();

fn precheck() -> &'static AhoCorasick {
    PRECHECK.get_or_init(|| {
        AhoCorasickBuilder::new()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostFirst)
            .build(all_patterns())
            .expect("failed to build redaction precheck automaton")
    })
}

/// 预热：在系统启动时构建预检自动机，让构建失败在启动期暴露而非运行期
pub fn warmup() {
    let _ = precheck();
}

/// 就地递归脱敏 JSON，返回是否发生了修改
///
/// 保留完整结构与值类型：字符串值替换为脱敏串，数值 / 布尔值在非 `Any` 规则下
/// 视为统计值保留，不会产生 `"1536"` 这类类型破坏。
///
/// 返回值供中间件等调用方判断是否值得重新序列化——未修改时可直接复用原字节。
pub fn redact_json(value: &mut Value, policy: RedactPolicy) -> bool {
    redact_value(value, policy, 0)
}

fn redact_value(value: &mut Value, policy: RedactPolicy, depth: usize) -> bool {
    if depth > policy.max_depth {
        return false;
    }

    match value {
        Value::Object(map) => {
            let mut changed = false;
            for (key, val) in map.iter_mut() {
                let hit = match match_key(key) {
                    Some(rule) => redact_matched_value(val, rule, policy, depth),
                    None => redact_value(val, policy, depth + 1),
                };
                changed |= hit;
            }
            changed
        }
        Value::Array(items) => {
            let mut changed = false;
            for item in items.iter_mut() {
                changed |= redact_value(item, policy, depth + 1);
            }
            changed
        }
        Value::String(s) => {
            if !policy.scan_free_text || s.len() > policy.max_text_bytes {
                return false;
            }
            match redact_text(s, policy) {
                Cow::Owned(owned) => {
                    *s = owned;
                    true
                }
                Cow::Borrowed(_) => {
                    // 文本扫描无命中：再试「值形态」兜底（键名未命中但值长得像凭证）。
                    // 整串即值，无边界歧义，直接全量遮蔽。
                    if match_value_shape(s).is_some() {
                        *s = mask_value(s, MaskStyle::Full);
                        true
                    } else {
                        false
                    }
                }
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => false,
    }
}

/// 处理命中敏感键的值，返回是否发生了修改
fn redact_matched_value(
    value: &mut Value,
    rule: &KeyRule,
    policy: RedactPolicy,
    depth: usize,
) -> bool {
    match rule.value_class {
        // Any：整体替换为遮蔽标记（结构被压平，用于「整块都是秘密」的字段）
        ValueClass::Any => {
            *value = Value::String(MASK_FULL.to_string());
            true
        }
        ValueClass::StringOnly => match value {
            // 字符串：真正需要脱敏的情况
            Value::String(s) => {
                *s = mask_value(s, policy.style);
                true
            }
            // 容器：字段级脱敏，继续下钻保证内部敏感值不漏
            Value::Object(_) | Value::Array(_) => redact_value(value, policy, depth + 1),
            // 数值 / 布尔 / null：统计值或占位，保留
            _ => false,
        },
    }
}

/// 文本级脱敏：扫描「敏感键 + 分隔符 + 值」模式，把值替换为脱敏串
///
/// 支持的值形态：
/// - `key=value` / `key: value`（裸值，止于空白、逗号、分号、`}`、`]`、引号）
/// - `"key":"value"`（JSON 字符串形态，**保留两侧引号**，不破坏 JSON 合法性）
/// - `key="value"`（引号包裹值）
/// - `--key value`（CLI flag 形态，要求 key 前为 `-`）
/// - `Authorization: Bearer xxx`（整体全量遮蔽）
///
/// 无敏感词时返回 `Cow::Borrowed`，调用方可据此跳过写回。
pub fn redact_text(text: &str, policy: RedactPolicy) -> Cow<'_, str> {
    if !policy.scan_free_text || text.len() > policy.max_text_bytes {
        return Cow::Borrowed(text);
    }
    // 预检：全文无敏感词 → 零拷贝返回
    if !precheck().is_match(text) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(scan_and_mask(text, policy))
}

fn scan_and_mask(text: &str, policy: RedactPolicy) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;

    while i < bytes.len() {
        let mut matched = false;

        // 裸凭证值形态识别（sk- / ghp_ / eyJ ...）：不依赖敏感键名，但必须锚定在
        // token 边界（前导为空白/引号/分隔符/行首），避免把 "ask-" 里的 "sk-" 误判为凭证。
        if let Some(end) = value_shape_boundary_match(bytes, i) {
            out.push_str(&mask_value(&text[i..end], MaskStyle::Full));
            i = end;
            matched = true;
        } else {
            for pattern in all_patterns() {
                let key = pattern.as_bytes();
                let key_end = i + key.len();
                if key_end > bytes.len() || !bytes[i..key_end].eq_ignore_ascii_case(key) {
                    continue;
                }

                let mut j = key_end;
                let had_space = j < bytes.len() && bytes[j] == b' ';
                while j < bytes.len() && bytes[j] == b' ' {
                    j += 1;
                }
                if j >= bytes.len() {
                    continue;
                }

                // 定位值的完整区间 [start, end)；quoted 表示值被引号包裹
                let (value_start, value_end, quoted) = match bytes[j] {
                    // JSON 形态："key":"value"
                    b'"' => {
                        let mut k = j + 1;
                        while k < bytes.len() && (bytes[k] == b' ' || bytes[k] == b':') {
                            k += 1;
                        }
                        match quoted_value_range(bytes, k) {
                            Some((vs, ve)) => (vs, ve, true),
                            None => continue,
                        }
                    }
                    // KV 形态：key=value / key: value
                    b'=' | b':' => {
                        let mut k = j + 1;
                        while k < bytes.len() && bytes[k] == b' ' {
                            k += 1;
                        }
                        let (ve, q) = bare_value_end(bytes, k);
                        (k, ve, q)
                    }
                    // CLI flag 形态：`--key value`
                    // 依赖 bytes[i - 1] 的 `-` 上下文，禁止对扫描区间做切片优化
                    _ if had_space && i > 0 && bytes[i - 1] == b'-' => {
                        // 值以 `-` 开头表示下一个 flag，不脱敏
                        if bytes.get(j) == Some(&b'-') {
                            continue;
                        }
                        let (ve, q) = bare_value_end(bytes, j);
                        (j, ve, q)
                    }
                    _ => continue,
                };

                if value_end <= value_start {
                    break;
                }

                let raw = if quoted {
                    &text[value_start + 1..value_end - 1]
                } else {
                    &text[value_start..value_end]
                };

                // Authorization 头整体就是一个凭证，保留首尾无定位价值，强制全量遮蔽
                let style = if raw.len() > 7 && raw.as_bytes()[..7].eq_ignore_ascii_case(b"bearer ")
                {
                    MaskStyle::Full
                } else {
                    policy.style
                };

                out.push_str(&text[i..value_start]);
                if quoted {
                    out.push('"');
                }
                out.push_str(&mask_value(raw, style));
                if quoted {
                    out.push('"');
                }
                i = value_end;
                matched = true;
                break;
            }
        } // else（键名模式未命中，走裸凭证值形态兜底）

        if !matched {
            // 逐字符推进（多字节 UTF-8 一次推一个 char，避免切断字符边界）
            let mut next = i + 1;
            while next < bytes.len() && !text.is_char_boundary(next) {
                next += 1;
            }
            out.push_str(&text[i..next]);
            i = next;
        }
    }

    out
}

/// 在位置 `i` 尝试「裸凭证值形态」匹配，返回值的结束下标（不含）。
///
/// 要求 `i` 处于 token 起点（前导为空白/引号/分隔符或行首），否则返回 `None`，
/// 避免把 `"ask-Ed"` 里的 `sk-` 误判为凭证。命中后值区间止于常规分隔符。
fn value_shape_boundary_match(bytes: &[u8], i: usize) -> Option<usize> {
    let prev_ok = i == 0
        || matches!(
            bytes[i - 1],
            b' ' | b'\t'
                | b'\n'
                | b'\r'
                | b'"'
                | b'\''
                | b'{'
                | b'}'
                | b'['
                | b']'
                | b'('
                | b')'
                | b','
                | b';'
                | b':'
                | b'='
                | b'/'
                | b'\\'
        );
    if !prev_ok {
        return None;
    }
    for prefix in value_prefixes() {
        if bytes[i..].starts_with(prefix.as_bytes()) {
            let (end, _) = bare_value_end(bytes, i + prefix.len());
            // 必须存在前缀之后的内容，避免把孤立的 "sk-" 误判为凭证
            if end > i + prefix.len() {
                return Some(end);
            }
        }
    }
    None
}

/// 定位引号包裹值的完整区间（含两端引号）
///
/// 无收尾引号时返回 `None` —— 畸形输入宁可不脱敏，也不产生破坏结构的输出。
fn quoted_value_range(bytes: &[u8], start: usize) -> Option<(usize, usize)> {
    if bytes.get(start) != Some(&b'"') {
        return None;
    }
    let mut j = start + 1;
    while j < bytes.len() && bytes[j] != b'"' {
        if bytes[j] == b'\\' {
            j += 1; // 跳过转义符
        }
        j += 1;
    }
    if j >= bytes.len() {
        return None;
    }
    Some((start, j + 1))
}

/// 定位裸值的结束位置，返回 `(end, quoted)`
fn bare_value_end(bytes: &[u8], start: usize) -> (usize, bool) {
    if let Some((_, end)) = quoted_value_range(bytes, start) {
        return (end, true);
    }

    let mut end = start;
    while end < bytes.len() {
        let c = bytes[end];
        if c.is_ascii_whitespace() || matches!(c, b',' | b';' | b'}' | b']' | b'"' | b'\'') {
            break;
        }
        end += 1;
    }

    // `Bearer xxx` —— 凭证在第二个 token，需一并吞掉
    if bytes[start..end].eq_ignore_ascii_case(b"bearer") {
        let mut k = end;
        while k < bytes.len() && bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        while k < bytes.len() {
            let c = bytes[k];
            if c.is_ascii_whitespace() || matches!(c, b',' | b';' | b'}' | b']' | b'"' | b'\'') {
                break;
            }
            k += 1;
        }
        if k > end {
            end = k;
        }
    }

    (end, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_object_redacts_sensitive_keys() {
        let mut value = json!({
            "username": "alice",
            "password": "hunter2hunter2",
            "chat_model": { "api_key": "sk-abcdef123456" }
        });
        redact_json(&mut value, RedactPolicy::default());
        assert_eq!(value["username"], "alice");
        assert_eq!(value["password"], "hunt***ter2");
        assert_eq!(value["chat_model"]["api_key"], "sk-a***3456");
    }

    #[test]
    fn json_preserves_structure_and_types() {
        // 数值型 token 字段是统计值，不得被抹掉或转成字符串
        let mut value = json!({
            "max_tokens": 4096,
            "total_tokens": 1536,
            "prompt_tokens": 512,
            "enabled": true
        });
        redact_json(&mut value, RedactPolicy::default());
        assert_eq!(value["max_tokens"], 4096);
        assert_eq!(value["total_tokens"], 1536);
        assert_eq!(value["prompt_tokens"], 512);
        assert_eq!(value["enabled"], true);
    }

    #[test]
    fn json_recurses_into_arrays() {
        let mut value = json!({"items": [{"password": "hunter2hunter2"}, {"note": "ok"}]});
        redact_json(&mut value, RedactPolicy::default());
        assert_eq!(value["items"][0]["password"], "hunt***ter2");
        assert_eq!(value["items"][1]["note"], "ok");
    }

    #[test]
    fn json_recurses_inside_sensitive_container() {
        // 命中敏感键的对象继续下钻，保证内部真凭证不漏
        let mut value = json!({"credentials": {"password": "hunter2hunter2", "user": "bob"}});
        redact_json(&mut value, RedactPolicy::default());
        assert_eq!(value["credentials"]["password"], "hunt***ter2");
        assert_eq!(value["credentials"]["user"], "bob");
    }

    #[test]
    fn json_scans_string_values_as_free_text() {
        let mut value = json!({"command": "git push --token secret123"});
        redact_json(&mut value, RedactPolicy::default());
        assert_eq!(value["command"], "git push --token ***");
    }

    #[test]
    fn json_depth_limit_is_enforced() {
        let mut deep = json!({"password": "hunter2hunter2"});
        for _ in 0..40 {
            deep = json!({"nested": deep});
        }
        let shallow = RedactPolicy {
            max_depth: 4,
            ..RedactPolicy::default()
        };
        redact_json(&mut deep, shallow);
        // 深度超出后原样保留，不得 panic
        let mut cur = &deep;
        for _ in 0..40 {
            cur = &cur["nested"];
        }
        assert_eq!(cur["password"], "hunter2hunter2");
    }

    #[test]
    fn text_kv_patterns() {
        assert_eq!(
            redact_text(
                "connecting with api_key=sk-abcdef123456 ok",
                RedactPolicy::default()
            ),
            "connecting with api_key=sk-a***3456 ok"
        );
        assert_eq!(
            redact_text("password: hunter2hunter2, retry", RedactPolicy::default()),
            "password: hunt***ter2, retry"
        );
    }

    #[test]
    fn text_json_shape_keeps_quotes() {
        // 现状实现会吞掉两侧引号产出非法 JSON，这里必须保留引号
        assert_eq!(
            redact_text(
                r#"{"api_key":"sk-abcdef123456","n":1}"#,
                RedactPolicy::default()
            ),
            r#"{"api_key":"sk-a***3456","n":1}"#
        );
    }

    #[test]
    fn text_quoted_value_keeps_quotes() {
        assert_eq!(
            redact_text(
                r#"client_secret="hunter2hunter2";"#,
                RedactPolicy::default()
            ),
            r#"client_secret="hunt***ter2";"#
        );
    }

    #[test]
    fn text_bearer_is_fully_masked() {
        assert_eq!(
            redact_text(
                "Authorization: Bearer abc.def.ghi next",
                RedactPolicy::default()
            ),
            "Authorization: *** next"
        );
    }

    #[test]
    fn text_flag_form_redacts_value() {
        assert_eq!(
            redact_text("git push --token secret123", RedactPolicy::default()),
            "git push --token ***"
        );
    }

    #[test]
    fn text_does_not_mistake_prose_for_flag() {
        // 无 `-` 前缀的敏感子串不触发 flag 形态，避免误伤自然语言
        assert_eq!(
            redact_text(
                "my token is abc123 and secret stays",
                RedactPolicy::default()
            ),
            "my token is abc123 and secret stays"
        );
        assert_eq!(
            redact_text("tokenize the input", RedactPolicy::default()),
            "tokenize the input"
        );
    }

    #[test]
    fn text_flag_without_value_is_untouched() {
        assert_eq!(
            redact_text("run --token --verbose", RedactPolicy::default()),
            "run --token --verbose"
        );
    }

    #[test]
    fn text_multibyte_is_preserved() {
        assert_eq!(
            redact_text(
                "创建 Agent：api_key=sk-abcdef123456 完成",
                RedactPolicy::default()
            ),
            "创建 Agent：api_key=sk-a***3456 完成"
        );
    }

    #[test]
    fn text_returns_borrowed_when_clean() {
        // 无任何敏感词 → 零拷贝
        let policy = RedactPolicy::default();
        assert!(matches!(
            redact_text("nothing sensitive here", policy),
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            redact_text("api_key=sk-abcdef123456", policy),
            Cow::Owned(_)
        ));
    }

    #[test]
    fn text_respects_scan_free_text_switch() {
        let policy = RedactPolicy {
            scan_free_text: false,
            ..RedactPolicy::default()
        };
        assert!(matches!(
            redact_text("api_key=sk-abcdef123456", policy),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn text_respects_max_text_bytes() {
        let policy = RedactPolicy {
            max_text_bytes: 10,
            ..RedactPolicy::default()
        };
        assert!(matches!(
            redact_text("api_key=sk-abcdef123456", policy),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn warmup_is_idempotent() {
        warmup();
        warmup();
    }

    #[test]
    fn json_value_shape_redacts_bare_credential_under_generic_key() {
        // 键名是泛型 data/result，但值长得像 OpenAI key —— 值形态兜底必须命中
        let mut value = json!({
            "data": "sk-abcdef123456",
            "result": "ghp_xxxxxxxxxxxxxxxxxxxx"
        });
        redact_json(&mut value, RedactPolicy::default());
        assert_eq!(value["data"], "***");
        assert_eq!(value["result"], "***");
    }

    #[test]
    fn json_value_shape_does_not_touch_plain_values() {
        let mut value = json!({
            "note": "ask Ed about the skateboard",
            "repo": "github.com/owner/repo"
        });
        redact_json(&mut value, RedactPolicy::default());
        assert_eq!(value["note"], "ask Ed about the skateboard");
        assert_eq!(value["repo"], "github.com/owner/repo");
    }

    #[test]
    fn text_value_shape_redacts_bare_credential_token() {
        // 自由文本里孤立的 sk- 凭证（前导为空格，token 边界）必须被识别
        assert_eq!(
            redact_text("token is sk-abcdef123456 done", RedactPolicy::default()),
            "token is *** done"
        );
        // 不带 token 边界、嵌在正常词里的 sk- 不得误伤
        assert_eq!(
            redact_text("ask Ed about skateboard", RedactPolicy::default()),
            "ask Ed about skateboard"
        );
    }

    #[test]
    fn json_value_shape_respects_scan_free_text_switch() {
        let policy = RedactPolicy {
            scan_free_text: false,
            ..RedactPolicy::default()
        };
        let mut value = json!({ "data": "sk-abcdef123456" });
        redact_json(&mut value, policy);
        // 关闭自由文本扫描时，值形态识别也随之关闭（保持原文）
        assert_eq!(value["data"], "sk-abcdef123456");
    }
}
