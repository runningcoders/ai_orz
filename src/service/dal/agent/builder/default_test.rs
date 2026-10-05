//! DefaultPromptBuilder 单元测试（vision parts 升级 + token 裁剪口径）
//!
//! 拆分自 `default.rs` 尾部 tests 模块（文件瘦身），惯例照同目录
//! `prompt_builder_test.rs`：`mod.rs` 里 `#[cfg(test)] mod xxx_test;`。
//!
//! 覆盖批4「vision 携带机制」的核心口径：
//! - **仅当前消息升级**为 `UserMultimodal`，消息链/历史保持纯文本
//! - **sleep / summary / intent_analyze 场景零改动**，即使缓存了 vision parts 也不升级
//! - **base64 不进 `build()` 输出**（token 裁剪，trace 保持纯文本）
//! - 资源上下文块（`【资源上下文】`）渲染进当前消息

// 注意：原先内联 `mod tests` 里的 `use super::*` 能顺带引入 default.rs 的私有
// `use` 导入；拆成独立文件后 `use super::default::*` 只导入公开项，私有 use
// 不传递，因此 ChatMessage / ImagePart 必须在此显式导入。
use super::default::*;
use crate::models::cortex_types::{ChatMessage, ImagePart};
use crate::models::file::FileMeta;
use crate::models::message::Message;
use crate::models::prompt_builder::PromptBuilder;

// 构造消息 PO 走 `MessagePo::new`（本金库红线）：`Default` + 逐字段赋值会让
// created_at 归零，也会让测试 PO 与真实写入路径形态不一致。
fn make_text_message(content: &str) -> Message {
    let po = crate::models::message::MessagePo::new(
        "msg-test".to_string(),
        None, // project_id
        None, // task_id
        "user-test".to_string(),
        "agent-test".to_string(),
        common::enums::MessageRole::User,
        common::enums::MessageRole::Agent,
        common::enums::MessageType::Text,
        content.to_string(),
        None, // file_type
        FileMeta::new("".to_string(), "".to_string(), 0),
        None, // reply_to_id
        None, // root_id
        None, // organization_id
        "test-user".to_string(),
    );
    Message::from_po(po)
}

fn make_image(mime: &str) -> ImagePart {
    ImagePart {
        mime_type: mime.to_string(),
        data_base64: "aGVsbG8=".to_string(),
    }
}

#[test]
fn initial_messages_stay_plain_text_without_vision_parts() {
    let mut b = DefaultPromptBuilder::new();
    b.current_message(&make_text_message("hi"));
    let msgs = b.build_initial_messages();
    assert_eq!(msgs.len(), 2);
    assert!(matches!(msgs[1], ChatMessage::User { .. }));
}

#[test]
fn initial_messages_upgrade_to_user_multimodal_with_vision_parts() {
    let mut b = DefaultPromptBuilder::new();
    b.current_message(&make_text_message("看图"));
    b.set_current_message_vision(vec![make_image("image/png")]);
    let msgs = b.build_initial_messages();
    assert_eq!(msgs.len(), 2);
    match &msgs[1] {
        ChatMessage::UserMultimodal { text, images } => {
            assert!(text.contains("看图"));
            assert_eq!(images.len(), 1);
            assert_eq!(images[0].mime_type, "image/png");
        }
        other => panic!("expected UserMultimodal, got {other:?}"),
    }
}

#[test]
fn build_prompt_stays_text_only_even_with_vision_parts() {
    // token 裁剪口径：base64 不进 build() 输出（trace raw_input/RoundDigest 纯文本）
    let mut b = DefaultPromptBuilder::new();
    b.current_message(&make_text_message("hi"));
    b.set_current_message_vision(vec![make_image("image/png")]);
    let prompt = b.build();
    assert!(!prompt.contains("aGVsbG8="));
    assert!(prompt.contains("hi"));
}

#[test]
fn resource_context_block_rendered_into_current_message() {
    let mut b = DefaultPromptBuilder::new();
    b.current_message(&make_text_message("引用了产物"));
    assert!(!b.build().contains("【资源上下文】"));

    let mut b2 = DefaultPromptBuilder::new();
    b2.set_resource_context_lines(vec![
        "附件「截图.png」 · image/png · 125952 字节".to_string(),
        "产物「调研报告」 · text/plain · 核心结论…".to_string(),
    ]);
    b2.current_message(&make_text_message("引用了产物"));
    let prompt = b2.build();
    assert!(prompt.contains("【资源上下文】"));
    assert!(prompt.contains("截图.png"));
    assert!(prompt.contains("调研报告"));
}

#[test]
fn thread_and_history_stay_text_with_vision_parts() {
    // token 裁剪口径：消息链/历史为纯文本，仅当前消息携带图像 part
    let mut b = DefaultPromptBuilder::new();
    b.set_current_message_vision(vec![make_image("image/png")]);
    b.message_thread(&["链头消息".to_string(), "链内回复".to_string()]);
    b.current_message(&make_text_message("hi"));
    let msgs = b.build_initial_messages();
    assert_eq!(msgs.len(), 2);
    assert!(matches!(msgs[0], ChatMessage::System { .. }));
    match &msgs[1] {
        ChatMessage::UserMultimodal { text, images } => {
            assert_eq!(images.len(), 1);
            assert!(text.contains("链头消息"));
            assert!(text.contains("链内回复"));
        }
        _ => panic!("expected UserMultimodal"),
    }
}

#[test]
fn sleep_scene_stays_plain_text_despite_vision_parts() {
    // sleep/summary/intent_analyze 场景零改动：即使缓存了 vision parts 也不升级
    let mut b = DefaultPromptBuilder::new();
    b.set_current_message_vision(vec![make_image("image/png")]);
    let msgs = b.build_sleep_initial_messages("待沉淀摘要", &["t1".to_string()]);
    // DefaultPromptBuilder 覆写 sleep 场景为 [System, User] 双消息——
    // 真口径 = 全部消息零 UserMultimodal（场景零改动，vision parts 不升级）
    assert!(
        msgs.iter()
            .all(|m| !matches!(m, ChatMessage::UserMultimodal { .. }))
    );
}
