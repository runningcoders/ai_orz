use super::*;
use common::enums::{ChannelStatus, ChannelType};
use sqlx::types::Json;

fn test_channel_with_status(status: ChannelStatus) -> MessageChannel {
    let po = MessageChannelPoBuilder::default()
        .id("channel_001".to_string())
        .org_id("org_001".to_string())
        .user_id("user_001".to_string())
        .agent_id(Some("agent_001".to_string()))
        .channel_type(ChannelType::Lark)
        .channel_name("飞书通知".to_string())
        .webhook_url(Some("https://example.com/webhook".to_string()))
        .access_token(None)
        .secret(None)
        .config_json(Json(ChannelConfig::default()))
        .status(status)
        .created_by("creator".to_string())
        .modified_by("creator".to_string())
        .created_at(1000)
        .updated_at(1000)
        .build()
        .unwrap();

    MessageChannel::from_po(po)
}

#[test]
fn active_channel_can_transition_to_disabled() {
    let mut channel = test_channel_with_status(ChannelStatus::Active);

    channel
        .transition_status(ChannelStatus::Disabled, "operator")
        .unwrap();

    assert_eq!(channel.status(), ChannelStatus::Disabled);
    assert_eq!(channel.po.modified_by, "operator");
    assert!(channel.po.updated_at >= 1000);
}

#[test]
fn disabled_channel_can_transition_to_active() {
    let mut channel = test_channel_with_status(ChannelStatus::Disabled);

    channel
        .transition_status(ChannelStatus::Active, "operator")
        .unwrap();

    assert_eq!(channel.status(), ChannelStatus::Active);
    assert_eq!(channel.po.modified_by, "operator");
    assert!(channel.po.updated_at >= 1000);
}

#[test]
fn status_transition_action_cannot_mark_channel_deleted() {
    let mut channel = test_channel_with_status(ChannelStatus::Active);

    let result = channel.transition_status(ChannelStatus::Deleted, "operator");

    assert!(result.is_err());
    assert_eq!(channel.status(), ChannelStatus::Active);
    assert_eq!(channel.po.modified_by, "creator");
    assert_eq!(channel.po.updated_at, 1000);
}

#[test]
fn deleted_channel_is_terminal_for_status_transition_action() {
    let mut channel = test_channel_with_status(ChannelStatus::Deleted);

    assert!(channel.available_statuses().is_empty());
    assert!(!channel.can_transition_to(ChannelStatus::Active));

    let result = channel.transition_status(ChannelStatus::Active, "operator");

    assert!(result.is_err());
    assert_eq!(channel.status(), ChannelStatus::Deleted);
    assert_eq!(channel.po.modified_by, "creator");
    assert_eq!(channel.po.updated_at, 1000);
}

#[test]
fn receipt_config_defaults_to_judged_and_recommended_emoji_sets() {
    let config = ChannelConfig::default();

    assert_eq!(config.receipt_mode(), ReceiptMode::Judged);
    assert_eq!(
        config.processing_receipt_emojis(),
        vec!["OnIt", "OneSecond", "Typing", "SALUTE"]
    );
    assert_eq!(
        config.skipped_receipt_emojis(),
        vec!["Get", "StatusReading", "GLANCE"]
    );
    assert_eq!(
        config.undetermined_receipt_emojis(),
        vec!["THINKING", "DIZZY", "WHAT"]
    );
    assert!(config.wechat_typing_enabled());
}

#[test]
fn receipt_mode_parses_known_values_and_falls_back_to_judged() {
    assert_eq!(ReceiptMode::from_config(Some("off")), ReceiptMode::Off);
    assert_eq!(ReceiptMode::from_config(Some("none")), ReceiptMode::Off);
    // 历史值 addressed / all / always 一律归一到 judged
    assert_eq!(
        ReceiptMode::from_config(Some("ADDRESSED")),
        ReceiptMode::Judged
    );
    assert_eq!(ReceiptMode::from_config(Some("all")), ReceiptMode::Judged);
    assert_eq!(
        ReceiptMode::from_config(Some("judged")),
        ReceiptMode::Judged
    );
    assert_eq!(
        ReceiptMode::from_config(Some("unknown")),
        ReceiptMode::Judged
    );
    assert_eq!(ReceiptMode::from_config(None), ReceiptMode::Judged);
}

#[test]
fn explicit_empty_receipt_emojis_disable_that_rotation() {
    let config = ChannelConfig {
        receipt_processing_emojis: Some(vec![]),
        receipt_skipped_emojis: Some(vec!["  ".to_string()]),
        receipt_undetermined_emojis: Some(vec![String::new()]),
        ..Default::default()
    };

    assert!(config.processing_receipt_emojis().is_empty());
    assert!(config.skipped_receipt_emojis().is_empty());
    assert!(config.undetermined_receipt_emojis().is_empty());
}
