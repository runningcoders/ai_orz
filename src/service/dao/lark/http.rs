//! 飞书 DAO HTTP 实现（多应用模型）
//!
//! 包含：
//! - tenant_access_token 获取与缓存（per-app，带提前 5 分钟刷新）
//! - 消息发送（`/open-apis/im/v1/messages`）
//! - WebSocket 长连接生命周期管理（per-app 连接池，委托 `ws` 模块）
//!
//! 凭证不来自全局配置：出站（push/test_connection）与入站（start_event_listener）
//! 均由调用方（DAL 层）传入已解析的 `LarkAppCredentials`，DAO 不做凭证解析。

use super::error::{LarkResponse, from_reqwest, validate_config};
use super::token::{SharedTokenCache, shared as shared_token_cache};
use super::ws::WsState;
use super::{LarkAppCredentials, LarkDao, LarkImageResource, LarkReply};
use crate::models::events::LarkMessageEvent;
use crate::models::message::Message;
use crate::models::message_channel::MessageChannel;
use crate::pkg::RequestContext;
use common::api::{LarkWsAppMetrics, LarkWsMetrics};
use common::error::{Result, err};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

// ==================== 飞书 OpenAPI 端点 ====================

const API_BASE: &str = "https://open.feishu.cn";
const PATH_TOKEN: &str = "/open-apis/auth/v3/tenant_access_token/internal";
const PATH_SEND_MESSAGE: &str = "/open-apis/im/v1/messages";
const PATH_REPLY_MESSAGE: &str = "/open-apis/im/v1/messages/{message_id}/reply";
/// 消息表情回复（入站轻回执；`{message_id}` 为飞书 `om_xxx`）
const PATH_ADD_REACTION: &str = "/open-apis/im/v1/messages/{message_id}/reactions";
/// 消息资源下载（`?type=image`）：`{message_id}` 为消息 ID，`{file_key}` 为 image_key
const PATH_MESSAGE_RESOURCE: &str = "/open-apis/im/v1/messages/{message_id}/resources/{file_key}";
/// 应用机器人信息（返回顶层 `bot.open_id`，不套 `data`）
const PATH_BOT_INFO: &str = "/open-apis/bot/v3/info";
/// 上传图片（`?image_type=message`）：multipart 字段名 `image`，返回 `data.image_key`
const PATH_UPLOAD_IMAGE: &str = "/open-apis/im/v1/images";
/// 上传文件：multipart 字段名 `file`/`file_name`/`file_type`，返回 `data.file_key`
const PATH_UPLOAD_FILE: &str = "/open-apis/im/v1/files";

/// 飞书图片消息 content（与 text 的 `{"text":...}` 平行）
fn image_message_content(image_key: &str) -> String {
    serde_json::json!({ "image_key": image_key }).to_string()
}

/// 飞书文件消息 content（与 text / image 平行）
fn file_message_content(file_key: &str) -> String {
    serde_json::json!({ "file_key": file_key }).to_string()
}

/// 飞书视频消息 content（`msg_type:media`）
///
/// `image_key` 为可选封面（默认空 = 无封面），这里不生成封面缩略图，省略即可。
fn media_message_content(file_key: &str) -> String {
    serde_json::json!({ "file_key": file_key }).to_string()
}

/// 由文件名推断飞书 `file_type`（飞书用扩展名语义，`stream` 为未知兜底）。
///
/// 飞书上传文件接口按 `file_type` 决定预览形态；传错会被当作未知文件处理。
/// 常见办公文档按扩展名，视频必须 `mp4`（`msg_type:media` 要求），未在内的
/// 一律 `stream`（可下载，客户端按原名识别）。
fn lark_file_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "pdf" => "pdf",
        "doc" | "docx" => "doc",
        // 飞书枚举只有 xls（无 xlsx/csv），xlsx/csv 归一到 xls，否则接口 234001 拒收
        "xls" | "xlsx" | "csv" => "xls",
        "ppt" | "pptx" => "ppt",
        // 视频必须用 mp4 上传，否则 msg_type:media 拒收
        "mp4" => "mp4",
        _ => "stream",
    }
}

/// multipart 文件名清洗：避免引号 / 反斜杠 / 换行破坏 Content-Disposition
fn safe_upload_filename(name: &str) -> String {
    let mut cleaned = String::with_capacity(name.len());
    for c in name.chars() {
        if c == '"' || c == '\\' || c == '\r' || c == '\n' {
            cleaned.push('_');
        } else {
            cleaned.push(c);
        }
    }
    if cleaned.trim().is_empty() {
        "image".to_string()
    } else {
        cleaned
    }
}

// ==================== 工厂方法 + 单例 ====================

static LARK_DAO: OnceLock<Arc<dyn LarkDao>> = OnceLock::new();

/// 创建一个全新的飞书 DAO 实例（无全局凭证，凭证按调用传入）
pub fn new() -> Arc<dyn LarkDao> {
    Arc::new(LarkDaoHttpImpl::new())
}

