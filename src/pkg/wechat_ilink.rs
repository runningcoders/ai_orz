//! 微信 iLink（ClawBot）协议基建（pkg 层，与 lark_integration 平行）
//!
//! 本文件是 iLink 协议的 **SSOT**：协议常量、公共请求信封（`base_info`）与通用请求头
//! 都在此处，由**配置面**（本文件：扫码登录）与**消息面**（`dao/wechat/ilink.rs`：
//! `getupdates` / `sendmessage`）共用——两侧协议字段必须同源，否则又是一处双份漂移。
//! **唯一例外**是扫码状态字面量：它们同时是前端的分支条件，故定义在 `common::api`
//! （本文件以短别名转发，见 `QR_STATUS_*`）。
//!
//! 登录协议（两接口，扫码前**无**令牌可鉴权）：
//! - `POST {base}/ilink/bot/get_bot_qrcode?bot_type=3`，body `{ local_token_list: [...] }`
//!   → `{ qrcode, qrcode_img_content }`（`qrcode` 为轮询标识；`qrcode_img_content` 是一条
//!   **可在手机微信直接打开的授权链接**，同时充当二维码渲染内容）
//! - `GET {base}/ilink/bot/get_qrcode_status?qrcode=<urlencoded>[&verify_code=<urlencoded>]`
//!   （服务端 hold ~35s 长轮询）
//!   → `{ status: <8 态>, redirect_host?, bot_token?, ilink_bot_id?, ilink_user_id?, baseurl? }`
//!   confirmed 时返回 `bot_token` / `ilink_bot_id` / `ilink_user_id` / `baseurl`
//!
//! `local_token_list` 是**重绑的关键**：把本机已持有的 bot token 报给服务端，服务端才知道
//! "这个 bot 已经绑过本客户端"，从而可能直接回 `binded_redirect`（幂等成功、不重发凭据）。
//! 不带它则服务端只能走"新建"，`binded_redirect` 永不出现。
//!
//! 协议来源（权威 spec）：腾讯官方插件 `@tencent-weixin/openclaw-weixin` 的
//! `src/api/types.ts`（proto 的 TypeScript 镜像）与 `src/api/api.ts`（传输层），
//! 本次核对版本 `2.4.9`。`NousResearch/hermes-agent` 的 `gateway/platforms/weixin.py`
//! 是独立实现，关键字段与官方插件完全一致，可互证。
//! 详见 docs/plan/微信iLink协议对齐重构方案.md 与设计文档 §5.1/§5.2。
//! 协议字段如有变更，只需修改本文件与 `dao/wechat/ilink.rs`。

use common::error::{Result, err};
use serde::Deserialize;
use std::sync::OnceLock;

/// iLink 接入域默认值（登录响应 `baseurl` 优先，不硬编码假设）
pub const ILINK_DEFAULT_BASE_URL: &str = "https://ilinkai.weixin.qq.com";

// ==================== 协议常量（iLink 协议 SSOT）====================
//
// 取值全部出自官方插件（见文件头「协议来源」）：
// - 枚举取值出自 `src/api/types.ts`（proto 镜像）
// - `ILINK_APP_ID` 出自 `package.json.ilink_appid`
// - 版本编码与请求头构造出自 `src/api/api.ts`

/// 消息方向：未设置（官方 `MessageType.NONE`）
pub const MESSAGE_TYPE_NONE: i64 = 0;
/// 消息方向：对端用户发来（入站投递的唯一来源）
pub const MESSAGE_TYPE_USER: i64 = 1;
/// 消息方向：本 bot 发出（入站回声必须过滤，否则自问自答死循环）
pub const MESSAGE_TYPE_BOT: i64 = 2;

/// 消息状态：新消息（官方 `MessageState.NEW`）
pub const MESSAGE_STATE_NEW: i64 = 0;
/// 消息状态：生成中（分片中间态，不应投递）
pub const MESSAGE_STATE_GENERATING: i64 = 1;
/// 消息状态：完整（可投递）
pub const MESSAGE_STATE_FINISH: i64 = 2;

