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

    let load = move || {
        loading.set(true);
        spawn(async move {
            match list_cron_triggers().await {
                Ok(list) => triggers.set(list.triggers),
                Err(e) => error.set(e),
            }
            loading.set(false);
        });
    };

    use_effect(move || { load(); });

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
                                let id = t.trigger_id.clone();
                                let status = t.status;
                                rsx! {
                                    tr { key: "{id}",
                                        td { style: "font-weight: 500;", "{t.name}" }
                                        td { class: "text-mono", "{t.cron_expression}" }
                                        td {
                                            if status == 1 { span { class: "badge badge-success", "运行中" } }
                                            else if status == 0 { span { class: "badge badge-neutral", "暂停" } }
                                            else { span { class: "badge badge-error", "已禁用" } }
                                        }
                                        td {
                                            if status == 1 {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = pause_cron_trigger(&id).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "暂停"
                                                }
                                            } else {
                                                button { class: "btn btn-ghost btn-sm",
                                                    onclick: move |_| {
                                                        let id = id.clone();
                                                        spawn(async move {
                                                            if let Err(e) = resume_cron_trigger(&id).await { error.set(e); } else { load(); }
                                                        });
                                                    }, "恢复"
                                                }
                                            }
                                            button { class: "btn btn-danger btn-sm",
                                                onclick: move |_| {
                                                    let id = id.clone();
                                                    spawn(async move {
                                                        if let Err(e) = delete_cron_trigger(&id).await { error.set(format!("删除失败: {}", e)); } else { load(); }
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
