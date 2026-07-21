use common::api::a2a::A2aMessagePart;

pub const A2A_TASK_ID_TAG_PREFIX: &str = "a2a_task_id:";
pub const A2A_SYNCED_MSG_COUNT_PREFIX: &str = "a2a_synced_msgs:";

pub fn extract_a2a_task_id(tags: &[String]) -> Option<String> {
    tags.iter()
        .find(|t| t.starts_with(A2A_TASK_ID_TAG_PREFIX))
        .map(|t| t[A2A_TASK_ID_TAG_PREFIX.len()..].to_string())
}

pub fn make_a2a_task_tag(remote_task_id: &str) -> String {
    format!("{}{}", A2A_TASK_ID_TAG_PREFIX, remote_task_id)
}

pub fn get_synced_msg_count(tags: &[String]) -> usize {
    tags.iter()
        .find(|t| t.starts_with(A2A_SYNCED_MSG_COUNT_PREFIX))
        .and_then(|t| t[A2A_SYNCED_MSG_COUNT_PREFIX.len()..].parse::<usize>().ok())
        .unwrap_or(0)
}

pub fn make_synced_msg_tag(count: usize) -> String {
    format!("{}{}", A2A_SYNCED_MSG_COUNT_PREFIX, count)
}

pub fn extract_text_from_parts(parts: &[A2aMessagePart]) -> String {
    let mut texts = Vec::new();
    for part in parts {
        if let A2aMessagePart::Text { text } = part {
            texts.push(text.clone());
        }
    }
    texts.join("\n")
}
