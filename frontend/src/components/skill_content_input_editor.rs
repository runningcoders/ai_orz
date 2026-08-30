//! 技能内容输入编辑器（3 Tab：文本 / URL / 附件）
//!
//! 前后端共享 SkillContentInput DTO，前端通过 3 个 Tab 切换内容源。
//! 与后端同源，规则变动需同步。

use crate::components::state::Loading;
use dioxus::prelude::*;

use crate::api::finance::upload_attachment;
use crate::store::toast::use_toast;
use common::api::{SkillContentInput, SkillFileInput};

/// 附件条目（前端临时状态：上传后保留 attachment_id + target_path）
#[derive(Clone, PartialEq)]
struct AttachmentEntry {
    attachment_id: String,
    filename: String,
    target_path: String,
    size: usize,
}

#[derive(Props, Clone, PartialEq)]
pub struct SkillContentInputEditorProps {
    /// 当前内容输入（用于回显，更新场景传入已有值）
    value: Option<SkillContentInput>,
    /// 内容变更回调
    on_change: EventHandler<Option<SkillContentInput>>,
}

#[component]
pub fn SkillContentInputEditor(props: SkillContentInputEditorProps) -> Element {
    let toast = use_toast();
    let mut active_tab = use_signal(|| 0u8); // 0=文本, 1=URL, 2=附件
    let mut content = use_signal(String::new);
    let mut url = use_signal(String::new);
    let mut attachments = use_signal(Vec::<AttachmentEntry>::new);
    let mut uploading = use_signal(|| false);

    // 从 props 初始化（更新场景回显已有值）
    use_effect(move || {
        if let Some(ci) = &props.value {
            if let Some(c) = &ci.content {
                content.set(c.clone());
            }
            if let Some(u) = &ci.url {
                url.set(u.clone());
                active_tab.set(1);
            }
        }
    });

    // 组装 SkillContentInput 并回调
    let emit_change = move || {
        let ci = match active_tab() {
            0 if !content().is_empty() => Some(SkillContentInput {
                content: Some(content()),
                url: None,
                files: None,
            }),
            1 if !url().is_empty() => Some(SkillContentInput {
                content: None,
                url: Some(url()),
                files: None,
            }),
            2 if !attachments().is_empty() => Some(SkillContentInput {
                content: None,
                url: None,
                files: Some(
                    attachments()
                        .iter()
                        .map(|a| SkillFileInput {
                            attachment_id: a.attachment_id.clone(),
                            target_path: a.target_path.clone(),
                        })
                        .collect(),
                ),
            }),
            _ => None,
        };
        props.on_change.call(ci);
    };

    rsx! {
        div { class: "w-full",
            // Tab 切换
            div { role: "tablist", class: "flex flex-wrap gap-2 mb-4",
                button {
                    role: "tab",
                    class: if active_tab() == 0 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" },
                    onclick: move |_| { active_tab.set(0); emit_change(); },
                    "📝 文本"
                }
                button {
                    role: "tab",
                    class: if active_tab() == 1 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" },
                    onclick: move |_| { active_tab.set(1); emit_change(); },
                    "🔗 URL"
                }
                button {
                    role: "tab",
                    class: if active_tab() == 2 { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" },
                    onclick: move |_| { active_tab.set(2); emit_change(); },
                    "📎 附件"
                }
            }

            // Tab 内容
            match active_tab() {
                0 => rsx! {
                    div { class: "form-control w-full",
                        textarea {
                            class: "textarea textarea-bordered w-full h-48 font-mono",
                            value: "{content}",
                            placeholder: "技能的 Markdown 内容，将写入 skill.md",
                            oninput: move |e| {
                                content.set(e.value());
                                emit_change();
                            }
                        }
                    }
                },
                1 => rsx! {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text text-sm text-base-content/60",
                                "仅支持 HTTPS 协议"
                            }
                        }
                        input {
                            class: "input input-bordered w-full",
                            r#type: "url",
                            value: "{url}",
                            placeholder: "https://example.com/skill.md",
                            oninput: move |e| {
                                url.set(e.value());
                                emit_change();
                            }
                        }
                    }
                },
                2 => rsx! {
                    div { class: "space-y-3",
                        // 已添加附件列表
                        if !attachments().is_empty() {
                            div { class: "space-y-2",
                                for (i, att) in attachments().iter().enumerate() {
                                    div { key: "{i}", class: "flex items-center gap-2 p-2 bg-base-200 rounded-lg",
                                        span { class: "text-sm font-mono truncate flex-1", "{att.filename}" }
                                        span { class: "text-xs text-base-content/50",
                                            "{crate::utils::format_file_size(att.size as u64)}"
                                        }
                                        input {
                                            class: "input input-bordered input-sm w-40",
                                            value: "{att.target_path}",
                                            placeholder: "目标路径",
                                            oninput: move |e| {
                                                let v = e.value();
                                                attachments.with_mut(|list| {
                                                    if i < list.len() {
                                                        list[i].target_path = v;
                                                    }
                                                });
                                                emit_change();
                                            }
                                        }
                                        button {
                                            class: "btn hud-btn btn-ghost btn-xs",
                                            onclick: move |_| {
                                                attachments.with_mut(|list| {
                                                    list.remove(i);
                                                });
                                                emit_change();
                                            },
                                            "✕"
                                        }
                                    }
                                }
                            }
                        }

                        // 文件上传
                        if uploading() {
                            div { class: "flex items-center gap-2",
                                Loading { size: "sm" }
                                span { class: "text-sm text-base-content/60", "上传中..." }
                            }
                        } else {
                            input {
                                class: "file-input file-input-bordered file-input-sm w-full",
                                r#type: "file",
                                multiple: true,
                                onchange: move |e| {
                                    let files = e.files();
                                    if files.is_empty() {
                                        return;
                                    }
                                    uploading.set(true);
                                    spawn(async move {
                                        for fd in files {
                                            let filename = fd.name().to_string();
                                            let size = fd.size() as usize;

                                            // 读取文件 bytes
                                            let bytes = match fd.read_bytes().await {
                                                Ok(b) => b,
                                                Err(err) => {
                                                    toast.error(format!("读取文件 {} 失败: {:?}", filename, err));
                                                    uploading.set(false);
                                                    return;
                                                }
                                            };

                                            // 构造 Blob + FormData（与 chat.rs 同模式）
                                            let blob_parts = js_sys::Array::new();
                                            let uint8 = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
                                            uint8.copy_from(&bytes);
                                            blob_parts.push(&uint8);
                                            let bag = web_sys::BlobPropertyBag::new();
                                            bag.set_type("application/octet-stream");
                                            let blob = web_sys::Blob::new_with_str_sequence_and_options(&blob_parts, &bag).ok();

                                            let form = match web_sys::FormData::new() {
                                                Ok(f) => f,
                                                Err(_) => {
                                                    toast.error("无法初始化上传表单");
                                                    uploading.set(false);
                                                    return;
                                                }
                                            };
                                            let _ = form.append_with_str("purpose", "skill_import");
                                            if let Some(blob) = blob {
                                                let _ = form.append_with_blob_and_filename("file", &blob, &filename);
                                            }

                                            match upload_attachment(form).await {
                                                Ok(detail) => {
                                                    let target_path = filename.clone();
                                                    attachments.with_mut(|list| {
                                                        list.push(AttachmentEntry {
                                                            attachment_id: detail.id.clone(),
                                                            filename,
                                                            target_path,
                                                            size,
                                                        });
                                                    });
                                                }
                                                Err(e) => {
                                                    toast.error(format!("上传 {} 失败: {}", filename, e));
                                                }
                                            }
                                        }
                                        uploading.set(false);
                                        emit_change();
                                    });
                                }
                            }
                        }
                        p { class: "text-xs text-base-content/50",
                            "上传的文件将作为附件导入技能目录。每个文件可指定目标路径（相对路径）。"
                        }
                    }
                },
                _ => rsx! {},
            }
        }
    }
}