/// 消息条目类型：未设置
pub const MESSAGE_ITEM_TYPE_NONE: i64 = 0;
/// 消息条目类型：文本
pub const MESSAGE_ITEM_TYPE_TEXT: i64 = 1;
/// 消息条目类型：图片（阶段一不处理）
pub const MESSAGE_ITEM_TYPE_IMAGE: i64 = 2;
/// 消息条目类型：语音（阶段一不处理）
pub const MESSAGE_ITEM_TYPE_VOICE: i64 = 3;
/// 消息条目类型：文件（阶段一不处理）
pub const MESSAGE_ITEM_TYPE_FILE: i64 = 4;
/// 消息条目类型：视频（阶段一不处理）
pub const MESSAGE_ITEM_TYPE_VIDEO: i64 = 5;
/// 消息条目类型：工具调用开始（Agent 侧能力，阶段一不处理）
pub const MESSAGE_ITEM_TYPE_TOOL_CALL_START: i64 = 11;
/// 消息条目类型：工具调用结果（Agent 侧能力，阶段一不处理）
pub const MESSAGE_ITEM_TYPE_TOOL_CALL_RESULT: i64 = 12;

/// 客户端身份（官方 `package.json` 的 `ilink_appid`，字面量）
///
/// 随每个请求的通用头发送（`iLink-App-Id`），与 `base_info.bot_agent` 的自述标识
/// 语义不同——这个是身份字段，不要与 [`BOT_AGENT`] 混用。
pub const ILINK_APP_ID: &str = "bot";

/// 我方渠道实现版本（`base_info.channel_version`）
///
/// 语义是**我方** iLink 渠道实现的版本，非官方插件版本——镜像官方版本号属无必要冒名
/// （官方注释明确 `bot_agent`「仅用于观测，不参与鉴权与路由」）。
pub const CHANNEL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 客户端自述标识（`base_info.bot_agent`，类比 HTTP `User-Agent`）
///
/// 官方缺省为 `OpenClaw`；我方填自身标识。格式为 UA 风格 `Name/Version` 若干 token
/// 空格分隔，官方限制 ASCII、总长 ≤256 字节且不接受空值（空值会回落其默认）。
pub const BOT_AGENT: &str = concat!("ai_orz/", env!("CARGO_PKG_VERSION"));

/// `iLink-App-ClientVersion` 头值：`0x00MMNNPP` 编码（高 8 位固定 0）
///
/// 由 [`CHANNEL_VERSION`] 编译期算出，GET / POST 全带。官方编码规则见
/// `api.ts::buildClientVersion`（`"1.0.11"` → `0x0001000B` = 65547）。
///
/// 若线上真机验证发现服务端做最低版本门禁而被拒，只需改 [`CHANNEL_VERSION`] 一处。
pub const ILINK_APP_CLIENT_VERSION: u32 = encode_client_version(CHANNEL_VERSION);

/// `bot_type`（官方登录接口查询参数；微信 ClawBot 固定 `"3"`）
pub const BOT_TYPE: &str = "3";

// ==================== 扫码登录状态（官方 8 态）====================
//
// 取值出自官方 `src/auth/login-qr.ts` 的 `StatusResponse["status"]` 联合类型。
// 只识别其中 4 态会让 IDC 重定向场景**一直空转到超时**、配对码挑战无解。
//
// 字面量的**唯一定义**在 `common::api::wechat_integration`：这些值后端原样透传、
// 前端按值分支，属前后端共用契约（放 pkg 会让前端只能硬编码字符串 → 必然漂移）。
// 此处按协议口径起短别名，便于消息面/配置面引用。

