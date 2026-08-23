//! Skill 详情页 - 展示元信息 + 文件列表 + 文件内容查看/编辑 + 元信息编辑

use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::{
    get_skill, get_skill_file_content, list_skill_files, update_skill, update_skill_file_content,
};
use crate::components::code_editor::CodeEditor;
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::skill_content_input_editor::SkillContentInputEditor;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    GetSkillFileContentRequest, SkillContentInput, SkillDetail, SkillFileItem,
    UpdateSkillFileContentRequest, UpdateSkillRequest,
};

#[component]
pub fn HrSkillDetail(id: String) -> Element {
    let toast = use_toast();

    let mut skill = use_signal(|| Option::<SkillDetail>::None);
    let mut files = use_signal(Vec::<SkillFileItem>::new);
    let mut loading = use_signal(|| true);
    let mut selected_file = use_signal(String::new);
    let mut file_content = use_signal(String::new);
    let mut file_content_loading = use_signal(|| false);
    let mut file_content_dirty = use_signal(|| false);
    let mut saving_file = use_signal(|| false);
    // Markdown 文件预览/源码切换（true=渲染预览，false=源码编辑）
    let mut preview_mode = use_signal(|| false);

    // 元信息编辑 Modal
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_name = use_signal(String::new);
    let mut edit_description = use_signal(String::new);
    let mut edit_tags = use_signal(String::new);
    let mut edit_category = use_signal(String::new);
    let mut edit_content_input = use_signal(|| Option::<SkillContentInput>::None);
    let mut saving_meta = use_signal(|| false);

    // 初始加载
    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_skill(&id).await {
                Ok(s) => skill.set(Some(s)),
                Err(e) => toast.error(format!("加载 Skill 失败: {}", e)),
            }
            match list_skill_files(&id).await {
                Ok(resp) => files.set(resp.files),
                Err(e) => toast.error(format!("加载文件列表失败: {}", e)),
            }
            loading.set(false);
        });
    });

    // 保存文件内容
    let on_save_file = {
        let id = id.clone();
        move |_| {
            let skill_id = id.clone();
            let filename = selected_file();
            let content = file_content();
            if filename.is_empty() {
                return;
            }
            saving_file.set(true);
            spawn(async move {
                let req = UpdateSkillFileContentRequest {
                    skill_id,
                    filename,
                    content,
                    expected_updated_at: None,
                };
                match update_skill_file_content(req).await {
                    Ok(_) => {
                        toast.success("文件已保存");
                        file_content_dirty.set(false);
                    }
                    Err(e) => toast.error(format!("保存失败: {}", e)),
                }
                saving_file.set(false);
            });
        }
    };

    // 打开元信息编辑 Modal（填入当前值）
    let on_open_edit = move |_| {
        if let Some(s) = skill() {
            edit_name.set(s.name.clone());
            edit_description.set(s.description.clone());
            edit_tags.set(s.tags.join(", "));
            edit_category.set(s.category.clone());
            edit_content_input.set(None);
            show_edit_modal.set(true);
        }
    };

    // 提交元信息更新
    let on_submit_edit = {
        let id = id.clone();
        move |_| {
            let skill_id = id.clone();
            let name = edit_name().trim().to_string();
            if name.is_empty() {
                toast.error("名称不能为空");
                return;
            }
            let tags: Vec<String> = edit_tags()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let category = if edit_category().trim().is_empty() {
                None
            } else {
                Some(edit_category().trim().to_string())
            };
            let req = UpdateSkillRequest {
                skill_id: skill_id.clone(),
                name: Some(name),
                description: Some(edit_description()),
                tags: Some(tags),
                category,
                status: None,
                content_input: edit_content_input(),
                file_deletes: None,
            };
            saving_meta.set(true);
            spawn(async move {
                match update_skill(req).await {
                    Ok(_) => {
                        toast.success("Skill 元信息已更新");
                        show_edit_modal.set(false);
                        // 重新拉取详情与文件列表
                        match get_skill(&skill_id).await {
                            Ok(s) => skill.set(Some(s)),
                            Err(e) => toast.error(format!("加载 Skill 失败: {}", e)),
                        }
                        match list_skill_files(&skill_id).await {
                            Ok(resp) => files.set(resp.files),
                            Err(e) => toast.error(format!("加载文件列表失败: {}", e)),
                        }
                    }
                    Err(e) => toast.error(format!("更新失败: {}", e)),
                }
                saving_meta.set(false);
            });
        }
    };

    let skill_data = skill.read().clone();
    let files_list = files.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "Skill 详情" }
                Link { class: "btn btn-ghost", to: crate::pages::Route::HrSkills {},
                    "← 返回列表"
                }
            }
            if loading() {
                Loading {}
            } else if let Some(s) = skill_data {
                // 主信息卡
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{s.name}" }
                            div { class: "flex gap-2",
                                button { class: "btn btn-ghost btn-sm", onclick: on_open_edit, "✏️ 编辑" }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div {
                                div { class: "text-sm text-base-content/60", "描述" }
                                MarkdownRenderer { content: s.description.clone(), compact: true }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "分类" }
                                div { class: "font-medium", "{s.category}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "标签" }
                                div { class: "flex flex-wrap gap-1",
                                    for tag in &s.tags {
                                        span { class: "badge badge-neutral", "{tag}" }
                                    }
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "状态" }
                                span { class: "badge", "{skill_status_text(s.status)}" }
                            }
                        }
                    }
                }
                // 文件列表区
                div { class: "card bg-base-100 shadow-md mb-6",
                    div { class: "card-body",
                        h2 { class: "card-title text-lg mb-2", "📁 文件列表 ({files_list.len()})" }
                        if files_list.is_empty() {
                            EmptyState { icon: "📄".to_string(), message: "此 Skill 暂无文件".to_string() }
                        } else {
                            div { class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                                // 左侧文件列表
                                div { class: "md:col-span-1",
                                    ul { class: "menu bg-base-200 rounded-box",
                                        for f in files_list.iter() {
                                            {
                                                let fname = f.filename.clone();
                                                let active = selected_file() == fname;
                                                let click_skill_id = id.clone();
                                                rsx! {
                                                    li {
                                                        button {
                                                            class: if active { "active" } else { "" },
                                                            onclick: move |_| {
                                                                let skill_id = click_skill_id.clone();
                                                                let filename = fname.clone();
                                                                selected_file.set(filename.clone());
                                                                file_content.set(String::new());
                                                                file_content_dirty.set(false);
                                                                file_content_loading.set(true);
                                                                spawn(async move {
                                                                    let req = GetSkillFileContentRequest {
                                                                        skill_id,
                                                                        filename,
                                                                    };
                                                                    match get_skill_file_content(req).await {
                                                                        Ok(resp) => file_content.set(resp.content),
                                                                        Err(e) => toast.error(format!("加载文件内容失败: {}", e)),
                                                                    }
                                                                    file_content_loading.set(false);
                                                                });
                                                            },
                                                            div { class: "flex justify-between items-center w-full",
                                                                span { class: "font-mono text-sm truncate", "{f.filename}" }
                                                                span { class: "text-xs text-base-content/50",
                                                                    "{crate::utils::format_file_size(f.file_size)}"
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // 右侧内容区
                                div { class: "md:col-span-2",
                                    if selected_file().is_empty() {
                                        EmptyState { icon: "👈".to_string(), message: "请选择左侧文件查看内容".to_string() }
                                    } else if file_content_loading() {
                                        Loading {}
                                    } else {
                                        div { class: "flex flex-col gap-2",
                                            div { class: "flex justify-between items-center",
                                                span { class: "font-mono text-sm text-base-content/70",
                                                    "当前文件: {selected_file()}"
                                                }
                                                div { class: "flex gap-2",
                                                    if selected_file().ends_with(".md") {
                                                        button {
                                                            class: "btn btn-ghost btn-sm",
                                                            onclick: move |_| {
                                                                let cur = preview_mode();
                                                                preview_mode.set(!cur);
                                                            },
                                                            if preview_mode() { "✏️ 源码" } else { "👁️ 预览" }
                                                        }
                                                    }
                                                    if file_content_dirty() {
                                                        span { class: "text-xs text-warning", "● 未保存" }
                                                    }
                                                    button {
                                                        class: "btn btn-primary btn-sm",
                                                        disabled: saving_file() || !file_content_dirty(),
                                                        onclick: on_save_file,
                                                        if saving_file() { "保存中..." } else { "💾 保存" }
                                                    }
                                                }
                                            }
                                            if selected_file().ends_with(".md") && preview_mode() {
                                                div { class: "border border-base-300 rounded-lg p-4 bg-base-100 overflow-x-auto",
                                                    MarkdownRenderer { content: file_content() }
                                                }
                                            } else {
                                                CodeEditor {
                                                    value: file_content(),
                                                    on_input: move |v| {
                                                        file_content.set(v);
                                                        file_content_dirty.set(true);
                                                    },
                                                    language: "markdown".to_string(),
                                                    min_lines: 20,
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "Skill 不存在或已被删除".to_string() }
            }

            // 元信息编辑 Modal
            Modal {
                title: "编辑 Skill 元信息".to_string(),
                show: show_edit_modal(),
                on_close: move |_| show_edit_modal.set(false),
                footer: rsx! {
                    button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button {
                        class: "btn btn-primary",
                        disabled: saving_meta(),
                        onclick: on_submit_edit,
                        if saving_meta() { "保存中..." } else { "保存" }
                    }
                },
                div { class: "space-y-4",
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "名称 *" } }
                        input { class: "input input-bordered w-full", value: "{edit_name}",
                            oninput: move |e| edit_name.set(e.value()), placeholder: "Skill 名称" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "描述" } }
                        textarea { class: "textarea textarea-bordered w-full", value: "{edit_description}",
                            oninput: move |e| edit_description.set(e.value()), placeholder: "Skill 描述" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "标签（逗号分隔）" } }
                        input { class: "input input-bordered w-full", value: "{edit_tags}",
                            oninput: move |e| edit_tags.set(e.value()), placeholder: "tag1, tag2" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "分类" } }
                        input { class: "input input-bordered w-full", value: "{edit_category}",
                            oninput: move |e| edit_category.set(e.value()), placeholder: "如 uncategorized / neural" }
                    }
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "技能内容（可选）" } }
                        SkillContentInputEditor {
                            value: None,
                            on_change: move |ci| edit_content_input.set(ci),
                        }
                    }
                }
            }
        }
    }
}

fn skill_status_text(status: common::enums::SkillStatus) -> &'static str {
    use common::enums::SkillStatus::*;
    match status {
        Draft => "草稿",
        Published => "已发布",
        Expired => "已过期",
    }
}
