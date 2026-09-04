//! 关联组织（组织组网）
//!
//! 用户侧组网入口（评审稿 §4.2 / §7 前端落点）：
//! - 发起建联：输入对端管理员签发的配对码 + 对端联邦地址，服务端出站完成
//!   验证与双向凭证交换（红线：绝不输入对端账密）
//! - 签发配对码：本端管理员生成 24 字符配对码（10 分钟有效、单用途）
//! - 已建联列表：对端组织条目 + 连接状态；管理员可断联（二次确认）
//!
//! 交互遵循后台管理页惯例：仅组织管理员可见建联/签发/断联操作；
//! 其余成员只读列表。

use dioxus::prelude::*;

use common::api::{CreateLinkRequest, LinkItem};
use common::enums::UserRole;

use crate::api::organization::{create_link, issue_pairing_code, list_links, revoke_link};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::hud::{HudCallout, HudPanel};
use crate::components::state::Loading;
use crate::layouts::app_layout::AppLayout;
use crate::store::auth::use_auth_state;
use crate::store::toast::use_toast;
use crate::utils::status::{org_link_status_badge, org_link_status_text, short_id};
use crate::utils::time::format_datetime;

#[component]
pub fn OrganizationLinks() -> Element {
    let mut loading = use_signal(|| true);
    let mut links = use_signal(Vec::<LinkItem>::new);
    // 建联表单
    let mut pairing_code_input = use_signal(String::new);
    let mut peer_endpoint_input = use_signal(String::new);
    let mut creating = use_signal(|| false);
    // 签发的配对码（签发后展示，过期自动隐藏）
    let mut issued_code = use_signal(String::new);
    let mut issued_expires_at = use_signal(|| 0i64);
    // 断联确认
    let mut show_revoke_confirm = use_signal(|| false);
    let mut pending_revoke_id = use_signal(String::new);
    let mut revoking = use_signal(|| false);

    let toast = use_toast();
    let auth = use_auth_state();
    let can_manage = UserRole::has_permission(UserRole::from_i32(auth().role), UserRole::Admin);

    let load_links = move || {
        spawn(async move {
            match list_links().await {
                Ok(resp) => links.set(resp.links),
                Err(e) => toast.error(&e),
            }
            loading.set(false);
        });
    };

    use_effect(move || {
        load_links();
    });

    // 发起建联
    let handle_create = move |_| {
        let code = pairing_code_input().trim().to_string();
        let endpoint = peer_endpoint_input()
            .trim()
            .trim_end_matches('/')
            .to_string();
        if code.is_empty() || endpoint.is_empty() {
            toast.error("请填写配对码与对端地址");
            return;
        }
        spawn(async move {
            creating.set(true);
            let req = CreateLinkRequest {
                pairing_code: code,
                peer_endpoint: endpoint,
            };
            match create_link(req).await {
                Ok(resp) => {
                    toast.success(format!("已与「{}」建立关联", resp.link.peer_org.name));
                    pairing_code_input.set(String::new());
                    peer_endpoint_input.set(String::new());
                    match list_links().await {
                        Ok(list) => links.set(list.links),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&e),
            }
            creating.set(false);
        });
    };

    // 签发配对码（10 分钟有效，签发后到点自动隐藏展示）
    let handle_issue = move |_| {
        spawn(async move {
            match issue_pairing_code().await {
                Ok(resp) => {
                    issued_code.set(resp.pairing_code.clone());
                    issued_expires_at.set(resp.expires_at);
                    // TTL 到点后自动清掉展示（独立任务睡眠等待，不阻塞当前协程）
                    let code = resp.pairing_code;
                    let ttl = resp.ttl_seconds.max(1) as u64;
                    spawn(async move {
                        gloo_timers::future::sleep(std::time::Duration::from_secs(ttl)).await;
                        if issued_code() == code {
                            issued_code.set(String::new());
                            issued_expires_at.set(0);
                        }
                    });
                }
                Err(e) => toast.error(&e),
            }
        });
    };

    // 断联（ConfirmDialog 确认后）
    let handle_revoke = move |_| {
        let peer_id = pending_revoke_id();
        show_revoke_confirm.set(false);
        spawn(async move {
            revoking.set(true);
            match revoke_link(&peer_id).await {
                Ok(()) => {
                    toast.success("已断联");
                    match list_links().await {
                        Ok(list) => links.set(list.links),
                        Err(e) => toast.error(&e),
                    }
                }
                Err(e) => toast.error(&e),
            }
            revoking.set(false);
        });
    };

    rsx! {
        AppLayout {
        HudPanel { signal: Some(true),
            title: Some("关联组织".to_string()),
            div { class: "card-body",

                if loading() {
                    Loading {}
                } else {
                    div { class: "space-y-6",

                        // ===== 发起建联（管理员）=====
                        if can_manage {
                            div {
                                h3 { class: "font-display text-base font-semibold mb-2", "发起建联" }
                                div { class: "grid gap-3 sm:grid-cols-2",
                                    div { class: "form-control w-full",
                                        label { class: "label",
                                            span { class: "label-text font-medium", "对端配对码" }
                                        }
                                        input {
                                            class: "input input-bordered w-full font-mono",
                                            placeholder: "对端管理员签发的 24 位配对码",
                                            value: "{pairing_code_input}",
                                            oninput: move |e| pairing_code_input.set(e.value()),
                                        }
                                    }
                                    div { class: "form-control w-full",
                                        label { class: "label",
                                            span { class: "label-text font-medium", "对端地址" }
                                        }
                                        input {
                                            class: "input input-bordered w-full",
                                            placeholder: "https://peer.example.com",
                                            value: "{peer_endpoint_input}",
                                            oninput: move |e| peer_endpoint_input.set(e.value()),
                                        }
                                    }
                                }
                                div { class: "mt-3 flex items-center gap-3",
                                    button {
                                        class: "btn hud-btn btn-primary btn-sm",
                                        disabled: creating(),
                                        onclick: handle_create,
                                        if creating() { "建联中..." } else { "发起建联" }
                                    }
                                    span { class: "label-text-alt",
                                        "建联仅需对端配对码，无需输入对端账密。"
                                    }
                                }
                            }

                            div { class: "hud-divider divider" }

                            // ===== 签发配对码（管理员）=====
                            div {
                                h3 { class: "font-display text-base font-semibold mb-2", "签发配对码" }
                                p { class: "label-text-alt mb-2",
                                    "将对端生成的配对码告知对方管理员（口头/私密渠道），对方在其节点发起建联时填入。配对码 10 分钟有效、单用途。"
                                }
                                button {
                                    class: "btn hud-btn btn-ghost btn-sm",
                                    onclick: handle_issue,
                                    "生成配对码"
                                }
                                if !issued_code().is_empty() {
                                    div { class: "mt-3",
                                        HudCallout { tone: Some("success".to_string()), extra_class: Some("text-sm".to_string()),
                                            div { class: "flex flex-col gap-1",
                                                span { class: "font-mono text-lg font-bold tracking-widest", "{issued_code}" }
                                                span { class: "label-text-alt",
                                                    "有效期至 {format_datetime(issued_expires_at())}（过期自动失效）"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "hud-divider divider" }
                        } else {
                            HudCallout { tone: Some("warning".to_string()), extra_class: Some("text-sm".to_string()),
                                "仅组织管理员可发起建联与签发配对码。"
                            }
                        }

                        // ===== 已建联列表 =====
                        div {
                            h3 { class: "font-display text-base font-semibold mb-2", "已建联组织" }
                            if links().is_empty() {
                                HudCallout { tone: Some("info".to_string()), extra_class: Some("text-sm".to_string()),
                                    "尚未与任何组织建联。向对端管理员索取配对码，或为本端签发配对码交给对方。"
                                }
                            } else {
                                div { class: "space-y-3",
                                    for link in links() {
                                        div { key: "{link.peer_org.id}",
                                            class: "flex flex-col gap-2 rounded-lg border border-base-300 p-4 sm:flex-row sm:items-center sm:justify-between",
                                            div { class: "min-w-0 flex-1",
                                                div { class: "flex flex-wrap items-center gap-2",
                                                    span { class: "font-semibold", "{link.peer_org.name}" }
                                                    span { class: org_link_status_badge(link.status),
                                                        "{org_link_status_text(link.status)}"
                                                    }
                                                    if let Some(group) = &link.peer_org.group_name {
                                                        if !group.is_empty() {
                                                            span { class: "badge orz-tag badge-sm", "{group}" }
                                                        }
                                                    }
                                                }
                                                if !link.peer_org.description.is_empty() {
                                                    p { class: "text-sm text-base-content/60 mt-1", "{link.peer_org.description}" }
                                                }
                                                p { class: "text-xs text-base-content/50 mt-1 font-mono truncate",
                                                    "{link.endpoint}"
                                                }
                                                p { class: "text-xs text-base-content/40 mt-0.5",
                                                    "组织 {short_id(&link.peer_org.id)} · 建联于 {format_datetime(link.created_at)}"
                                                }
                                            }
                                            if can_manage && link.status == 1 {
                                                button {
                                                    class: "btn hud-btn btn-ghost btn-sm text-error shrink-0",
                                                    disabled: revoking(),
                                                    onclick: move |_| {
                                                        pending_revoke_id.set(link.peer_org.id.clone());
                                                        show_revoke_confirm.set(true);
                                                    },
                                                    "断联"
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
            show: show_revoke_confirm(),
            title: "确认断联".to_string(),
            message: "断联后对端将无法再调用本组织（下次调用感知 401），连接记录保留用于审计。确定断联？".to_string(),
            confirm_text: Some("断联".to_string()),
            on_confirm: handle_revoke,
            on_cancel: move |_| show_revoke_confirm.set(false),
        }
    }
}
