//! 身份凭证微信区块（Finance → Identity 子组件）
//!
//! 展示当前用户微信侧凭证。现阶段：iLink 机器人（扫码授权产生，bot_token 落库加密）；
//! 未来微信侧新增凭据类型（如企微应用）时在本区块内追加子卡，与 lark 区块的双卡结构一致。
//!
//! 数据来源 = `GET /api/v1/finance/identity/wechat/status` 聚合端点（不缓存 localStorage）。
//!
//! 扫码流程 = `POST /qrcode` 取码 → 弹窗打开即**自动长轮询** `GET /qrcode/status`
//! → confirmed 时凭据自动落库、弹窗就地切「已授权」。
//!
//! ## 自动轮询必须**平铺 spawn**
//!
//! 取码任务与轮询 `loop` 在点击回调里**并列** spawn，`loop` 体内只管 `await`
//! （样板照抄 `pages/finance/identity.rs` 的飞书绑定轮询）。
//! ⚠️ 上一版把轮询降级为「我已扫码完成」手动按钮，根因是**嵌套 spawn**：在外层 async
//! 任务内部再 spawn，该位置拿不到 Dioxus 作用域上下文 → 内层任务从未启动（表现为整段
//! 零次状态请求、二维码永不消失）。平铺写法下该问题不存在。
//!
//! 因为服务端本身就是长轮询（无新事件时 hold ~35s 才返回 `wait`），循环体天然自带节流，
//! **无需额外 `sleep`**——`loop` 体即「查询 → 处理 → 再查询」。
//!
//! ## 8 态与前端动作（协议口径见 `common::api::WECHAT_QR_STATUS_*`）
//!
//! | status | 前端动作 |
//! |--------|----------|
//! | `wait` | 无（继续轮询） |
//! | `scaned` | 切「已扫码待确认」；清空配对码暂存（说明码已被接受） |
//! | `need_verifycode` | 显示配对码输入框；输入后随下一轮 `verify_code` 回传 |
//! | `scaned_but_redirect` | 记下 `redirect_host`，其后每轮回传以切换接入点 |
//! | `expired` / `verify_code_blocked` | **自动换码**（上限 [`MAX_QR_REFRESH_COUNT`] 次） |
//! | `confirmed` | 切「已授权」并结束轮询 |
//! | `binded_redirect` | 切「已授权（早已绑过）」，后端未签发新凭据 |

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use qrcode::QrCode;
use qrcode::render::svg;

use crate::api::wechat_integration::{
    get_wechat_integration_status, get_wechat_login_qrcode, poll_wechat_login_status,
};
use crate::components::hud::HudCallout;
use crate::components::modal::Modal;
use crate::store::toast::use_toast;
use crate::utils::format_datetime_full as format_timestamp;
use common::api::{
    WECHAT_QR_STATUS_BINDED_REDIRECT, WECHAT_QR_STATUS_CONFIRMED, WECHAT_QR_STATUS_EXPIRED,
    WECHAT_QR_STATUS_NEED_VERIFYCODE, WECHAT_QR_STATUS_SCANED,
    WECHAT_QR_STATUS_SCANED_BUT_REDIRECT, WECHAT_QR_STATUS_VERIFY_CODE_BLOCKED,
    WECHAT_QR_STATUS_WAIT, WechatIntegrationStatusResponse,
};

/// 二维码自动换码上限（对齐官方 `MAX_QR_REFRESH_COUNT`）
const MAX_QR_REFRESH_COUNT: usize = 3;

/// 等待用户输入配对码 / 等取码任务完成时自旋间隔（避免无谓地打服务端）
const POLL_IDLE_MS: u32 = 500;

/// 轮询出错后的重试间隔（服务端抖动时避免打成热循环）
const POLL_ERROR_RETRY_MS: u32 = 2_000;