/// 获取 LarkDao 单例
pub fn dao() -> Arc<dyn LarkDao> {
    LARK_DAO
        .get()
        .cloned()
        .expect("LarkDao not initialized, call init() first")
}

/// 初始化单例（无全局凭证，凭证由 DAL 层解析后按调用传入）
pub fn init() {
    let _ = LARK_DAO.set(new());
}

// ==================== 实现 ====================

pub struct LarkDaoHttpImpl {
    http: reqwest::Client,
    /// per-app token 缓存（app_id 键控；Arc 包装供 WS token source 共享，避免循环引用）
    token_caches: Arc<RwLock<HashMap<String, SharedTokenCache>>>,
    /// per-app WebSocket 连接状态（app_id 键控）
    ws_conns: RwLock<HashMap<String, WsState>>,
}

impl LarkDaoHttpImpl {
    pub fn new() -> Self {
        Self {
            // 此前为 `reqwest::Client::new()`（无超时）：网络抖动时 token/消息请求会永久挂起
            http: crate::pkg::http::presets::outbound()
                .build()
                .expect("构建飞书 HTTP 客户端失败"),
            token_caches: Arc::new(RwLock::new(HashMap::new())),
            ws_conns: RwLock::new(HashMap::new()),
        }
    }

    /// 获取（或创建）指定应用的 token 缓存句柄
    async fn token_cache_for(
        caches: &RwLock<HashMap<String, SharedTokenCache>>,
        app_id: &str,
    ) -> SharedTokenCache {
        {
            let caches = caches.read().await;
            if let Some(cache) = caches.get(app_id) {
                return cache.clone();
            }
        }
        let mut caches = caches.write().await;
        caches
            .entry(app_id.to_string())
            .or_insert_with(shared_token_cache)
            .clone()
    }

    /// 获取指定应用的 tenant_access_token（带缓存，提前 5 分钟刷新）
    ///
    /// 使用双重检查锁防止并发刷新。
    pub async fn get_tenant_access_token(&self, app_id: &str, app_secret: &str) -> Result<String> {
        fetch_token_with_caches(&self.http, &self.token_caches, app_id, app_secret).await
    }

    /// 按给定 msg_type 发送飞书消息（content 为已序列化的 JSON 字符串）
    ///
    /// `receive_id_type` 为飞书接收方类型：`open_id`（私聊用户）或 `chat_id`（群会话）。
    /// 返回飞书 message_id；服务端未给 ID（含字段缺失被 `#[serde(default)]`
    /// 吞成空串）时返回 `None`——空串当成功会产生脏键 `"lark:"`，与微信侧
    /// `None` = 无 ID 的语义对齐。
    async fn send_message(
        &self,
        token: &str,
        receive_id: &str,
        receive_id_type: &str,
        msg_type: &'static str,
        content: String,
    ) -> Result<Option<String>> {
        #[derive(Serialize)]
        struct SendMessageReq<'a> {
            receive_id: &'a str,
            msg_type: &'static str,
            content: String,
        }

        #[derive(Default, Deserialize)]
        struct SendMessageData {
            #[serde(default)]
            message_id: String,
        }

        let url = format!(
            "{}{}?receive_id_type={}",
            API_BASE, PATH_SEND_MESSAGE, receive_id_type
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&SendMessageReq {
                receive_id,
                msg_type,
                content,
            })
            .send()
            .await
            .map_err(|e| from_reqwest("send_message", e))?
            .json::<LarkResponse<SendMessageData>>()
            .await
            .map_err(|e| from_reqwest("send_message", e))?;

