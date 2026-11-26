//! tests 单元测试（拆分自 scheduler.rs）
//!
//! 文件瘦身：原 1116 行 → 902 行，测试体 215 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

fn schedule_event(payload: &str) -> CronTriggerEvent {
    CronTriggerEvent {
        event_id: "e-1".into(),
        trigger_id: "t-1".into(),
        trigger_name: "晨间简报".into(),
        payload: payload.into(),
        created_at: 0,
        trigger_type: 3,
        behavior: 2, // AsyncAgentWakeup
        action: Some("daily_brief".into()),
        description: "汇总昨日进度".into(),
        org_id: None,
        owner_user_id: Some("user-1".into()),
        creator_type: Some(1), // CreatorType::Agent
        creator_id: Some("agent-1".into()),
        agent_id: Some("agent-2".into()),
        lease_owner: None,
    }
}

/// 唤醒正文必须带齐身份三字段：Agent 到点执行时要知道「谁安排的、
/// 算谁的、自己是谁」，否则「谁安排的」只落库不进执行上下文。
#[test]
fn wakeup_content_carries_identity() {
    let event = CronTriggerEvent {
        event_id: "e-1".into(),
        trigger_id: "t-1".into(),
        trigger_name: "晨间简报".into(),
        payload: "{}".into(),
        created_at: 0,
        trigger_type: 3,
        behavior: 2,
        action: Some("daily_brief".into()),
        description: "汇总昨日进度".into(),
        org_id: None,
        owner_user_id: Some("user-1".into()),
        creator_type: Some(1), // CreatorType::Agent
        creator_id: Some("agent-1".into()),
        agent_id: Some("agent-2".into()),
        lease_owner: None,
    };
    let content = build_schedule_wakeup_content(&event, "daily_brief");
    assert!(content.contains("安排者：Agent/agent-1"), "{content}");
    assert!(content.contains("归属用户：user-1"), "{content}");
    assert!(content.contains("负责 Agent：agent-2"), "{content}");
    assert!(content.contains("用户原始意图：汇总昨日进度"), "{content}");
}

/// 旧事件没有 creator 字段：显示「系统」，其余缺省显示 `-`，不能 panic。
#[test]
fn wakeup_content_handles_missing_identity() {
    let event = CronTriggerEvent {
        event_id: "e-2".into(),
        trigger_id: "t-2".into(),
        trigger_name: "旧日程".into(),
        payload: "{}".into(),
        created_at: 0,
        trigger_type: 0,
        behavior: 0,
        action: None,
        description: String::new(),
        org_id: None,
        owner_user_id: None,
        creator_type: None,
        creator_id: None,
        agent_id: None,
        lease_owner: None,
    };
    let content = build_schedule_wakeup_content(&event, "");
    assert!(content.contains("安排者：系统"), "{content}");
    assert!(content.contains("归属用户：-"), "{content}");
    assert!(content.contains("负责 Agent：-"), "{content}");
}

/// 顶层 action 优先；存量记录（顶层为 None/空）回落 payload.action
#[test]
fn test_resolve_action_prefers_top_level() {
    assert_eq!(
        resolve_action(Some("agent_schedule"), "agent_rest"),
        "agent_schedule"
    );
    assert_eq!(resolve_action(None, "agent_rest"), "agent_rest");
    assert_eq!(resolve_action(Some(""), "agent_rest"), "agent_rest");
}

