//! 系统设置

use dioxus::prelude::*;

use crate::components::state::{ErrorAlert, SuccessAlert};
use crate::config::FrontendConfig;

#[component]
pub fn Settings() -> Element {
    let mut config = use_signal(FrontendConfig::load);
    let mut error = use_signal(String::new);
    let mut success = use_signal(String::new);

    let handle_save = move |_| {
        let cfg = config.read().clone();
        match cfg.save() {
            Ok(_) => success.set("配置已保存".to_string()),
            Err(e) => error.set(e),
        }
    };

    let handle_reset = move |_| {
        let mut cfg = config.write();
        cfg.reset_to_default();
        drop(cfg);
        success.set("已重置为默认配置".to_string());
    };

    let current = config.read().clone();

    rsx! {
        div { class: "card",
            ErrorAlert { message: error() }
            SuccessAlert { message: success() }
            div { class: "card-header",
                h2 { class: "card-title", "系统设置" }
            }
            div { class: "form-group",
                label { class: "form-label", "后端 API 地址" }
                input { class: "form-input", value: "{current.api_base_url}",
                    oninput: move |e| config.write().api_base_url = e.value(),
                    placeholder: "http://localhost:3000" }
                p { class: "form-hint", "配置保存在浏览器 localStorage 中" }
            }
            div { class: "flex gap-3",
                button { class: "btn btn-accent", onclick: handle_save, "保存配置" }
                button { class: "btn btn-ghost", onclick: handle_reset, "重置为默认" }
            }
        }
    }
}