// ===== 弹窗阶段（前端自有状态，不是协议状态）=====
/// 等待扫码
const STAGE_WAIT: &str = "wait";
/// 已扫码，等待手机端确认
const STAGE_SCANED: &str = "scaned";
/// 正在自动换码
const STAGE_REFRESHING: &str = "refreshing";
/// 等待用户输入配对码
const STAGE_NEED_VERIFYCODE: &str = "need_verifycode";
/// 配对码连错被风控拦截（提示重试）
const STAGE_BLOCKED: &str = "verify_code_blocked";
/// 换码次数用尽
const STAGE_LIMIT: &str = "expired_limit";
/// 取码 / 换码请求失败
const STAGE_FAILED: &str = "failed";

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

/// 阶段 → `(提示语气, 提示文案)`
///
/// 抽成纯函数便于单测：未知阶段一律回落"等待扫码"，不 panic 也不空白。
fn stage_hint(stage: &str) -> (&'static str, &'static str) {
    match stage {
        STAGE_SCANED => ("info", "已扫码，请在手机上确认登录"),
        STAGE_REFRESHING => ("info", "二维码已过期，正在自动刷新…"),
        STAGE_NEED_VERIFYCODE => ("warning", "手机微信要求配对码，请输入手机上显示的数字"),
        STAGE_BLOCKED => ("warning", "配对码多次输入错误，请稍后重试"),
        STAGE_LIMIT => ("warning", "二维码多次失效，请关闭后重试"),
        STAGE_FAILED => ("warning", "二维码获取或刷新失败，请关闭后重试"),
        _ => ("info", "请使用微信扫码，并在手机上确认登录"),
    }
}

/// 「已授权」形态的展示数据（confirmed / binded_redirect 后弹窗就地切换用）
#[derive(Debug, Clone, PartialEq)]
struct WechatAuthorized {
    /// 凭证 ID（渠道以 credential_id 引用；`binded_redirect` 时为空）
    credential_id: String,
    /// iLink bot 标识（`binded_redirect` 时为空——服务端不回该字段）
    bot_id: String,
    /// true = 本次为重新扫码，既有默认凭据已整组轮换
    rotated: bool,
    /// true = `binded_redirect`：该 bot 早已绑过本客户端，服务端未签发新凭据
    already_bound: bool,
    /// 扫码者标识（bot 侧用户 ID；登录响应未返回时为 None）
    user_id: Option<String>,
    /// 本次绑定 / 轮换时间（epoch 毫秒）
    bound_at: Option<i64>,
}

