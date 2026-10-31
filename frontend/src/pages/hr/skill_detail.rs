//! Skill 详情页 - 展示元信息 + 文件列表 + 文件内容查看/编辑 + 元信息编辑

use crate::components::hud::{HudPanel, HudSection};
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::{
    get_skill, get_skill_file_content, list_skill_files, publish_skill, update_skill,
    update_skill_file_content,
};
use crate::components::code_editor::CodeEditor;
use crate::components::markdown::MarkdownRenderer;
use crate::components::modal::Modal;
use crate::components::skill_content_input_editor::SkillContentInputEditor;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::{
    GetSkillFileContentRequest, PublishSkillRequest, SkillContentInput,
    UpdateSkillFileContentRequest, UpdateSkillRequest,
};

#[component]
pub fn HrSkillDetail(id: String) -> Element {
    // 方案 B：订阅路由并把 id 同步到响应式 rid，use_resource 绑定 rid，
    // 拉取仅在 id 变化时触发（skill / files 各自一个 resource）
    let route = dioxus_router::use_route::<crate::pages::Route>();
    let mut rid = use_signal(String::new);
    if let crate::pages::Route::HrSkillDetail { id: route_id } = &route
        && *rid.peek() != *route_id
    {
        rid.set(route_id.clone());
    }
    let toast = use_toast();

    let mut skill_res = use_resource(move || {
        let id = rid();
        async move { get_skill(&id).await }
    });
    let mut files_res = use_resource(move || {
        let id = rid();
        async move { list_skill_files(&id).await }
    });
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
    // 发布中（独立接口：Draft → Published，见 skill_detail 头部按钮）
    let mut publishing = use_signal(|| false);

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
        if let Some(s) = skill_res
            .read()
            .as_ref()
            .and_then(|r| r.as_ref().ok())
            .cloned()
        {
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
                            Ok(s) => skill_res.set(Some(Ok(s))),
                            Err(e) => toast.error(format!("加载 Skill 失败: {}", e)),
                        }
                        match list_skill_files(&skill_id).await {
                            Ok(resp) => files_res.set(Some(Ok(resp))),
                            Err(e) => toast.error(format!("加载文件列表失败: {}", e)),
                        }
                    }
                    Err(e) => toast.error(format!("更新失败: {}", e)),
                }
                saving_meta.set(false);
            });
        }
    };

    let skill_data = skill_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .cloned();
    let files_list = files_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().ok())
        .map(|resp| resp.files.clone());
    // 加载失败时（Some(Err)）取出错误文案，供下方 else / 文件列表分支显式展示
    let skill_err_msg = skill_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(|e| format!("Skill 加载失败: {}", e));
    let files_err_msg = files_res
        .read()
        .as_ref()
        .and_then(|r| r.as_ref().err())
        .map(|e| format!("文件列表加载失败: {}", e));

    // 选中文件的元信息（附件只暴露路径，不再经接口取内容）
    let selected_meta = files_list
        .as_ref()
        .and_then(|fs| fs.iter().find(|f| f.filename == selected_file()).cloned());
    let selected_is_main = selected_meta.as_ref().is_some_and(|m| m.is_main);
    let selected_abs_path = selected_meta
        .as_ref()
        .map(|m| m.abs_path.clone())
        .unwrap_or_default();
    let selected_executable = selected_meta.as_ref().is_some_and(|m| m.executable);
    let selected_size = selected_meta.as_ref().map(|m| m.file_size).unwrap_or(0);
    let copy_path = selected_abs_path.clone();

    // 删除附件：file_deletes 走 update_skill（禁删主文件由后端兜底）。
    // 用 Rc 包装：for 循环里每个列表项都要持有一份回调，普通闭包无法 Clone。
    let on_delete_file: std::rc::Rc<dyn Fn(String)> = {
        let id = id.clone();
        std::rc::Rc::new(move |filename: String| {
            let skill_id = id.clone();
            let fname = filename.clone();
            spawn(async move {
                let req = UpdateSkillRequest {
                    skill_id,
                    name: None,
                    description: None,
                    tags: None,
                    category: None,
                    status: None,
                    content_input: None,
                    file_deletes: Some(vec![fname.clone()]),
                };
                match update_skill(req).await {
                    Ok(_) => {
                        toast.success(format!("已删除附件 {}", fname));
                        selected_file.set(String::new());
                        file_content.set(String::new());
                        file_content_dirty.set(false);
                        files_res.restart();
                    }
                    Err(e) => toast.error(format!("删除失败: {}", e)),
                }
            });
        })
    };

    // 发布按钮的 move 闭包独占一份 id：与下方文件列表 / 删除按钮的 id 所有权分开，
    // 否则 `id` 会被第一个 move 闭包按值捕获，后续借用直接报 E0382。
    let publish_id = id.clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "Skill 详情" }
                Link { class: "btn hud-btn btn-ghost", to: crate::pages::Route::HrSkills {},
                    "← 返回列表"
                }
            }
            if skill_res.read().as_ref().is_none() || files_res.read().as_ref().is_none() {
                Loading {}
            } else if let Some(s) = skill_data {
                // 主信息卡
                HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                    title: Some(s.name.clone()),
                    actions: Some(rsx!{
                        // 发布（Draft → Published）：走独立接口，语义与 update 分开。
                        // 显隐由后端 `can_publish` 决定（仅 Admin/SuperAdmin；普通成员
                        // 的草稿要由管理员或收口人 Agent 审核发布），前端不猜角色。
                        if s.status == common::enums::SkillStatus::Draft && s.can_publish {
                            button {
                                class: "btn hud-btn btn-primary btn-sm mr-2",
                                disabled: publishing(),
                                onclick: move |_| {
                                    let skill_id = publish_id.clone();
                                    publishing.set(true);
                                    spawn(async move {
                                        let req = PublishSkillRequest { skill_id: skill_id.clone() };
                                        match publish_skill(req).await {
                                            Ok(_) => {
                                                toast.success("技能已发布");
                                                match get_skill(&skill_id).await {
                                                    Ok(updated) => skill_res.set(Some(Ok(updated))),
                                                    Err(e) => toast.error(format!("加载 Skill 失败: {}", e)),
                                                }
                                            }
                                            Err(e) => toast.error(format!("发布失败: {}", e)),
                                        }
                                        publishing.set(false);
                                    });
                                },
                                if publishing() { "发布中…" } else { "🚀 发布" }
                            }
                        }
                        button { class: "btn hud-btn btn-ghost btn-sm", onclick: on_open_edit, "✏️ 编辑" }
                    }),
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
                                        span { class: "badge orz-tag badge-sm", "{tag}" }
                                    }
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "状态" }
                                span { class: "{skill_status_badge(s.status)}", "{skill_status_text(s.status)}" }
                            }
                        }
                }
                // 文件列表区
                HudPanel { signal: Some(true), extra_class: Some("mb-6".to_string()),
                    div { class: "card-body",
                        HudSection { title: format!("📁 文件列表 ({})", files_list.as_ref().map(|f| f.len()).unwrap_or(0)) }
                        if let Some(e) = &files_err_msg {
                            EmptyState { icon: "❓".to_string(), message: e.clone() }
                        } else if files_list.as_ref().map(|f| f.is_empty()).unwrap_or(true) {
                            EmptyState { icon: "📄".to_string(), message: "此 Skill 暂无文件".to_string() }
                        } else {
                            div { class: "grid grid-cols-1 md:grid-cols-3 gap-4",
                                // 左侧文件列表
                                div { class: "md:col-span-1",
                                    ul { class: "menu bg-base-200 rounded-box",
                                        for f in files_list.as_ref().unwrap().iter() {
                                            {
                                                let fname = f.filename.clone();
                                                let is_main = f.is_main;
                                                let executable = f.executable;
                                                let active = selected_file() == fname;
                                                let click_skill_id = id.clone();
                                                let delete_cb = on_delete_file.clone();
                                                let delete_name = f.filename.clone();
                                                rsx! {
                                                    li {
                                                        div { class: "flex items-center gap-1",
                                                            button {
                                                                class: if active { "active flex-1" } else { "flex-1" },
                                                                onclick: move |_| {
                                                                    let skill_id = click_skill_id.clone();
                                                                    let filename = fname.clone();
                                                                    selected_file.set(filename.clone());
                                                                    file_content.set(String::new());
                                                                    file_content_dirty.set(false);
                                                                    // 附件内容不经接口返回：只选中、展示路径，
                                                                    // 由 Agent 用 read_file / shell 工具按路径访问
                                                                    if !is_main {
                                                                        return;
                                                                    }
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
                                                                    span { class: "font-mono text-sm truncate",
                                                                        "{f.filename}"
                                                                        if is_main { " · 主文件" } else if executable { " · 可执行" }
                                                                    }
                                                                    span { class: "text-xs text-base-content/50",
                                                                        "{crate::utils::format_file_size(f.file_size)}"
                                                                    }
                                                                }
                                                            }
                                                            if !is_main {
                                                                button {
                                                                    class: "btn hud-btn btn-ghost btn-xs",
                                                                    title: "删除该附件",
                                                                    onclick: move |_| {
                                                                        let cb = delete_cb.clone();
                                                                        cb(delete_name.clone());
                                                                    },
                                                                    "🗑"
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
                                    } else if !selected_is_main {
                                        // 附件：接口只给路径。内容由 Agent 用 read_file 按需读、
                                        // 脚本类用 shell 工具按同一路径执行——接口回传二进制
                                        // 既读不了，大文件还会撑爆上下文。
                                        div { class: "flex flex-col gap-3",
                                            div { class: "flex justify-between items-center",
                                                span { class: "font-mono text-sm text-base-content/70",
                                                    "附件: {selected_file()}"
                                                }
                                                span { class: "text-xs text-base-content/50",
                                                    "{crate::utils::format_file_size(selected_size)}"
                                                }
                                            }
                                            div { class: "rounded-lg border border-base-300 bg-base-200 p-3",
                                                div { class: "text-xs text-base-content/50 mb-1",
                                                    "绝对路径（Agent 用 read_file 读取 / shell 执行）"
                                                }
                                                div { class: "flex items-start gap-2",
                                                    code { class: "font-mono text-sm break-all flex-1", "{selected_abs_path}" }
                                                    button {
                                                        class: "btn hud-btn btn-ghost btn-sm",
                                                        onclick: move |_| {
                                                            copy_to_clipboard(&copy_path.clone(), toast);
                                                        },
                                                        "📋 复制"
                                                    }
                                                }
                                            }
                                            div { class: "text-xs text-base-content/60",
                                                if selected_executable {
                                                    "带可执行位：Agent 可直接用 shell 工具按上述路径执行。"
                                                } else {
                                                    "内容不通过接口返回：请让 Agent 用 read_file 工具按上述路径按需读取。"
                                                }
                                            }
                                        }
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
                                                            class: "btn hud-btn btn-ghost btn-sm",
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
                                                        class: "btn hud-btn btn-primary btn-sm",
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
                EmptyState { icon: "❓".to_string(), message: skill_err_msg.clone().unwrap_or_else(|| "Skill 不存在或已被删除".to_string()) }
            }

            // 元信息编辑 Modal
            Modal {
                title: "编辑 Skill 元信息".to_string(),
                show: show_edit_modal(),
                on_close: move |_| show_edit_modal.set(false),
                footer: rsx! {
                    button { class: "btn hud-btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                    button {
                        class: "btn hud-btn btn-primary",
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

/// 复制文本到剪贴板（附件路径复制用），通过 toast 反馈结果
fn copy_to_clipboard(content: &str, toast: crate::store::toast::ToastState) {
    if let Some(window) = web_sys::window() {
        let clipboard = window.navigator().clipboard();
        let promise = clipboard.write_text(content);
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => toast.info("路径已复制到剪贴板"),
                Err(_) => toast.error("复制失败"),
            }
        });
    } else {
        toast.error("剪贴板不可用");
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

/// 技能状态徽章（HUD 皮肤，单一事实源）。取代详情页原本 `class: "badge"` 缺 hud 皮肤的写法。
fn skill_status_badge(status: common::enums::SkillStatus) -> &'static str {
    use common::enums::SkillStatus::*;
    match status {
        Draft => "badge hud-badge badge-warning",
        Published => "badge hud-badge badge-success",
        Expired => "badge hud-badge badge-error",
    }
}
