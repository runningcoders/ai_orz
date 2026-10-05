//! tests 单元测试（拆分自 engine.rs）
//!
//! 文件瘦身：原 620 行 → 362 行，测试体 259 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

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
