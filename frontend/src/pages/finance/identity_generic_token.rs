//! 身份凭证 通用 API Token 区块（Finance → Identity 子组件）
//!
//! 统一管理所有「单字段 API Key」类平台凭证（tavily / doubao_search / 自定义 platform）：
//! 「全部」Tab 跨平台全量展示（含自定义 platform 凭证，前端本地平台筛选 +
//! platform 徽章点击联动），固定平台 Tab（Tavily / 豆包搜索）按 platform 隔离
//! 并保留平台专属录入提示（HudCallout）；
//! 凭证 token 永不回显（仅尾号 4 位），支持增删改 + 设默认；
//! Agent 经对应工具（tavily_search / doubao_search / 自定义 platform 工具）
//! 以 (GenericToken, platform) 二元匹配解析到该凭证。
//!
//! 数据来源 = `GET /api/v1/finance/identity/generic-token/status`（platform 可选：
//! 固定 Tab 传平台 slug，「全部」Tab 传空串取全量；ALL_SLUG 仅为前端视图标识，永不落库）。
//! 原自定义凭证区块（identity_custom_credentials.rs）已合并入「全部」Tab，防重复维护。

use crate::components::hud::HudCallout;
use dioxus::prelude::*;

use crate::api::generic_token_integration::{
    create_generic_token_credential, delete_generic_token_credential, get_generic_token_status,
    set_default_generic_token_credential, update_generic_token_credential,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::{
    CreateGenericTokenCredentialRequest, GenericTokenCredentialSnapshot,
    GenericTokenIntegrationStatusResponse, UpdateGenericTokenCredentialRequest,
};

/// 「全部」Tab 视图标识（纯前端 Tab slug，非落库 platform 值；
/// 对应后端查询语义 = platform 空串全量，见 [`status_query_platform`]）
const ALL_SLUG: &str = "all";

/// 内置平台清单（顺序即固定 Tab 顺序；label 展示，value 落库/匹配）
struct PlatformMeta {
    slug: &'static str,
    caption: &'static str,
    hint: &'static str,
    placeholder: &'static str,
}

const PLATFORMS: &[PlatformMeta] = &[
    PlatformMeta {
        slug: "tavily",
        caption: "Tavily",
        hint: "在 app.tavily.com 注册后于 API Keys 页面生成；免费档每月有搜索配额。",
        placeholder: "tvly-xxx，加密存储，永不回显",
    },
    PlatformMeta {
        slug: "doubao_search",
        caption: "豆包搜索",
        hint: "火山引擎豆包搜索 API Key；默认限流 5 QPS，免费额度 500 次/月。",
        placeholder: "豆包搜索 API Key，加密存储，永不回显",
    },
];

/// Tab slug → 后端 status 查询 platform 值：
/// 「全部」Tab（ALL_SLUG）→ 空串（后端 platform 可选化全量语义）；
/// 固定 Tab → 平台 slug 原样。缓存键与该查询值一一对应。
fn status_query_platform(tab: &str) -> &str {
    if tab == ALL_SLUG { "" } else { tab }
}

/// 「全部」Tab 平台筛选（纯函数，供内联测试直测）：
/// filter 空串 = 全量返回；否则按 platform 精确匹配（非前缀/子串匹配）。
fn filter_credentials_by_platform<'a>(
    credentials: &'a [GenericTokenCredentialSnapshot],
    filter: &str,
) -> Vec<&'a GenericTokenCredentialSnapshot> {
    if filter.is_empty() {
        credentials.iter().collect()
    } else {
        credentials
            .iter()
            .filter(|c| c.platform == filter)
            .collect()
    }
}

/// 添加 Modal 双模式判定（纯函数，供内联测试直测）：
/// 「全部」Tab → platform 自由输入（true）；固定 Tab → platform 锁定预填充（false）。
fn modal_platform_editable(active_platform: &str) -> bool {
    active_platform == ALL_SLUG
}

