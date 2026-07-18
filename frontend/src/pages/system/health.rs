//! 健康检查

use dioxus::prelude::*;

use crate::api::system::check_health;
use crate::store::toast::use_toast;

#[component]
pub fn SystemHealth() -> Element {
    let mut loading = use_signal(|| false);
    let toast = use_toast();

    let check = move |_| {
        loading.set(true);
        spawn(async move {
            match check_health().await {
                Ok(msg) => toast.success(&format!("服务正常: {}", msg)),
                Err(e) => toast.error(&format!("健康检查失败: {}", e)),
            }
            loading.set(false);
        });
    };

    rsx! {
        div { class: "card",
            div { class: "card-header",
                h2 { class: "card-title", "健康检查" }
            }
            p { class: "text-secondary mb-6", "检查后端服务运行状态" }
            button { class: "btn btn-accent", disabled: loading(), onclick: check,
                if loading() { "检查中..." } else { "执行检查" }
            }
        }
    }
}
