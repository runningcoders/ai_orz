//! 组织信息管理
//!
//! 组织基本信息（名称/描述）与组织级配置（如消息向量索引开关）统一在此维护，
//! 便于管理员在同一页面完成组织相关设置。
//!
//! 交互遵循后台管理页惯例：默认查看态（字段只读），点击「编辑」进入编辑态，
//! 编辑态下字段可改、组织级配置开关可切换，并显示「保存 / 取消」。仅组织管理员
//! 可见「编辑」入口；其余成员始终只读。

use dioxus::prelude::*;

use common::api::{
    OrganizationConfig, TaskProgressSnapshot, TaskStatus, UpdateCurrentOrganizationRequest,
};
use common::enums::UserRole;

use crate::api::background_task::get_task_progress;
use crate::api::organization::{
    get_current_organization, get_invite_code, regenerate_invite_code, update_current_organization,
};
use crate::api::system::rebuild_vectors;
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudCallout, HudPanel};
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::auth::use_auth_state;
use crate::store::toast::use_toast;
use crate::utils::status::config_dimension_badge;

#[component]
pub fn OrganizationInfo() -> Element {
    let mut loading = use_signal(|| true);
    // 工作副本（编辑态可改）
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut org_id = use_signal(String::new);
    let mut org_config = use_signal(OrganizationConfig::default);
    // 已落库副本（取消编辑时回滚用）
    let mut saved_name = use_signal(String::new);
    let mut saved_description = use_signal(String::new);
    let mut saved_config = use_signal(OrganizationConfig::default);
    // 编辑态开关
    let mut editing = use_signal(|| false);
    let mut saving = use_signal(|| false);
    let toast = use_toast();
    // 组织级配置（含消息向量索引）仅组织管理员可改；其余成员只读。
    // 依赖 AuthState 正确回填（见 use_require_auth 的 identity 回填修复）。
    let auth = use_auth_state();
    // 组织级配置权限判定走 common 的层级权限方法：满足 Admin 最低权限即可
    // （SuperAdmin 或 Admin），与后端 update_current_organization 门控保持一致。
    let can_edit = UserRole::has_permission(UserRole::from_i32(auth().role), UserRole::Admin);
    // 向量库维护是平台级高危操作（全量重打 Embedding），仅 SuperAdmin 可见
    let is_super_admin =
        UserRole::has_permission(UserRole::from_i32(auth().role), UserRole::SuperAdmin);

    // ===== 邀请码（仅 Admin 及以上加载/可见）=====
    // None = 尚未加载或加载失败；Some(code) = 当前有效邀请码（后端懒签发）
    let mut invite_code = use_signal(|| Option::<String>::None);
    let mut invite_loading = use_signal(|| false);
    let mut show_invite_confirm = use_signal(|| false);
    let mut regenerating = use_signal(|| false);

    let handle_regenerate = move |_| {
        show_invite_confirm.set(false);
        spawn(async move {
            regenerating.set(true);
            match regenerate_invite_code().await {
                Ok(resp) => {
                    invite_code.set(Some(resp.invite_code));
                    toast.success("已生成新邀请码，旧码已失效");
                }
                Err(e) => toast.error(format!("重新生成失败: {}", e)),
            }
            regenerating.set(false);
        });
    };

    // ===== 向量库维护状态 =====
    let mut show_rebuild_confirm = use_signal(|| false);
    // Some(task_id) = 有任务在跑（轮询中）；None = 空闲
    let mut rebuild_task_id = use_signal(|| Option::<String>::None);
    // 最近一次进度快照（终态后保留展示，新任务启动时覆盖）
    let mut rebuild_snapshot = use_signal(|| Option::<TaskProgressSnapshot>::None);

    let handle_rebuild = move |_| {
        show_rebuild_confirm.set(false);
        spawn(async move {
            match rebuild_vectors().await {
                Ok(resp) => {
                    toast.success("重建任务已启动");
                    rebuild_task_id.set(Some(resp.task_id));
                    // 轮询进度：2s 间隔，到终态（Completed/Failed）后停止；
                    // 连续失败 3 次才放弃（短暂网络抖动不应让用户误以为任务没了）
                    let mut consecutive_errors = 0u32;
                    while let Some(id) = rebuild_task_id() {
                        match get_task_progress(&id).await {
                            Ok(p) => {
                                consecutive_errors = 0;
                                let finished = p.status == TaskStatus::Completed;
                                let failed = p.status == TaskStatus::Failed;
                                let err = p.error.clone();
                                rebuild_snapshot.set(Some(p));
                                if finished {
                                    toast.success("向量索引重建完成");
                                    rebuild_task_id.set(None);
                                    break;
                                }
                                if failed {
                                    toast.error(format!(
                                        "向量索引重建失败：{}",
                                        err.unwrap_or_else(|| "未知错误".to_string())
                                    ));
                                    rebuild_task_id.set(None);
                                    break;
                                }
                            }
                            Err(e) => {
                                consecutive_errors += 1;
                                if consecutive_errors >= 3 {
                                    toast.error(format!("进度查询连续失败，已停止轮询：{}", e));
                                    rebuild_task_id.set(None);
                                    break;
                                }
                            }
                        }
                        gloo_timers::future::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
                Err(e) => toast.error(e),
            }
        });
    };

    use_effect(move || {
        spawn(async move {
            match get_current_organization().await {
                Ok(org) => {
                    let org_name = org.data.name;
                    let org_desc = org.data.description.unwrap_or_default();
                    let org_cfg = org.data.config;
                    name.set(org_name.clone());
                    saved_name.set(org_name);
                    description.set(org_desc.clone());
                    saved_description.set(org_desc);
                    org_id.set(org.data.organization_id);
                    org_config.set(org_cfg.clone());
                    saved_config.set(org_cfg);
                }
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    });

    // 管理员进入页面时加载邀请码（GET 语义为懒签发：后端无码时自动生成并落库）。
    // 普通成员无此面板，不发请求（后端对 Member 返回 403，徒增噪音日志）。
    use_effect(move || {
        if !can_edit {
            return;
        }
        spawn(async move {
            invite_loading.set(true);
            match get_invite_code().await {
                Ok(resp) => invite_code.set(Some(resp.invite_code)),
                Err(e) => toast.error(format!("邀请码加载失败: {}", e)),
            }
            invite_loading.set(false);
        });
    });

    let handle_edit = move |_| {
        // 进入编辑态前先把工作副本同步为已落库值，避免残留上一次半截编辑
        name.set(saved_name());
        description.set(saved_description());
        org_config.set(saved_config());
        editing.set(true);
    };

    let handle_cancel = move |_| {
        // 回滚到已落库值并退出编辑态
        name.set(saved_name());
        description.set(saved_description());
        org_config.set(saved_config());
        editing.set(false);
    };

    let handle_save = move |_| {
        spawn(async move {
            saving.set(true);
            let req = UpdateCurrentOrganizationRequest {
                name: Some(name()),
                description: if description().is_empty() {
                    None
                } else {
                    Some(description())
                },
                base_url: None,
                config: Some(org_config()),
            };
            match update_current_organization(req).await {
                Ok(resp) => {
                    let cfg = resp.data.config;
                    saved_config.set(cfg.clone());
                    org_config.set(cfg);
                    saved_name.set(name());
                    saved_description.set(description());
                    editing.set(false);
                    toast.success("保存成功");
                }
                Err(e) => toast.error(&e),
            }
            saving.set(false);
        });
    };

    // 注册链接：随邀请码一起变化，打开后直达登录页注册 Tab 并自动填入邀请码
    let register_link = match invite_code() {
        Some(code) => {
            let origin = web_sys::window()
                .and_then(|w| w.location().origin().ok())
                .unwrap_or_default();
            format!("{}/login?invite={}", origin.trim_end_matches('/'), code)
        }
        None => String::new(),
    };

    rsx! {
        AppLayout {
        HudPanel { signal: Some(true),
            title: Some("组织信息".to_string()),
            actions: Some(rsx! {
                if can_edit {
                    if editing() {
                        button { class: "btn hud-btn btn-ghost btn-sm", disabled: saving(), onclick: handle_cancel,
                            "取消"
                        }
                        button { class: "btn hud-btn btn-primary btn-sm", disabled: saving(), onclick: handle_save,
                            if saving() { "保存中..." } else { "保存" }
                        }
                    } else {
                        button { class: "btn hud-btn btn-primary btn-sm", onclick: handle_edit,
                            "编辑"
                        }
                    }
                }
            }),
            div { class: "card-body",

                if loading() {
                    Loading {}
                } else {
                    div { class: "space-y-4",
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "组织 ID" }
                            }
                            input { class: "input input-bordered w-full", disabled: true, value: "{org_id}" }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "组织名称" }
                            }
                            input { class: "input input-bordered w-full", value: "{name}",
                                disabled: !editing(),
                                oninput: move |e| name.set(e.value()) }
                        }
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium", "组织描述" }
                            }
                            textarea { class: "textarea textarea-bordered w-full", value: "{description}",
                                disabled: !editing(),
                                oninput: move |e| description.set(e.value()) }
                        }

                        div { class: "hud-divider divider" }

                        // ===== 组织级配置（服务端，跟随当前组织）=====
                        div { class: "form-control w-full",
                            label { class: "label",
                                span { class: "label-text font-medium flex items-center gap-2",
                                    "组织级配置"
                                    span { class: config_dimension_badge("org"), "组织级" }
                                }
                            }
                            div { class: "flex items-center justify-between gap-4",
                                div { class: "flex-1 min-w-0",
                                    div { class: "font-medium", "消息向量索引" }
                                    // 长描述不能用 DaisyUI `.label`（v5 带 white-space:nowrap，会撑破面板）
                                    p { class: "text-sm text-base-content/60 mt-1",
                                        "开启后，普通消息会构建语义向量以支持语义检索；默认关闭以避免无意义的 Embedding 开销。配置仅对当前组织生效。"
                                    }
                                }
                                // 统一 HUD 开关：.hud-switch（状态由 .is-on 控制，禁用态降透明度）
                                div { class: "flex shrink-0 items-center gap-3",
                                    span { class: if org_config().enable_message_vector { "text-sm font-medium text-primary" } else { "text-sm font-medium text-base-content/50" },
                                        if org_config().enable_message_vector { "开启" } else { "关闭" }
                                    }
                                    button {
                                        r#type: "button",
                                        class: "hud-switch",
                                        class: if org_config().enable_message_vector { "is-on" } else { "" },
                                        disabled: !can_edit || !editing(),
                                        onclick: move |_| {
                                            let mut c = org_config.write();
                                            c.enable_message_vector = !c.enable_message_vector;
                                        },
                                        span { class: "hud-switch-thumb" }
                                    }
                                }
                            }
                            div { class: "hud-divider divider" }

                            // ===== Agent 入职配置：本组织要求每个 Agent 会哪些包 =====
                            div { class: "space-y-3",
                                div { class: "font-medium", "Agent 入职包（组织要求）" }
                                // 长描述不能用 DaisyUI `.label`（v5 带 white-space:nowrap，会撑破面板）
                                p { class: "text-sm text-base-content/60 mt-1",
                                    "Agent 入职时自动安装这里配置的包：工具包负责工具授权，技能包负责把技能副本放进 Agent 池子。"
                                    "「同名双重身份」的包（如 project_management）两侧都要配 —— 少配一侧会导致工具被拒或技能不进 Prompt。"
                                }
                                PackTagEditor {
                                    title: "工具包 tags",
                                    tags: org_config().agent_onboard.required_tool_packs,
                                    disabled: !can_edit || !editing(),
                                    on_add: move |tag: String| {
                                        let mut c = org_config.write();
                                        if !c.agent_onboard.required_tool_packs.contains(&tag) {
                                            c.agent_onboard.required_tool_packs.push(tag);
                                        }
                                    },
                                    on_remove: move |tag: String| {
                                        let mut c = org_config.write();
                                        c.agent_onboard.required_tool_packs.retain(|t| t != &tag);
                                    },
                                }
                                PackTagEditor {
                                    title: "技能包 tags",
                                    tags: org_config().agent_onboard.required_skill_packs,
                                    disabled: !can_edit || !editing(),
                                    on_add: move |tag: String| {
                                        let mut c = org_config.write();
                                        if !c.agent_onboard.required_skill_packs.contains(&tag) {
                                            c.agent_onboard.required_skill_packs.push(tag);
                                        }
                                    },
                                    on_remove: move |tag: String| {
                                        let mut c = org_config.write();
                                        c.agent_onboard.required_skill_packs.retain(|t| t != &tag);
                                    },
                                }
                            }

                            if !can_edit {
                                div { class: "mt-2",
                                    HudCallout { tone: Some("warning".to_string()), extra_class: Some("text-sm".to_string()),
                                        "仅组织管理员可修改组织级配置。"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // ===== 邀请注册（仅 Admin 及以上可见）=====
        if can_edit {
            HudPanel { signal: Some(true),
                title: Some("邀请注册".to_string()),
                actions: Some(rsx! {
                    button {
                        class: "btn hud-btn btn-ghost btn-sm",
                        disabled: regenerating() || invite_loading(),
                        onclick: move |_| show_invite_confirm.set(true),
                        if regenerating() { "生成中..." } else { "重新生成" }
                    }
                }),
                div { class: "card-body",
                    p { class: "text-sm text-base-content/60",
                        "把邀请码或注册链接发给要加入的成员：对方在登录页注册后即成为本组织普通成员（Member）。重新生成后旧码立即失效。"
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "邀请码" }
                        }
                        div { class: "flex items-stretch gap-2",
                            input {
                                class: "input input-bordered w-full font-mono",
                                disabled: true,
                                value: if invite_loading() {
                                    "加载中…".to_string()
                                } else {
                                    invite_code().clone().unwrap_or_else(|| "—".to_string())
                                }
                            }
                            button {
                                class: "btn hud-btn btn-ghost shrink-0",
                                disabled: invite_code().is_none(),
                                onclick: move |_| {
                                    if let Some(code) = invite_code() {
                                        copy_to_clipboard(&code, toast);
                                    }
                                },
                                "复制"
                            }
                        }
                    }
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "注册链接" }
                        }
                        div { class: "flex items-stretch gap-2",
                            input {
                                class: "input input-bordered w-full font-mono text-sm",
                                disabled: true,
                                placeholder: "邀请码加载后生成",
                                value: "{register_link}"
                            }
                            button {
                                class: "btn hud-btn btn-ghost shrink-0",
                                disabled: register_link.is_empty(),
                                onclick: move |_| {
                                    // 点击时从 signal 现算，避免把 register_link move 进闭包
                                    // 与同节点 value/disabled 的借用冲突
                                    if let Some(code) = invite_code() {
                                        let origin = web_sys::window()
                                            .and_then(|w| w.location().origin().ok())
                                            .unwrap_or_default();
                                        let link = format!(
                                            "{}/login?invite={}",
                                            origin.trim_end_matches('/'),
                                            code
                                        );
                                        copy_to_clipboard(&link, toast);
                                    }
                                },
                                "复制"
                            }
                        }
                    }
                }
            }

            ConfirmDialog {
                show: show_invite_confirm(),
                title: "确认重新生成邀请码".to_string(),
                message: "新码生成后旧邀请码立即失效，已拿到旧码但尚未注册的人将无法加入本组织。确定继续？".to_string(),
                confirm_class: Some("btn hud-btn btn-primary".to_string()),
                on_confirm: handle_regenerate,
                on_cancel: move |_| show_invite_confirm.set(false),
            }
        }

        // ===== 向量库维护（仅 SuperAdmin 可见）=====
        if is_super_admin {
            HudPanel { signal: Some(true),
                title: Some("向量库维护".to_string()),
                div { class: "card-body",
                    div { class: "space-y-4",
                        div { class: "flex items-start justify-between gap-4",
                            // min-w-0：允许文本列收缩换行，否则把右侧按钮挤出面板
                            div { class: "flex-1 min-w-0",
                                div { class: "font-medium", "重建向量库" }
                                // ⚠️ 长描述不能用 DaisyUI `.label`（v5 带 white-space:nowrap，
                                // 无头实测 720px 列下文本列被撑到 1302px、按钮被推出面板外）
                                p { class: "text-sm text-base-content/60 mt-1",
                                    "对全部 7 类实体（Agent / 记忆 / 技能 / 任务 / 项目 / 消息 / 工具）的语义向量索引做全量重建，分页逐条重新向量化。"
                                    "源数据不受影响，但会全量调用 Embedding 接口（可能产生费用），数据量大时持续数分钟。"
                                }
                            }
                            button {
                                class: "btn hud-btn btn-ghost btn-sm shrink-0",
                                disabled: rebuild_task_id().is_some(),
                                onclick: move |_| show_rebuild_confirm.set(true),
                                if rebuild_task_id().is_some() { "重建中..." } else { "重建向量库" }
                            }
                        }

                        // 进度展示（运行中或终态后保留）
                        if let Some(p) = rebuild_snapshot() {
                            div { class: "space-y-2",
                                div { class: "flex items-center justify-between text-sm",
                                    span { "{p.step_message}" }
                                    span { class: "text-base-content/60",
                                        "{p.current_step} / {p.total_steps}"
                                    }
                                }
                                progress {
                                    class: if p.status == TaskStatus::Failed { "progress progress-error w-full" } else { "progress progress-primary w-full" },
                                    value: "{p.current_step}",
                                    max: "{p.total_steps}",
                                }
                                if p.status == TaskStatus::Failed {
                                    if let Some(err) = &p.error {
                                        div { class: "text-sm text-error", "{err}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            ConfirmDialog {
                show: show_rebuild_confirm(),
                title: "确认重建向量库".to_string(),
                message: "将清空现有向量索引并按页全量重新向量化（源数据不变）。过程中会持续调用 Embedding 接口，可能产生费用并持续数分钟。确定继续？".to_string(),
                confirm_class: Some("btn hud-btn btn-primary".to_string()),
                on_confirm: handle_rebuild,
                on_cancel: move |_| show_rebuild_confirm.set(false),
            }
        }
        }
    }
}

/// 包 tag 编辑器：chips + 回车新增
///
/// 只回调「加/删哪个 tag」，不回调整个列表 —— 避免在闭包里捕获 props 的 `tags`
/// （闭包要 'static，而 `tags` 同时被渲染循环借用）。
#[component]
fn PackTagEditor(
    title: String,
    tags: Vec<String>,
    disabled: bool,
    on_add: Callback<String>,
    on_remove: Callback<String>,
) -> Element {
    let mut input = use_signal(String::new);
    rsx! {
        div { class: "form-control w-full",
            label { class: "label",
                span { class: "label-text", "{title}" }
            }
            div { class: "flex flex-wrap gap-2",
                if tags.is_empty() {
                    span { class: "text-sm text-base-content/50", "未配置" }
                }
                for tag in tags.iter() {
                    span { key: "{tag}", class: "badge orz-tag badge-sm gap-1",
                        "{tag}"
                        if !disabled {
                            button {
                                class: "cursor-pointer",
                                onclick: {
                                    let tag = tag.clone();
                                    move |_| on_remove.call(tag.clone())
                                },
                                "✕"
                            }
                        }
                    }
                }
            }
            input {
                class: "input hud-input w-full",
                disabled: disabled,
                placeholder: "输入包 tag 后回车添加",
                value: "{input}",
                oninput: move |e| input.set(e.value()),
                onkeydown: move |e| {
                    if e.key() == Key::Enter {
                        e.prevent_default();
                        let tag = input().trim().to_string();
                        if tag.is_empty() {
                            return;
                        }
                        on_add.call(tag);
                        input.set(String::new());
                    }
                },
            }
        }
    }
}

/// 复制文本到剪贴板并 toast 反馈（与系统备份页同一实现模式）
fn copy_to_clipboard(content: &str, toast: crate::store::toast::ToastState) {
    if let Some(window) = web_sys::window() {
        let promise = window.navigator().clipboard().write_text(content);
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => toast.success("已复制到剪贴板"),
                Err(_) => toast.error("复制失败"),
            }
        });
    } else {
        toast.error("剪贴板不可用");
    }
}
