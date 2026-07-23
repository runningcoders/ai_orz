//! 系统设置

use dioxus::prelude::*;

use crate::config::FrontendConfig;
use crate::hooks::{use_theme, AVAILABLE_THEMES};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;

#[component]
pub fn Settings() -> Element {
    let mut config = use_signal(FrontendConfig::load);
    let toast = use_toast();
    let mut theme_ctrl = use_theme();

    let handle_save = move |_| {
        let cfg = config.read().clone();
        match cfg.save() {
            Ok(_) => toast.success("配置已保存"),
            Err(e) => toast.error(&e),
        }
    };

    let handle_reset = move |_| {
        let mut cfg = config.write();
        cfg.reset_to_default();
        drop(cfg);
        toast.success("已重置为默认配置");
    };

    let current = config.read().clone();

    rsx! {
        AppLayout {
            div { class: "card bg-base-100 shadow-md",
                div { class: "card-body",
                    h2 { class: "card-title text-xl mb-2", "系统设置" }

                    div { class: "divider" }

                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "主题外观" }
                        }
                        div { class: "flex flex-wrap gap-2",
                            for (theme_id, theme_name) in AVAILABLE_THEMES.iter().copied() {
                                button {
                                    class: if theme_ctrl.current() == theme_id { "btn btn-sm btn-primary" } else { "btn btn-sm btn-outline" },
                                    "data-theme": theme_id,
                                    onclick: {
                                        let theme_id = theme_id.to_string();
                                        move |_| theme_ctrl.set(theme_id.clone())
                                    },
                                    "{theme_name}"
                                }
                            }
                        }
                        label { class: "label",
                            span { class: "label-text-alt", "选择喜欢的界面主题，设置自动保存到浏览器" }
                        }
                    }

                    div { class: "divider" }

                    div { class: "form-control w-full",
                        label { class: "label",
                            span { class: "label-text font-medium", "后端 API 地址" }
                        }
                        input {
                            class: "input input-bordered w-full",
                            value: "{current.api_base_url}",
                            oninput: move |e| config.write().api_base_url = e.value(),
                            placeholder: "http://localhost:3000"
                        }
                        label { class: "label",
                            span { class: "label-text-alt", "配置保存在浏览器 localStorage 中" }
                        }
                    }

                    div { class: "flex gap-3 mt-4",
                        button { class: "btn btn-primary", onclick: handle_save, "保存配置" }
                        button { class: "btn btn-ghost", onclick: handle_reset, "重置为默认" }
                    }
                }
            }
        }
    }
}
