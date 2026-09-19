//! 身份凭证微信区块（Finance → Identity 子组件）
//!
//! 展示当前用户微信侧凭证。现阶段：iLink 机器人（扫码授权产生，bot_token 落库加密）；
//! 未来微信侧新增凭据类型（如企微应用）时在本区块内追加子卡，与 lark 区块的双卡结构一致。
//!
//! 数据来源 = `GET /api/v1/finance/identity/wechat/status` 聚合端点（不缓存 localStorage）。
//! 扫码流程 = `POST /qrcode` 拿二维码 → 用户扫码后在弹窗内主动点「我已扫码完成」→
//! 单次 `GET /qrcode/status`（服务端 hold ~35s 属长轮询常态）→ confirmed 时凭据自动落库，
//! 弹窗就地切换为「已授权」形态并列关键信息。
//!
//! ⚠️ 为什么是「手动按钮单次查询」而非自动轮询：轮询循环需要在外层 async 任务内部再 `spawn`，
//! 而该位置拿不到 Dioxus 的作用域上下文 → 内层任务从未启动（实测整段操作零次状态请求，
//! 表现为扫码后二维码永不消失）。手动按钮在**点击回调里直接 spawn**，与 lark 绑定轮询同构。

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use dioxus::prelude::*;
use qrcode::QrCode;
use qrcode::render::svg;

use crate::api::wechat_integration::{
    get_wechat_integration_status, get_wechat_login_qrcode, poll_wechat_login_status,
};
use crate::components::hud::HudCallout;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use common::api::WechatIntegrationStatusResponse;

/// 二维码渲染内容 → `<img>` 可用的 src
///
/// ⚠️ 关键：iLink `get_bot_qrcode` 返回的 `qrcode_img_content` **不是图片数据**，
/// 而是二维码里编码的文本（实测为 `https://liteapp.weixin.qq.com/q/...` 这类扫码跳转 URL，
/// 长度约 89 字符）。旧实现见到 `http(s)://` 就原样塞进 `<img src>`，浏览器去 GET 一个网页，
/// 返回的不是图像 → 必然破图。这与网络无关，任何网页 URL 都会如此。
///
/// 所以这里统一按「把内容编码成二维码图像」处理：
/// - 已经是图片数据（`data:image/...`）→ 直接渲染（防御性保留）
/// - 其余一律视为二维码内容 → 生成 SVG 并转成 base64 data URI
fn qr_img_src(content: &str) -> String {
    let c = content.trim();
    if c.is_empty() {
        return String::new();
    }
    if c.starts_with("data:image/") {
        return c.to_string();
    }
    match QrCode::new(c.as_bytes()) {
        Ok(code) => {
            // 显式给白底：HUD 主题是深色背景，透明底的黑码既看不清也扫不出
            let svg = code
                .render::<svg::Color>()
                .min_dimensions(240, 240)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build();
            format!(
                "data:image/svg+xml;base64,{}",
                BASE64.encode(svg.as_bytes())
            )
        }
        Err(e) => {
            tracing::warn!("二维码生成失败（内容长度 {}）: {e}", c.len());
            String::new()
        }
    }
}

/// 「已授权」形态的展示数据（confirmed 后弹窗就地切换用）
#[derive(Debug, Clone, PartialEq)]
struct WechatAuthorized {
    /// 凭证 ID（渠道以 credential_id 引用）
    credential_id: String,
    /// iLink bot 标识
    bot_id: String,
    /// true = 本次为重新扫码，既有默认凭据已整组轮换
    rotated: bool,
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
    // 轮询标识（`POST /qrcode` 返回；「我已扫码完成」查询时回传）
    let mut qr_id = use_signal(String::new);
    // wait | scaned | expired
    let mut qr_stage = use_signal(|| "wait".to_string());
    let mut starting = use_signal(|| false);
    // 单次查询进行中（服务端长轮询最长 ~35s，必须给 loading 且允许重试）
    let mut querying = use_signal(|| false);
    // Some = 「已授权」形态；None = 「待扫码授权」形态
    let mut authorized = use_signal(|| Option::<WechatAuthorized>::None);

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

    let handle_close_modal = move |_| show_qr_modal.set(false);

    // 发起扫码：只取二维码，不再自动轮询（查询改为「我已扫码完成」手动触发）
    let handle_start_scan = move |_| {
        spawn(async move {
            starting.set(true);
            match get_wechat_login_qrcode().await {
                Ok(resp) => {
                    qr_img.set(qr_img_src(&resp.qrcode_img_content));
                    qr_id.set(resp.qrcode);
                    qr_stage.set("wait".to_string());
                    authorized.set(None);
                    // 换码即作废在途查询：新码的二维码/按钮立即可用（旧任务回来会自检 qr_id 后丢弃）
                    querying.set(false);
                    show_qr_modal.set(true);
                }
                Err(e) => toast.error(format!("获取登录二维码失败: {}", e)),
            }
            starting.set(false);
        });
    };

