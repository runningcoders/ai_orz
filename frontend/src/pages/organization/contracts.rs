//! 联邦合约（组织级能力授权）
//!
//! S3 起 `federation_contracts` 是**能力白名单的唯一事实源**：
//! - 无 active 合约 = 对端无任何能力（fail-closed，入站调用一律 403）；
//! - 管理员在此编辑能力集，下一次入站请求即按新能力集判定；
//! - 「终止合约」是熔断开关（对端立即失去全部能力），记录保留审计线索。
//!
//! 归属说明：合约在**组织维度**授权（local_org ↔ peer_org，写入影响整个组织），
//! 而其生效落地在**项目对话**里（项目内 @ 提及对端 Agent 触发委派）。本页是唯一
//! 写入口——刻意不做项目级配置，避免"改一个项目却影响全组织"的语义错位。
//!
//! 交互遵循后台管理页惯例：仅组织管理员可编辑/终止，其余成员只读。

use dioxus::prelude::*;

use common::api::{
    ContractItem, LinkItem, TerminateContractRequest, UpdateContractCapabilitiesRequest,
};
use common::enums::UserRole;

use crate::api::organization::{
    list_contracts, list_links, terminate_contract, update_contract_capabilities,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudCallout, HudPanel};
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::auth::use_auth_state;
use crate::store::toast::use_toast;
use crate::utils::status::{
    capability_desc, capability_label, contract_state_badge, contract_state_text, short_id,
};
use crate::utils::time::format_datetime;

/// 已知能力白名单（对齐后端 `validate_capabilities`：未知能力服务端拒绝 400）
const KNOWN_CAPABILITIES: [&str; 1] = [common::api::CAPABILITY_A2A_TASK];