/// 系统默认触发器的 payload **没有 agent_id**——历史版本因 AgentRestPayload
/// 必填 agent_id 导致 agent_rest 每次触发都在解析阶段失败（沉淀从未执行）。
/// 锁定：缺省 agent_id 可解析为 None（全局沉淀语义）。
#[test]
fn test_agent_rest_payload_without_agent_id_parses() {
    let payload: AgentRestPayload =
        serde_json::from_str(r#"{"settle_limit":10}"#).expect("系统默认 payload 应可解析");
    assert_eq!(payload.agent_id, None);
    assert_eq!(payload.settle_limit, Some(10));

    // 兼容指定单个 Agent 的旧语义
    let payload: AgentRestPayload =
        serde_json::from_str(r#"{"agent_id":"agent-001","settle_limit":5}"#)
            .expect("指定 agent_id 的 payload 应可解析");
    assert_eq!(payload.agent_id.as_deref(), Some("agent-001"));
    assert_eq!(payload.settle_limit, Some(5));
}

/// 外框解析：`{"action":..,"extra":{..}}` 里的 `extra` 必须原样透传给动作层。
///
/// 回归 `#[serde(flatten)] extra: Value` 那个坑：flatten 会把 `extra` 键名本身
/// 也收进 Value（`{"extra":{"settle_limit":10}}`），参数静默丢失、且不报错。
#[test]
fn test_cron_trigger_payload_passes_extra_through() {
    // 与真实存量 payload（`.ai_orz/ai_orz.db::cron_triggers`）逐字一致
    let payload: CronTriggerPayload =
        serde_json::from_str(r#"{"action":"agent_rest","extra":{"settle_limit":10}}"#)
            .expect("系统默认 agent_rest payload 应可解析");
    assert_eq!(payload.action, "agent_rest");
    assert_eq!(
        payload.action_params(),
        serde_json::json!({"settle_limit": 10})
    );

    // 透传到 AgentRestPayload 后参数仍在（旧实现这里 settle_limit 会丢成 None）
    let rest: AgentRestPayload =
        serde_json::from_value(payload.action_params()).expect("透传后应可解析");
    assert_eq!(rest.settle_limit, Some(10));

    // 指定单个 Agent 的写法同样必须透传（否则退化成全局扫描）
    let payload: CronTriggerPayload = serde_json::from_str(
        r#"{"action":"agent_rest","extra":{"agent_id":"agent-001","settle_limit":5}}"#,
    )
    .expect("指定 agent_id 的 payload 应可解析");
    let rest: AgentRestPayload =
        serde_json::from_value(payload.action_params()).expect("透传后应可解析");
    assert_eq!(rest.agent_id.as_deref(), Some("agent-001"));
    assert_eq!(rest.settle_limit, Some(5));
}

/// 缺省 / `null` 的 `extra` 归一为空对象，动作层解析不应因 `null` 报错
/// （避免又回到「解析失败 → 只打日志 → 沉淀静默不执行」的老路）。
#[test]
fn test_cron_trigger_payload_tolerates_missing_extra() {
    for raw in [
        r#"{"action":"tool_log_cleanup"}"#,
        r#"{"action":"tool_log_cleanup","extra":null}"#,
        r#"{"action":"project_followup","extra":{}}"#,
    ] {
        let payload: CronTriggerPayload =
            serde_json::from_str(raw).unwrap_or_else(|e| panic!("{} 应可解析: {}", raw, e));
        assert_eq!(
            payload.action_params(),
            serde_json::json!({}),
            "raw={}",
            raw
        );
    }

    // 无 extra 的 agent_rest 必须仍能解析成「全局沉淀」语义
    let payload: CronTriggerPayload =
        serde_json::from_str(r#"{"action":"agent_rest"}"#).expect("应可解析");
    let rest: AgentRestPayload =
        serde_json::from_value(payload.action_params()).expect("空参数应解析成全局沉淀而不是报错");
    assert_eq!(rest.agent_id, None);
    assert_eq!(rest.settle_limit, None);
}

/// 未知字段不能影响外框解析（用户可能自行加注释字段）
#[test]
fn test_cron_trigger_payload_ignores_unknown_fields() {
    let payload: CronTriggerPayload =
        serde_json::from_str(r#"{"action":"agent_rest","note":"手写","extra":{}}"#)
            .expect("应可解析");
    assert_eq!(payload.action, "agent_rest");
    assert_eq!(payload.action_params(), serde_json::json!({}));
}

/// 外框缺 action 时仍可解析（兼容历史外框）；日程本身不走这条解析。
#[test]
fn test_payload_envelope_tolerates_missing_action() {
    let payload: CronTriggerPayload = serde_json::from_str(r#"{"note":"无 action 的历史外框"}"#)
        .expect("缺 action 的外框应可解析");
    assert_eq!(payload.action, "");
    assert_eq!(payload.action_params(), serde_json::json!({}));
}

/// Agent 留在日程上的自由信息必须**原样**进唤醒正文：
/// 框架无协议、消费端不 parse / 不再序列化，键序、空格、换行都保持写入时的样子。
#[test]
fn test_wakeup_content_carries_custom_payload_verbatim() {
    let payload = r#"{"project_ids":["p-1", "p-2"],"note":"别忘了带上周报"}"#;
    let content = build_schedule_wakeup_content(&schedule_event(payload), "daily_brief");
    assert!(
        content.contains("日程信息 / 备注（自由 JSON，Agent 自定义，原样回灌）"),
        "{content}"
    );
    assert!(
        content.contains(payload),
        "payload 应逐字出现在正文里: {content}"
    );

    let notify = build_schedule_notify_content(&schedule_event(payload), "daily_brief");
    assert!(notify.contains(payload), "{notify}");
}

/// 缺省 / null / 空串不附段，保持正文简洁。
#[test]
fn test_wakeup_content_skips_empty_payload() {
    for payload in ["{}", "null", "  "] {
        let content = build_schedule_wakeup_content(&schedule_event(payload), "daily_brief");
        assert!(
            !content.contains("日程信息 / 备注"),
            "payload={payload}, content={content}"
        );
    }
}