pub use common::api::{
    WECHAT_QR_STATUS_BINDED_REDIRECT as QR_STATUS_BINDED_REDIRECT,
    WECHAT_QR_STATUS_CONFIRMED as QR_STATUS_CONFIRMED,
    WECHAT_QR_STATUS_EXPIRED as QR_STATUS_EXPIRED,
    WECHAT_QR_STATUS_NEED_VERIFYCODE as QR_STATUS_NEED_VERIFYCODE,
    WECHAT_QR_STATUS_SCANED as QR_STATUS_SCANED,
    WECHAT_QR_STATUS_SCANED_BUT_REDIRECT as QR_STATUS_SCANED_BUT_REDIRECT,
    WECHAT_QR_STATUS_VERIFY_CODE_BLOCKED as QR_STATUS_VERIFY_CODE_BLOCKED,
    WECHAT_QR_STATUS_WAIT as QR_STATUS_WAIT,
};

/// 按官方规则编码版本号：`major << 16 | minor << 8 | patch`（各段取低 8 位）
///
/// `const fn` 以便 [`ILINK_APP_CLIENT_VERSION`] 编译期求值；缺段按 0（官方对
/// `parseInt` 失败亦按 0 处理）。
pub const fn encode_client_version(version: &str) -> u32 {
    let bytes = version.as_bytes();
    let mut parts = [0u32; 3];
    let mut part = 0usize;
    let mut value: u32 = 0;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'.' {
            if part < 2 {
                parts[part] = value;
                part += 1;
            }
            value = 0;
        } else if b >= b'0' && b <= b'9' {
            value = value * 10 + (b - b'0') as u32;
        }
        i += 1;
    }
    parts[part] = value;
    ((parts[0] & 0xff) << 16) | ((parts[1] & 0xff) << 8) | (parts[2] & 0xff)
}

// ==================== 公共请求信封与请求头 ====================

/// `base_info`：消息面请求体附带的公共信封
///
/// 官方在这些接口上带它：`getupdates` / `getuploadurl` / `notifystart` / `notifystop`。
/// **`sendmessage` 不带** —— 官方 `send.ts` 构造的请求体只有 `msg` 一个键，本函数
/// 亦不在那里调用（对齐官方，不画蛇添足；多带未知字段无收益）。
pub fn base_info() -> serde_json::Value {
    serde_json::json!({
        "channel_version": CHANNEL_VERSION,
        "bot_agent": BOT_AGENT,
    })
}

/// 通用请求头（官方 `buildCommonHeaders`）：**GET 与 POST 全带**的客户端身份两件套
pub fn common_headers(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    builder.header("iLink-App-Id", ILINK_APP_ID).header(
        "iLink-App-ClientVersion",
        ILINK_APP_CLIENT_VERSION.to_string(),
    )
}

/// `X-WECHAT-UIN` 头值：随机 uint32 → **十进制字符串** → base64（官方 `randomWechatUin`）
///
/// ⚠️ 编码对象是**十进制字符串**而非 uint32 的原始 4 字节。此前按小端原始字节
/// base64，头格式与官方不符。
pub fn wechat_uin_header_value() -> String {
    use base64::Engine as _;
    let uint32 = rand::random::<u32>();
    base64::engine::general_purpose::STANDARD.encode(uint32.to_string())
}

/// POST 请求头（官方 `buildHeaders` 的完整口径）
///
/// 恒定带 `Content-Type` + `AuthorizationType` + `X-WECHAT-UIN`，`bot_token` 为 `None`
/// 时**不带** `Authorization`——扫码取码接口正是这一形态（官方 `fetchQRCode` 调
/// `apiPostFetch` 未传 token），**不要想当然地补 Bearer**。
fn post_headers(
    builder: reqwest::RequestBuilder,
    bot_token: Option<&str>,
) -> reqwest::RequestBuilder {
    let builder = common_headers(builder)
        .header("Content-Type", "application/json")
        .header("AuthorizationType", "ilink_bot_token")
        .header("X-WECHAT-UIN", wechat_uin_header_value());
    match bot_token {
        Some(token) => builder.header("Authorization", format!("Bearer {token}")),
        None => builder,
    }
}

/// bot 令牌鉴权头（**消息面 POST**）：`getupdates` / `sendmessage` / `notify*`
pub fn bot_auth_headers(
    builder: reqwest::RequestBuilder,
    bot_token: &str,
) -> reqwest::RequestBuilder {
    post_headers(builder, Some(bot_token))
}

