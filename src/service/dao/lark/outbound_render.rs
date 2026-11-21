//! 飞书出站内容统一渲染：markdown 源文 → (msg_type, content)
//!
//! 按飞书官方《发送消息内容结构》（2026-07 更新），富文本 `post` 的 `md` 标签
//! 原生承载 CommonMark 0.31 + GFM 原文——标题 / 列表 / 代码块 / 引用 / 表格 /
//! 任务列表 / 删除线 / 自动链接均由飞书客户端渲染，无需 IR→节点映射或卡片。
//!
//! 两条协议约定：
//! - 服务端 API（send/reply）的 `content` **不包 `post` 外层**，类型由 `msg_type`
//!   表达；`{"post": ...}` 只属于自定义机器人 webhook；
//! - `md` 标签独占一个段落，不能与其他标签同行。
//!
//! 兜底：空内容 / 超 [`POST_CONTENT_LIMIT`] → `text`（富文本共用体量上限）。
//! 协议引用（`attachment:` 等）在上游 `degrade_content_for_external` 已降级为
//! `[附件：名]` 纯文本；`md` 标签里普通 http(s) 链接由飞书原生渲染。

use serde_json::json;

/// 飞书富文本 content 体量上限（30KB 留余量给 JSON 结构与转义膨胀）
pub const POST_CONTENT_LIMIT: usize = 28 * 1024;

/// Markdown 源文 → (msg_type, 序列化后的 content 字符串)
pub fn render_outbound(content: &str) -> (&'static str, String) {
    if content.trim().is_empty() || content.len() > POST_CONTENT_LIMIT {
        return ("text", json!({ "text": content }).to_string());
    }
    // md 标签独占一段；服务端 API 的 content 顶层直接是 zh_cn（不包 post）
    let rendered = json!({
        "zh_cn": {
            "content": [[{ "tag": "md", "text": content }]]
        }
    })
    .to_string();
    ("post", rendered)
}

#[cfg(test)]
#[path = "outbound_render_tests.rs"]
mod tests;
