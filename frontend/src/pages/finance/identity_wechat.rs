//! 身份凭证微信区块（Finance → Identity 子组件）
//!
//! 展示当前用户微信侧凭证。现阶段：iLink 机器人（扫码授权产生，bot_token 落库加密）；
//! 未来微信侧新增凭据类型（如企微应用）时在本区块内追加子卡，与 lark 区块的双卡结构一致。
//!
//! 数据来源 = `GET /api/v1/finance/identity/wechat/status` 聚合端点（不缓存 localStorage）。
//! 扫码流程 = `POST /qrcode` 拿二维码 → 前端长轮询 `GET /qrcode/status`
//! （服务端 hold ~35s，Wait 态立即重发即可）→ confirmed 后凭据自动落库。

use std::sync::Arc;
use std::sync::atomic::Ordering;

use dioxus::prelude::*;

use crate::api::wechat_integration::{
    get_wechat_integration_status, get_wechat_login_qrcode, poll_wechat_login_status,
};
use crate::components::hud::HudCallout;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::WechatIntegrationStatusResponse;

/// 二维码渲染内容 → `<img>` 可用的 src（兼容 data URI / URL / 裸 base64 PNG）
fn qr_img_src(content: &str) -> String {
    let c = content.trim();
    if c.starts_with("data:") || c.starts_with("http://") || c.starts_with("https://") {
        c.to_string()
    } else {
        format!("data:image/png;base64,{c}")
    }
}