        let data = resp.check("send_message")?;
        if data.message_id.is_empty() {
            // 字段缺失 ≠ 发送失败，但也不能当成拿到了 ID：留痕后按无 ID 处理
            log_warn!(
                "lark send_message succeeded without message_id: receive_id={} type={}",
                receive_id,
                receive_id_type
            );
            return Ok(None);
        }
        Ok(Some(data.message_id))
    }

    /// 以「引用回复」形式按给定 msg_type 发送飞书消息
    ///
    /// 端点：`POST /open-apis/im/v1/messages/:message_id/reply`。
    /// `reply_in_thread = true` 时以话题形式回复（父消息属于话题时使用）。
    /// 返回值语义与 [`Self::send_message`] 一致（无 message_id 时返回 `None`）。
    async fn reply_message(
        &self,
        token: &str,
        reply_to_message_id: &str,
        msg_type: &'static str,
        content: String,
        reply_in_thread: bool,
    ) -> Result<Option<String>> {
        #[derive(Serialize)]
        struct ReplyMessageReq {
            content: String,
            msg_type: &'static str,
            reply_in_thread: bool,
        }

        #[derive(Default, Deserialize)]
        struct ReplyMessageData {
            #[serde(default)]
            message_id: String,
        }

        let url = format!(
            "{}{}",
            API_BASE,
            PATH_REPLY_MESSAGE.replace("{message_id}", reply_to_message_id)
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&ReplyMessageReq {
                content,
                msg_type,
                reply_in_thread,
            })
            .send()
            .await
            .map_err(|e| from_reqwest("reply_message", e))?
            .json::<LarkResponse<ReplyMessageData>>()
            .await
            .map_err(|e| from_reqwest("reply_message", e))?;

        let data = resp.check("reply_message")?;
        if data.message_id.is_empty() {
            log_warn!(
                "lark reply_message succeeded without message_id: reply_to={}",
                reply_to_message_id
            );
            return Ok(None);
        }
        Ok(Some(data.message_id))
    }

    /// 上传图片到飞书，返回 `image_key`（供 `msg_type:image` 使用）
    ///
    /// `image_type=message` 走消息图片（非头像），单张大小上限由上游
    /// `extract_outbound_images` 约束；业务码非 0 / 空 image_key 均视为失败，
    /// 由调用方降级为文本占位。
    pub async fn upload_image(
        &self,
        token: &str,
        name: &str,
        mime_type: &str,
        bytes: &[u8],
    ) -> Result<String> {
        #[derive(Default, Deserialize)]
        struct UploadImageData {
            #[serde(default)]
            image_key: String,
        }

        let part = reqwest::multipart::Part::bytes(bytes.to_vec())
            .file_name(safe_upload_filename(name))
            .mime_str(mime_type)
            .map_err(|e| {
                err!(
                    InvalidRequest,
                    "飞书图片 MIME 类型无效: mime={} err={}",
                    mime_type,
                    e
                )
            })?;
        let form = reqwest::multipart::Form::new().part("image", part);
        let url = format!("{}{}?image_type=message", API_BASE, PATH_UPLOAD_IMAGE);
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| from_reqwest("upload_image", e))?
            .json::<LarkResponse<UploadImageData>>()
            .await
            .map_err(|e| from_reqwest("upload_image", e))?;

        let data = resp.check("upload_image")?;
        if data.image_key.is_empty() {
            return Err(err!(
                ThirdPartyError,
                "lark upload_image succeeded without image_key"
            ));
        }
        Ok(data.image_key)
    }

    /// 上传文件到飞书，返回 `file_key`（供 `msg_type:file` 使用）
    ///
    /// `file_type` 由 [`lark_file_type`] 按扩展名推断（未知用 `stream`）。
    pub async fn upload_file(&self, token: &str, name: &str, bytes: &[u8]) -> Result<String> {
        let file_type = lark_file_type(name);
        self.upload_file_as(token, name, bytes, file_type, "upload_file")
            .await
    }

    /// 上传视频到飞书（`file_type=mp4`，供 `msg_type:media` 使用）
    ///
    /// 不依赖文件名后缀：`.mov` / `.mkv` 等视频统一按 mp4 上传，否则 media 消息拒收。
    pub async fn upload_video(&self, token: &str, name: &str, bytes: &[u8]) -> Result<String> {
        self.upload_file_as(token, name, bytes, "mp4", "upload_video")
            .await
    }

    /// 按指定 `file_type` 上传文件的通用实现
    async fn upload_file_as(
        &self,
        token: &str,
        name: &str,
        bytes: &[u8],
        file_type: &'static str,
        label: &'static str,
    ) -> Result<String> {
        #[derive(Default, Deserialize)]
        struct UploadFileData {
            #[serde(default)]
            file_key: String,
        }

        let part =
            reqwest::multipart::Part::bytes(bytes.to_vec()).file_name(safe_upload_filename(name));
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("file_name", safe_upload_filename(name))
            .text("file_type", file_type);
        let url = format!("{API_BASE}{PATH_UPLOAD_FILE}");
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| from_reqwest(label, e))?
            .json::<LarkResponse<UploadFileData>>()
            .await
            .map_err(|e| from_reqwest(label, e))?;

        let data = resp.check(label)?;
        if data.file_key.is_empty() {
            return Err(err!(
                ThirdPartyError,
                "lark upload_file succeeded without file_key"
            ));
        }
        Ok(data.file_key)
    }

    /// 按 `msg_type` + `content` 发送媒体消息（image / file / media 通用发送口）
    ///
    /// 这是所有非文本消息的唯一发送实现：调用方按附件的媒体类型算出
    /// `msg_type` 与 content JSON，本方法只管发。新增媒体类型无需再加发送方法。
    async fn send_media_message(
        &self,
        token: &str,
        receive_id: &str,
        receive_id_type: &str,
        msg_type: &'static str,
        content: String,
        label: &'static str,
    ) -> Result<Option<String>> {
        #[derive(Serialize)]
        struct SendMessageReq<'a> {
            receive_id: &'a str,
            msg_type: &'static str,
            content: String,
        }
        #[derive(Default, Deserialize)]
        struct SendMessageData {
            #[serde(default)]
            message_id: String,
        }

        let url = format!(
            "{}{}?receive_id_type={}",
            API_BASE, PATH_SEND_MESSAGE, receive_id_type
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&SendMessageReq {
                receive_id,
                msg_type,
                content,
            })
            .send()
            .await
            .map_err(|e| from_reqwest(label, e))?
            .json::<LarkResponse<SendMessageData>>()
            .await
            .map_err(|e| from_reqwest(label, e))?;

        let data = resp.check(label)?;
        if data.message_id.is_empty() {
            log_warn!("lark {label} succeeded without message_id");
            return Ok(None);
        }
        Ok(Some(data.message_id))
    }

    /// 按 `msg_type` + `content` 以引用回复形式发送媒体消息（与发送口成对）
    async fn reply_media_message(
        &self,
        token: &str,
        reply_to_message_id: &str,
        msg_type: &'static str,
        content: String,
        reply_in_thread: bool,
        label: &'static str,
    ) -> Result<Option<String>> {
        #[derive(Serialize)]
        struct ReplyMessageReq {
            content: String,
            msg_type: &'static str,
            reply_in_thread: bool,
        }
        #[derive(Default, Deserialize)]
        struct ReplyMessageData {
            #[serde(default)]
            message_id: String,
        }

        let url = format!(
            "{}{}",
            API_BASE,
            PATH_REPLY_MESSAGE.replace("{message_id}", reply_to_message_id)
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(token)
            .json(&ReplyMessageReq {
                content,
                msg_type,
                reply_in_thread,
            })
            .send()
            .await
            .map_err(|e| from_reqwest(label, e))?
            .json::<LarkResponse<ReplyMessageData>>()
            .await
            .map_err(|e| from_reqwest(label, e))?;

        let data = resp.check(label)?;
        if data.message_id.is_empty() {
            log_warn!("lark {label} succeeded without message_id");
            return Ok(None);
        }
        Ok(Some(data.message_id))
    }

    /// 发送图片消息到指定接收方（`msg_type:image`）
    pub async fn send_image_message(
        &self,
        token: &str,
        receive_id: &str,
        receive_id_type: &str,
        image_key: &str,
    ) -> Result<Option<String>> {
        self.send_media_message(
            token,
            receive_id,
            receive_id_type,
            "image",
            image_message_content(image_key),
            "send_image",
        )
        .await
    }

    /// 以引用回复形式发送图片消息
    pub async fn reply_image_message(
        &self,
        token: &str,
        reply_to_message_id: &str,
        image_key: &str,
        reply_in_thread: bool,
    ) -> Result<Option<String>> {
        self.reply_media_message(
            token,
            reply_to_message_id,
            "image",
            image_message_content(image_key),
            reply_in_thread,
            "reply_image",
        )
        .await
    }

    /// 发送文件消息到指定接收方（`msg_type:file`）
    pub async fn send_file_message(
        &self,
        token: &str,
        receive_id: &str,
        receive_id_type: &str,
        file_key: &str,
    ) -> Result<Option<String>> {
        self.send_media_message(
            token,
            receive_id,
            receive_id_type,
            "file",
            file_message_content(file_key),
            "send_file",
        )
        .await
    }

    /// 以引用回复形式发送文件消息
    pub async fn reply_file_message(
        &self,
        token: &str,
        reply_to_message_id: &str,
        file_key: &str,
        reply_in_thread: bool,
    ) -> Result<Option<String>> {
        self.reply_media_message(
            token,
            reply_to_message_id,
            "file",
            file_message_content(file_key),
            reply_in_thread,
            "reply_file",
        )
        .await
    }

    /// 发送视频消息到指定接收方（`msg_type:media`，无封面）
    pub async fn send_video_message(
        &self,
        token: &str,
        receive_id: &str,
        receive_id_type: &str,
        file_key: &str,
    ) -> Result<Option<String>> {
        self.send_media_message(
            token,
            receive_id,
            receive_id_type,
            "media",
            media_message_content(file_key),
            "send_media",
        )
        .await
    }

    /// 以引用回复形式发送视频消息（`msg_type:media`）
    pub async fn reply_video_message(
        &self,
        token: &str,
        reply_to_message_id: &str,
        file_key: &str,
        reply_in_thread: bool,
    ) -> Result<Option<String>> {
        self.reply_media_message(
            token,
            reply_to_message_id,
            "media",
            media_message_content(file_key),
            reply_in_thread,
            "reply_media",
        )
        .await
    }

    /// 逐条上传并发送附件，按统一媒体类型分派：
    /// - `Image` → `upload_image` + `msg_type:image`
    /// - `Video` → `upload_video`(file_type=mp4) + `msg_type:media`
    /// - `File` / `Audio` → `upload_file` + `msg_type:file`（音视频转码能力缺失时保底）
    ///
    /// 单条失败仅告警跳过，返回成功条数。文本推送已经成功，附件属于附加能力：
    /// 任何一条的上传/发送失败都不改变 `push` 的返回值，也不影响 `external_key` 回写。
    async fn push_attachments(
        &self,
        ctx: &RequestContext,
        token: &str,
        reply: Option<&LarkReply>,
        receive: Option<(&str, &str)>,
        attachments: &[crate::models::outbound_media::OutboundAttachment],
    ) -> usize {
        use crate::models::outbound_media::OutboundMediaKind;

        let mut sent = 0;
        for attachment in attachments {
            let kind = attachment.kind();
            // 图片走 images 端点；视频强制 mp4 上传；其余走 files 端点
            let key = if kind == OutboundMediaKind::Image {
                self.upload_image(
                    token,
                    &attachment.name,
                    &attachment.mime_type,
                    &attachment.bytes,
                )
                .await
            } else if kind == OutboundMediaKind::Video {
                self.upload_video(token, &attachment.name, &attachment.bytes)
                    .await
            } else {
                self.upload_file(token, &attachment.name, &attachment.bytes)
                    .await
            };
            let key = match key {
                Ok(key) => key,
                Err(e) => {
                    log_warn!(
                        ctx,
                        "lark_push_attachments",
                        "附件上传失败，跳过: name={} size={} kind={:?} err={}",
                        attachment.name,
                        attachment.bytes.len(),
                        kind,
                        e
                    );
                    continue;
                }
            };
            let result = match (kind, reply) {
                (OutboundMediaKind::Image, Some(r)) => {
                    self.reply_image_message(token, &r.message_id, &key, r.in_thread)
                        .await
                }
                (OutboundMediaKind::Video, Some(r)) => {
                    self.reply_video_message(token, &r.message_id, &key, r.in_thread)
                        .await
                }
                (_, Some(r)) => {
                    self.reply_file_message(token, &r.message_id, &key, r.in_thread)
                        .await
                }
                (_, None) => match receive {
                    Some((receive_id, receive_id_type)) => match kind {
                        OutboundMediaKind::Image => {
                            self.send_image_message(token, receive_id, receive_id_type, &key)
                                .await
                        }
                        OutboundMediaKind::Video => {
                            self.send_video_message(token, receive_id, receive_id_type, &key)
                                .await
                        }
                        _ => {
                            self.send_file_message(token, receive_id, receive_id_type, &key)
                                .await
                        }
                    },
                    None => {
                        log_warn!(ctx, "lark_push_attachments", "附件发送缺少接收方，跳过");
                        continue;
                    }
                },
            };
            match result {
                Ok(_) => sent += 1,
                Err(e) => log_warn!(
                    ctx,
                    "lark_push_attachments",
                    "附件消息发送失败: name={} kind={:?} err={}",
                    attachment.name,
                    kind,
                    e
                ),
            }
        }
        sent
    }

    /// HTTP client 引用（供 ws 模块使用）
    pub fn http_client(&self) -> &reqwest::Client {
        &self.http
    }
}