#[component]
pub fn OrganizationContracts() -> Element {
    let mut loading = use_signal(|| true);
    let mut contracts = use_signal(Vec::<ContractItem>::new);
    // 对端组织名关联解析用（合约只带 peer_org_id，展示名来自已建联列表）
    let mut links = use_signal(Vec::<LinkItem>::new);
    // 能力编辑（内联展开；单一编辑态，一次只编辑一个合约）
    let mut editing_id = use_signal(|| None::<String>);
    let mut edit_caps = use_signal(Vec::<String>::new);
    let mut saving = use_signal(|| false);
    // 终止确认
    let mut show_terminate_confirm = use_signal(|| false);
    let mut pending_terminate_peer = use_signal(String::new);
    let mut terminating = use_signal(|| false);

    let toast = use_toast();
    let auth = use_auth_state();
    let can_manage = UserRole::has_permission(UserRole::from_i32(auth().role), UserRole::Admin);

    let load = move || {
        spawn(async move {
            match list_contracts().await {
                Ok(resp) => contracts.set(resp.contracts),
                Err(e) => toast.error(&e),
            }
            // 对端展示名关联（失败不影响合约列表本身，静默降级为短 ID）
            if let Ok(resp) = list_links().await {
                links.set(resp.links);
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load();
    });

    // 对端展示名：优先已建联组织名，回退短 ID
    let peer_name = move |peer_org_id: String| -> String {
        links()
            .iter()
            .find(|l| l.peer_org.id == peer_org_id)
            .map(|l| l.peer_org.name.clone())
            .unwrap_or_else(|| short_id(&peer_org_id))
    };

    // 勾选/取消某能力（编辑态内切换，保存时才提交）
    let mut toggle_cap = move |cap: String| {
        let mut cur = edit_caps();
        if cur.contains(&cap) {
            cur.retain(|c| c != &cap);
        } else {
            cur.push(cap);
        }
        edit_caps.set(cur);
    };

    // 保存能力集
    let handle_save = move |contract_id: String| {
        spawn(async move {
            saving.set(true);
            let req = UpdateContractCapabilitiesRequest {
                contract_id: contract_id.clone(),
                capabilities: edit_caps(),
            };
            match update_contract_capabilities(req).await {
                Ok(resp) => {
                    toast.success("能力集已更新，下一次调用即生效");
                    editing_id.set(None);
                    // 就地替换该合约，避免整页刷新闪烁
                    let updated = resp.contract;
                    contracts.set(
                        contracts()
                            .into_iter()
                            .map(|c| {
                                if c.id == updated.id {
                                    updated.clone()
                                } else {
                                    c
                                }
                            })
                            .collect(),
                    );
                }
                Err(e) => toast.error(&e),
            }
            saving.set(false);
        });
    };

    // 终止合约（ConfirmDialog 确认后）
    let handle_terminate = move |_| {
        let peer_id = pending_terminate_peer();
        show_terminate_confirm.set(false);
        spawn(async move {
            terminating.set(true);
            let req = TerminateContractRequest {
                peer_org_id: peer_id.clone(),
            };
            match terminate_contract(req).await {
                Ok(_) => {
                    toast.success("合约已终止，对端已失去全部能力");
                    match list_contracts().await {
                        Ok(resp) => contracts.set(resp.contracts),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&e),
            }
            terminating.set(false);
        });
    };

    rsx! {
        AppLayout {
        HudPanel { signal: Some(true),
            title: Some("联邦合约".to_string()),
            eyebrow: Some("ORG".to_string()),
            div { class: "card-body",

                if loading() {
                    Loading {}
                } else {
                    div { class: "space-y-6",

                        HudCallout { tone: Some("info".to_string()), extra_class: Some("text-sm".to_string()),
                            "合约是能力授权的唯一事实源：对端能调用什么，完全由这里的能力集决定。无生效合约 = 对端无任何能力（其调用一律拒绝）。"
                        }

                        if !can_manage {
                            HudCallout { tone: Some("warning".to_string()), extra_class: Some("text-sm".to_string()),
                                "仅组织管理员可编辑能力集与终止合约，当前为只读视图。"
                            }
                        }

                        if contracts().is_empty() {
                            HudCallout { tone: Some("info".to_string()), extra_class: Some("text-sm".to_string()),
                                "尚无联邦合约。与对端组织建联后会自动生成基础合约（默认开放跨组织任务委派）。"
                            }
                        } else {
                            div { class: "space-y-3",
                                for c in contracts() {
                                    div { key: "{c.id}",
                                        class: "rounded-lg border border-base-300 p-4",
                                        div { class: "flex flex-col gap-3",

                                            // ===== 头部：对端 + 状态 =====
                                            div { class: "flex flex-wrap items-center justify-between gap-2",
                                                div { class: "flex flex-wrap items-center gap-2 min-w-0",
                                                    span { class: "font-semibold", "{peer_name(c.peer_org_id.clone())}" }
                                                    span { class: contract_state_badge(&c.state),
                                                        "{contract_state_text(&c.state)}"
                                                    }
                                                    span { class: "badge orz-tag badge-sm", "{c.kind}" }
                                                }
                                                p { class: "text-xs text-base-content/40",
                                                    "更新于 {format_datetime(c.updated_at)}"
                                                }
                                            }
                                            p { class: "text-xs text-base-content/50 font-mono truncate",
                                                "组织 {short_id(&c.peer_org_id)}"
                                            }

                                            // ===== 能力集展示 =====
                                            div {
                                                if c.capabilities.is_empty() {
                                                    p { class: "text-sm text-base-content/60",
                                                        "未开放任何能力（对端调用将被全部拒绝）"
                                                    }
                                                } else {
                                                    div { class: "flex flex-wrap gap-2",
                                                        for cap in c.capabilities.clone() {
                                                            span { key: "{cap}", class: "badge orz-tag badge-sm",
                                                                "{capability_label(&cap)}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }

                                            // ===== 能力编辑（内联展开）=====
                                            if editing_id().as_deref() == Some(c.id.as_str()) {
                                                div { class: "rounded-lg bg-base-200 p-3",
                                                    p { class: "text-sm font-medium mb-2", "编辑开放能力" }
                                                    div { class: "space-y-2",
                                                        for cap in KNOWN_CAPABILITIES.iter() {
                                                            label { key: "{cap}", class: "flex items-start gap-2 cursor-pointer",
                                                                input {
                                                                    class: "checkbox checkbox-sm",
                                                                    r#type: "checkbox",
                                                                    checked: edit_caps().contains(&cap.to_string()),
                                                                    onchange: {
                                                                        let cap = cap.to_string();
                                                                        move |_| toggle_cap(cap.clone())
                                                                    },
                                                                }
                                                                div { class: "min-w-0",
                                                                    div { class: "text-sm font-medium", "{capability_label(cap)}" }
                                                                    div { class: "text-xs text-base-content/60", "{capability_desc(cap)}" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    div { class: "mt-3 flex items-center gap-2",
                                                        button {
                                                            class: "btn hud-btn btn-primary btn-sm",
                                                            disabled: saving(),
                                                            onclick: {
                                                                let id = c.id.clone();
                                                                move |_| handle_save(id.clone())
                                                            },
                                                            if saving() { "保存中..." } else { "保存" }
                                                        }
                                                        button {
                                                            class: "btn hud-btn btn-ghost btn-sm",
                                                            disabled: saving(),
                                                            onclick: move |_| editing_id.set(None),
                                                            "取消"
                                                        }
                                                    }
                                                }
                                            }

                                            // ===== 操作区（管理员）=====
                                            if can_manage {
                                                div { class: "flex flex-wrap items-center gap-2",
                                                    if editing_id().as_deref() != Some(c.id.as_str()) {
                                                        if c.state == "active" {
                                                            button {
                                                                class: "btn hud-btn btn-ghost btn-sm",
                                                                onclick: {
                                                                    let id = c.id.clone();
                                                                    let caps = c.capabilities.clone();
                                                                    move |_| {
                                                                        edit_caps.set(caps.clone());
                                                                        editing_id.set(Some(id.clone()));
                                                                    }
                                                                },
                                                                "编辑能力"
                                                            }
                                                            button {
                                                                class: "btn hud-btn btn-ghost btn-sm text-error",
                                                                disabled: terminating(),
                                                                onclick: {
                                                                    let peer = c.peer_org_id.clone();
                                                                    move |_| {
                                                                        pending_terminate_peer.set(peer.clone());
                                                                        show_terminate_confirm.set(true);
                                                                    }
                                                                },
                                                                "终止合约"
                                                            }
                                                        } else {
                                                            p { class: "text-xs text-base-content/50",
                                                                "已终止的合约不可编辑；重新建联将生成新合约。"
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
        }
        }

        ConfirmDialog {
            show: show_terminate_confirm(),
            title: "确认终止合约".to_string(),
            message: "终止后对端将立即失去全部能力（其调用一律拒绝），合约记录保留用于审计。确定终止？".to_string(),
            confirm_text: Some("终止合约".to_string()),
            on_confirm: handle_terminate,
            on_cancel: move |_| show_terminate_confirm.set(false),
        }
    }
}
