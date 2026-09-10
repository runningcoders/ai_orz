//! Seed 配置迁移管理页面
//!
//! 功能：
//! - 列出 seeds/ 目录下的快照文件
//! - 导出当前组织配置（save）→ 异步任务，轮询进度
//! - 加载快照文件（load）→ 异步任务，轮询进度
//! - 应用默认模板（apply-default）→ 异步任务，轮询进度
//! - 查看文件内容 / 删除文件
//!
//! 所有异步任务通过 `TaskProgress` 组件统一展示进度，300ms 间隔轮询。

use crate::components::hud::{HudPanel, StatGrid, StatReadout};
use dioxus::prelude::*;

use crate::api::seed::{
    apply_default, delete_seed_file, get_default_seed, get_seed_file, get_task_progress,
    list_seeds, load_seed, save_seed,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, Loading};
use crate::components::task_progress::TaskProgress;
use crate::layouts::app_layout::AppLayout;
use crate::pages::system::seed_sensitive_fields::{
    SeedSensitiveFields, SensitiveField, check_sensitive_filled, extract_sensitive_fields,
    parse_sensitive_fields,
};
use crate::store::toast::use_toast;
use crate::utils::{format_datetime_full, format_file_size as format_size};
use common::api::seed::ImportStrategy;
use common::api::{TaskProgressSnapshot, TaskStatus};
use std::collections::HashMap;

/// 策略下拉框的 4 个选项
fn strategy_options() -> Vec<(&'static str, ImportStrategy)> {
    vec![
        ("保留 ID（同组织恢复）", ImportStrategy::PreserveIds),
        ("生成新 ID（跨组织迁移）", ImportStrategy::RegenerateIds),
        ("仅预演（DryRun）", ImportStrategy::DryRun),
        ("跳过已存在", ImportStrategy::SkipExisting),
    ]
}

/// 把策略转成下拉框 value（索引字符串）
fn strategy_to_value(s: ImportStrategy) -> &'static str {
    match s {
        ImportStrategy::PreserveIds => "0",
        ImportStrategy::RegenerateIds => "1",
        ImportStrategy::DryRun => "2",
        ImportStrategy::SkipExisting => "3",
    }
}

/// 把下拉框 value 解析回策略
fn value_to_strategy(v: &str) -> ImportStrategy {
    match v {
        "1" => ImportStrategy::RegenerateIds,
        "2" => ImportStrategy::DryRun,
        "3" => ImportStrategy::SkipExisting,
        _ => ImportStrategy::PreserveIds,
    }
}

/// 标记当前任务类型，决定完成后的提示文案
#[derive(Debug, Clone, Copy, PartialEq)]
enum TaskKind {
    Save,
    Load,
    ApplyDefault,
}

impl TaskKind {
    fn label(self) -> &'static str {
        match self {
            TaskKind::Save => "导出",
            TaskKind::Load => "导入",
            TaskKind::ApplyDefault => "应用默认模板",
        }
    }
}

