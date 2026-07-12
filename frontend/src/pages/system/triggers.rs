//! 定时触发器管理

use dioxus::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::api::system::{
    create_cron_trigger, delete_cron_trigger, list_cron_triggers, pause_cron_trigger,
    resume_cron_trigger,
};
use crate::components::modal::Modal;
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::{CreateCronTriggerRequest, ListCronTriggersResponseItem};
use common::enums::TriggerType;

#[component]
pub fn SystemTriggers() -> Element {
    let mut triggers = use_signal(Vec::<ListCronTriggersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);
    let mut show_add_modal = use_signal(|| false);
    let mut new_name = use_signal(String::new);
    let mut new_type = use_signal(|| "interval".to_string());
    let mut new_interval = use_signal(|| "300".to_string());
    let mut new_payload = use_signal(String::new);
    let mut creating = use_signal(|| false);

    use_effect(move || {
        loading.set(true);
        spawn(async move {
            match list_cron_triggers().await {
                Ok(list) => triggers.set(list.triggers),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    });

    let handle_create = move |_| {
        spawn(async move {
            if new_name().is_empty() || new_payload().is_empty() {
                error.set("名称和 Payload 不能为空".to_string());
                return;
            }
            creating.set(true);
            let req = match new_type().as_str() {
                "once" => {
                    let now = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    CreateCronTriggerRequest {
                        name: new_name(),
                        trigger_type: TriggerType::Once,
                        cron_expression: None,
                        interval_seconds: None,
                        run_at: Some(now),
                        payload: new_payload(),
                    }
                }
                _ => CreateCronTriggerRequest {
                    name: new_name(),
                    trigger_type: TriggerType::Interval,
                    cron_expression: None,
                    interval_seconds: Some(new_interval().parse().unwrap_or(300)),
                    run_at: None,
                    payload: new_payload(),
                },
            };
            match create_cron_trigger(req).await {
                Ok(_) => {
                    show_add_modal.set(false);
                    new_name.set(String::new());
                    new_type.set("interval".to_string());
                    new_interval.set("300".to_string());
                    new_payload.set(String::new());
                    match list_cron_triggers().await {
                        Ok(list) => triggers.set(list.triggers),
                        Err(e) => error.set(e),
                    }
                }
                Err(e) => error.set(format!("创建失败: {}", e)),
            }
            creating.set(false);
        });
    };

    let triggers_list = triggers.read().clone();
    let payload_placeholder =
        r#"{"action":"agent_rest","extra":{"agent_id":"xxx","settle_limit":5}}"#;

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            div { class: "card-header",
                h2 { class: "card-title", "定时触发器" }
                button { class: "btn btn-accent", onclick: move |_| show_add_modal.set(true), "+ 创建触发器" }
            }
            if loading() {
                Loading {}
            } else if triggers_list.is_empty() {
                EmptyState { icon: "⏰".to_string(), message: "暂无触发器".to_string() }
            } else {
                table { class: "table",
                    thead { tr { th { "名称" }, th { "Cron" }, th { "状态" }, th { "操作" } }}
                    tbody {
                        for t in triggers_list.iter() {
                            {
                                let id = t.id.clone();
                                let name = t.name.clone();
                                let cron_expr = t.cron_expression.clone().unwrap_or_default();
                                let is_enabled = t.is_enabled;
                                let id_pause = id.clone();
                                let id_resume = id.clone();
                                let id_delete = id.clone();
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{name}" }
                                        td { class: "text-mono", "{cron_expr}" }
                                        td {
                                            if is_enabled { span { class: "badge badge-success", "运行中" } }
                                            else { span { class: "badge badge-neutral", "暂停" } }
                                        }
                                        td {
                                            if is_enabled {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id_pause = id_pause.clone();
                                                        spawn(async move {
                                                            if let Err(e) = pause_cron_trigger(&id_pause).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_cron_triggers().await {
                                                                    Ok(list) => triggers.set(list.triggers),
                                                                    Err(e) => error.set(e),
                                                                }
                                                            }
                                                        });
                                                    }, "暂停"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id_resume = id_resume.clone();
                                                        spawn(async move {
                                                            if let Err(e) = resume_cron_trigger(&id_resume).await {
                                                                error.set(e);
                                                            } else {
                                                                match list_cron_triggers().await {
                                                                    Ok(list) => triggers.set(list.triggers),
                                                                    Err(e) => error.set(e),
                                                                }
                                                            }
                                                        });
                                                    }, "恢复"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id_delete = id_delete.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_cron_trigger(&id_delete).await {
                                                            error.set(format!("删除失败: {}", e));
                                                        } else {
                                                            match list_cron_triggers().await {
                                                                Ok(list) => triggers.set(list.triggers),
                                                                Err(e) => error.set(e),
                                                            }
                                                        }
                                                    });
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

        Modal {
            title: "创建触发器".to_string(),
            show: show_add_modal(),
            on_close: move |_| show_add_modal.set(false),
            footer: rsx! {
                button { class: "btn btn-ghost", onclick: move |_| show_add_modal.set(false), "取消" }
                button { class: "btn btn-accent", disabled: creating(), onclick: handle_create,
                    if creating() { "创建中..." } else { "创建" }
                }
            },
            div {
                div { class: "form-group",
                    label { class: "form-label", "触发器名称 *" }
                    input { class: "form-input", value: "{new_name}",
                        oninput: move |e| new_name.set(e.value()) }
                }
                div { class: "form-group",
                    label { class: "form-label", "类型" }
                    select { class: "form-select", value: "{new_type}",
                        onchange: move |e| new_type.set(e.value()),
                        option { value: "interval", "固定间隔触发" }
                        option { value: "once", "一次性触发" }
                    }
                }
                if new_type() == "interval" {
                    div { class: "form-group",
                        label { class: "form-label", "间隔秒数 *" }
                        input { class: "form-input", r#type: "number",
                            value: "{new_interval}",
                            placeholder: "300（即 5 分钟）",
                            oninput: move |e| new_interval.set(e.value()) }
                    }
                }
                div { class: "form-group",
                    label { class: "form-label", "Payload JSON *" }
                    textarea { class: "form-textarea",
                        style: "min-height: 100px; font-family: monospace;",
                        value: "{new_payload}",
                        placeholder: "{payload_placeholder}",
                        oninput: move |e| new_payload.set(e.value()) }
                }
            }
        }
    }
}