impl Default for LarkDaoHttpImpl {
    fn default() -> Self {
        Self::new()
    }
}

// ==================== token 获取（自由函数，供 DAO 与 WS token source 共享） ====================

/// 获取指定应用的 tenant_access_token（带缓存，提前 5 分钟刷新，双重检查锁防并发刷新）
async fn fetch_token_with_caches(
    http: &reqwest::Client,
    caches: &RwLock<HashMap<String, SharedTokenCache>>,
    app_id: &str,
    app_secret: &str,
) -> Result<String> {
    let cache = LarkDaoHttpImpl::token_cache_for(caches, app_id).await;
    // 第一次读检查
    {
        let c = cache.read().await;
        if let Some(token) = c.get_valid_token() {
            return Ok(token);
        }
    }
    // 升级写锁
    let mut c = cache.write().await;
    // 第二次检查（防止等待期间其他任务已刷新）
    if let Some(token) = c.get_valid_token() {
        return Ok(token);
    }
    let (token, expire) = fetch_tenant_access_token(http, app_id, app_secret).await?;
    c.update(token.clone(), expire);
    Ok(token)
}

/// 调用飞书 API 获取 tenant_access_token
async fn fetch_tenant_access_token(
    http: &reqwest::Client,
    app_id: &str,
    app_secret: &str,
) -> Result<(String, u64)> {
    validate_config(app_id, app_secret)?;

    #[derive(Serialize)]
    struct TokenReq<'a> {
        app_id: &'a str,
        app_secret: &'a str,
    }

    #[derive(Deserialize)]
    struct TokenResp {
        code: i32,
        #[serde(default)]
        msg: String,
        #[serde(default)]
        tenant_access_token: String,
        #[serde(default)]
        expire: u64,
    }

    let url = format!("{}{}", API_BASE, PATH_TOKEN);
    let resp = http
        .post(&url)
        .json(&TokenReq { app_id, app_secret })
        .send()
        .await
        .map_err(|e| from_reqwest("fetch_token", e))?
        .json::<TokenResp>()
        .await
        .map_err(|e| from_reqwest("fetch_token", e))?;

    if resp.code != 0 {
        return Err(err!(
            ThirdPartyError,
            "lark fetch_token failed: code={} msg={}",
            resp.code,
            resp.msg
        ));
    }
    if resp.tenant_access_token.is_empty() {
        return Err(err!(
            ThirdPartyError,
            "lark fetch_token returned empty token"
        ));
    }
    Ok((resp.tenant_access_token, resp.expire))
}