#[component]
pub fn SystemSeed() -> Element {
    let toast = use_toast();

    let mut seeds = use_signal(Vec::<common::api::SeedFileInfo>::new);
    let loading = use_signal(|| true);

    // 当前进行中的任务（Some 时显示 TaskProgress，None 时显示列表）
    let mut current_task = use_signal(|| Option::<TaskProgressSnapshot>::None);
    let mut current_task_kind = use_signal(|| Option::<TaskKind>::None);

    // 保存弹窗
    let mut show_save_modal = use_signal(|| false);
    let mut save_name = use_signal(String::new);
    let mut save_description = use_signal(String::new);
    let mut save_submitting = use_signal(|| false);

    // 加载弹窗
    let mut show_load_modal = use_signal(|| false);
    let mut load_file_name = use_signal(String::new);
    let mut load_strategy = use_signal(|| "0".to_string());
    let mut load_submitting = use_signal(|| false);
    // 待补填敏感字段（快照解析得出）+ 用户输入值
    let mut load_fields = use_signal(Vec::<SensitiveField>::new);
    let mut load_sensitive = use_signal(HashMap::<String, String>::new);

    // 应用默认模板弹窗
    let mut show_apply_default_modal = use_signal(|| false);
    let mut apply_default_strategy = use_signal(|| "0".to_string());
    let mut apply_default_submitting = use_signal(|| false);
    let mut apply_default_fields = use_signal(Vec::<SensitiveField>::new);
    let mut apply_default_sensitive = use_signal(HashMap::<String, String>::new);

    // 稳定的敏感字段回调：use_callback 保证父重渲染时不重建，避免
    // SeedSensitiveFields 被无谓重渲染而打断输入（见 seed_sensitive_fields.rs 注释）
    let on_load_sensitive_change =
        use_callback(move |m: HashMap<String, String>| load_sensitive.set(m));
    let on_apply_default_sensitive_change =
        use_callback(move |m: HashMap<String, String>| apply_default_sensitive.set(m));

    // 查看文件内容弹窗
    let mut show_view_modal = use_signal(|| false);
    let mut view_file_name = use_signal(String::new);
    let mut view_content = use_signal(String::new);
    let mut view_loading = use_signal(|| false);

    // 删除确认弹窗
    let mut show_delete_modal = use_signal(|| false);
    let mut delete_file_name = use_signal(String::new);
    let mut delete_loading = use_signal(|| false);

    // seed 任务轮询循环的卸载守卫：组件卸载时置 false，避免 spawn 的 loop 在
    // 导出/导入/应用任务进行中离开页面后永久运行（每 300ms 打 get_task_progress）。
    let poll_running = use_signal(|| std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)));
    use_drop(move || {
        poll_running
            .read()
            .store(false, std::sync::atomic::Ordering::SeqCst);
    });

    /// 刷新 seed 列表
    fn reload(
        mut loading: Signal<bool>,
        mut seeds: Signal<Vec<common::api::SeedFileInfo>>,
        toast: crate::store::toast::ToastState,
    ) {
        loading.set(true);
        spawn(async move {
            match list_seeds().await {
                Ok(resp) => seeds.set(resp.data),
                Err(e) => toast.error(format!("加载 seed 列表失败: {}", e)),
            }
            loading.set(false);
        });
    }

    // 初始加载
    use_effect(move || {
        reload(loading, seeds, toast);
    });

    /// 启动任务轮询：300ms 间隔，完成/失败时退出循环
    fn start_polling(
        task_id: String,
        kind: TaskKind,
        mut current_task: Signal<Option<TaskProgressSnapshot>>,
        mut current_task_kind: Signal<Option<TaskKind>>,
        mut seeds: Signal<Vec<common::api::SeedFileInfo>>,
        toast: crate::store::toast::ToastState,
        running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) {
        spawn(async move {
            loop {
                if !running.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                gloo_timers::future::TimeoutFuture::new(300).await;
                match get_task_progress(&task_id).await {
                    Ok(snapshot) => {
                        let is_done = snapshot.status == TaskStatus::Completed
                            || snapshot.status == TaskStatus::Failed;
                        let is_completed = snapshot.status == TaskStatus::Completed;
                        current_task.set(Some(snapshot));
                        if is_done {
                            if is_completed {
                                toast.success(format!("{}完成", kind.label()));
                                // 完成后刷新列表（save/apply-default 会新增文件，load 不会但保持一致性）
                                match list_seeds().await {
                                    Ok(resp) => seeds.set(resp.data),
                                    Err(e) => toast.error(format!("刷新列表失败: {}", e)),
                                }
                            }
                            // 失败时保留 current_task，让用户看到错误信息和返回按钮
                            break;
                        }
                    }
                    Err(e) => {
                        toast.error(format!("查询进度失败: {}", e));
                        current_task.set(None);
                        current_task_kind.set(None);
                        break;
                    }
                }
            }
        });
    }

    // ===== 保存（导出）=====
    let on_submit_save = move |_| {
        let name = save_name().trim().to_string();
        if name.is_empty() {
            toast.error("文件名不能为空");
            return;
        }
        save_submitting.set(true);
        let description = save_description();
        let description = if description.trim().is_empty() {
            None
        } else {
            Some(description)
        };
        spawn(async move {
            let req = common::api::SaveSeedRequest { name, description };
            match save_seed(req).await {
                Ok(resp) => {
                    show_save_modal.set(false);
                    save_submitting.set(false);
                    save_name.set(String::new());
                    save_description.set(String::new());
                    current_task_kind.set(Some(TaskKind::Save));
                    current_task.set(Some(TaskProgressSnapshot {
                        task_id: resp.task_id.clone(),
                        task_type: "seed_save".to_string(),
                        status: TaskStatus::Pending,
                        current_step: 0,
                        total_steps: 0,
                        step_message: "等待开始...".to_string(),
                        started_at: 0,
                        finished_at: None,
                        error: None,
                        result: None,
                    }));
                    start_polling(
                        resp.task_id,
                        TaskKind::Save,
                        current_task,
                        current_task_kind,
                        seeds,
                        toast,
                        poll_running.read().clone(),
                    );
                }
                Err(e) => {
                    toast.error(format!("提交导出任务失败: {}", e));
                    save_submitting.set(false);
                }
            }
        });
    };

    // ===== 加载（导入）=====
    let on_submit_load = move |_| {
        let name = load_file_name();
        if name.is_empty() {
            toast.error("未选择文件");
            return;
        }
        let strategy = value_to_strategy(&load_strategy());
        // 敏感字段前置校验：后端在后台任务里才校验，失败后只剩任务 Failed，
        // 用户已看不到表单上下文，这里挡在提交之前
        let sensitive_values = load_sensitive.read().clone();
        if let Err(msg) = check_sensitive_filled(&load_fields(), &sensitive_values, strategy) {
            toast.error(msg);
            return;
        }
        load_submitting.set(true);
        spawn(async move {
            let req = common::api::LoadSeedRequest {
                name: name.clone(),
                strategy,
                sensitive_values,
            };
            match load_seed(&name, req).await {
                Ok(resp) => {
                    show_load_modal.set(false);
                    load_submitting.set(false);
                    current_task_kind.set(Some(TaskKind::Load));
                    current_task.set(Some(TaskProgressSnapshot {
                        task_id: resp.task_id.clone(),
                        task_type: "seed_load".to_string(),
                        status: TaskStatus::Pending,
                        current_step: 0,
                        total_steps: 0,
                        step_message: "等待开始...".to_string(),
                        started_at: 0,
                        finished_at: None,
                        error: None,
                        result: None,
                    }));
                    start_polling(
                        resp.task_id,
                        TaskKind::Load,
                        current_task,
                        current_task_kind,
                        seeds,
                        toast,
                        poll_running.read().clone(),
                    );
                }
                Err(e) => {
                    toast.error(format!("提交导入任务失败: {}", e));
                    load_submitting.set(false);
                }
            }
        });
    };

    // ===== 应用默认模板 =====
    let on_submit_apply_default = move |_| {
        let strategy = value_to_strategy(&apply_default_strategy());
        let sensitive_values = apply_default_sensitive.read().clone();
        if let Err(msg) =
            check_sensitive_filled(&apply_default_fields(), &sensitive_values, strategy)
        {
            toast.error(msg);
            return;
        }
        apply_default_submitting.set(true);
        spawn(async move {
            let req = common::api::ApplyDefaultSeedRequest {
                strategy,
                sensitive_values,
            };
            match apply_default(req).await {
                Ok(resp) => {
                    show_apply_default_modal.set(false);
                    apply_default_submitting.set(false);
                    current_task_kind.set(Some(TaskKind::ApplyDefault));
                    current_task.set(Some(TaskProgressSnapshot {
                        task_id: resp.task_id.clone(),
                        task_type: "seed_apply_default".to_string(),
                        status: TaskStatus::Pending,
                        current_step: 0,
                        total_steps: 0,
                        step_message: "等待开始...".to_string(),
                        started_at: 0,
                        finished_at: None,
                        error: None,
                        result: None,
                    }));
                    start_polling(
                        resp.task_id,
                        TaskKind::ApplyDefault,
                        current_task,
                        current_task_kind,
                        seeds,
                        toast,
                        poll_running.read().clone(),
                    );
                }
                Err(e) => {
                    toast.error(format!("提交应用默认模板任务失败: {}", e));
                    apply_default_submitting.set(false);
                }
            }
        });
    };

    // ===== 查看文件 =====
    let mut on_click_view = move |name: String| {
        view_file_name.set(name.clone());
        view_content.set(String::new());
        show_view_modal.set(true);
        view_loading.set(true);
        spawn(async move {
            match get_seed_file(&name).await {
                Ok(resp) => view_content.set(resp.content),
                Err(e) => {
                    toast.error(format!("读取文件失败: {}", e));
                    show_view_modal.set(false);
                }
            }
            view_loading.set(false);
        });
    };

    // ===== 删除文件 =====
    let mut on_click_delete = move |name: String| {
        delete_file_name.set(name);
        show_delete_modal.set(true);
    };

    let handle_confirm_delete = move |_| {
        let name = delete_file_name();
        if name.is_empty() {
            return;
        }
        delete_loading.set(true);
        let name_for_delete = name.clone();
        spawn(async move {
            match delete_seed_file(&name_for_delete).await {
                Ok(_) => {
                    toast.success("已删除文件");
                    show_delete_modal.set(false);
                    match list_seeds().await {
                        Ok(resp) => seeds.set(resp.data),
                        Err(e) => toast.error(format!("刷新列表失败: {}", e)),
                    }
                }
                Err(e) => toast.error(format!("删除失败: {}", e)),
            }
            delete_loading.set(false);
        });
    };

    // 取消当前任务视图（清除进度展示）
    let on_cancel_current_task = move |_| {
        current_task.set(None);
        current_task_kind.set(None);
    };

    let seeds_list = seeds.read().clone();
    let total_count = seeds_list.len();

    rsx! {
        AppLayout {
            HudPanel { signal: Some(true),
            title: Some("Seed 配置迁移".to_string()),
            actions: Some(rsx!{
                button {
                    class: "btn hud-btn btn-ghost btn-sm",
                    onclick: move |_| reload(loading, seeds, toast),
                    "🔄 刷新"
                }
                button {
                    class: "btn hud-btn btn-outline btn-sm",
                    // 打开弹窗时拉取内置模板快照，解析出需补填的敏感字段
                    onclick: move |_| {
                        apply_default_fields.set(Vec::new());
                        apply_default_sensitive.set(HashMap::new());
                        show_apply_default_modal.set(true);
                        spawn(async move {
                            match get_default_seed().await {
                                Ok(snapshot) => {
                                    apply_default_fields.set(extract_sensitive_fields(&snapshot))
                                }
                                Err(e) => toast.error(format!("读取默认模板失败: {}", e)),
                            }
                        });
                    },
                    "应用默认模板"
                }
                button {
                    class: "btn hud-btn btn-primary btn-sm",
                    onclick: move |_| show_save_modal.set(true),
                    "+ 导出当前配置"
                }
            }),

                // 顶部统计
                StatGrid {
                    StatReadout { label: "快照文件数".to_string(), value: format!("{}", total_count), accent: Some("primary".to_string()) }
                }

                // 主体：当前有任务时显示进度，否则显示列表
                if let Some(p) = &current_task() {
                    TaskProgress {
                        progress: p.clone(),
                        on_cancel: on_cancel_current_task,
                    }
                } else if loading() {
                    Loading {}
                } else if seeds_list.is_empty() {
                    EmptyState { icon: "🌱".to_string(), message: "暂无 seed 文件".to_string() }
                } else {
                    table { class: "table hud-table table-zebra",
                        thead { tr {
                            th { "文件名" }
                            th { "大小" }
                            th { "修改时间" }
                            th { "类型" }
                            th { "操作" }
                        }}
                        tbody {
                            for s in seeds_list.iter() {
                                {
                                    let name = s.name.clone();
                                    let size = format_size(s.size);
                                    let modified = format_datetime_full(s.modified_at);
                                    let is_default = s.is_default;
                                    let name_for_view = s.name.clone();
                                    let name_for_load = s.name.clone();
                                    let name_for_delete = s.name.clone();

                                    rsx! {
                                        tr { key: "{name}",
                                            td { class: "font-mono", "{name}" }
                                            td { "{size}" }
                                            td { class: "font-mono text-base-content/70",
                                                style: "white-space: nowrap;",
                                                "{modified}"
                                            }
                                            td {
                                                if is_default {
                                                    span { class: "badge orz-tag badge-sm", "默认" }
                                                } else {
                                                    span { class: "badge orz-tag badge-sm", "自定义" }
                                                }
                                            }
                                            td { class: "flex gap-2",
                                                button {
                                                    class: "btn hud-btn btn-outline btn-xs",
                                                    onclick: move |_| on_click_view(name_for_view.clone()),
                                                    "查看"
                                                }
                                                button {
                                                    class: "btn hud-btn btn-primary btn-xs",
                                                    // 打开弹窗时读取该快照文件，解析出需补填的敏感字段。
                                                    // 这里不抽公共闭包：rsx 的 for 循环内每个迭代都会
                                                    // 独占捕获一份 `name_for_load`，复用外部闭包会被 move 冲突。
                                                    onclick: move |_| {
                                                        let file = name_for_load.clone();
                                                        load_file_name.set(file.clone());
                                                        load_strategy.set("0".to_string());
                                                        load_fields.set(Vec::new());
                                                        load_sensitive.set(HashMap::new());
                                                        show_load_modal.set(true);
                                                        spawn(async move {
                                                            match get_seed_file(&file).await {
                                                                Ok(resp) => match parse_sensitive_fields(
                                                                    &resp.content,
                                                                ) {
                                                                    Ok(fields) => {
                                                                        load_fields.set(fields)
                                                                    }
                                                                    Err(msg) => toast.error(msg),
                                                                },
                                                                Err(e) => toast
                                                                    .error(format!("读取快照失败: {}", e)),
                                                            }
                                                        });
                                                    },
                                                    "加载"
                                                }
                                                button {
                                                    class: "btn hud-btn btn-error btn-xs",
                                                    onclick: move |_| on_click_delete(name_for_delete.clone()),
                                                    "删除"
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

        // 保存（导出）弹窗
        Modal {
            title: "导出当前配置".to_string(),
            show: show_save_modal(),
            on_close: move |_| show_save_modal.set(false),
            footer: rsx! {
                button {
                    class: "btn hud-btn btn-ghost",
                    disabled: save_submitting(),
                    onclick: move |_| show_save_modal.set(false),
                    "取消"
                }
                button {
                    class: "btn hud-btn btn-primary",
                    disabled: save_submitting(),
                    onclick: on_submit_save,
                    if save_submitting() { "提交中..." } else { "确认导出" }
                }
            },
            div { class: "form-control w-full py-2",
                label { class: "form-label", "文件名 *" }
                input {
                    class: "input input-bordered w-full",
                    r#type: "text",
                    value: "{save_name}",
                    oninput: move |e| save_name.set(e.value()),
                    placeholder: "例如：my_org_snapshot（无需 .json 后缀）",
                }
            }
            div { class: "form-control w-full py-2",
                label { class: "form-label", "描述" }
                textarea {
                    class: "textarea textarea-bordered w-full",
                    value: "{save_description}",
                    oninput: move |e| save_description.set(e.value()),
                    placeholder: "快照描述（可选）",
                }
            }
        }

        // 加载（导入）弹窗
        Modal {
            title: format!("加载快照 - {}", load_file_name()),
            show: show_load_modal(),
            on_close: move |_| show_load_modal.set(false),
            footer: rsx! {
                button {
                    class: "btn hud-btn btn-ghost",
                    disabled: load_submitting(),
                    onclick: move |_| show_load_modal.set(false),
                    "取消"
                }
                button {
                    class: "btn hud-btn btn-primary",
                    disabled: load_submitting(),
                    onclick: on_submit_load,
                    if load_submitting() { "提交中..." } else { "确认加载" }
                }
            },
            div { class: "form-control w-full py-2",
                label { class: "form-label", "导入策略" }
                select {
                    class: "select select-bordered w-full",
                    value: "{load_strategy}",
                    onchange: move |e| load_strategy.set(e.value()),
                    for (label, s) in strategy_options() {
                        option { value: "{strategy_to_value(s)}", "{label}" }
                    }
                }
                p { class: "text-base-content/60 text-xs mt-1",
                    "DryRun 模式仅预演不写入；SkipExisting 跳过已存在的实体。"
                }
            }
            SeedSensitiveFields {
                fields: load_fields(),
                required: !matches!(value_to_strategy(&load_strategy()), ImportStrategy::DryRun),
                on_change: on_load_sensitive_change,
            }
        }

        // 应用默认模板弹窗
        Modal {
            title: "应用默认模板".to_string(),
            show: show_apply_default_modal(),
            on_close: move |_| show_apply_default_modal.set(false),
            footer: rsx! {
                button {
                    class: "btn hud-btn btn-ghost",
                    disabled: apply_default_submitting(),
                    onclick: move |_| show_apply_default_modal.set(false),
                    "取消"
                }
                button {
                    class: "btn hud-btn btn-primary",
                    disabled: apply_default_submitting(),
                    onclick: on_submit_apply_default,
                    if apply_default_submitting() { "提交中..." } else { "确认应用" }
                }
            },
            div { class: "form-control w-full py-2",
                label { class: "form-label", "导入策略" }
                select {
                    class: "select select-bordered w-full",
                    value: "{apply_default_strategy}",
                    onchange: move |e| apply_default_strategy.set(e.value()),
                    for (label, s) in strategy_options() {
                        option { value: "{strategy_to_value(s)}", "{label}" }
                    }
                }
                p { class: "text-base-content/60 text-xs mt-1",
                    "默认模板包含系统预置的 Provider/Agent/Skill 配置。"
                }
            }
            SeedSensitiveFields {
                fields: apply_default_fields(),
                required: !matches!(
                    value_to_strategy(&apply_default_strategy()),
                    ImportStrategy::DryRun
                ),
                on_change: on_apply_default_sensitive_change,
            }
        }

        // 查看文件内容弹窗
        Modal {
            title: format!("文件内容 - {}", view_file_name()),
            show: show_view_modal(),
            on_close: move |_| show_view_modal.set(false),
            footer: rsx! {
                button {
                    class: "btn hud-btn btn-ghost",
                    onclick: move |_| show_view_modal.set(false),
                    "关闭"
                }
            },
            if view_loading() {
                Loading {}
            } else {
                pre {
                    class: "font-mono",
                    style: "white-space: pre-wrap; word-break: break-word; background: var(--color-mistral-black, oklch(0.2 0 0)); color: var(--color-text-on-dark, #eee); padding: 1rem; border-radius: 0.375rem; max-height: 60vh; overflow: auto; font-size: 12px; line-height: 1.5;",
                    "{view_content()}"
                }
            }
        }

        // 删除确认弹窗
        ConfirmDialog {
            show: show_delete_modal(),
            title: "确认删除文件".to_string(),
            message: format!("即将删除快照文件 {}，此操作不可恢复。", delete_file_name()),
            confirm_text: "确认删除".to_string(),
            on_confirm: handle_confirm_delete,
            on_cancel: move |_| show_delete_modal.set(false),
        }
    }
}
