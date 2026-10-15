//! 身份凭证 自定义凭证区块（Finance → Identity 子组件）
//!
//! 「自定义凭证」区域：platform 自由输入的通用 API Token 凭证统一管理——
//! 跨平台全量凭证表格 + 添加 Modal（platform 自由输入）+ 行内设默认 / 编辑 / 删除。
//!
//! 数据来源 = `GET /api/v1/finance/identity/generic-token/status`（platform 空串 =
//! 返回该用户全部 GenericToken 凭证；依赖平台后端 status 端点 platform 可选化）。
//!
//! 语义对齐 identity_generic_token.rs（固定 Tab 区）：token 永不回显（仅尾号 4 位）；
//! 编辑 Modal platform 锁定不可改（改 platform = 删旧建新，防匹配键漂移）；
//! 自定义 platform 值即匹配键（与 Agent 工具 credential_requirements 声明二元匹配）；
//! 全量表与固定 Tab 区数据重叠属预期设计（同一份凭证数据的两种视图），非缺陷。

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
    UpdateGenericTokenCredentialRequest,
};

/// 自定义凭证区块（嵌入 FinanceIdentity 页面区块序列末尾 = 页面最下方）
#[component]
pub fn IdentityCustomCredentialsSection() -> Element {
    let toast = use_toast();

    // ===== 全量凭证列表（platform 空串 = 跨平台全量）=====
    let mut credentials = use_signal(Vec::<GenericTokenCredentialSnapshot>::new);
    let mut loading = use_signal(|| true);

    // ===== 添加凭证 =====
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

    // ===== 删除凭证 =====
    let mut show_delete_confirm = use_signal(|| false);
    let mut pending_delete_id = use_signal(String::new);

    let refresh_all = move || {
        spawn(async move {
            match get_generic_token_status("").await {
                Ok(s) => credentials.set(s.credentials),
                Err(e) => toast.error(format!("加载自定义凭证失败: {}", e)),
            }
            loading.set(false);
        });
    };

    // 首次挂载拉取全量（effect 闭包零 signal 读取 = 仅首挂一次）
    {
        let refresh = refresh_all;
        use_effect(move || {
            refresh();
        });
    }

    // ===== 添加提交（platform 自由输入，trim 非空校验；platform 值即匹配键保存后不可修改）=====
    let handle_create = move |_| {
        spawn(async move {
            let platform = new_platform().trim().to_string();
            let name = new_name().trim().to_string();
            let token = new_token().trim().to_string();
            if platform.is_empty() {
                toast.error("平台标识不能为空（保存后作为匹配键，不可修改）");
                return;
            }
            if name.is_empty() || token.is_empty() {
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
                    refresh_all();
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
            saving.set(true);
            let opt = |v: String| if v.trim().is_empty() { None } else { Some(v) };
            let req = UpdateGenericTokenCredentialRequest {
                id,
                name: opt(edit_name()),
                api_token: opt(edit_token()),
            };
            match update_generic_token_credential(req).await {
                Ok(_) => {
                    show_edit_modal.set(false);
                    edit_token.set(String::new());
                    toast.success("凭证已更新，下次工具调用即生效");
                    refresh_all();
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
                    refresh_all();
                }
                Err(e) => toast.error(format!("设置默认凭证失败: {}", e)),
            }
        });
    };

    // ===== 删除 =====
    let handle_delete = move |_| {
        let id = pending_delete_id();
        show_delete_confirm.set(false);
        spawn(async move {
            match delete_generic_token_credential(&id).await {
                Ok(_) => {
                    toast.success("凭证已删除");
                    refresh_all();
                }
                Err(e) => toast.error(format!("删除失败: {}", e)),
            }
        });
    };

    let credentials_list = credentials();
    let is_loading = loading();

    rsx! {
        div { class: "border border-base-300 rounded-lg p-4 mt-4",
            div { class: "flex items-center justify-between flex-wrap gap-2",
                div { class: "flex items-center gap-2",
                    h3 { class: "font-semibold text-lg", "自定义凭证" }
                    span { class: "badge orz-tag badge-sm", "GenericToken" }
                }
                button {
                    class: "btn hud-btn btn-sm btn-primary",
                    onclick: move |_| {
                        new_platform.set(String::new());
                        new_name.set(String::new());
                        new_token.set(String::new());
                        show_create_modal.set(true);
                    },
                    "+ 添加自定义凭证"
                }
            }
            p { class: "text-xs text-base-content/50 mt-1",
                "platform 自由输入的通用 API Token 凭证（跨平台全量列表）；platform 值即匹配键，保存后不可修改。"
            }

            if is_loading && credentials_list.is_empty() {
                div { class: "text-base-content/50 text-sm py-4", "加载中..." }
            } else if credentials_list.is_empty() {
                div { class: "text-sm text-base-content/50 py-3", "暂无自定义凭证" }
            } else {
                div { class: "overflow-x-auto mt-3",
                    table { class: "table table-sm",
                        thead { tr {
                            th { "平台" }
                            th { "名称" }
                            th { "Token" }
                            th { "默认" }
                            th { "操作" }
                        }}
                        tbody {
                            for cred in credentials_list.iter() {
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
                                        tr { key: "{credential_id}",
                                            td { span { class: "badge orz-tag badge-sm font-mono", "{platform}" } }
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
                                                        onclick: move |_| {
                                                            handle_set_default(plat_for_default.clone(), id_for_default.clone())
                                                        },
                                                        "设为默认"
                                                    }
                                                }
                                            }
                                            td { class: "flex gap-2",
                                                button {
                                                    class: "btn hud-btn btn-ghost btn-xs",
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
                                                    class: "btn hud-btn btn-ghost btn-xs text-error",
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
            }
        }

        // ===== 添加自定义凭证 Modal =====
        Modal {
            title: "添加自定义凭证".to_string(),
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
                    label { class: "label", span { class: "label-text font-medium", "平台标识 *" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        value: "{new_platform}",
                        oninput: move |e| new_platform.set(e.value()),
                        placeholder: "如 ledger / linear（保存后作为匹配键，不可修改）"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "凭证名称 *" } }
                    input {
                        class: "input input-bordered hud-input w-full",
                        value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()),
                        placeholder: "如：个人号"
                    }
                }
                div { class: "form-control w-full",
                    label { class: "label", span { class: "label-text font-medium", "API Token *" } }
                    input {
                        class: "input input-bordered hud-input w-full font-mono",
                        r#type: "password",
                        value: "{new_token}",
                        oninput: move |e| new_token.set(e.value()),
                        placeholder: "加密存储，永不回显"
                    }
                }
                p { class: "text-xs text-base-content/50",
                    "platform 值即匹配键：工具凭据需求（credential_requirements）声明中的 platform 与此值精确匹配。保存后不可修改，如需变更请删除后重新创建（删旧建新）。"
                }
            }
        }

        // ===== 编辑自定义凭证 Modal =====
        Modal {
            title: "编辑自定义凭证".to_string(),
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
