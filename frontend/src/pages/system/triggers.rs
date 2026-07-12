//! 定时触发器管理

use dioxus::prelude::*;

use crate::api::system::{delete_cron_trigger, list_cron_triggers, pause_cron_trigger, resume_cron_trigger};
use crate::components::state::{EmptyState, ErrorAlert, Loading};
use common::api::ListCronTriggersResponseItem;

#[component]
pub fn SystemTriggers() -> Element {
    let mut triggers = use_signal(Vec::<ListCronTriggersResponseItem>::new);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(String::new);

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

    let triggers_list = triggers.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            div { class: "card-header",
                h2 { class: "card-title", "定时触发器" }
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
    }
}