// 出站凭证解析归 DAL 层（dal::message_channel）：内联字段已在二期重构中删除，
// 渠道仅存引用，DAL 解析后以 LarkAppCredentials 传入本 DAO。

#[async_trait::async_trait]
impl LarkDao for LarkDaoHttpImpl {
    async fn push(
        &self,
        ctx: RequestContext,
        message: &Message,
        channel: &MessageChannel,
        credentials: &LarkAppCredentials,
        reply: Option<&LarkReply>,
        attachments: &[crate::models::outbound_media::OutboundAttachment],
    ) -> Result<Option<String>> {
        let config = channel.config();
        let content = &message.po.content;
        if content.is_empty() {
            return Ok(None);
        }

        let token = self
            .get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        // 有回复参数走回复端点（飞书按父消息定位会话，话题消息以话题形式回复）；
        // 无回复参数时：消息带会话 ID（群聊/私聊均有）则按会话直发，
        // 否则回退私聊 open_id。
        // 无回复参数时解析接收方（群聊 chat_id 优先，私聊 open_id 兜底）；
        // 有回复参数时走回复端点，无需接收方。图片发送复用同一目标。
        let receive = if reply.is_none() {
            Some(
                match message.po.chat_id.as_deref().filter(|s| !s.is_empty()) {
                    Some(chat_id) => (chat_id, "chat_id"),
                    None => {
                        let open_id = config
                            .lark_open_id
                            .as_deref()
                            .filter(|s| !s.is_empty())
                            .ok_or_else(|| {
                                err!(
                                    InvalidRequest,
                                    "飞书渠道缺少 lark_open_id 配置 channel_id={}",
                                    channel.po.id
                                )
                            })?;
                        (open_id, "open_id")
                    }
                },
            )
        } else {
            None
        };
        // 统一渲染：markdown 原文走 post + md 标签，空/超限回落 text；
        // content 为已序列化的飞书原生 JSON 串（见 outbound_render）。
        let (msg_type, rendered) = super::outbound_render::render_outbound(content);
        let message_id = match reply {
            Some(r) => {
                self.reply_message(&token, &r.message_id, msg_type, rendered, r.in_thread)
                    .await?
            }
            None => {
                let (receive_id, receive_id_type) = receive.expect("reply=None 时已解析");
                self.send_message(&token, receive_id, receive_id_type, msg_type, rendered)
                    .await?
            }
        };

        // 附件逐条上传 + 独立发送（图片 image / 其余 file）；单条失败仅告警，
        // 不影响文本推送结果和外部键回写。
        if !attachments.is_empty() {
            let sent = self
                .push_attachments(&ctx, &token, reply, receive, attachments)
                .await;
            log_info!(
                &ctx,
                "lark_push_attachments",
                "channel_id={} total={} sent={}",
                channel.po.id,
                attachments.len(),
                sent
            );
        }

        log_info!(
            &ctx,
            "lark_push",
            "推送消息到飞书 channel_id={} app_id={} chat_id={:?} open_id={:?} reply_to={:?} lark_message_id={:?}",
            channel.po.id,
            credentials.app_id,
            message.po.chat_id,
            config.lark_open_id,
            reply.map(|r| r.message_id.as_str()),
            message_id
        );
        Ok(message_id)
    }