/// 微信凭证子区块（嵌入 FinanceIdentity 页面）
#[component]
pub fn IdentityWechatSection() -> Element {
    let toast = use_toast();

    // ===== 集成状态 =====
    let mut status = use_signal(|| Option::<WechatIntegrationStatusResponse>::None);
    let mut loading = use_signal(|| true);

    // ===== 扫码授权 =====
    let mut show_qr_modal = use_signal(|| false);
    let mut qr_img = use_signal(String::new);
    // wait | scaned | expired
    let mut qr_stage = use_signal(|| "wait".to_string());
    let mut starting = use_signal(|| false);

    // 轮询循环的卸载守卫：每次扫码发起新 Arc，组件卸载时置 false，
    // 避免 35s 长轮询挂着的 loop 在离开页面后继续打接口（同 lark 绑定轮询模式）。
    let mut qr_poll_running = use_signal(|| Arc::new(std::sync::atomic::AtomicBool::new(true)));
    use_drop(move || {
        qr_poll_running.read().store(false, Ordering::SeqCst);
    });

    let refresh = move || {
        spawn(async move {
            loading.set(true);
            match get_wechat_integration_status().await {
                Ok(s) => status.set(Some(s)),
                Err(e) => toast.error(format!("加载微信凭证状态失败: {}", e)),
            }
            loading.set(false);
        });
    };
    use_effect(refresh);

    let handle_close_modal = move |_| {
        show_qr_modal.set(false);
        qr_poll_running.read().store(false, Ordering::SeqCst);
    };

    let handle_start_scan = move |_| {
        spawn(async move {
            starting.set(true);
            match get_wechat_login_qrcode().await {
                Ok(resp) => {
                    // 每次扫码用新守卫，规避旧循环残留（关闭弹窗已置 false，此处防御性重建）
                    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
                    qr_poll_running.set(running.clone());
                    qr_img.set(qr_img_src(&resp.qrcode_img_content));
                    qr_stage.set("wait".to_string());
                    show_qr_modal.set(true);

                    let poll_id = resp.qrcode;
                    spawn(async move {
                        loop {
                            if !running.load(Ordering::SeqCst) {
                                break;
                            }
                            // 服务端 hold ~35s 属正常长轮询；失败退避 2s 重试
                            let Ok(resp) = poll_wechat_login_status(&poll_id).await else {
                                gloo_timers::future::TimeoutFuture::new(2000).await;
                                continue;
                            };
                            if !running.load(Ordering::SeqCst) {
                                break;
                            }
                            match resp.status.as_str() {
                                "wait" => {}
                                "scaned" => qr_stage.set("scaned".to_string()),
                                "confirmed" => {
                                    show_qr_modal.set(false);
                                    if resp.rotated.unwrap_or(false) {
                                        toast.success("微信 iLink 凭证已整组轮换");
                                    } else {
                                        toast.success("微信 iLink 凭证绑定成功");
                                    }
                                    if let Ok(s) = get_wechat_integration_status().await {
                                        status.set(Some(s));
                                    }
                                    break;
                                }
                                "expired" => {
                                    qr_stage.set("expired".to_string());
                                    break;
                                }
                                _ => {}
                            }
                        }
                    });
                }
                Err(e) => toast.error(format!("获取登录二维码失败: {}", e)),
            }
            starting.set(false);
        });
    };

    let snapshot = status.read().clone();
    let credentials = snapshot
        .as_ref()
        .map(|s| s.credentials.clone())
        .unwrap_or_default();

    rsx! {
        // ==================== 微信凭证子区块 ====================
        div { class: "border border-base-300 rounded-lg p-4 mt-4",
            div { class: "flex items-center gap-2",
                h3 { class: "font-semibold text-lg", "微信" }
                span { class: "badge orz-tag badge-sm", "WechatIlink" }
            }
            p { class: "text-xs text-base-content/50 mt-1",
                "微信 iLink 机器人凭据（扫码授权产生）；未来消息渠道通过引用凭证接入。"
            }

            if loading() && snapshot.is_none() {
                div { class: "text-base-content/50 text-sm py-4", "加载中..." }
            } else {
                // ===== iLink 机器人卡 =====
                div { class: "border border-base-300 rounded-lg p-4 mt-3",
                    div { class: "flex items-center justify-between flex-wrap gap-2",
                        h4 { class: "font-semibold", "iLink 机器人" }
                        button { class: "btn hud-btn btn-sm btn-primary", disabled: starting(),
                            onclick: handle_start_scan,
                            if starting() { "获取中..." } else { "扫码授权" }
                        }
                    }
                    if credentials.is_empty() {
                        div { class: "text-sm text-base-content/50 py-3",
                            "尚未绑定微信 iLink 凭证，点击「扫码授权」用微信扫码完成绑定"
                        }
                    } else {
                        div { class: "space-y-3 mt-3",
                            for cred in credentials.iter() {
                                {
                                    let credential_id = cred.credential_id.clone();
                                    let cred_name = cred.name.clone();
                                    let bot_id = cred.bot_id.clone();
                                    let is_default = cred.is_default;
                                    rsx! {
                                        div { key: "{credential_id}", class: "border border-base-200 rounded p-3",
                                            div { class: "flex items-center justify-between flex-wrap gap-2",
                                                div { class: "flex items-center gap-2 flex-wrap",
                                                    span { class: "font-medium", "{cred_name}" }
                                                    span { class: "badge orz-tag badge-sm font-mono", "{bot_id}" }
                                                    if is_default {
                                                        span { class: "badge hud-badge badge-success badge-sm", "默认" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    p { class: "text-xs text-base-content/40 mt-3",
                        "重新扫码会整组轮换既有凭据（bot_token / bot_id / base_url 以最新登录为准）。"
                    }
                }
            }
        }

        // ===== 扫码授权 Modal =====
        Modal {
            title: "微信扫码授权".to_string(),
            show: show_qr_modal(),
            on_close: handle_close_modal,
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost",
                    onclick: move |_| {
                        show_qr_modal.set(false);
                        qr_poll_running.read().store(false, Ordering::SeqCst);
                    },
                    "关闭"
                }
            },
            div { class: "flex flex-col items-center gap-3",
                if !qr_img().is_empty() {
                    img { src: "{qr_img()}",
                        class: "w-56 h-56 rounded border border-base-300 bg-white",
                        alt: "微信登录二维码"
                    }
                }
                if qr_stage() == "scaned" {
                    HudCallout { tone: Some("info".to_string()),
                        span { "已扫码，请在手机上确认登录" }
                    }
                } else if qr_stage() == "expired" {
                    HudCallout { tone: Some("warning".to_string()),
                        div { class: "flex items-center gap-3 w-full",
                            span { "二维码已过期" }
                            button { class: "btn hud-btn btn-sm btn-primary", disabled: starting(),
                                onclick: handle_start_scan,
                                "重新生成"
                            }
                        }
                    }
                } else {
                    span { class: "text-sm text-base-content/60", "请使用微信扫码并确认登录" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::qr_img_src;

    #[test]
    fn test_qr_img_src_data_uri_passthrough() {
        assert_eq!(
            qr_img_src("data:image/png;base64,AAAA"),
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn test_qr_img_src_url_passthrough() {
        assert_eq!(
            qr_img_src("https://example.com/qr.png"),
            "https://example.com/qr.png"
        );
    }

    #[test]
    fn test_qr_img_src_bare_base64_wrapped() {
        assert_eq!(qr_img_src("AAAA"), "data:image/png;base64,AAAA");
    }
}
