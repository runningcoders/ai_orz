//! 技能内容输入编辑器（3 Tab：文本 / URL / 附件）
//!
//! 前后端共享 SkillContentInput DTO，前端通过 3 个 Tab 切换内容源。
//! 与后端同源，规则变动需同步。

use crate::components::state::Loading;
use dioxus::prelude::*;

use crate::api::finance::upload_attachment;
use crate::store::toast::use_toast;
use common::api::{SkillContentInput, SkillFileInput, default_target_path};

/// 附件条目（前端临时状态：上传后保留 attachment_id + target_path）
#[derive(Clone, PartialEq)]
struct AttachmentEntry {
    attachment_id: String,
    filename: String,
    target_path: String,
    size: usize,
}

impl AttachmentEntry {
    /// 目标路径输入框的提示语。
    ///
    /// zip 是**技能包**：对它 `target_path` 不表示「落盘到哪」，而是「包内哪个条目是
    /// 主文件」（后端见 `resolve_declared_main`），所以提示语不能写成通用目标路径。
    fn target_hint(&self) -> &'static str {
        if is_skill_package(&self.filename) {
            "包内主文件（可选）"
        } else {
            "目标路径"
        }
    }
}

/// 是否是技能包（zip）。
///
/// 后端按 zip **magic bytes** 判定，前端只能看扩展名——这里只用于「预填值 / 提示语」
/// 这类展示决策，不参与任何校验（真伪由后端解包时定）。
fn is_skill_package(filename: &str) -> bool {
    std::path::Path::new(filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("zip"))
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
                            attachment_id: Some(a.attachment_id.clone()),
                            // UI 目前只走「上传附件」一种来源；artifact_id 供任务产出
                            // 的技能包由 Agent 工具调用直接传入，不由本编辑器产生。
                            artifact_id: None,
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
                                            placeholder: att.target_hint(),
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
                                                    // 预填默认归位路径（与后端导入推断同源 SSOT）：
                                                    // 图片→assets/、脚本→scripts/、md→skill.md。
                                                    //
                                                    // zip 技能包例外：`target_path` 对它表示
                                                    // 「包内哪个条目是主文件」，没有默认落盘名可言。
                                                    // `default_target_path` 对未知扩展名会原样回显
                                                    // 文件名（即包名），而包名不是包内条目，预填进去
                                                    // 等于给后端一个假指令，故留空。
                                                    let target_path = if is_skill_package(&filename) {
                                                        String::new()
                                                    } else {
                                                        default_target_path(&filename)
                                                    };
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
                            "上传的文件将作为附件导入技能目录。每个文件可指定目标路径（相对路径）；上传 zip 技能包时该字段改为指定「包内主文件」（如 main.md，留空则自动识别 skill.md）。"
                        }
                    }
                },
                _ => rsx! {},
            }
        }
    }
}
