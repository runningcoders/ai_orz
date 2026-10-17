use super::McpServerConfig;

#[test]
fn redacted_for_management_redacts_url_userinfo_and_query() {
    let config = McpServerConfig {
        url: Some("https://user:password@api.example.com/mcp?token=secret#section".to_string()),
        ..McpServerConfig::default_streamable_http()
    };

    let redacted = config.redacted_for_management();

    assert_eq!(
        redacted.url.as_deref(),
        Some("https://[REDACTED]@api.example.com/mcp?[REDACTED]#section")
    );
    let redacted_url = redacted.url.as_deref().unwrap_or_default();
    assert!(!redacted_url.contains("password"));
    assert!(!redacted_url.contains("secret"));
}

// ==================== redact_url_for_logging（Task 3.3 顺带项 B 日志面脱敏） ====================

#[test]
fn redact_url_for_logging_masks_query_string_in_embedded_urls() {
    use super::redact_url_for_logging;

    let log_line = format!(
        "error sending request for url ({}): connection refused",
        "https://mcp.amap.com/mcp?key=SECRET-VALUE&other=1"
    );
    let redacted = redact_url_for_logging(&log_line);
    assert!(
        !redacted.contains("SECRET-VALUE"),
        "raw key must not leak: {redacted}"
    );
    assert!(
        redacted.contains("[REDACTED]"),
        "query must be masked: {redacted}"
    );
    assert!(
        redacted.contains("https://mcp.amap.com/mcp"),
        "origin kept: {redacted}"
    );

    let mixed = redact_url_for_logging(
        "failed for url (http://127.0.0.1:9/mcp?key=TOKEN123) and plain text",
    );
    assert!(
        !mixed.contains("TOKEN123"),
        "raw token must not leak: {mixed}"
    );
    assert!(
        mixed.contains("[REDACTED]"),
        "query must be masked: {mixed}"
    );

    let plain = redact_url_for_logging("connection refused: no url involved");
    assert_eq!(
        plain, "connection refused: no url involved",
        "plain text passes through"
    );
}

#[test]
fn redact_url_for_logging_handles_non_ascii_tokens_without_panicking() {
    use super::redact_url_for_logging;

    let error = "重试失败 目标连接中断 size=8字节以上片段仍不中断";
    assert_eq!(
        redact_url_for_logging(error),
        error,
        "non-ASCII tokens pass through without panicking"
    );

    let mixed = redact_url_for_logging("连接 https://mcp.amap.com/mcp?key=SECRET-VALUE 超时");
    assert!(
        !mixed.contains("SECRET-VALUE"),
        "raw key must not leak: {mixed}"
    );
    assert!(
        mixed.contains("[REDACTED]"),
        "query must be masked: {mixed}"
    );
    assert!(
        mixed.contains("https://mcp.amap.com/mcp"),
        "origin kept: {mixed}"
    );
    assert!(mixed.contains("连接"), "non-ASCII prefix kept: {mixed}");
}