/// 扫码取码接口的 POST 头（**不带** `Authorization`，见 [`post_headers`]）
pub fn qr_request_headers(builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
    post_headers(builder, None)
}

/// 二维码状态长轮询：服务端 hold ~35s，客户端超时必须大于它
const QR_POLL_TIMEOUT_MS: u64 = 45_000;

// ==================== 数据结构 ====================

/// 登录二维码
#[derive(Debug, Clone)]
pub struct IlinkQrCode {
    /// 轮询标识（`get_qrcode_status` 的 `qrcode` 参数，非二维码渲染内容）
    pub qrcode: String,
    /// 二维码内容（前端渲染用；也可在浏览器直接打开）
    pub qrcode_img_content: String,
}

/// 二维码状态（官方 8 态）
///
/// 出站字面量一律走 [`IlinkQrStatusKind::as_str`]，禁各处手写状态字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IlinkQrStatusKind {
    /// 等待扫码
    Wait,
    /// 已扫码，等待手机确认
    Scaned,
    /// 二维码过期（调用方负责换新码重试）
    Expired,
    /// 确认登录（`confirmed` 字段携带凭据）
    Confirmed,
    /// 已扫码但 bot 归属其它 IDC：按 [`IlinkQrStatus::redirect_host`] 换接入点继续轮询
    ScanedButRedirect,
    /// 需要配对码：把手机微信显示的数字作为 `verify_code` 随下次轮询回传
    NeedVerifyCode,
    /// 配对码连续错误被风控拦截（换新码重来）
    VerifyCodeBlocked,
    /// 该 bot 已绑过本客户端（**幂等成功**，不签发新凭据）
    BindedRedirect,
}

impl IlinkQrStatusKind {
    /// 协议字面量（对外 DTO / 日志口径 SSOT）
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Wait => QR_STATUS_WAIT,
            Self::Scaned => QR_STATUS_SCANED,
            Self::Expired => QR_STATUS_EXPIRED,
            Self::Confirmed => QR_STATUS_CONFIRMED,
            Self::ScanedButRedirect => QR_STATUS_SCANED_BUT_REDIRECT,
            Self::NeedVerifyCode => QR_STATUS_NEED_VERIFYCODE,
            Self::VerifyCodeBlocked => QR_STATUS_VERIFY_CODE_BLOCKED,
            Self::BindedRedirect => QR_STATUS_BINDED_REDIRECT,
        }
    }

    /// 是否已终局（调用方应停止轮询）：确认登录、或"早已绑过"的幂等成功
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Confirmed | Self::BindedRedirect)
    }
}

impl std::fmt::Display for IlinkQrStatusKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// confirmed 状态的登录凭据
#[derive(Debug, Clone)]
pub struct IlinkQrConfirmed {
    pub bot_token: String,
    pub bot_id: String,
    /// 部分登录响应未返回
    pub user_id: Option<String>,
    /// 接入域（响应缺省时回落 [`ILINK_DEFAULT_BASE_URL`]）
    pub base_url: String,
}

/// 长轮询单次返回
#[derive(Debug, Clone)]
pub struct IlinkQrStatus {
    pub status: IlinkQrStatusKind,
    /// 仅 `ScanedButRedirect` 时存在：应切换到的接入点**裸主机名**
    /// （无 scheme / 端口 / 路径；调用方下次轮询回传该值）
    pub redirect_host: Option<String>,
    /// 仅 status == Confirmed 时存在
    pub confirmed: Option<IlinkQrConfirmed>,
}

