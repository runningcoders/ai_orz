//! agent_identity 单元测试（纯派生规则）
//!
//! 用 `#[path]` 而非内联 `mod tests`：保住 `use super::*` 可见父模块私有 helper，
//! 同时规避 inline-test-lint 对内联测试体量的限制。
//!
//! DB 派生与缓存已随「L1 结构层删除」一起移除（取值统一走凭据注入通道，
//! 由 `identity_dataset` 负责查库与缓存，测试在彼处覆盖）。

use super::*;

#[test]
fn derive_identity_builds_role_slug_and_email() {
    let identity = derive_identity(Some(r#"["tech_lead"]"#), "000000006aae6de3", 1)
        .expect("role 数组应派生出身份");
    assert_eq!(identity.name, "tech_lead");
    assert_eq!(identity.email, "tech_lead@agents.ai-orz.local");
}

#[test]
fn derive_identity_multi_agent_role_appends_short_agent_id() {
    // 同一 role 有第二人时，邮箱加 agent_id 前 8 位消歧（当前 role 唯一，分支预留）
    let identity = derive_identity(Some(r#"["backend_developer"]"#), "abcdef0123456789", 2)
        .expect("多 Agent 同 role 应仍可派生");
    assert_eq!(identity.name, "backend_developer");
    assert_eq!(
        identity.email,
        "backend_developer+abcdef01@agents.ai-orz.local"
    );
}

#[test]
fn derive_identity_tolerates_scalar_and_rejects_missing_role() {
    let scalar = derive_identity(Some("\"reception\""), "agent-1", 1).expect("裸字符串也应支持");
    assert_eq!(scalar.email, "reception@agents.ai-orz.local");

    // 无 role / 空数组 / 空串 / 非法 JSON → None（不凭空造值）
    assert!(derive_identity(None, "agent-1", 1).is_none());
    assert!(derive_identity(Some("[]"), "agent-1", 1).is_none());
    assert!(derive_identity(Some("  "), "agent-1", 1).is_none());
    assert!(derive_identity(Some("not-json"), "agent-1", 1).is_none());
}

#[test]
fn derive_identity_sanitizes_role_slug() {
    let spaced = derive_identity(Some(r#"["Tech Lead"]"#), "agent-1", 1).expect("应归一化");
    assert_eq!(spaced.name, "tech-lead", "空格折成 '-'，大写转小写");
    // 纯符号 role 归一后为空 → None
    assert!(derive_identity(Some(r#"["///"]"#), "agent-1", 1).is_none());
}