    async fn add_reaction(
        &self,
        ctx: RequestContext,
        credentials: &LarkAppCredentials,
        message_id: &str,
        emoji_type: &str,
    ) -> Result<()> {
        let message_id = message_id.trim();
        let emoji_type = emoji_type.trim();
        if message_id.is_empty() || emoji_type.is_empty() {
            return Err(err!(
                InvalidRequest,
                "飞书回执缺少 message_id 或 emoji_type"
            ));
        }
        let token = self
            .get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        let url = format!(
            "{}{}",
            API_BASE,
            PATH_ADD_REACTION.replace("{message_id}", message_id)
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "reaction_type": { "emoji_type": emoji_type }
            }))
            .send()
            .await
            .map_err(|e| from_reqwest("add_reaction", e))?
            .json::<LarkResponse<serde_json::Value>>()
            .await
            .map_err(|e| from_reqwest("add_reaction", e))?;
        resp.check("add_reaction")?;
        log_debug!(
            &ctx,
            "lark_dao",
            "飞书回执已添加 message_id={} emoji_type={}",
            message_id,
            emoji_type
        );
        Ok(())
    }

    async fn get_bot_open_id(&self, credentials: &LarkAppCredentials) -> Result<Option<String>> {
        // `/bot/v3/info` 的响应体为 `{code,msg,bot:{...}}`（不套 data），
        // 不能复用 `LarkResponse<T>` 的 data 包装。
        #[derive(Default, Deserialize)]
        struct BotInfoData {
            #[serde(default)]
            open_id: String,
        }
        #[derive(Default, Deserialize)]
        struct BotInfoResp {
            #[serde(default)]
            code: i32,
            #[serde(default)]
            msg: String,
            #[serde(default)]
            bot: Option<BotInfoData>,
        }

        let token = self
            .get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        let url = format!("{}{}", API_BASE, PATH_BOT_INFO);
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| from_reqwest("bot_info", e))?
            .json::<BotInfoResp>()
            .await
            .map_err(|e| from_reqwest("bot_info", e))?;
        if resp.code != 0 {
            return Err(err!(
                ThirdPartyError,
                "lark bot_info failed: code={} msg={}",
                resp.code,
                resp.msg
            ));
        }
        Ok(resp
            .bot
            .map(|bot| bot.open_id)
            .filter(|open_id| !open_id.is_empty()))
    }

    async fn download_image_resource(
        &self,
        ctx: RequestContext,
        credentials: &LarkAppCredentials,
        message_id: &str,
        file_key: &str,
    ) -> Result<LarkImageResource> {
        let token = self
            .get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        let url = format!(
            "{}{}",
            API_BASE,
            PATH_MESSAGE_RESOURCE
                .replace("{message_id}", message_id)
                .replace("{file_key}", file_key)
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .query(&[("type", "image")])
            .send()
            .await
            .map_err(|e| from_reqwest("download_image_resource", e))?;

        let status = resp.status();
        if !status.is_success() {
            // 飞书错误响应是 JSON（code/msg），只留前 300 字符避免灌日志
            let body = resp.text().await.unwrap_or_default();
            let snippet: String = body.chars().take(300).collect();
            return Err(err!(
                ThirdPartyError,
                "lark download_image_resource failed: status={} message_id={} body={}",
                status.as_u16(),
                message_id,
                snippet
            ));
        }

        // MIME 以响应头为准；非 image/* 或缺失时按常见图片类型兜底
        let mime_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.split(';').next().unwrap_or(v).trim().to_string())
            .filter(|v| v.starts_with("image/"))
            .unwrap_or_else(|| "image/jpeg".to_string());

        if let Some(len) = resp.content_length()
            && len as usize > crate::pkg::adapter::MAX_INBOUND_ATTACHMENT_BYTES
        {
            return Err(err!(
                InvalidRequest,
                "lark image too large: message_id={} bytes={} limit={}",
                message_id,
                len,
                crate::pkg::adapter::MAX_INBOUND_ATTACHMENT_BYTES
            ));
        }
        let mut bytes: Vec<u8> = Vec::new();
        let mut resp = resp;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| from_reqwest("download_image_resource", e))?
        {
            // 边读边限流：content-length 缺失（chunked）时也不能把整包读进内存
            if bytes.len() + chunk.len() > crate::pkg::adapter::MAX_INBOUND_ATTACHMENT_BYTES {
                return Err(err!(
                    InvalidRequest,
                    "lark image exceeds size limit: message_id={} limit={}",
                    message_id,
                    crate::pkg::adapter::MAX_INBOUND_ATTACHMENT_BYTES
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        if bytes.is_empty() {
            return Err(err!(
                ThirdPartyError,
                "lark download_image_resource returned empty body: message_id={}",
                message_id
            ));
        }
        if bytes.len() > crate::pkg::adapter::MAX_INBOUND_ATTACHMENT_BYTES {
            return Err(err!(
                InvalidRequest,
                "lark image too large: message_id={} bytes={} limit={}",
                message_id,
                bytes.len(),
                crate::pkg::adapter::MAX_INBOUND_ATTACHMENT_BYTES
            ));
        }

        log_info!(
            &ctx,
            "lark_image_download",
            "入站图片下载成功 message_id={} file_key={} mime={} bytes={}",
            message_id,
            file_key,
            mime_type,
            bytes.len()
        );
        Ok(LarkImageResource { bytes, mime_type })
    }

    async fn test_connection(
        &self,
        ctx: RequestContext,
        credentials: &LarkAppCredentials,
    ) -> Result<()> {
        self.get_tenant_access_token(&credentials.app_id, &credentials.app_secret)
            .await?;
        log_info!(
            &ctx,
            "lark_test_connection",
            "飞书连接测试成功 app_id={}",
            credentials.app_id
        );
        Ok(())
    }

    async fn start_event_listener(&self, credentials: LarkAppCredentials) -> Result<()> {
        validate_config(&credentials.app_id, &credentials.app_secret)?;

        // 幂等：已连接直接返回
        {
            let conns = self.ws_conns.read().await;
            if conns.contains_key(&credentials.app_id) {
                log_debug!(
                    "lark event listener already running for app_id={}",
                    credentials.app_id
                );
                return Ok(());
            }
        }

        // 长连接只认**应用凭证**（AppID/AppSecret 直接进 `POST /callback/ws/endpoint` 请求体），
        // **不需要** tenant_access_token —— 那是 REST 出站那条线的产物（两条鉴权线互不相干）
        let mut conns = self.ws_conns.write().await;
        // 双重检查（防止等待期间其他任务已建连）
        if conns.contains_key(&credentials.app_id) {
            return Ok(());
        }

        let app_id = credentials.app_id.clone();
        let state = super::ws::start_event_loop(self.http.clone(), credentials).await?;

        conns.insert(app_id.clone(), state);
        log_info!("lark event listener started for app_id={}", app_id);
        Ok(())
    }

    async fn stop_event_listener(&self, app_id: &str) -> Result<()> {
        let state = self.ws_conns.write().await.remove(app_id);
        if let Some(state) = state {
            super::ws::stop_event_loop(state).await;
            log_info!("lark event listener stopped for app_id={}", app_id);
        }
        Ok(())
    }

    async fn stop_all_event_listeners(&self) -> Result<()> {
        let states: Vec<(String, WsState)> = self.ws_conns.write().await.drain().collect();
        for (app_id, state) in states {
            super::ws::stop_event_loop(state).await;
            log_info!("lark event listener stopped for app_id={}", app_id);
        }
        Ok(())
    }

    async fn is_listening(&self, app_id: &str) -> bool {
        self.ws_conns.read().await.contains_key(app_id)
    }

    async fn listener_stats(&self) -> LarkWsMetrics {
        let conns = self.ws_conns.read().await;
        let mut apps = Vec::with_capacity(conns.len());
        for (app_id, state) in conns.iter() {
            let snap = state.conn_state_snapshot().await;
            apps.push(LarkWsAppMetrics {
                app_id: app_id.clone(),
                state: snap.phase.as_str().to_string(),
                reconnect_count: snap.reconnect_count,
                frames_received: snap.frames_received,
                // 0 = 从未收到（与微信 `last_poll_at_ms` 口径一致）
                last_frame_at_ms: snap.last_frame_at_ms.unwrap_or(0),
                last_close_code: snap.last_close_code.map(i64::from).unwrap_or(0),
                last_close_reason: snap.last_close_reason.clone(),
                terminal_reason: snap.terminal_reason.clone(),
            });
        }
        LarkWsMetrics {
            active_connections: apps.len() as u64,
            apps,
        }
    }
}

// 防止未使用警告（event 模块在 push 链路中被 trait 间接使用）
#[allow(dead_code)]
fn _ensure_event_linked(_: &LarkMessageEvent) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::dao::lark::LarkDao;

    /// 图片 content 与文件名清洗（纯函数，无网络）
    #[test]
    fn image_helpers_wrap_key_and_sanitize_filename() {
        assert_eq!(
            image_message_content("img_v2_abc"),
            r#"{"image_key":"img_v2_abc"}"#
        );
        assert_eq!(safe_upload_filename("截图.png"), "截图.png");
        assert_eq!(safe_upload_filename("a\"b\\c\r\nd.png"), "a_b_c__d.png");
        assert_eq!(safe_upload_filename("  "), "image");
    }

    /// 文件 content 与 file_type 按扩展名推断（未知一律 stream 兜底）
    #[test]
    fn file_helpers_wrap_key_and_map_file_type() {
        assert_eq!(
            file_message_content("file_v2_abc"),
            r#"{"file_key":"file_v2_abc"}"#
        );
        assert_eq!(lark_file_type("季度报告.PDF"), "pdf");
        assert_eq!(lark_file_type("合同.docx"), "doc");
        assert_eq!(lark_file_type("名单.XLSX"), "xls");
        assert_eq!(lark_file_type("汇总.csv"), "xls");
        assert_eq!(lark_file_type("老表.xls"), "xls");
        assert_eq!(lark_file_type("方案.pptx"), "ppt");
        // 视频必须 mp4 上传，否则 msg_type:media 拒收
        assert_eq!(lark_file_type("片段.MP4"), "mp4");
        assert_eq!(lark_file_type("demo.mp4"), "mp4");
        assert_eq!(lark_file_type("data.json"), "stream");
        assert_eq!(lark_file_type("无扩展名"), "stream");
    }

    /// 视频 content（`msg_type:media`）：只带 file_key，封面可选故省略
    #[test]
    fn media_helpers_wrap_key_without_cover() {
        assert_eq!(
            media_message_content("file_v2_video"),
            r#"{"file_key":"file_v2_video"}"#
        );
    }

    /// per-app token 缓存隔离：不同 app_id 各自独立条目
    #[tokio::test]
    async fn token_cache_is_isolated_per_app() {
        let dao = LarkDaoHttpImpl::new();
        let _ = LarkDaoHttpImpl::token_cache_for(&dao.token_caches, "cli_app_a").await;
        let _ = LarkDaoHttpImpl::token_cache_for(&dao.token_caches, "cli_app_b").await;
        // 重复访问同一 app 不新增条目
        let _ = LarkDaoHttpImpl::token_cache_for(&dao.token_caches, "cli_app_a").await;
        let caches = dao.token_caches.read().await;
        assert_eq!(caches.len(), 2);
        assert!(caches.contains_key("cli_app_a"));
        assert!(caches.contains_key("cli_app_b"));
    }

    /// WS 连接池初始为空，stop 对不存在的 app 幂等不报错；无监听时 stats 为空快照
    #[tokio::test]
    async fn listener_state_starts_empty_and_stop_is_idempotent() {
        let dao = LarkDaoHttpImpl::new();
        assert!(!dao.is_listening("cli_app_x").await);
        dao.stop_event_listener("cli_app_x").await.unwrap();
        dao.stop_all_event_listeners().await.unwrap();
        assert!(!dao.is_listening("cli_app_x").await);
        let stats = dao.listener_stats().await;
        assert_eq!(stats.active_connections, 0);
        assert!(stats.apps.is_empty());
    }
}