/// 校验并归一服务端下发的重定向接入点主机名
///
/// 该值由服务端下发、经前端回传后**变成我方出站请求的目标主机**，属典型的
/// "外部可控出站目标"。官方直接 `https://${redirect_host}` 拼接，我们收紧一步：
/// - 只接受**裸主机名**（字母数字 / `.` / `-`，长度为 1..=253），拒绝任何带
///   scheme、端口、路径、`@` 的形态（否则可被用于把请求打向任意 host:port）；
/// - 必须落在腾讯接入域后缀内（`*.weixin.qq.com` / `*.qq.com`）。
///
/// 不合规时返回 `None`（调用方继续用原接入点，与官方"redirect_host 缺失则沿用当前
/// host"的行为一致），并留 `warn` 痕迹——静默忽略会让"为什么一直在旧接入点空转"无从排查。
fn sanitize_redirect_host(raw: &str) -> Option<String> {
    let host = raw.trim().to_ascii_lowercase();
    let charset_ok = !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-');
    let suffix_ok = host.ends_with(".weixin.qq.com") || host.ends_with(".qq.com");
    if !charset_ok || !suffix_ok {
        let shown: String = host.chars().take(64).collect();
        log_warn!(
            "iLink 下发的 redirect_host 形态非预期，已忽略并沿用当前接入点: {}",
            shown
        );
        return None;
    }
    Some(host)
}

// ==================== 协议客户端 ====================

/// 共享出站客户端（登录为低频配置面操作，无需每调用新建连接池）
fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        // 长轮询专用：45s > 服务端 hold 35s；两接口共用（宽松超时对即取接口无影响）
        crate::pkg::http::presets::with_timeout_ms(QR_POLL_TIMEOUT_MS)
            .and_then(|opts| opts.build())
            .expect("构建 iLink HTTP 客户端失败")
    })
}

fn from_http(op: &str, e: reqwest::Error) -> common::error::Error {
    err!(ThirdPartyError, "ilink {} http error: {}", op, e)
}

/// 获取登录二维码
///
/// `local_token_list`：本机（本用户）已持有的 bot token，**最新在前、最多 10 个**。
/// 服务端据此判断"该 bot 是否已绑过本客户端"，是 `binded_redirect` 能出现的前提；
/// 传空数组等价于旧的无参行为（每次都走新建）。
///
/// 注意官方此接口是 **POST 且不带 `base_info`**（body 只有 `local_token_list`），
/// 请求头也只有通用两件套 + `AuthorizationType` + `X-WECHAT-UIN`（无 Bearer）。
pub async fn get_login_qrcode(local_token_list: &[String]) -> Result<IlinkQrCode> {
    let url = format!("{ILINK_DEFAULT_BASE_URL}/ilink/bot/get_bot_qrcode");
    let body = serde_json::json!({ "local_token_list": local_token_list });
    let resp = qr_request_headers(client().post(&url))
        .query(&[("bot_type", BOT_TYPE)])
        .json(&body)
        .send()
        .await
        .map_err(|e| from_http("get_bot_qrcode", e))?
        .error_for_status()
        .map_err(|e| from_http("get_bot_qrcode", e))?;

    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        qrcode: String,
        #[serde(default)]
        qrcode_img_content: String,
    }
    let raw: Raw = resp
        .json()
        .await
        .map_err(|e| from_http("get_bot_qrcode", e))?;
    if raw.qrcode.is_empty() || raw.qrcode_img_content.is_empty() {
        return Err(err!(
            ThirdPartyError,
            "iLink 返回的登录二维码缺少 qrcode / qrcode_img_content 字段"
        ));
    }
    Ok(IlinkQrCode {
        qrcode: raw.qrcode,
        qrcode_img_content: raw.qrcode_img_content,
    })
}