/// 通用 API Token 凭证子区块（嵌入 FinanceIdentity 页面区块序列末尾 = 页面最下方）
#[component]
pub fn IdentityGenericTokenSection() -> Element {
    let toast = use_toast();

    // ===== 当前选中 Tab（ALL_SLUG = 全部，默认激活；固定 Tab = 平台 slug）=====
    let mut active_platform = use_signal(|| ALL_SLUG.to_string());

    // ===== 每个 platform 的状态缓存（键 = 后端查询值："" 全量 / 平台 slug）=====
    let mut status_map =
        use_signal(std::collections::HashMap::<String, GenericTokenIntegrationStatusResponse>::new);
    let mut loading_map = use_signal(std::collections::HashMap::<String, bool>::new);

    // ===== 「全部」Tab 平台筛选（空串 = 全部；前端本地过滤，零额外请求）=====
    let mut filter_platform = use_signal(String::new);

    // ===== 录入凭证（单 Modal 双模式：全部 Tab platform 自由输入 / 固定 Tab 锁定）=====
    let mut show_create_modal = use_signal(|| false);
    let mut new_platform = use_signal(String::new);
    let mut new_name = use_signal(String::new);
    let mut new_token = use_signal(String::new);
    let mut creating = use_signal(|| false);

    // ===== 编辑凭证 =====
    let mut show_edit_modal = use_signal(|| false);
    let mut edit_id = use_signal(String::new);
    let mut edit_platform = use_signal(String::new);
    let mut edit_name = use_signal(String::new);
    let mut edit_token = use_signal(String::new);
    let mut saving = use_signal(|| false);

    // ===== 删除凭证（记录待删凭证所属平台，供操作后精准刷新，避免误用当前 Tab 键）=====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);
    let mut pending_delete_platform = use_signal(String::new);

    let refresh_platform = move |platform: String| {
        spawn(async move {
            loading_map.write().insert(platform.clone(), true);
            match get_generic_token_status(&platform).await {
                Ok(s) => {
                    status_map.write().insert(platform.clone(), s);
                }
                Err(e) => toast.error(format!("加载凭证状态失败 ({}): {}", platform, e)),
            }
            loading_map.write().remove(&platform);
        });
    };

    // 首次加载：全量（「全部」Tab 默认视图）+ 所有固定平台
    {
        let refresh = refresh_platform;
        use_effect(move || {
            refresh(String::new());
            for p in PLATFORMS {
                refresh(p.slug.to_string());
            }
        });
    }

    // ===== 写操作成功后的统一刷新策略：
    // 受影响平台缓存重拉 + 全量缓存失效（移除 "" 键，防切 Tab 显示旧数据）；
    // 当前视图若为「全部」则立即重拉全量表格，固定 Tab 则重拉本平台 =====
    let mut after_write_refresh = move |affected_platform: String| {
        status_map.write().remove("");
        status_map.write().remove(&affected_platform);
        if active_platform() == ALL_SLUG {
            refresh_platform(String::new());
        } else {
            refresh_platform(affected_platform);
        }
    };

    // ===== 录入提交（platform 必填 trim 非空校验；固定 Tab platform 即当前 Tab slug）=====
    let handle_create = move |_| {
        spawn(async move {
            let name = new_name();
            let token = new_token();
            let platform = if modal_platform_editable(&active_platform()) {
                new_platform().trim().to_string()
            } else {
                active_platform()
            };
            if platform.is_empty() {
                toast.error("平台标识不能为空（保存后作为匹配键，不可修改）");
                return;
            }
            if name.trim().is_empty() || token.trim().is_empty() {
                toast.error("名称 / API Token 均为必填");
                return;
            }
            creating.set(true);
            let req = CreateGenericTokenCredentialRequest {
                name,
                platform: platform.clone(),
                api_token: token,
            };
            match create_generic_token_credential(req).await {
                Ok(_) => {
                    show_create_modal.set(false);
                    new_platform.set(String::new());
                    new_name.set(String::new());
                    new_token.set(String::new());
                    toast.success("凭证绑定成功");
                    after_write_refresh(platform);
                }
                Err(e) => toast.error(format!("绑定失败: {}", e)),
            }
            creating.set(false);
        });
    };

    // ===== 编辑提交（platform 锁定不可改；名称留空保持不变；token 留空保留原值，填写则轮换）=====
    let handle_save = move |_| {
        spawn(async move {
            let id = edit_id();
            let platform = edit_platform();
            saving.set(true);
            let opt = |v: String| if v.trim().is_empty() { None } else { Some(v) };
            let req = UpdateGenericTokenCredentialRequest {
                id: id.clone(),
                name: opt(edit_name()),
                api_token: opt(edit_token()),
            };
            match update_generic_token_credential(req).await {
                Ok(_) => {
                    show_edit_modal.set(false);
                    edit_token.set(String::new());
                    toast.success("凭证已更新，下次工具调用即生效");
                    after_write_refresh(platform);
                }
                Err(e) => toast.error(format!("更新失败: {}", e)),
            }
            saving.set(false);
        });
    };

    // ===== 设为默认 =====
    let handle_set_default = move |platform: String, credential_id: String| {
        spawn(async move {
            match set_default_generic_token_credential(&platform, &credential_id).await {
                Ok(_) => {
                    toast.success("默认凭证已更新");
                    after_write_refresh(platform);
                }
                Err(e) => toast.error(format!("设置默认凭证失败: {}", e)),
            }
        });
    };

    // ===== 删除 =====
    let handle_delete = move |_| {
        let id = pending_delete_id();
        let platform = pending_delete_platform();
        show_delete_confirm.set(false);
        spawn(async move {
            match delete_generic_token_credential(&id).await {
                Ok(_) => {
                    toast.success("凭证已删除");
                    after_write_refresh(platform);
                }
                Err(e) => toast.error(format!("删除失败: {}", e)),
            }
        });
    };

    // ===== 当前视图数据面（缓存键 = 后端查询值）=====
    let active_tab = active_platform();
    let active_is_all = active_tab == ALL_SLUG;
    let fixed_meta = PLATFORMS
        .iter()
        .find(|p| p.slug == active_tab)
        .unwrap_or(&PLATFORMS[0]);
    let query_key = status_query_platform(&active_tab).to_string();
    let snapshot = status_map.read().get(&query_key).cloned();
    let is_loading = loading_map.read().get(&query_key).copied().unwrap_or(false);
    let view_credentials = snapshot
        .as_ref()
        .map(|s| s.credentials.clone())
        .unwrap_or_default();

    // 「全部」Tab 筛选面：本地过滤 + 筛选下拉选项（从当前全量数据 distinct 动态生成）
    let filtered = filter_credentials_by_platform(&view_credentials, &filter_platform());
    let mut filter_options: Vec<String> = view_credentials
        .iter()
        .map(|c| c.platform.clone())
        .collect();
    filter_options.sort();
    filter_options.dedup();

    // 录入 Modal 标题（固定 Tab 沿原「绑定 {平台} Token」；「全部」Tab = 添加凭证）
    let create_modal_title = if active_is_all {
        "添加凭证".to_string()
    } else {
        format!("绑定 {} Token", fixed_meta.caption)
    };
    // 录入 Modal token 占位文案（固定 Tab 沿原平台专属 placeholder）
    let token_placeholder = if active_is_all {
        "加密存储，永不回显"
    } else {
        fixed_meta.placeholder
    };

    rsx! {
        div { class: "border border-base-300 rounded-lg p-4 mt-4",
            div { class: "flex items-center gap-2 flex-wrap",
                h3 { class: "font-semibold text-lg", "通用 API Token" }
                span { class: "badge orz-tag badge-sm", "GenericToken" }
            }
            p { class: "text-xs text-base-content/50 mt-1",
                "统一管理单字段 API Key 类平台凭证：「全部」Tab 跨平台全量（含自定义 platform，支持本地平台筛选），固定 Tab 按平台隔离；未绑定对应平台凭证时工具调用返回绑定引导。"
            }

            // ===== 平台 Tab（「全部」置序列首位且默认激活）=====
            div { class: "flex flex-wrap gap-2 mt-3",
                role: "tablist",
                {
                    let plat = ALL_SLUG.to_string();
                    let active = active_platform() == ALL_SLUG;
                    rsx! {
                        button {
                            key: "{plat}",
                            class: if active { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" },
                            onclick: move |_| {
                                active_platform.set(ALL_SLUG.to_string());
                                if !status_map.read().contains_key("") {
                                    refresh_platform(String::new());
                                }
                            },
                            "全部"
                        }
                    }
                }
                for meta in PLATFORMS.iter() {
                    {
                        let plat = meta.slug.to_string();
                        let caption = meta.caption;
                        let active = plat == active_platform();
                        rsx! {
                            button {
                                key: "{plat}",
                                class: if active { "btn hud-btn btn-sm btn-primary" } else { "btn hud-btn btn-sm btn-ghost" },
                                onclick: move |_| {
                                    let target = plat.clone();
                                    active_platform.set(target.clone());
                                    if !status_map.read().contains_key(&target) {
                                        refresh_platform(target);
                                    }
                                },
                                "{caption}"
                            }
                        }
                    }
                }
            }

            // ===== 当前 Tab 面板 =====
            if is_loading && snapshot.is_none() {
                div { class: "text-base-content/50 text-sm py-4", "加载中..." }
            } else if active_is_all {
                // ==================== 「全部」Tab：跨平台全量表格（原自定义凭证区块并入） ====================
                div { class: "border border-base-300 rounded-lg p-4 mt-3",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "全部凭证" }
                        button {
                            class: "btn hud-btn btn-sm btn-primary",
                            onclick: move |_| {
                                new_platform.set(String::new());
                                new_name.set(String::new());
                                new_token.set(String::new());
                                show_create_modal.set(true);
                            },
                            "+ 添加凭证"
                        }
                    }

                    // ===== 平台筛选（前端本地过滤零额外请求；platform 徽章点击联动选中）=====
                    div { class: "flex items-center gap-2 mt-3 flex-wrap",
                        label { class: "text-xs text-base-content/60", "平台筛选" }
                        select {
                            class: "select select-bordered select-sm w-44",
                            value: "{filter_platform}",
                            onchange: move |e| filter_platform.set(e.value()),
                            option { value: "", "全部" }
                            for opt in filter_options.iter() {
                                option { key: "{opt}", value: "{opt}", "{opt}" }
                            }
                        }
                        if !filter_platform().is_empty() {
                            button {
                                class: "btn hud-btn btn-ghost btn-xs",
                                onclick: move |_| filter_platform.set(String::new()),
                                "清除筛选"
                            }
                        }
                    }

                    p { class: "text-xs text-base-content/50 mt-2",
                        "跨平台全量视图：固定平台与自定义 platform 凭证均在此展示；platform 值即匹配键（保存后不可修改）。"
                    }

                    if view_credentials.is_empty() {
                        div { class: "text-sm text-base-content/50 py-3",
                            "暂无凭证（点右上「+ 添加凭证」以自定义 platform 绑定）"
                        }
                    } else if filtered.is_empty() {
                        div { class: "text-sm text-base-content/50 py-3", "该筛选条件下暂无凭证" }
                    } else {
                        div { class: "overflow-x-auto mt-2",
                            table { class: "table table-sm",
                                thead { tr {
                                    th { "平台" }
                                    th { "名称" }
                                    th { "Token" }
                                    th { "默认" }
                                    th { "操作" }
                                }}
                                tbody {
                                    for cred in filtered.iter().copied() {
                                        {
                                            let credential_id = cred.credential_id.clone();
                                            let cred_name = cred.name.clone();
                                            let platform = cred.platform.clone();
                                            let token_tail = cred.api_token_tail.clone();
                                            let is_default = cred.is_default;
                                            let id_for_edit = credential_id.clone();
                                            let id_for_delete = credential_id.clone();
                                            let id_for_default = credential_id.clone();
                                            let plat_for_default = platform.clone();
                                            let plat_for_filter = platform.clone();
                                            let plat_for_edit = platform.clone();
                                            rsx! {
                                                tr { key: "{credential_id}",
                                                    td {
                                                        span {
                                                            class: "badge orz-tag badge-sm font-mono cursor-pointer",
                                                            title: "点击筛选该平台",
                                                            onclick: move |_| filter_platform.set(plat_for_filter.clone()),
                                                            "{platform}"
                                                        }
                                                    }
                                                    td { class: "font-medium", "{cred_name}" }
                                                    td {
                                                        if !token_tail.is_empty() {
                                                            span { class: "badge orz-tag badge-sm font-mono", "****{token_tail}" }
                                                        }
                                                    }
                                                    td {
                                                        if is_default {
                                                            span { class: "badge hud-badge badge-success badge-sm", "默认" }
                                                        } else {
                                                            button {
                                                                class: "btn hud-btn btn-ghost btn-xs",
                                                                onclick: move |_| handle_set_default(plat_for_default.clone(), id_for_default.clone()),
                                                                "设为默认"
                                                            }
                                                        }
                                                    }
                                                    td { class: "flex gap-2",
                                                        button {
                                                            class: "btn hud-btn btn-ghost btn-xs",
                                                            onclick: move |_| {
                                                                edit_id.set(id_for_edit.clone());
                                                                edit_platform.set(plat_for_edit.clone());
                                                                edit_name.set(String::new());
                                                                edit_token.set(String::new());
                                                                show_edit_modal.set(true);
                                                            },
                                                            "编辑"
                                                        }
                                                        button {
                                                            class: "btn hud-btn btn-ghost btn-xs text-error",
                                                            onclick: move |_| {
                                                                pending_delete_id.set(id_for_delete.clone());
                                                                pending_delete_platform.set(platform.clone());
                                                                show_delete_confirm.set(true);
                                                            },
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
            } else {
                // ==================== 固定平台 Tab（原有面板，平台专属 hint 沿原） ====================
                div { class: "border border-base-300 rounded-lg p-4 mt-3",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "{fixed_meta.caption} API Token" }
                        button {
                            class: "btn hud-btn btn-sm btn-primary",
                            onclick: move |_| {
                                new_name.set(String::new());
                                new_token.set(String::new());
                                show_create_modal.set(true);
                            },
                            "+ 绑定 Token"
                        }
                    }
                    if view_credentials.is_empty() {
                        div { class: "text-sm text-base-content/50 py-3",
                            "尚未绑定 {fixed_meta.caption} token"
                        }
                    } else {
                        div { class: "space-y-3 mt-3",
                            for cred in view_credentials.iter() {
                                {
                                    let credential_id = cred.credential_id.clone();
                                    let cred_name = cred.name.clone();
                                    let platform = cred.platform.clone();
                                    let token_tail = cred.api_token_tail.clone();
                                    let is_default = cred.is_default;
                                    let id_for_edit = credential_id.clone();
                                    let id_for_delete = credential_id.clone();
                                    let id_for_default = credential_id.clone();
                                    let plat_for_default = platform.clone();
                                    let plat_for_edit = platform.clone();
                                    rsx! {
                                        div { key: "{credential_id}", class: "border border-base-200 rounded p-3",
                                            div { class: "flex items-center justify-between flex-wrap gap-2",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "font-medium", "{cred_name}" }
                                                    span { class: "badge orz-tag badge-sm font-mono", "{platform}" }
                                                    if !token_tail.is_empty() {
                                                        span { class: "badge orz-tag badge-sm font-mono", "****{token_tail}" }
                                                    }
                                                    if is_default {
                                                        span { class: "badge hud-badge badge-success badge-sm", "默认" }
                                                    }
                                                }
                                                div { class: "flex gap-2",
                                                    if !is_default {
                                                        button {
                                                            class: "btn hud-btn btn-ghost btn-xs",
                                                            onclick: move |_| handle_set_default(plat_for_default.clone(), id_for_default.clone()),
                                                            "设为默认"
                                                        }
                                                    }
                                                    button {
                                                        class: "btn hud-btn btn-ghost btn-xs",
                                                        onclick: move |_| {
                                                            edit_id.set(id_for_edit.clone());
                                                            edit_platform.set(plat_for_edit.clone());
                                                            edit_name.set(String::new());
                                                            edit_token.set(String::new());
                                                            show_edit_modal.set(true);
                                                        },
                                                        "编辑"
                                                    }
                                                    button {
                                                        class: "btn hud-btn btn-ghost btn-xs text-error",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(id_for_delete.clone());
                                                            pending_delete_platform.set(platform.clone());
                                                            show_delete_confirm.set(true);
                                                        },
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

                    // ===== 平台提示（固定 Tab 独有能力，沿原）=====
                    HudCallout { tone: Some("info".to_string()), extra_class: Some("mt-3".to_string()),
                        span { "{fixed_meta.hint}" }
                    }
                }
            }
        }

        // ===== 绑定/添加 Token Modal（单 Modal 双模式：全部 Tab platform 自由输入；固定 Tab 锁定）=====
        Modal {
            title: create_modal_title,
            show: show_create_modal(),
            on_close: move |_| show_create_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_create_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "绑定中..." } else { "绑定" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称 *" } }
                    input {
                        class: "input input-bordered hud-input w-full",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                        placeholder: "如：个人号"
                    }
                }
                // ===== 平台字段：双模式判定 = 纯函数 modal_platform_editable =====
                if modal_platform_editable(&active_platform()) {
                    div { class: "form-control w-full",
                        label { class: "label", span { class: "label-text font-medium", "平台标识 *" } }
                        input {
                            class: "input input-bordered hud-input w-full font-mono",
                            value: "{new_platform}",
                            oninput: move |e| new_platform.set(e.value()),
                            placeholder: "如 ledger / linear（保存后作为匹配键，不可修改）"
                        }
                        p { class: "text-xs text-base-content/50 mt-1",
                            "platform 值即匹配键：工具凭据需求（credential_requirements）声明中的 platform 与此值精确匹配。保存后不可修改，如需变更请删除后重新创建（删旧建新）。"
                        }
                    }
                } else {
                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "平台" }
                            span { class: "label-text-alt font-mono", "{fixed_meta.slug}" }
                        }
                        input {
                            class: "input input-bordered hud-input w-full",
                            value: "{fixed_meta.caption}",
                            disabled: "true",
                        }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "API Token *" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        r#type: "password",
                        value: "{new_token}",
                        oninput: move |e| new_token.set(e.value()),
                        placeholder: token_placeholder
                    }
                }
            }
        }

        // ===== 编辑凭证 Modal（platform 统一锁定不可改 + 删旧建新警示，沿原）=====
        Modal {
            title: "编辑通用 Token 凭证".to_string(),
            show: show_edit_modal(),
            on_close: move |_| show_edit_modal.set(false),
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                button { class: "btn hud-btn btn-primary", disabled: saving(), onclick: handle_save,
                    if saving() { "保存中..." } else { "保存" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "平台标识" }
                        span { class: "label-text-alt font-mono", "{edit_platform}" }
                    }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        value: "{edit_platform}",
                        disabled: "true",
                    }
                    label { class: "label",
                        span { class: "label-text-alt text-warning",
                            "platform 为匹配键不可修改；如需变更请删除后重新创建（删旧建新）" }
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称" } }
                    input {
                        class: "input input-bordered hud-input w-full",
                        value: "{edit_name}",
                        oninput: move |e| edit_name.set(e.value()),
                        placeholder: "留空保持不变"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "API Token" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        r#type: "password",
                        value: "{edit_token}",
                        oninput: move |e| edit_token.set(e.value()),
                        placeholder: "留空保留原值，填写则轮换"
                    }
                }
            }
        }

        ConfirmDialog {
            show: show_delete_confirm(),
            title: "确认删除凭证".to_string(),
            message: "删除后 Agent 将无法以此身份调用对应工具，确定删除？".to_string(),
            on_confirm: handle_delete,
            on_cancel: move |_| show_delete_confirm.set(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: &str, name: &str, platform: &str) -> GenericTokenCredentialSnapshot {
        GenericTokenCredentialSnapshot {
            credential_id: id.to_string(),
            name: name.to_string(),
            platform: platform.to_string(),
            api_token_tail: "abcd".to_string(),
            is_default: false,
        }
    }

    #[test]
    fn test_platform_filter_empty_returns_all() {
        // 筛选空串 = 全量返回
        let rows = vec![snap("c1", "n1", "tavily"), snap("c2", "n2", "ledger")];
        let out = filter_credentials_by_platform(&rows, "");
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_platform_filter_matches_slug_exact() {
        // 精确匹配：同前缀但非全等的 platform 不命中
        let rows = vec![
            snap("c1", "n1", "tavily"),
            snap("c2", "n2", "tavily_search"),
            snap("c3", "n3", "ledger"),
        ];
        let out = filter_credentials_by_platform(&rows, "tavily");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].credential_id, "c1");
    }

    #[test]
    fn test_platform_filter_no_match_returns_empty() {
        // 无匹配 = 空集
        let rows = vec![snap("c1", "n1", "tavily")];
        assert!(filter_credentials_by_platform(&rows, "linear").is_empty());
    }

    #[test]
    fn test_create_modal_platform_mode_dual() {
        // Modal 双模式判定：「全部」Tab platform 可编辑；固定 Tab platform 锁定
        assert!(modal_platform_editable(ALL_SLUG));
        assert!(!modal_platform_editable("tavily"));
        assert!(!modal_platform_editable("doubao_search"));
    }

    #[test]
    fn test_all_tab_slug_constant_semantics() {
        // ALL slug 为纯前端视图标识：不与任何落库 platform 重名；
        // 对应后端查询语义 = 空串全量（platform 可选化），固定 Tab 原样传 slug
        assert_eq!(ALL_SLUG, "all");
        assert_eq!(status_query_platform(ALL_SLUG), "");
        assert!(!PLATFORMS.iter().any(|p| p.slug == ALL_SLUG));
        assert_eq!(status_query_platform("tavily"), "tavily");
    }
}
