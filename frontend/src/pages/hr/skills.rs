//! 技能库管理

use crate::components::hud::HudPanel;
use crate::components::hud::PageHeader;
use dioxus::prelude::*;
use dioxus_router::Link;

use crate::api::hr::{create_skill, delete_skill, list_skills, query_skills, search_skills};
use crate::api::seed::{get_task_progress, preview_preset_skills, sync_preset_skills};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::skill_content_input_editor::SkillContentInputEditor;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use crate::utils::status::{short_id, skill_author_type_badge, skill_author_type_text};
use common::api::{
    CreateSkillRequest, ListSkillsRequest, ListSkillsResponseItem, PresetSkillSyncStrategy,
    SearchSkillsRequest, SkillContentInput, SkillQueryRequest, SyncPresetSkillsRequest,
    SyncPresetSkillsResponse,
};
use common::enums::SkillAuthorType;
use common::enums::SkillStatus;

#[component]
pub fn HrSkills() -> Element {
    let mut skills = use_signal(Vec::<ListSkillsResponseItem>::new);
    let mut loading = use_signal(|| true);
    let toast = use_toast();
    let mut show_add_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_description = use_signal(String::new);
    let mut new_tags = use_signal(String::new);
    let mut new_category = use_signal(String::new);
    let mut new_content_input = use_signal(|| Option::<SkillContentInput>::None);
    let mut creating = use_signal(|| false);
    let mut search_keyword = use_signal(String::new);
    // 修复 HIGH #12：搜索防抖 + race condition 机制
    let mut search_request_id = use_signal(|| 0u32);

    // 过滤条件
    let mut filter_category = use_signal(String::new);
    let mut filter_status = use_signal(|| -1i32);
    // -1 = 全部，0 = 用户（SkillAuthorType::User），1 = Agent（SkillAuthorType::Agent）
    let mut filter_author_type = use_signal(|| -1i32);

    // ===== 删除确认对话框 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    // ===== 同步预置技能弹窗 =====
    let mut show_sync_modal = use_signal(|| false);
    let mut sync_preview = use_signal(|| Option::<common::api::PreviewPresetSkillsResponse>::None);
    let mut sync_strategy = use_signal(|| PresetSkillSyncStrategy::Overwrite);
    let mut sync_installed = use_signal(|| false);
    let mut sync_loading = use_signal(|| false);
    let mut syncing = use_signal(|| false);
    // 轮询到的后台任务进度文案（同步中显示）
    let mut sync_progress = use_signal(String::new);

    // 加载数据（三场景切换：list / query / search）
    let load_data = move || {
        spawn(async move {
            loading.set(true);
            let keyword = search_keyword();
            let category = filter_category();
            let status = filter_status();
            let author_type = filter_author_type();
            let my_id = search_request_id() + 1;
            search_request_id.set(my_id);

            let category_opt = if category.trim().is_empty() {
                None
            } else {
                Some(category)
            };
            let author_type_opt = if author_type < 0 {
                None
            } else {
                Some(SkillAuthorType::from(author_type))
            };
            let has_filter = category_opt.is_some() || status >= 0 || author_type_opt.is_some();

            // 三场景切换：
            // 无关键词 + 无过滤 → list_skills
            // 无关键词 + 有过滤 → query_skills
            // 有关键词 → search_skills（可同时带过滤条件）
            let result = if keyword.trim().is_empty() && !has_filter {
                list_skills(ListSkillsRequest::default())
                    .await
                    .map(|p| p.items)
            } else if keyword.trim().is_empty() {
                query_skills(&SkillQueryRequest {
                    category: category_opt.clone(),
                    status: if status >= 0 {
                        Some(SkillStatus::from(status))
                    } else {
                        None
                    },
                    author_type: author_type_opt,
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            } else {
                search_skills(&SearchSkillsRequest {
                    keyword: Some(keyword),
                    category: category_opt,
                    status: if status >= 0 {
                        Some(SkillStatus::from(status))
                    } else {
                        None
                    },
                    author_type: author_type_opt,
                    ..Default::default()
                })
                .await
                .map(|p| p.items)
            };

            // 丢弃过期请求的结果
            if search_request_id() != my_id {
                return;
            }

            match result {
                Ok(v) => skills.set(v),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    // 打开弹窗即拉取预览（seed vs 技能库逐项对比）
    let open_sync_modal = move |_| {
        show_sync_modal.set(true);
        sync_preview.set(None);
        sync_strategy.set(PresetSkillSyncStrategy::Overwrite);
        sync_installed.set(false);
        sync_progress.set(String::new());
        sync_loading.set(true);
        spawn(async move {
            match preview_preset_skills().await {
                Ok(v) => sync_preview.set(Some(v)),
                Err(e) => toast.error(format!("加载预置技能清单失败: {}", e)),
            }
            sync_loading.set(false);
        });
    };

    // 确认同步：提交后台任务 → 轮询进度 → 完成后 toast 结果并刷新列表
    let confirm_sync = move |_| {
        let req = SyncPresetSkillsRequest {
            strategy: sync_strategy(),
            sync_installed_copies: sync_installed(),
        };
        syncing.set(true);
        sync_progress.set("正在提交同步任务...".to_string());
        spawn(async move {
            let task_id = match sync_preset_skills(req).await {
                Ok(r) => r.task_id,
                Err(e) => {
                    toast.error(format!("提交同步任务失败: {}", e));
                    syncing.set(false);
                    return;
                }
            };

            loop {
                gloo_timers::future::TimeoutFuture::new(500).await;
                match get_task_progress(&task_id).await {
                    Ok(p) => {
                        sync_progress.set(p.step_message.clone());
                        match p.status {
                            common::api::TaskStatus::Completed => {
                                let resp: Option<SyncPresetSkillsResponse> =
                                    p.result.and_then(|v| serde_json::from_value(v).ok());
                                match resp {
                                    Some(r) => toast.success(format!(
                                        "同步完成：新增 {}、覆盖 {}、跳过 {}，同步副本 {}（seed 共 {} 项）",
                                        r.created, r.updated, r.skipped, r.updated_copies, r.total
                                    )),
                                    None => toast.success("同步完成"),
                                }
                                show_sync_modal.set(false);
                                load_data();
                                break;
                            }
                            common::api::TaskStatus::Failed => {
                                toast.error(format!(
                                    "同步失败: {}",
                                    p.error.unwrap_or_else(|| "未知错误".to_string())
                                ));
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        toast.error(format!("查询同步进度失败: {}", e));
                        break;
                    }
                }
            }
            syncing.set(false);
        });
    };

    // 初始加载
    use_effect(move || {
        load_data();
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().trim().is_empty() || new_description().trim().is_empty() {
                toast.error("技能名称和描述不能为空");
                return;
            }
            creating.set(true);
            let tags: Vec<String> = new_tags()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let req = CreateSkillRequest {
                name: new_name().trim().to_string(),
                description: new_description().trim().to_string(),
                tags,
                category: if new_category().trim().is_empty() {
                    None
                } else {
                    Some(new_category().trim().to_string())
                },
                status: None,
                content_input: new_content_input(),
            };
            match create_skill(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_description.set(String::new());
                    new_tags.set(String::new());
                    new_category.set(String::new());
                    new_content_input.set(None);
                    load_data();
                }
                Err(e) => toast.error(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let skills_list = skills.read().clone();

    rsx! {
        AppLayout {
            PageHeader {
                eyebrow: Some("HR".to_string()),
                title: "技能库".to_string(),
                actions: Some(rsx!{
                div { class: "flex gap-2 flex-wrap",
                    if !search_keyword().is_empty() || !filter_category().is_empty() || filter_status() >= 0 || filter_author_type() >= 0 {
                        button { class: "btn hud-btn btn-ghost",
                            onclick: move |_| {
                                search_keyword.set(String::new());
                                filter_category.set(String::new());
                                filter_status.set(-1);
                                filter_author_type.set(-1);
                                load_data();
                            },
                            "重置"
                        }
                    }
                    button { class: "btn hud-btn btn-ghost", onclick: open_sync_modal, "⟳ 同步预置技能" }
                    button { class: "btn hud-btn btn-primary", onclick: move |_| show_add_modal.set(true), "+ 创建技能" }
                }
                }),
            },

            // 筛选栏（独立卡片）
            HudPanel { signal: Some(true), extra_class: Some("mb-4".to_string()),
                div { class: "card-body",
                    div { class: "flex flex-wrap gap-4 items-end",
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "分类" }
                            input {
                                class: "input input-bordered w-full",
                                placeholder: "分类名称",
                                value: "{filter_category}",
                                oninput: move |e| {
                                    filter_category.set(e.value());
                                    let my_id = search_request_id() + 1;
                                    search_request_id.set(my_id);
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(300).await;
                                        if search_request_id() != my_id {
                                            return;
                                        }
                                        load_data();
                                    });
                                }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "状态" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_status}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_status.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "1", "已发布" }
                                option { value: "2", "草稿" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "作者类型" }
                            select {
                                class: "select select-bordered w-full",
                                value: "{filter_author_type}",
                                onchange: move |e| {
                                    if let Ok(v) = e.value().parse::<i32>() {
                                        filter_author_type.set(v);
                                    }
                                    load_data();
                                },
                                option { value: "-1", "全部" }
                                option { value: "0", "用户" }
                                option { value: "1", "Agent" }
                            }
                        }
                        div { class: "flex flex-col gap-1 min-w-[140px] flex-1",
                            label { class: "form-label", "搜索" }
                            input {
                                class: "input input-bordered w-full",
                                placeholder: "搜索技能...",
                                value: "{search_keyword}",
                                oninput: move |e| {
                                    search_keyword.set(e.value());
                                    let my_id = search_request_id() + 1;
                                    search_request_id.set(my_id);
                                    spawn(async move {
                                        gloo_timers::future::TimeoutFuture::new(300).await;
                                        if search_request_id() != my_id {
                                            return;
                                        }
                                        load_data();
                                    });
                                }
                            }
                        }
                    }
                }
            }

            // 列表卡片
            HudPanel { signal: Some(true),
                div { class: "card-body",
                if loading() {
                    Loading {}
                } else if skills_list.is_empty() {
                    EmptyState { icon: "📚".to_string(), message: "暂无技能".to_string() }
                } else {
                    div { class: "overflow-x-auto",
                        table { class: "table hud-table table-zebra table-pin-rows",
                            thead { tr { th { "名称" }, th { "描述" }, th { "标签" }, th { "创建者" }, th { "操作" } }}
                            tbody {
                                for s in skills_list.iter() {
                                    {
                                        let id = s.id.clone();
                                        let name = s.name.clone();
                                        let description = s.description.clone();
                                        let tags = s.tags.clone();
                                        let author_type = s.author_type;
                                        let author_id_short = short_id(&s.author_id);
                                        rsx! {
                                            tr { key: "{id}",
                                                td { class: "font-semibold", "data-label": "名称", "{name}" }
                                                td { class: "text-base-content/70", "data-label": "描述", "{description}" }
                                                td { "data-label": "标签",
                                                    div { class: "flex flex-wrap gap-1",
                                                        for tag in &tags {
                                                            span { class: "badge orz-tag badge-sm", "{tag}" }
                                                        }
                                                    }
                                                }
                                                td { "data-label": "创建者",
                                                    div { class: "flex items-center gap-2",
                                                        span {
                                                            class: "{skill_author_type_badge(author_type)}",
                                                            "{skill_author_type_text(author_type)}"
                                                        }
                                                        span { class: "font-mono text-xs text-base-content/60 select-all", "{author_id_short}" }
                                                    }
                                                }
                                                td { "data-label": "操作",
                                                    div { class: "flex gap-1",
                                                        Link {
                                                            class: "btn hud-btn btn-ghost btn-sm",
                                                            to: crate::pages::Route::HrSkillDetail { id: id.clone() },
                                                            "详情"
                                                        }
                                                        button { class: "btn hud-btn btn-error btn-sm",
                                                            onclick: move |_| {
                                                                pending_delete_id.set(id.clone());
                                                                show_delete_confirm.set(true);
                                                            }, "删除"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 创建技能弹窗
        Modal {
            title: "创建新技能".to_string(),
            show: show_add_modal(),
            on_close: move |_| {
                show_add_modal.set(false);
                new_name.set(String::new());
                new_description.set(String::new());
                new_tags.set(String::new());
                new_category.set(String::new());
                new_content_input.set(None);
            },
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "技能名称 *" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()), placeholder: "请输入技能名称" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "技能描述 *" }
                    }
                    textarea { class: "textarea textarea-bordered w-full", value: "{new_description}",
                        oninput: move |e| new_description.set(e.value()), placeholder: "请输入技能描述" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "标签" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_tags}",
                        oninput: move |e| new_tags.set(e.value()), placeholder: "coding, backend" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "分类" }
                    }
                    input { class: "input input-bordered w-full", value: "{new_category}",
                        oninput: move |e| new_category.set(e.value()), placeholder: "development" }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "技能内容" }
                    }
                    SkillContentInputEditor {
                        value: None,
                        on_change: move |ci| new_content_input.set(ci),
                    }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除".to_string(),
            message: "确定删除此技能？此操作不可撤销。".to_string(),
            on_confirm: move |_| {
                let id = pending_delete_id();
                show_delete_confirm.set(false);
                spawn(async move {
                    if let Err(e) = delete_skill(&id).await {
                        toast.error(format!("删除失败: {}", e));
                    } else {
                        load_data();
                    }
                });
            },
            on_cancel: move |_| {
                show_delete_confirm.set(false);
            }
        }

        // 同步预置技能弹窗（策略二选一 + 影响清单）
        Modal {
            title: "同步预置技能".to_string(),
            show: show_sync_modal(),
            width_class: Some("max-w-2xl".to_string()),
            on_close: move |_| show_sync_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_sync_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: syncing() || sync_loading(),
                    onclick: confirm_sync,
                    if syncing() { "同步中..." } else { "确认同步" }
                }
            },
            div { class: "space-y-4",
                // 策略选择（二选一）
                div { class: "grid gap-2",
                    div {
                        class: if sync_strategy() == PresetSkillSyncStrategy::Overwrite {
                            "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                        } else {
                            "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                        },
                        onclick: move |_| sync_strategy.set(PresetSkillSyncStrategy::Overwrite),
                        div { class: "card-body p-3",
                            div { class: "font-semibold", "1 · 用 seed 覆盖重置" }
                            div { class: "text-xs text-base-content/70",
                                "已存在的同 ID 技能将覆写回初始状态（名称、描述、标签与 skill.md）；你在技能目录下额外添加的文件会保留"
                            }
                        }
                    }
                    div {
                        class: if sync_strategy() == PresetSkillSyncStrategy::OnlyMissing {
                            "card cursor-pointer border-2 border-primary bg-base-200 transition-colors"
                        } else {
                            "card cursor-pointer border border-base-300 bg-base-200 transition-colors"
                        },
                        onclick: move |_| sync_strategy.set(PresetSkillSyncStrategy::OnlyMissing),
                        div { class: "card-body p-3",
                            div { class: "font-semibold", "2 · 保留本地，仅补缺" }
                            div { class: "text-xs text-base-content/70",
                                "已存在的技能原样保留，只把 seed 中缺失的技能加入技能库"
                            }
                        }
                    }
                }

                // 可选：同步已安装到 Agent 的副本
                div { class: "flex items-center gap-3",
                    input {
                        class: "toggle toggle-primary toggle-sm",
                        r#type: "checkbox",
                        checked: sync_installed(),
                        onchange: move |_| sync_installed.set(!sync_installed()),
                    }
                    span { class: "text-sm", "同时更新已安装到 Agent 的技能副本" }
                }

                // 影响清单
                if sync_loading() {
                    Loading {}
                } else if let Some(preview) = sync_preview() {
                    {
                        // 汇总文案在 rsx 外计算：嵌套 if 表达式放进 format! 会破坏 rsx 解析
                        let overwrite = sync_strategy() == PresetSkillSyncStrategy::Overwrite;
                        let summary = format!(
                            "seed 共 {} 项：缺失 {} 项将新增，已存在 {} 项{}，涉及已安装副本 {} 个",
                            preview.items.len(),
                            preview.missing_count,
                            preview.existing_count,
                            if overwrite { "将被覆盖" } else { "将保留" },
                            preview.total_installed_copies
                        );
                        rsx! {
                            div {
                                div { class: "text-sm mb-2", "{summary}" }
                                div { class: "max-h-56 overflow-y-auto",
                                    for item in preview.items.iter() {
                                        div { class: "flex items-center justify-between gap-2 py-1.5 border-b border-base-300 last:border-0",
                                            div { class: "min-w-0 flex-1",
                                                div { class: "text-sm font-medium truncate", "{item.name}" }
                                                div { class: "text-xs text-base-content/50 font-mono truncate", "{item.id}" }
                                            }
                                            div { class: "flex items-center gap-1 shrink-0",
                                                if item.installed_copies > 0 {
                                                    span { class: "badge orz-tag badge-sm", "副本 {item.installed_copies}" }
                                                }
                                                if item.exists {
                                                    span { class: "badge orz-tag badge-sm", "将覆盖" }
                                                } else {
                                                    span { class: "badge orz-tag badge-sm", "将新增" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 同步进度（后台任务执行中显示）
                if syncing() {
                    div { class: "flex items-center gap-2 text-sm text-base-content/70",
                        span { class: "loading loading-spinner loading-sm" }
                        span { "{sync_progress}" }
                    }
                }

                // 风险提示
                div { class: "text-xs text-base-content/50",
                    "注意：覆盖重置会把手动改过的技能状态重置为已发布；同步以后台任务执行，可关闭弹窗稍后查看结果。"
                }
            }
        }
        }
    }
}