    // 「我已扫码完成」→ 单次查询。
    // ⚠️ 必须在**点击回调内直接 spawn**：若在外层 async 任务内部再 spawn，该位置拿不到
    // Dioxus 的作用域上下文，内层任务不会启动（旧自动轮询即因此全程零请求）。
    let handle_query_result = move |_| {
        if querying() {
            return;
        }
        let qrcode = qr_id();
        if qrcode.is_empty() {
            toast.error("二维码标识缺失，请重新获取二维码");
            return;
        }
        spawn(async move {
            querying.set(true);
            // 服务端 hold ~35s 属正常长轮询语义，此期间按钮保持 loading
            let outcome = poll_wechat_login_status(&qrcode).await;
            // 查询在途时用户换过码 → 本次结果作废（qr_id 已被新码覆盖，querying 也已复位）
            if qr_id() != qrcode {
                return;
            }
            match outcome {
                Ok(resp) => match resp.status.as_str() {
                    "confirmed" => {
                        let rotated = resp.rotated.unwrap_or(false);
                        // 弹窗就地切「已授权」形态，并带上关键信息
                        authorized.set(Some(WechatAuthorized {
                            credential_id: resp.credential_id.clone().unwrap_or_default(),
                            bot_id: resp.bot_id.clone().unwrap_or_default(),
                            rotated,
                        }));
                        if rotated {
                            toast.success("微信 iLink 凭证已整组轮换");
                        } else {
                            toast.success("微信 iLink 凭证绑定成功");
                        }
                        // 同步刷新页面下方的凭据列表
                        if let Ok(s) = get_wechat_integration_status().await {
                            status.set(Some(s));
                        }
                    }
                    "scaned" => {
                        qr_stage.set("scaned".to_string());
                        toast.info("已识别到扫码，请在手机上确认登录后再次点击");
                    }
                    "expired" => qr_stage.set("expired".to_string()),
                    // wait：iLink 侧尚未收到确认（长轮询 hold 到超时同样回 wait）
                    _ => toast.info("尚未检测到登录确认，请稍等几秒后再次点击"),
                },
                Err(e) => toast.error(format!("查询扫码结果失败: {}", e)),
            }
            querying.set(false);
        });
    };

    let snapshot = status.read().clone();
    let credentials = snapshot
        .as_ref()
        .map(|s| s.credentials.clone())
        .unwrap_or_default();

    // 已授权形态（确认成功后弹窗就地切换；关闭弹窗不重置，重新扫码时复位）
    let auth = authorized.read().clone();

    // 弹窗主体两态二选一：确认成功 → 「已授权」；否则 → 「待扫码授权」
    let modal_body = match auth.clone() {
        Some(info) => rsx! {
            div { class: "flex flex-col items-center gap-3",
                HudCallout { tone: Some("success".to_string()),
                    span { "授权成功，微信 iLink 凭据已写入当前账号" }
                }
                div { class: "w-full space-y-2",
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm text-base-content/50 shrink-0 w-20", "Bot ID" }
                        span { class: "badge orz-tag badge-sm font-mono", "{info.bot_id}" }
                    }
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm text-base-content/50 shrink-0 w-20", "凭证 ID" }
                        span { class: "badge orz-tag badge-sm font-mono", "{info.credential_id}" }
                    }
                    div { class: "flex items-center gap-2",
                        span { class: "text-sm text-base-content/50 shrink-0 w-20", "绑定方式" }
                        span { class: "text-sm",
                            if info.rotated { "重新扫码 · 已整组轮换既有凭据" } else { "首次绑定" }
                        }
                    }
                }
            }
        },
        None => rsx! {
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
                    span { class: "text-sm text-base-content/60", "请使用微信扫码，并在手机上确认登录" }
                }
                button { class: "btn hud-btn btn-primary w-full",
                    disabled: querying() || starting(),
                    onclick: handle_query_result,
                    if querying() { "查询中..." } else { "我已扫码完成" }
                }
                p { class: "text-xs text-base-content/40 text-center",
                    "在手机上确认登录后点击上方按钮拉取结果（可能需等待数秒）；若提示未检测到，稍后再点一次。"
                }
            }
        },
    };

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
            title: if auth.is_some() { "微信已授权".to_string() } else { "微信扫码授权".to_string() },
            show: show_qr_modal(),
            on_close: handle_close_modal,
            footer: rsx! {
                button { class: "btn hud-btn btn-ghost",
                    // 注意：不能复用 handle_close_modal —— on_close 与 onclick 的事件参数类型不同，
                    // 同一闭包签名无法同时满足两者，必须各写一个。
                    onclick: move |_| show_qr_modal.set(false),
                    "关闭"
                }
            },
            {modal_body}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::qr_img_src;

    #[test]
    fn test_qr_img_src_data_uri_passthrough() {
        // 已经是图片数据 URI → 原样返回，不重复生成
        assert_eq!(
            qr_img_src("data:image/png;base64,AAAA"),
            "data:image/png;base64,AAAA"
        );
    }

    #[test]
    fn test_qr_img_src_url_generates_qr_svg() {
        // iLink 返回的是扫码跳转 URL（非图片数据），应编码为二维码 SVG data URI
        let src = qr_img_src("https://example.com/qr.png");
        assert!(
            src.starts_with("data:image/svg+xml;base64,"),
            "expected svg data uri, got: {src}"
        );
    }

    #[test]
    fn test_qr_img_src_bare_text_generates_qr_svg() {
        // 纯文本内容也应编码为二维码 SVG，而非当作 base64 包装为 png
        let src = qr_img_src("AAAA");
        assert!(
            src.starts_with("data:image/svg+xml;base64,"),
            "expected svg data uri, got: {src}"
        );
    }

    #[test]
    fn test_qr_img_src_empty_returns_empty() {
        assert_eq!(qr_img_src(""), "");
        assert_eq!(qr_img_src("   "), "");
    }
}
