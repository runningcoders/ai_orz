//! 身份凭证 通用 API Token 区块（Finance → Identity 子组件）
//!
//! 统一管理所有「单字段 API Key」类平台凭证（tavily / doubao_search / 未来任意平台）：
//! 同一区块内按 platform 分子 Tab，凭证 token 永不回显（仅尾号 4 位），
//! 支持增删改 + 设默认；Agent 经对应工具（tavily_search / doubao_search ...）
//! 以 (GenericToken, platform) 二元匹配解析到该凭证。
//!
//! 数据来源 = `GET /api/v1/finance/identity/generic-token/status?platform=xxx`。

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
    CreateGenericTokenCredentialRequest, GenericTokenIntegrationStatusResponse,
    UpdateGenericTokenCredentialRequest,
};

/// 内置平台清单（顺序即 Tab 顺序；label 展示，value 落库/匹配）
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

/// 通用 API Token 凭证子区块（嵌入 FinanceIdentity 页面）
#[component]
pub fn IdentityGenericTokenSection() -> Element {
    let toast = use_toast();

    // ===== 当前选中 platform =====
    let mut active_platform = use_signal(|| PLATFORMS[0].slug.to_string());

    // ===== 每个 platform 的状态缓存 =====
    let mut status_map =
        use_signal(std::collections::HashMap::<String, GenericTokenIntegrationStatusResponse>::new);
    let mut loading_map = use_signal(std::collections::HashMap::<String, bool>::new);

    // ===== 录入凭证 =====
    let mut show_create_modal = use_signal(|| false);
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

    // ===== 删除凭证 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

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

    // 首次加载所有平台
    {
        let refresh = refresh_platform;
        use_effect(move || {
            for p in PLATFORMS {
                refresh(p.slug.to_string());
            }
        });
    }

    let active_meta = PLATFORMS
        .iter()
        .find(|p| p.slug == active_platform())
        .unwrap_or(&PLATFORMS[0]);

    // ===== 录入提交 =====
    let handle_create = move |_| {
        spawn(async move {
            let name = new_name();
            let token = new_token();
            let platform = active_platform();
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
                    new_name.set(String::new());
                    new_token.set(String::new());
                    toast.success("凭证绑定成功");
                    refresh_platform(platform);
                }
                Err(e) => toast.error(format!("绑定失败: {}", e)),
            }
            creating.set(false);
        });
    };

    // ===== 编辑提交 =====
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
                    refresh_platform(platform);
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
                    refresh_platform(platform);
                }
                Err(e) => toast.error(format!("设置默认凭证失败: {}", e)),
            }
        });
    };

    // ===== 删除 =====
    let handle_delete = move |_| {
        let id = pending_delete_id();
        let platform = active_platform();
        show_delete_confirm.set(false);
        spawn(async move {
            match delete_generic_token_credential(&id).await {
                Ok(_) => {
                    toast.success("凭证已删除");
                    refresh_platform(platform);
                }
                Err(e) => toast.error(format!("删除失败: {}", e)),
            }
        });
    };

    let snapshot = status_map.read().get(&active_platform()).cloned();
    let credentials = snapshot
        .as_ref()
        .map(|s| s.credentials.clone())
        .unwrap_or_default();
    let is_loading = loading_map
        .read()
        .get(&active_platform())
        .copied()
        .unwrap_or(false);

    rsx! {
        div { class: "border border-base-300 rounded-lg p-4 mt-4",
            div { class: "flex items-center gap-2 flex-wrap",
                h3 { class: "font-semibold text-lg", "通用 API Token" }
                span { class: "badge badge-outline badge-sm", "GenericToken" }
            }
            p { class: "text-xs text-base-content/50 mt-1",
                "统一管理单字段 API Key 类平台凭证（按 platform 分 Tab）；未绑定对应平台凭证时工具调用返回绑定引导。"
            }

            // ===== 平台 Tab =====
            div { class: "tabs tabs-boxed mt-3",
                role: "tablist",
                for meta in PLATFORMS.iter() {
                    {
                        let plat = meta.slug.to_string();
                        let caption = meta.caption;
                        let active = plat == active_platform();
                        rsx! {
                            button {
                                key: "{plat}",
                                class: if active { "tab tab-active" } else { "tab" },
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

            // ===== 当前 platform 面板 =====
            if is_loading && snapshot.is_none() {
                div { class: "text-base-content/50 text-sm py-4", "加载中..." }
            } else {
                div { class: "border border-base-300 rounded-lg p-4 mt-3",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "{active_meta.caption} API Token" }
                        button {
                            class: "btn btn-sm btn-primary",
                            onclick: move |_| {
                                new_name.set(String::new());
                                new_token.set(String::new());
                                show_create_modal.set(true);
                            },
                            "+ 绑定 Token"
                        }
                    }
                    if credentials.is_empty() {
                        div { class: "text-sm text-base-content/50 py-3",
                            "尚未绑定 {active_meta.caption} token"
                        }
                    } else {
                        div { class: "space-y-3 mt-3",
                            for cred in credentials.iter() {
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
                                    rsx! {
                                        div { key: "{credential_id}", class: "border border-base-200 rounded p-3",
                                            div { class: "flex items-center justify-between flex-wrap gap-2",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "font-medium", "{cred_name}" }
                                                    span { class: "badge badge-ghost badge-sm font-mono", "{platform}" }
                                                    if !token_tail.is_empty() {
                                                        span { class: "badge badge-outline font-mono badge-sm", "****{token_tail}" }
                                                    }
                                                    if is_default {
                                                        span { class: "badge badge-success badge-sm", "默认" }
                                                    }
                                                }
                                                div { class: "flex gap-2",
                                                    if !is_default {
                                                        button {
                                                            class: "btn btn-ghost btn-xs",
                                                            onclick: move |_| handle_set_default(plat_for_default.clone(), id_for_default.clone()),
                                                            "设为默认"
                                                        }
                                                    }
                                                    button {
                                                        class: "btn btn-ghost btn-xs",
                                                        onclick: move |_| {
                                                            edit_id.set(id_for_edit.clone());
                                                            edit_platform.set(platform.clone());
                                                            edit_name.set(String::new());
                                                            edit_token.set(String::new());
                                                            show_edit_modal.set(true);
                                                        },
                                                        "编辑"
                                                    }
                                                    button {
                                                        class: "btn btn-ghost btn-xs text-error",
                                                        onclick: move |_| {
                                                            pending_delete_id.set(id_for_delete.clone());
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

                    // ===== 平台提示 =====
                    HudCallout { tone: Some("info".to_string()), extra_class: Some("mt-3".to_string()),
                        span { "{active_meta.hint}" }
                    }
                }
            }
        }

        // ===== 绑定 Token Modal =====
        Modal {
            title: format!("绑定 {} Token", active_meta.caption),
            show: show_create_modal(),
            on_close: move |_| show_create_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_create_modal.set(false), "取消" }
                button { class: "btn btn-primary", disabled: creating(), onclick: handle_create,
                    if creating() { "绑定中..." } else { "绑定" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称 *" } }
                    input {
                        class: "input input-bordered w-full",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                        placeholder: "如：个人号"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label",
                        span { class: "label-text font-medium", "平台" }
                        span { class: "label-text-alt font-mono", "{active_meta.slug}" }
                    }
                    input {
                        class: "input input-bordered w-full",
                        value: "{active_meta.caption}",
                        disabled: "true",
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "API Token *" } }
                    input {
                        class: "input input-bordered w-full font-mono",
                        r#type: "password",
                        value: "{new_token}",
                        oninput: move |e| new_token.set(e.value()),
                        placeholder: active_meta.placeholder
                    }
                }
            }
        }

        // ===== 编辑凭证 Modal =====
        Modal {
            title: "编辑通用 Token 凭证".to_string(),
            show: show_edit_modal(),
            on_close: move |_| show_edit_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_edit_modal.set(false), "取消" }
                button { class: "btn btn-primary", disabled: saving(), onclick: handle_save,
                    if saving() { "保存中..." } else { "保存" }
                }
            },
            div { class: "space-y-4",
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称" } }
                    input {
                        class: "input input-bordered w-full",
                        value: "{edit_name}",
                        oninput: move |e| edit_name.set(e.value()),
                        placeholder: "留空保持不变"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "API Token" } }
                    input {
                        class: "input input-bordered w-full font-mono",
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
