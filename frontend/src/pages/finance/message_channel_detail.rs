//! 消息渠道详情页 - 展示详情 + 启用/禁用 + 测试连接 + 删除

use dioxus::prelude::*;
use dioxus_router::{use_navigator, Link};

use crate::api::finance::{
    delete_message_channel, get_message_channel, test_message_channel,
    update_message_channel_status,
};
use crate::components::confirm_dialog::ConfirmDialog;
use crate::components::state::{EmptyState, Loading};
use crate::layouts::app_layout::AppLayout;
use crate::store::toast::use_toast;
use common::api::GetMessageChannelResponse;
use common::enums::{ChannelStatus, ChannelType};

#[component]
pub fn FinanceMessageChannelDetail(id: String) -> Element {
    let toast = use_toast();
    let navigator = use_navigator();

    let mut channel = use_signal(|| Option::<GetMessageChannelResponse>::None);
    let mut loading = use_signal(|| true);
    let mut toggling = use_signal(|| false);
    let mut testing = use_signal(|| false);
    let mut show_delete_confirm = use_signal(|| false);

    let id_for_effect = id.clone();
    use_effect(move || {
        loading.set(true);
        let id = id_for_effect.clone();
        spawn(async move {
            match get_message_channel(&id).await {
                Ok(c) => channel.set(Some(c)),
                Err(e) => toast.error(&format!("加载失败: {}", e)),
            }
            loading.set(false);
        });
    });

    let mut on_toggle = {
        let id = id.clone();
        move |new_status: i32| {
            let id = id.clone();
            toggling.set(true);
            spawn(async move {
                match update_message_channel_status(&id, new_status).await {
                    Ok(_) => {
                        toast.success(if new_status == 1 { "已启用" } else { "已禁用" });
                        match get_message_channel(&id).await {
                            Ok(c) => channel.set(Some(c)),
                            Err(e) => toast.error(&format!("刷新失败: {}", e)),
                        }
                    }
                    Err(e) => toast.error(&e),
                }
                toggling.set(false);
            });
        }
    };

    let on_test = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            testing.set(true);
            spawn(async move {
                match test_message_channel(&id).await {
                    Ok(resp) => {
                        if resp.success {
                            toast.success("连接测试通过");
                        } else {
                            toast.error(&format!("连接失败: {}", resp.error.unwrap_or_default()));
                        }
                    }
                    Err(e) => toast.error(&format!("测试失败: {}", e)),
                }
                testing.set(false);
            });
        }
    };

    let on_delete = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            show_delete_confirm.set(false);
            spawn(async move {
                match delete_message_channel(&id).await {
                    Ok(_) => {
                        toast.success("已删除");
                        let _ = navigator.push("/finance/message-channels".to_string());
                    }
                    Err(e) => toast.error(&format!("删除失败: {}", e)),
                }
            });
        }
    };

    let channel_data = channel.read().clone();

    rsx! {
        AppLayout {
            div { class: "mb-6 flex items-center justify-between",
                h1 { class: "text-2xl font-bold", "消息渠道详情" }
                Link { class: "btn btn-ghost", to: crate::pages::Route::FinanceMessageChannels {}, "← 返回列表" }
            }
            if loading() {
                Loading {}
            } else if let Some(c) = channel_data {
                div { class: "card bg-base-100 shadow-md",
                    div { class: "card-body",
                        div { class: "flex justify-between items-center mb-4",
                            h2 { class: "card-title", "{c.channel_name}" }
                            div { class: "flex gap-2",
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: toggling(),
                                    onclick: move |_| on_toggle(if c.status == ChannelStatus::Active { 2 } else { 1 }),
                                    if c.status == ChannelStatus::Active { "🚫 禁用" } else { "✅ 启用" }
                                }
                                button {
                                    class: "btn btn-ghost btn-sm",
                                    disabled: testing(),
                                    onclick: on_test,
                                    if testing() { "测试中..." } else { "🔌 测试连接" }
                                }
                                button {
                                    class: "btn btn-error btn-sm",
                                    onclick: move |_| show_delete_confirm.set(true),
                                    "🗑 删除"
                                }
                            }
                        }
                        div { class: "grid grid-cols-1 md:grid-cols-2 gap-4",
                            div {
                                div { class: "text-sm text-base-content/60", "渠道类型" }
                                div { class: "font-mono", "{channel_type_text(c.channel_type)}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "状态" }
                                div { span { class: "badge", "{status_text(c.status)}" } }
                            }
                            if let Some(url) = &c.webhook_url {
                                div {
                                    div { class: "text-sm text-base-content/60", "Webhook URL" }
                                    div { class: "font-mono text-sm break-all", "{url}" }
                                }
                            }
                            if let Some(aid) = &c.agent_id {
                                div {
                                    div { class: "text-sm text-base-content/60", "绑定 Agent" }
                                    div { class: "font-mono", "{aid}" }
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "凭据状态" }
                                div { class: "flex gap-2 flex-wrap",
                                    if c.has_access_token { span { class: "badge badge-success badge-sm", "Access Token" } }
                                    if c.has_secret { span { class: "badge badge-success badge-sm", "Secret" } }
                                    if c.has_config_secret { span { class: "badge badge-success badge-sm", "Config Secret" } }
                                    if !c.has_access_token && !c.has_secret && !c.has_config_secret {
                                        span { class: "text-base-content/50 text-sm", "无凭据" }
                                    }
                                }
                            }
                            if let Some(last_push) = c.last_pushed_at {
                                div {
                                    div { class: "text-sm text-base-content/60", "最后推送" }
                                    div { class: "font-mono", "{crate::utils::format_datetime(last_push)}" }
                                }
                            }
                            if let Some(err) = &c.last_error {
                                div { class: "md:col-span-2",
                                    div { class: "text-sm text-error mb-1", "最后推送错误" }
                                    pre {
                                        class: "font-mono text-xs bg-error/10 p-2 rounded",
                                        style: "white-space: pre-wrap; word-break: break-word;",
                                        "{err}"
                                    }
                                }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "创建时间" }
                                div { class: "font-mono", "{crate::utils::format_datetime(c.created_at)}" }
                            }
                            div {
                                div { class: "text-sm text-base-content/60", "更新时间" }
                                div { class: "font-mono", "{crate::utils::format_datetime(c.updated_at)}" }
                            }
                        }
                    }
                }
            } else {
                EmptyState { icon: "❓".to_string(), message: "消息渠道不存在或已被删除".to_string() }
            }

            ConfirmDialog {
                show: show_delete_confirm(),
                title: "确认删除".to_string(),
                message: "确定删除此消息渠道？".to_string(),
                on_confirm: on_delete,
                on_cancel: move |_| show_delete_confirm.set(false),
            }
        }
    }
}

fn channel_type_text(t: ChannelType) -> &'static str {
    match t {
        ChannelType::Lark => "飞书",
        ChannelType::Wechat => "微信",
        ChannelType::Slack => "Slack",
        ChannelType::Email => "邮件",
        ChannelType::Webhook => "Webhook",
        ChannelType::A2aCallback => "A2A 回调",
    }
}

fn status_text(s: ChannelStatus) -> &'static str {
    match s {
        ChannelStatus::Active => "启用",
        ChannelStatus::Disabled => "禁用",
        ChannelStatus::Deleted => "已删除",
    }
}