/// 轮询二维码状态（长轮询单次调用；服务端无新事件时 hold 至超时返回 Wait）
///
/// - `verify_code`：`need_verifycode` 时手机微信展示的配对码。**每次轮询都要带**，
///   直到服务端回 `scaned`（表示配对码已被接受，调用方即可清空暂存）；
/// - `redirect_host`：上一次 `ScanedButRedirect` 返回的接入点裸主机名。传入后本次改为
///   请求 `https://{redirect_host}`；形态不合规时忽略并沿用默认接入点
///   （见 [`sanitize_redirect_host`]）。
pub async fn poll_qrcode_status(
    qrcode: &str,
    verify_code: Option<&str>,
    redirect_host: Option<&str>,
) -> Result<IlinkQrStatus> {
    let base = match redirect_host
        .map(str::trim)
        .filter(|h| !h.is_empty())
        .and_then(sanitize_redirect_host)
    {
        Some(host) => format!("https://{host}"),
        None => ILINK_DEFAULT_BASE_URL.to_string(),
    };
    let url = format!("{base}/ilink/bot/get_qrcode_status");

    let mut query: Vec<(&str, &str)> = vec![("qrcode", qrcode)];
    if let Some(code) = verify_code.map(str::trim).filter(|c| !c.is_empty()) {
        query.push(("verify_code", code));
    }

    let resp = common_headers(client().get(&url))
        .query(&query)
        .send()
        .await;

    let body = match resp {
        Ok(r) => match r.error_for_status() {
            Ok(r) => r.text().await.map_err(|e| from_http("qr_status", e))?,
            Err(e) if e.is_timeout() => {
                // 服务端 hold 到期无事件：客户端先超时是长轮询常态，等价 Wait
                return Ok(wait());
            }
            Err(e) => return Err(from_http("qr_status", e)),
        },
        // 连接层超时同样视为本轮无事件
        Err(e) if e.is_timeout() => return Ok(wait()),
        Err(e) => return Err(from_http("qr_status", e)),
    };

    parse_qr_status(&body)
}

fn wait() -> IlinkQrStatus {
    IlinkQrStatus {
        status: IlinkQrStatusKind::Wait,
        redirect_host: None,
        confirmed: None,
    }
}

/// 解析状态长轮询响应（抽纯函数便于单测）
fn parse_qr_status(body: &str) -> Result<IlinkQrStatus> {
    #[derive(Deserialize)]
    struct Raw {
        #[serde(default)]
        status: String,
        #[serde(default)]
        bot_token: Option<String>,
        #[serde(default)]
        ilink_bot_id: Option<String>,
        #[serde(default)]
        ilink_user_id: Option<String>,
        #[serde(default)]
        baseurl: Option<String>,
        #[serde(default)]
        redirect_host: Option<String>,
    }
    let raw: Raw = serde_json::from_str(body)?;

    let kind = match raw.status.as_str() {
        QR_STATUS_WAIT => IlinkQrStatusKind::Wait,
        QR_STATUS_SCANED => IlinkQrStatusKind::Scaned,
        QR_STATUS_EXPIRED => IlinkQrStatusKind::Expired,
        QR_STATUS_CONFIRMED => IlinkQrStatusKind::Confirmed,
        QR_STATUS_SCANED_BUT_REDIRECT => IlinkQrStatusKind::ScanedButRedirect,
        QR_STATUS_NEED_VERIFYCODE => IlinkQrStatusKind::NeedVerifyCode,
        QR_STATUS_VERIFY_CODE_BLOCKED => IlinkQrStatusKind::VerifyCodeBlocked,
        QR_STATUS_BINDED_REDIRECT => IlinkQrStatusKind::BindedRedirect,
        other => {
            // 协议较新：未知状态宽容为 Wait（调用方按既定节奏继续轮询），但**必须留痕**——
            // 否则新状态出现时的症状是"扫码后永远等待"，与"用户根本没扫"完全同形。
            log_warn!("iLink 返回未知的二维码状态，按 wait 处理: {}", other);
            IlinkQrStatusKind::Wait
        }
    };

    let redirect_host = if kind == IlinkQrStatusKind::ScanedButRedirect {
        raw.redirect_host
            .map(|h| h.trim().to_string())
            .filter(|h| !h.is_empty())
    } else {
        None
    };

    let confirmed = if kind == IlinkQrStatusKind::Confirmed {
        let bot_token = raw.bot_token.unwrap_or_default();
        let bot_id = raw.ilink_bot_id.unwrap_or_default();
        if bot_token.is_empty() || bot_id.is_empty() {
            return Err(err!(
                ThirdPartyError,
                "iLink 登录确认但未返回 bot_token / ilink_bot_id"
            ));
        }
        Some(IlinkQrConfirmed {
            bot_token,
            bot_id,
            user_id: raw.ilink_user_id.filter(|s| !s.is_empty()),
            base_url: raw
                .baseurl
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| ILINK_DEFAULT_BASE_URL.to_string()),
        })
    } else {
        None
    };

    Ok(IlinkQrStatus {
        status: kind,
        redirect_host,
        confirmed,
    })
}

