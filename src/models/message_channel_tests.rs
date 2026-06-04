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
