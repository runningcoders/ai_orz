//! 附件详情页 - 展示元信息 + 内容查看/编辑（仅文本类型）

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::finance::{get_attachment_content, update_attachment_content};
use crate::components::code_editor::CodeEditor;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::UpdateAttachmentContentRequest;

#[component]
pub fn FinanceAttachmentDetail(id: String) -> Element {
    // 方案 B：订阅路由并把 id 同步到响应式 rid，use_resource 绑定 rid，
    // 拉取仅在 id 变化时触发
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::FinanceAttachmentDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let toast = use_toast();

    let attachment_res = use_resource(move || {
        let id = rid();
        async move { get_attachment_content(&id).await }
    });
    let mut content = use_signal(String::new);
    let mut content_dirty = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let mut is_text_type = use_signal(|| false);

    // 同步：resource 完成时填充内容与元数据；用户已编辑则保留编辑内容
    // （peek 读取 content_dirty，避免被自身写回再次触发 effect）
    // 非文本（二进制）附件后端返回错误，Err 分支静默忽略 → is_text_type 保持 false
    use_effect(move || {
        if let Some(resp) = attachment_res.read().as_ref().and_then(|r| r.as_ref().ok()) {
            if !*content_dirty.peek() {
                content.set(resp.text.content.clone());
            }
            is_text_type.set(true);
            content_dirty.set(false);
        }
    });

    let on_save = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            saving.set(true);
            spawn(async move {
                match update_attachment_content(UpdateAttachmentContentRequest {
                    id,
                    content: content(),
                    expected_updated_at: None,
                })
                .await
                {
                    Ok(_) => {
                        toast.success("内容已保存");
                        content_dirty.set(false);
                    }
                    Err(e) => toast.error(format!("保存失败: {}", e)),
                }
                saving.set(false);
            });
        }
    };

    let attachment_data = attachment_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|resp| resp.attachment.clone());

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "附件详情" }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceAttachments {}, "← 返回列表" }
            }
            if attachment_res.read().as_ref().is_none() {
                Loading {}
            } else if let Some(a) = attachment_data {
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        h2 { class: "card-title", "{a.original_name}" }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4 mt-4",
                            div { div { class: "text-sm text-base-content/60", "存储名" }, div { class: "font-mono text-sm", "{a.stored_name}" } }
                            div { div { class: "text-sm text-base-content/60", "大小" }, div { class: "font-mono", "{crate::utils::format_file_size(a.size)}" } }
                            div { div { class: "text-sm text-base-content/60", "MIME 类型" }, div { class: "font-mono", "{a.mime_type}" } }
                            div { div { class: "text-sm text-base-content/60", "用途" }, span { class: "badge badge-info", "{a.purpose}" } }
                            div { div { class: "text-sm text-base-content/60", "创建时间" }, div { class: "font-mono", "{crate::utils::format_datetime(a.created_at)}" } }
                        }
                    }
                }
                if is_text_type() {
                    div { class: "card bg-base-100 shadow-md",
                        div { class: "card-body",
                            div { class: "flex justify-between items-center mb-4",
                                h2 { class: "card-title text-lg", "📄 内容" }
                                div { class: "flex gap-2",
                                    if content_dirty() { span { class: "text-xs text-warning", "● 未保存" } }
                                    button {
                                        class: "btn btn-primary btn-sm",
                                        disabled: saving() || !content_dirty(),
                                        onclick: on_save,
                                        if saving() { "保存中..." } else { "💾 保存" }
                                    }
                                }
                            }
                            CodeEditor {
                                value: content(),
                                on_input: move |v| { content.set(v); content_dirty.set(true); },
                                language: "text".to_string(),
                                min_lines: 20,
                            }
                        }
                    }
                } else {
                    EmptyState { icon: "📦".to_string(), message: "此附件为二进制文件，不支持在线查看内容".to_string() }
                }
            } else {
                EmptyState { icon: "📦".to_string(), message: "该附件无法在线预览（可能为二进制或非文本内容）".to_string() }
            }
        }
    }
}