// ==================== 单测 ====================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_qr_status_all_eight_kinds() {
        for (wire, expected) in [
            (QR_STATUS_WAIT, IlinkQrStatusKind::Wait),
            (QR_STATUS_SCANED, IlinkQrStatusKind::Scaned),
            (QR_STATUS_EXPIRED, IlinkQrStatusKind::Expired),
            (QR_STATUS_CONFIRMED, IlinkQrStatusKind::Confirmed),
            (
                QR_STATUS_SCANED_BUT_REDIRECT,
                IlinkQrStatusKind::ScanedButRedirect,
            ),
            (QR_STATUS_NEED_VERIFYCODE, IlinkQrStatusKind::NeedVerifyCode),
            (
                QR_STATUS_VERIFY_CODE_BLOCKED,
                IlinkQrStatusKind::VerifyCodeBlocked,
            ),
            (QR_STATUS_BINDED_REDIRECT, IlinkQrStatusKind::BindedRedirect),
        ] {
            // confirmed 需带凭据，单独构造
            let body = if expected == IlinkQrStatusKind::Confirmed {
                format!(r#"{{"status":"{wire}","bot_token":"tk","ilink_bot_id":"b1"}}"#)
            } else {
                format!(r#"{{"status":"{wire}"}}"#)
            };
            let parsed = parse_qr_status(&body).unwrap();
            assert_eq!(parsed.status, expected, "wire={wire}");
            // 状态字面量往返一致（出站 DTO / 日志口径 SSOT）
            assert_eq!(parsed.status.as_str(), wire);
            if expected != IlinkQrStatusKind::Confirmed {
                assert!(parsed.confirmed.is_none(), "wire={wire}");
            }
        }
    }

    #[test]
    fn test_parse_qr_status_terminal_kinds() {
        assert!(IlinkQrStatusKind::Confirmed.is_terminal());
        assert!(IlinkQrStatusKind::BindedRedirect.is_terminal());
        for kind in [
            IlinkQrStatusKind::Wait,
            IlinkQrStatusKind::Scaned,
            IlinkQrStatusKind::Expired,
            IlinkQrStatusKind::ScanedButRedirect,
            IlinkQrStatusKind::NeedVerifyCode,
            IlinkQrStatusKind::VerifyCodeBlocked,
        ] {
            assert!(!kind.is_terminal(), "{kind}");
        }
    }

    #[test]
    fn test_parse_qr_status_wait_scaned_expired() {
        for (body, expected) in [
            (r#"{"status":"wait"}"#, IlinkQrStatusKind::Wait),
            (r#"{"status":"scaned"}"#, IlinkQrStatusKind::Scaned),
            (r#"{"status":"expired"}"#, IlinkQrStatusKind::Expired),
        ] {
            let parsed = parse_qr_status(body).unwrap();
            assert_eq!(parsed.status, expected);
            assert!(parsed.confirmed.is_none());
            assert!(parsed.redirect_host.is_none());
        }
    }

    #[test]
    fn test_parse_qr_status_redirect_host_only_on_redirect() {
        // scaned_but_redirect：带出 redirect_host
        let parsed = parse_qr_status(
            r#"{"status":"scaned_but_redirect","redirect_host":"idc2.weixin.qq.com"}"#,
        )
        .unwrap();
        assert_eq!(parsed.status, IlinkQrStatusKind::ScanedButRedirect);
        assert_eq!(parsed.redirect_host.as_deref(), Some("idc2.weixin.qq.com"));

        // 非重定向态即使响应里带了 redirect_host 也不透出（避免调用方误切换接入点）
        let parsed =
            parse_qr_status(r#"{"status":"scaned","redirect_host":"idc2.weixin.qq.com"}"#).unwrap();
        assert!(parsed.redirect_host.is_none());

        // 重定向态但缺字段：None（调用方沿用当前接入点）
        let parsed = parse_qr_status(r#"{"status":"scaned_but_redirect"}"#).unwrap();
        assert!(parsed.redirect_host.is_none());
    }

    #[test]
    fn test_sanitize_redirect_host_accepts_bare_tencent_host() {
        assert_eq!(
            sanitize_redirect_host("  IDC2.Weixin.QQ.com ").as_deref(),
            Some("idc2.weixin.qq.com")
        );
        assert_eq!(
            sanitize_redirect_host("ilink-bj.qq.com").as_deref(),
            Some("ilink-bj.qq.com")
        );
    }

    #[test]
    fn test_sanitize_redirect_host_rejects_non_tencent_or_dirty() {
        // 这四种都是"把出站目标交出去"的形态，必须拒绝
        for bad in [
            "",                               // 空
            "evil.example.com",               // 非腾讯域
            "weixin.qq.com.evil.example.com", // 后缀伪装
            "https://idc2.weixin.qq.com",     // 带 scheme
            "idc2.weixin.qq.com:8443",        // 带端口
            "idc2.weixin.qq.com/ilink",       // 带路径
            "user@idc2.weixin.qq.com",        // 带 userinfo
            "127.0.0.1",                      // 直连 IP
            "qq.com",                         // 裸后缀不算子域
        ] {
            assert!(sanitize_redirect_host(bad).is_none(), "bad={bad}");
        }
    }

    #[test]
    fn test_parse_qr_status_confirmed_full() {
        let parsed = parse_qr_status(
            r#"{"status":"confirmed","bot_token":"tk","ilink_bot_id":"b1","ilink_user_id":"u1","baseurl":"https://alt.example.com"}"#,
        )
        .unwrap();
        let c = parsed.confirmed.expect("confirmed 应携带凭据");
        assert_eq!(c.bot_token, "tk");
        assert_eq!(c.bot_id, "b1");
        assert_eq!(c.user_id.as_deref(), Some("u1"));
        assert_eq!(c.base_url, "https://alt.example.com");
    }

    #[test]
    fn test_parse_qr_status_binded_redirect_has_no_credentials() {
        // binded_redirect 是幂等成功，不携带任何凭据
        let parsed = parse_qr_status(r#"{"status":"binded_redirect"}"#).unwrap();
        assert_eq!(parsed.status, IlinkQrStatusKind::BindedRedirect);
        assert!(parsed.confirmed.is_none());
    }

    #[test]
    fn test_parse_qr_status_confirmed_defaults() {
        // user_id / baseurl 缺省：user_id=None，base_url 回落默认接入域
        let parsed =
            parse_qr_status(r#"{"status":"confirmed","bot_token":"tk","ilink_bot_id":"b1"}"#)
                .unwrap();
        let c = parsed.confirmed.expect("confirmed 应携带凭据");
        assert_eq!(c.user_id, None);
        assert_eq!(c.base_url, ILINK_DEFAULT_BASE_URL);
    }

    #[test]
    fn test_parse_qr_status_confirmed_missing_credentials() {
        // confirmed 但缺 token / bot_id：报错而非静默
        assert!(parse_qr_status(r#"{"status":"confirmed"}"#).is_err());
        assert!(parse_qr_status(r#"{"status":"confirmed","bot_token":"tk"}"#).is_err());
    }

    #[test]
    fn test_parse_qr_status_unknown_is_wait() {
        // 未知状态宽容为 Wait（协议较新，避免死循环式报错）
        let parsed = parse_qr_status(r#"{"status":"some_new_state"}"#).unwrap();
        assert_eq!(parsed.status, IlinkQrStatusKind::Wait);
    }
}