/// 复制文本到剪贴板，并通过 toast 反馈
fn copy_to_clipboard(content: &str, toast: crate::store::toast::ToastState) {
    if let Some(window) = web_sys::window() {
        let promise = window.navigator().clipboard().write_text(content);
        wasm_bindgen_futures::spawn_local(async move {
            match wasm_bindgen_futures::JsFuture::from(promise).await {
                Ok(_) => toast.info("已复制到剪贴板"),
                Err(_) => toast.error("复制失败"),
            }
        });
    } else {
        toast.error("剪贴板不可用");
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
    let mut starting = use_signal(|| false);
    // 二维码渲染内容（data URI）
    let mut qr_img = use_signal(String::new);
    // 轮询标识（`POST /qrcode` 返回；每轮状态查询都回传）
    let mut qr_id = use_signal(String::new);
    // 备用授权链接（`qrcode_img_content` 本身可在手机微信直接打开）
    let mut qr_url = use_signal(String::new);
    // 弹窗阶段（见模块头：这是前端自有状态，与协议 status 不同名）
    let mut qr_stage = use_signal(|| STAGE_WAIT.to_string());
    // 已自动换码次数（展示 x/MAX）
    let mut qr_refresh_count = use_signal(|| 0usize);
    // 配对码（need_verifycode 时用户输入；服务端回 scaned 即清空）
    let mut verify_code = use_signal(String::new);
    // 接入点重定向主机名（scaned_but_redirect 返回；换码时复位）
    let mut redirect_host = use_signal(String::new);
    // Some = 「已授权」形态；None = 「待扫码授权」形态
    let mut authorized = use_signal(|| Option::<WechatAuthorized>::None);

    // 轮询代次：每次发起扫码 +1，旧循环据此自杀。
    // 没有它，用户「关弹窗 → 再点扫码授权」会留下两条循环同时轮询同一个 qrcode。
    let mut poll_gen = use_signal(|| 0u64);

    // 轮询循环的卸载守卫：组件卸载后置 false，避免 spawn 的 loop 持有已卸载信号无限运行
    let poll_running = use_signal(|| Arc::new(AtomicBool::new(true)));
    use_drop(move || {
        poll_running.read().store(false, Ordering::SeqCst);
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

    // 发起扫码：取码 + 启动自动长轮询（两个 spawn 必须**并列**，见模块头注释）
    let handle_start_scan = move |_| {
        // 复位上一轮的全部扫码状态
        authorized.set(None);
        verify_code.set(String::new());
        redirect_host.set(String::new());
        qr_refresh_count.set(0);
        qr_img.set(String::new());
        qr_url.set(String::new());
        qr_stage.set(STAGE_WAIT.to_string());
        show_qr_modal.set(true);
        starting.set(true);
        poll_gen.set(poll_gen() + 1);
        let my_gen = poll_gen();

        spawn(async move {
            match get_wechat_login_qrcode().await {
                Ok(resp) => {
                    qr_img.set(qr_img_src(&resp.qrcode_img_content));
                    qr_url.set(resp.qrcode_img_content);
                    qr_id.set(resp.qrcode);
                }
                Err(e) => {
                    qr_stage.set(STAGE_FAILED.to_string());
                    toast.error(format!("获取登录二维码失败: {}", e));
                }
            }
            starting.set(false);
        });

        // 自动长轮询：平铺 spawn（**不要**放进上面那个 async 任务体内）
        let running = poll_running.read().clone();
        spawn(async move {
            loop {
                // 卸载 / 换码代次变更 / 弹窗关闭 / 已授权 → 结束
                if !running.load(Ordering::SeqCst)
                    || poll_gen() != my_gen
                    || !show_qr_modal()
                    || authorized().is_some()
                {
                    break;
                }
                // 已进入配对码挑战但用户还没输入：先等输入，避免拿空码空轮询（会被服务端立刻打回）
                if qr_stage() == STAGE_NEED_VERIFYCODE && verify_code().trim().is_empty() {
                    TimeoutFuture::new(POLL_IDLE_MS).await;
                    continue;
                }
                let qrcode = qr_id();
                if qrcode.is_empty() {
                    // 取码任务尚未返回
                    TimeoutFuture::new(POLL_IDLE_MS).await;
                    continue;
                }

                let code = verify_code();
                let host = redirect_host();
                let outcome =
                    poll_wechat_login_status(&qrcode, Some(code.as_str()), Some(host.as_str()))
                        .await;

                // 在途期间换过码 → 本次结果对应已废弃的码，直接丢弃
                if qr_id() != qrcode {
                    continue;
                }
                if !show_qr_modal() || authorized().is_some() {
                    break;
                }

                let resp = match outcome {
                    Ok(resp) => resp,
                    Err(e) => {
                        // 长轮询下偶发失败不应终止整个流程：留痕 + 短暂退避后重试
                        tracing::warn!("微信扫码状态轮询失败，将重试: {e}");
                        TimeoutFuture::new(POLL_ERROR_RETRY_MS).await;
                        continue;
                    }
                };

                match resp.status.as_str() {
                    // 无新事件（服务端 hold 到超时同样回 wait）：阶段不变，继续轮询
                    WECHAT_QR_STATUS_WAIT => {}
                    WECHAT_QR_STATUS_SCANED => {
                        // 已进入 scaned 说明配对码（若有）已被接受 → 清空暂存
                        if !verify_code().is_empty() {
                            verify_code.set(String::new());
                        }
                        qr_stage.set(STAGE_SCANED.to_string());
                    }
                    WECHAT_QR_STATUS_NEED_VERIFYCODE => {
                        qr_stage.set(STAGE_NEED_VERIFYCODE.to_string());
                    }
                    WECHAT_QR_STATUS_SCANED_BUT_REDIRECT => {
                        match resp.redirect_host.clone().filter(|h| !h.is_empty()) {
                            Some(host) => {
                                // 其后每轮都把该主机回传，后端据此切换接入点
                                redirect_host.set(host);
                                qr_stage.set(STAGE_SCANED.to_string());
                            }
                            None => {
                                tracing::warn!(
                                    "微信扫码返回 scaned_but_redirect 但未带 redirect_host，沿用当前接入点"
                                );
                            }
                        }
                    }
                    WECHAT_QR_STATUS_EXPIRED | WECHAT_QR_STATUS_VERIFY_CODE_BLOCKED => {
                        let blocked = resp.status == WECHAT_QR_STATUS_VERIFY_CODE_BLOCKED;
                        // 被风控拦截：清空配对码暂存（换码后重新挑战）
                        verify_code.set(String::new());
                        let count = qr_refresh_count() + 1;
                        if count > MAX_QR_REFRESH_COUNT {
                            qr_stage.set(STAGE_LIMIT.to_string());
                            toast.error(if blocked {
                                "配对码多次错误，请关闭后重试"
                            } else {
                                "二维码多次失效，请关闭后重试"
                            });
                            break;
                        }
                        qr_refresh_count.set(count);
                        // blocked：阶段条先呈现风控态（STAGE_BLOCKED 专属 warning 文案）
                        // 并停留片刻，让用户知道是「配对码错了」而非普通二维码过期；
                        // expired 无需停留，直接换码
                        if blocked {
                            qr_stage.set(STAGE_BLOCKED.to_string());
                            TimeoutFuture::new(1500).await;
                        }
                        qr_stage.set(STAGE_REFRESHING.to_string());
                        match get_wechat_login_qrcode().await {
                            Ok(qr) => {
                                // 就地换码：新 qr_id 会让在途的旧响应自检后被丢弃
                                qr_img.set(qr_img_src(&qr.qrcode_img_content));
                                qr_url.set(qr.qrcode_img_content);
                                qr_id.set(qr.qrcode);
                                // 新码是全新会话：接入点重定向复位
                                redirect_host.set(String::new());
                                qr_stage.set(STAGE_WAIT.to_string());
                            }
                            Err(e) => {
                                tracing::warn!("微信二维码自动刷新失败: {e}");
                                qr_stage.set(STAGE_FAILED.to_string());
                                toast.error(format!("二维码刷新失败: {}", e));
                                break;
                            }
                        }
                    }
                    WECHAT_QR_STATUS_CONFIRMED => {
                        let rotated = resp.rotated.unwrap_or(false);
                        authorized.set(Some(WechatAuthorized {
                            credential_id: resp.credential_id.clone().unwrap_or_default(),
                            bot_id: resp.bot_id.clone().unwrap_or_default(),
                            rotated,
                            already_bound: false,
                            user_id: resp.user_id.clone(),
                            bound_at: resp.bound_at,
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
                        break;
                    }
                    WECHAT_QR_STATUS_BINDED_REDIRECT => {
                        authorized.set(Some(WechatAuthorized {
                            credential_id: String::new(),
                            bot_id: String::new(),
                            rotated: false,
                            already_bound: true,
                            user_id: None,
                            bound_at: None,
                        }));
                        toast.info("该微信已绑过本系统，无需重复绑定");
                        if let Ok(s) = get_wechat_integration_status().await {
                            status.set(Some(s));
                        }
                        break;
                    }
                    other => {
                        // 协议较新：未知状态不终止流程（留痕后按等待处理）
                        tracing::warn!("未知的微信扫码状态，按等待处理: {other}");
                    }
                }
            }
        });
    };

    let handle_close_modal = move |_| show_qr_modal.set(false);

    let snapshot = status.read().clone();
    let credentials = snapshot
        .as_ref()
        .map(|s| s.credentials.clone())
        .unwrap_or_default();

    // 已授权形态（确认成功后弹窗就地切换；关闭弹窗不重置，重新扫码时复位）
    let auth = authorized.read().clone();

    let (stage_tone, stage_text) = stage_hint(qr_stage().as_str());
    let refresh_note = format!(
        "二维码已自动刷新 {}/{}",
        qr_refresh_count(),
        MAX_QR_REFRESH_COUNT
    );

    // 弹窗主体两态二选一：确认成功 → 「已授权」；否则 → 「待扫码授权」
    let modal_body = match auth.clone() {
        Some(info) => rsx! {
            div { class: "flex flex-col items-center gap-3",
                if info.already_bound {
                    HudCallout { tone: Some("success".to_string()),
                        span { "该微信已绑过本系统，无需重复绑定" }
                    }
                    p { class: "text-sm text-base-content/60 text-center",
                        "服务端未签发新凭据，你现有的 iLink 凭证继续有效。"
                    }
                } else {
                    HudCallout { tone: Some("success".to_string()),
                        span { "授权成功，微信 iLink 凭据已写入当前账号" }
                    }
                    div { class: "w-full space-y-2",
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm text-base-content/50 shrink-0 w-20", "Bot ID" }
                            span { class: "badge orz-tag badge-sm font-mono", "{info.bot_id}" }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm text-base-content/50 shrink-0 w-20", "扫码者" }
                            span { class: "text-sm font-mono truncate",
                                {info
                                    .user_id
                                    .clone()
                                    .filter(|s| !s.is_empty())
                                    .unwrap_or_else(|| "—".to_string())}
                            }
                        }
                        div { class: "flex items-center gap-2",
                            span { class: "text-sm text-base-content/50 shrink-0 w-20", "绑定时间" }
                            span { class: "text-sm",
                                {info
                                    .bound_at
                                    .map(format_timestamp)
                                    .unwrap_or_else(|| "—".to_string())}
                            }
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
            }
        },
        None => rsx! {
            div { class: "flex flex-col items-center gap-3",
                div { class: "flex items-center gap-2",
                    if qr_refresh_count() > 0 {
                        span { class: "badge hud-badge badge-warning badge-sm", "{refresh_note}" }
                    } else {
                        span { class: "badge orz-tag badge-sm", "等待扫码" }
                    }
                }
                if !qr_img().is_empty() {
                    img { src: "{qr_img()}",
                        class: "w-56 h-56 rounded border border-base-300 bg-white",
                        alt: "微信登录二维码"
                    }
                } else {
                    div { class: "w-56 h-56 rounded border border-base-300 flex items-center justify-center text-sm text-base-content/50",
                        "二维码加载中…"
                    }
                }
                HudCallout { tone: Some(stage_tone.to_string()),
                    span { "{stage_text}" }
                }
                if qr_stage() == STAGE_NEED_VERIFYCODE {
                    div { class: "w-full space-y-1",
                        label { class: "text-xs text-base-content/60",
                            "配对码（手机微信上显示的数字）"
                        }
                        input {
                            class: "input hud-input input-bordered input-sm w-full font-mono",
                            placeholder: "输入手机微信显示的数字",
                            value: "{verify_code}",
                            oninput: move |e| verify_code.set(e.value()),
                        }
                    }
                }
                if qr_stage() == STAGE_LIMIT || qr_stage() == STAGE_FAILED {
                    button { class: "btn hud-btn btn-sm btn-primary", disabled: starting(),
                        onclick: handle_start_scan,
                        "重新生成二维码"
                    }
                }
                // 备用授权链接：qrcode_img_content 本身就是可在手机微信打开的 URL。
                // 二维码渲染失败 / 人不在电脑前（看不到这块屏幕）时，这是唯一替代路径。
                if !qr_url().is_empty() {
                    div { class: "w-full space-y-1 pt-1 border-t border-base-300",
                        p { class: "text-xs text-base-content/50",
                            "若二维码无法显示或不便扫描，可用手机微信直接打开以下链接继续授权："
                        }
                        div { class: "flex items-center gap-2",
                            a {
                                class: "link link-primary text-xs break-all flex-1",
                                href: "{qr_url()}",
                                target: "_blank",
                                rel: "noopener noreferrer",
                                "{qr_url()}"
                            }
                            button { class: "btn hud-btn btn-xs btn-ghost shrink-0",
                                onclick: move |_| copy_to_clipboard(&qr_url(), toast),
                                "复制"
                            }
                        }
                    }
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
                                    let scan_user = cred
                                        .user_id
                                        .clone()
                                        .filter(|s| !s.is_empty())
                                        .unwrap_or_else(|| "—".to_string());
                                    // 历史数据可能没落 base_url（空串）→ 展示口径说清而不是留白
                                    let base_url = if cred.base_url.trim().is_empty() {
                                        "未记录（按默认接入域）".to_string()
                                    } else {
                                        cred.base_url.clone()
                                    };
                                    let created_at = format_timestamp(cred.created_at);
                                    let updated_at = format_timestamp(cred.updated_at);
                                    rsx! {
                                        div { key: "{credential_id}",
                                            class: "border border-base-200 rounded p-3 space-y-2",
                                            div { class: "flex items-center gap-2 flex-wrap",
                                                span { class: "font-medium", "{cred_name}" }
                                                span { class: "badge orz-tag badge-sm font-mono", "{bot_id}" }
                                                if is_default {
                                                    span { class: "badge hud-badge badge-success badge-sm", "默认" }
                                                }
                                            }
                                            div { class: "grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-1 text-xs",
                                                div { class: "flex items-center gap-2",
                                                    span { class: "text-base-content/50 shrink-0 w-16", "扫码者" }
                                                    span { class: "font-mono truncate", "{scan_user}" }
                                                }
                                                div { class: "flex items-center gap-2",
                                                    span { class: "text-base-content/50 shrink-0 w-16", "接入域" }
                                                    span { class: "font-mono truncate", title: "{base_url}", "{base_url}" }
                                                }
                                                div { class: "flex items-center gap-2",
                                                    span { class: "text-base-content/50 shrink-0 w-16", "绑定时间" }
                                                    span { "{created_at}" }
                                                }
                                                div { class: "flex items-center gap-2",
                                                    span { class: "text-base-content/50 shrink-0 w-16", "最后变更" }
                                                    span { "{updated_at}" }
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
    use super::*;

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

    #[test]
    fn test_stage_hint_covers_every_stage() {
        for (stage, tone) in [
            (STAGE_WAIT, "info"),
            (STAGE_SCANED, "info"),
            (STAGE_REFRESHING, "info"),
            (STAGE_NEED_VERIFYCODE, "warning"),
            (STAGE_BLOCKED, "warning"),
            (STAGE_LIMIT, "warning"),
            (STAGE_FAILED, "warning"),
        ] {
            let (got_tone, text) = stage_hint(stage);
            assert_eq!(got_tone, tone, "stage={stage}");
            assert!(!text.is_empty(), "stage={stage} 的提示文案不应为空");
        }
    }

    #[test]
    fn test_stage_hint_unknown_falls_back_to_waiting() {
        // 未知阶段（协议新增或状态错乱）必须回落为"等待扫码"，而不是空文案/崩溃
        assert_eq!(stage_hint("some_new_stage"), stage_hint(STAGE_WAIT));
        assert_eq!(stage_hint(""), stage_hint(STAGE_WAIT));
    }
}
