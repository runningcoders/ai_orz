//! 邮件渠道 DAO IMAP 入站实现（受管轮询）
//!
//! 轮询单元 = 一个代理邮箱（EmailBot 凭证），而非单个渠道：
//! 同一邮箱可能被 N 个渠道共用（与微信"一个 bot 微信号 = 一个 channel"不同），
//! 收件方 From 与渠道 `email_to_address` 的二维路由匹配由消费侧 DAL 完成——
//! 本模块只负责"按邮箱凭证拉新邮件 → MIME 解析 → publish [`EmailInboundEvent`]"。
//!
//! 阶段二策略（轻实现）：
//! - 每 tick 新建连接（TCP + TLS + LOGIN + EXAMINE + 拉取 + LOGOUT），
//!   60s 固定间隔——用户自建代理邮箱，不维持长驻会话；
//! - `BODY.PEEK[]` 拉取，不置 `\Seen` 标记，不污染用户真实邮箱已读状态；
//! - UID 游标仅存内存：首次连接以邮箱当前最大 UID 为基线（跳过历史邮件），
//!   重启后重新基线化——落库级幂等由 `email:<Message-ID>` 外部键在 DAL 去重兜底；
//! - UIDVALIDITY 变更（邮箱重建/迁移）→ 游标重新基线化，不产生重复事件。

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use mailparse::{MailAddr, MailHeaderMap, ParsedMail, addrparse_header, parse_mail};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_native_tls::{TlsConnector, TlsStream};

use crate::models::events::EmailInboundEvent;
use crate::models::user_credential::UserCredentialPo;
use crate::pkg::RequestContext;
use common::error::{Result, err};

// ==================== 凭证 ====================

/// 邮件渠道 IMAP 运行凭证（由 DAL 层按 `email_credential_id` 引用解析后传入，
/// DAO 不做凭证解析——与 SMTP 侧 [`super::smtp::EmailSmtpCredentials`] 同构）
#[derive(Clone)]
pub struct EmailImapCredentials {
    /// 凭证行 ID（轮询 registry 键控 + 事件 order_key）
    pub credential_id: String,
    /// 代理邮箱地址（INBOX 归属，如 bot@qq.com）
    pub email_address: String,
    /// IMAP 主机（如 imap.qq.com）
    pub imap_host: String,
    /// IMAP 端口（993 = 隐式 TLS）
    pub imap_port: u16,
    /// 登录账号（多数提供商与邮箱地址相同）
    pub username: String,
    /// 登录密码 / 授权码（已解密）
    pub password: String,
}

impl std::fmt::Debug for EmailImapCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailImapCredentials")
            .field("credential_id", &self.credential_id)
            .field("email_address", &self.email_address)
            .field("imap_host", &self.imap_host)
            .field("imap_port", &self.imap_port)
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

impl EmailImapCredentials {
    /// 凭证指纹：任一连接参数变化 → 轮询循环停旧重建（ensure 幂等依据）
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.email_address.hash(&mut hasher);
        self.imap_host.hash(&mut hasher);
        self.imap_port.hash(&mut hasher);
        self.username.hash(&mut hasher);
        self.password.hash(&mut hasher);
        hasher.finish()
    }
}

/// 从凭证行解析邮件渠道 IMAP 运行凭证（纯函数，可测）
///
/// 解析规则：凭证行（已由调用方按 `email_credential_id` 查得）→
/// 校验 kind=EmailBot → 解密授权码 → IMAP 必备参数完整性校验；
/// 任一环节失败返回引导性错误（轮询单元为凭证本身，无需渠道上下文）。
pub fn resolve_imap_credentials(credential: &UserCredentialPo) -> Result<EmailImapCredentials> {
    let credential_id = credential.id.as_str();
    if credential.kind != common::models::CredentialKind::EmailBot {
        return Err(err!(
            InvalidRequest,
            "邮件入站引用的凭证类型不匹配 credential_id={}",
            credential_id
        ));
    }
    let common::models::CredentialDetail::EmailBot {
        email_address,
        imap_host,
        imap_port,
        username,
        password,
        ..
    } = &credential.detail.0
    else {
        return Err(err!(
            InvalidRequest,
            "邮件入站引用的凭证类型不匹配 credential_id={}",
            credential_id
        ));
    };
    let password = crate::pkg::crypto::decrypt_channel_secret(password).map_err(|e| {
        err!(
            Internal,
            "邮件凭证授权码解密失败 credential_id={}: {}",
            credential_id,
            e
        )
    })?;
    if email_address.is_empty()
        || imap_host.is_empty()
        || *imap_port == 0
        || username.is_empty()
        || password.is_empty()
    {
        return Err(err!(
            InvalidRequest,
            "邮件凭证缺少 IMAP 必备参数（邮箱地址/主机/端口/账号/授权码）credential_id={}，请到身份凭证页补全",
            credential_id
        ));
    }
    Ok(EmailImapCredentials {
        credential_id: credential_id.to_string(),
        email_address: email_address.clone(),
        imap_host: imap_host.clone(),
        imap_port: *imap_port,
        username: username.clone(),
        password,
    })
}

// ==================== MIME 解析（纯函数，可测） ====================

/// 从已解析邮件提取 Message-ID 幂等键（保留 `<>` 包裹的原样形态）
///
/// 缺失时以原始报文 sha256 前 32 位兜底（`no-id-<hash>`，跨重启稳定）。
fn extract_message_key(parsed: &ParsedMail<'_>, raw: &[u8]) -> String {
    parsed
        .headers
        .get_first_value("Message-ID")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| format!("no-id-{}", &sha256::digest(raw)[..32]))
}

/// 从已解析邮件提取发件人地址（规范化小写，供二维路由匹配）
///
/// 取首个地址（单地址或分组内首个）；解析失败回落 `unknown@unknown.invalid`
/// （该值不可能命中任何渠道 `email_to_address`，事件将被消费侧安全丢弃）。
fn extract_from(parsed: &ParsedMail<'_>) -> String {
    parsed
        .headers
        .get_first_header("From")
        .and_then(|h| addrparse_header(h).ok())
        .and_then(|list| list.iter().next().cloned())
        .map(|addr| match addr {
            MailAddr::Single(info) => info.addr,
            MailAddr::Group(group) => group
                .addrs
                .into_iter()
                .next()
                .map(|i| i.addr)
                .unwrap_or_default(),
        })
        .map(|a| a.trim().to_lowercase())
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| "unknown@unknown.invalid".to_string())
}

/// 从已解析邮件提取主题（mailparse `get_value` 已内置 RFC2047 解码与折行合并）
fn extract_subject(parsed: &ParsedMail<'_>) -> String {
    parsed
        .headers
        .get_first_value("Subject")
        .map(|v| v.trim().to_string())
        .unwrap_or_default()
}

/// 提取纯文本正文：优先 text/plain，缺失降级 text/html 剥标签
fn extract_text_content(parsed: &ParsedMail<'_>) -> String {
    let mut plain: Option<String> = None;
    let mut html: Option<String> = None;
    collect_text_parts(parsed, &mut plain, &mut html);
    if let Some(p) = plain {
        return p;
    }
    html.map(|h| strip_html(&h)).unwrap_or_default()
}

/// 邮件正文换行规范化：CRLF/CR → LF（IMAP 原文为 CRLF，落库与渲染按 LF）
fn normalize_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// 深度优先收集首个非空 text/plain 与首个 text/html
fn collect_text_parts<'a>(
    part: &'a ParsedMail<'a>,
    plain: &mut Option<String>,
    html: &mut Option<String>,
) {
    let mimetype = part.ctype.mimetype.as_str();
    if mimetype == "text/plain" && plain.is_none() {
        if let Ok(body) = part.get_body() {
            let trimmed = normalize_newlines(&body);
            let trimmed = trimmed.trim();
            if !trimmed.is_empty() {
                *plain = Some(trimmed.to_string());
            }
        }
    } else if mimetype == "text/html" && html.is_none() {
        if let Ok(body) = part.get_body() {
            *html = Some(normalize_newlines(&body));
        }
    } else if mimetype.starts_with("multipart/") {
        for sub in &part.subparts {
            collect_text_parts(sub, plain, html);
        }
    }
}

static SCRIPT_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"(?is)<script[^>]*>.*?</script>").unwrap());
static STYLE_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"(?is)<style[^>]*>.*?</style>").unwrap());
static BREAK_RE: once_cell::sync::Lazy<regex::Regex> = once_cell::sync::Lazy::new(|| {
    regex::Regex::new(r"(?i)<br\s*/?>|</(p|div|tr|li|h[1-6])>").unwrap()
});
static TAG_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"<[^>]+>").unwrap());
static BLANK_LINES_RE: once_cell::sync::Lazy<regex::Regex> =
    once_cell::sync::Lazy::new(|| regex::Regex::new(r"\n{3,}").unwrap());

/// 最小化 HTML 剥标签：去 script/style 块 → 块级标签转换行 → 去剩余标签 → 实体解码 → 收敛空行
fn strip_html(html: &str) -> String {
    let text = SCRIPT_RE.replace_all(html, "");
    let text = STYLE_RE.replace_all(&text, "");
    let text = BREAK_RE.replace_all(&text, "\n");
    let text = TAG_RE.replace_all(&text, "");
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'");
    BLANK_LINES_RE.replace_all(&text, "\n\n").trim().to_string()
}

// ==================== 连接与轮询 ====================

/// IMAP 会话类型（隐式 TLS：TcpStream → native-tls → async-imap）
type ImapSession = async_imap::Session<TlsStream<TcpStream>>;

/// 连接超时（含 TCP + TLS 握手）
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// IMAP 轮询间隔（60s 固定；每 tick 新建连接，避免长驻会话被服务端踢线）
const POLL_INTERVAL_MS: u64 = 60_000;
/// 连续失败重试节奏：前 5 次间隔 2s，超过后退避 30s（对齐微信轮询）
const FAIL_RETRY_FAST_MS: u64 = 2_000;
const FAIL_RETRY_SLOW_MS: u64 = 30_000;
const FAIL_FAST_LIMIT: u32 = 5;

/// 建立 TLS 连接并登录（每 tick 新建）
async fn connect_session(credentials: &EmailImapCredentials) -> Result<ImapSession> {
    let tcp = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((credentials.imap_host.as_str(), credentials.imap_port)),
    )
    .await
    .map_err(|_| {
        err!(
            Internal,
            "邮件 IMAP 连接超时({}s) host={} port={}",
            CONNECT_TIMEOUT.as_secs(),
            credentials.imap_host,
            credentials.imap_port
        )
    })?
    .map_err(|e| {
        err!(
            Internal,
            "邮件 IMAP 连接失败 host={} port={}: {}",
            credentials.imap_host,
            credentials.imap_port,
            e
        )
    })?;
    let native_connector = tokio_native_tls::native_tls::TlsConnector::new()
        .map_err(|e| err!(Internal, "邮件 IMAP TLS 连接器构建失败: {}", e))?;
    let tls = TlsConnector::from(native_connector)
        .connect(&credentials.imap_host, tcp)
        .await
        .map_err(|e| {
            err!(
                Internal,
                "邮件 IMAP TLS 握手失败 host={}: {}",
                credentials.imap_host,
                e
            )
        })?;
    let client = async_imap::Client::new(tls);
    client
        .login(&credentials.username, &credentials.password)
        .await
        .map_err(|(e, _client)| {
            err!(
                Internal,
                "邮件 IMAP 登录失败 user={} host={}（请检查授权码与 IMAP 服务是否开启）: {}",
                credentials.username,
                credentials.imap_host,
                e
            )
        })
}

// ==================== 已确认消费的 UID（P2 的落点）====================

/// 已确认消费的 UID 游标（credential_id → 最大已确认 UID），**注入式共享存储**
///
/// **改造前**（P2 缺陷）：[`poll_with_session`] 在 publish 之后**无条件**推进
/// `PollCursor::last_uid`（原注释自称"解析失败的单封也已消费，游标照常越过"）
/// —— 事件还没被消费，游标已经越过它。一旦消费失败，同一封邮件再也不会被拉到
/// → **消息确定性丢失**：外部键去重只能防重复投递，**不等于可重放**。
///
/// **现在**：轮询循环只**读**本表作为增量拉取起点；`EmailInboundEvent.uid`
/// 已随事件带出，消费侧确认（`EmailDalImpl` 的 `on_consumed` →
/// `EmailDao::advance_inbound_cursor`）才推进。于是「上一封没消费完 → 游标不动
/// → 下一轮重拉」，重复由外部键 `email:<Message-ID>` 幂等去重吸收。
///
/// 持有方：[`ImapPollRegistry`] 创建并随轮询循环注入，[`super::smtp::EmailDaoImpl`]
/// 经 [`ImapPollRegistry::cursors`] 拿到**同一份** `Arc` 供消费确认回调写入
/// （与微信侧注入 `Arc<CursorStore>` 同构）。
pub(crate) struct UidCursorStore {
    confirmed: std::sync::RwLock<HashMap<String, u32>>,
}

impl UidCursorStore {
    pub(crate) fn new() -> Self {
        Self {
            confirmed: std::sync::RwLock::new(HashMap::new()),
        }
    }

    /// 已确认消费的最大 UID（`0` = 从未确认 → 起点由首次基线化决定）
    pub(crate) fn confirmed(&self, credential_id: &str) -> u32 {
        self.confirmed
            .read()
            .ok()
            .and_then(|m| m.get(credential_id).copied())
            .unwrap_or(0)
    }

    /// 推进已确认 UID（仅由首次基线化与消费侧回调调用；**单调不减**）
    ///
    /// `uid == 0` 忽略（0 是"未确认"的哨兵值，IMAP UID 从 1 起）。
    pub(crate) fn confirm(&self, credential_id: &str, uid: u32) {
        if uid == 0 {
            return;
        }
        if let Ok(mut m) = self.confirmed.write() {
            let entry = m.entry(credential_id.to_string()).or_insert(0);
            *entry = (*entry).max(uid);
        }
    }
}

/// UID 游标（仅存内存，重启后随首连重新基线化）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct PollCursor {
    /// UIDVALIDITY（None = 尚未基线化）
    uid_validity: Option<u32>,
    /// 已消费的最大 UID（基线语义：新游标从邮箱最大 UID 起跳）
    last_uid: u32,
}

/// 单次轮询 tick：EXAMINE INBOX → 基线化/增量拉取 → publish → 优雅登出
///
/// 返回本次 publish 的事件数；连接生命周期失败整体返回 Err（由循环重试）。
async fn poll_once(
    credentials: &EmailImapCredentials,
    cursor: &mut PollCursor,
    cursors: &UidCursorStore,
) -> Result<usize> {
    let mut session = connect_session(credentials).await?;
    let result = poll_with_session(&mut session, credentials, cursor, cursors).await;
    let _ = session.logout().await;
    result
}

async fn poll_with_session(
    session: &mut ImapSession,
    credentials: &EmailImapCredentials,
    cursor: &mut PollCursor,
    cursors: &UidCursorStore,
) -> Result<usize> {
    let mailbox = session.examine("INBOX").await.map_err(|e| {
        err!(
            Internal,
            "邮件 IMAP EXAMINE INBOX 失败 email={}: {}",
            credentials.email_address,
            e
        )
    })?;
    let validity = mailbox.uid_validity.unwrap_or(0);

    // 首次连接 / UIDVALIDITY 变更：以邮箱当前最大 UID 重新基线化（跳过历史，不产生事件）
    if cursor.uid_validity != Some(validity) {
        let all = session.uid_search("ALL").await.map_err(|e| {
            err!(
                Internal,
                "邮件 IMAP 搜索失败 email={}: {}",
                credentials.email_address,
                e
            )
        })?;
        let max_uid = all.into_iter().max().unwrap_or(0);
        log_info!(
            "email imap cursor baselined: credential_id={} email={} uid_validity={} max_uid={}",
            credentials.credential_id,
            credentials.email_address,
            validity,
            max_uid
        );
        *cursor = PollCursor {
            uid_validity: Some(validity),
            last_uid: max_uid,
        };
        // 基线化 = "已确认到 max_uid"：历史邮件不产生事件（跳过），无待确认项 → 直接推进
        cursors.confirm(&credentials.credential_id, max_uid);
        return Ok(0);
    }

    // 搜索新 UID：`n:*` 在无新邮件时仍会返回最后一封（IMAP 通配语义），需过滤 <= last
    //
    // ⚠️ 起点取**已确认消费**的最大 UID（P2），不是"已发布"的：上一封没消费完时
    // 游标不动 → 下一轮重拉同一批（外部键 `email:<Message-ID>` 幂等去重吸收重复）
    let last = cursors.confirmed(&credentials.credential_id);
    let uids: Vec<u32> = session
        .uid_search(format!("UID {}:*", last.saturating_add(1)))
        .await
        .map_err(|e| {
            err!(
                Internal,
                "邮件 IMAP 搜索失败 email={}: {}",
                credentials.email_address,
                e
            )
        })?
        .into_iter()
        .filter(|uid| *uid > last)
        .collect();

    let mut published = 0usize;
    for uid in &uids {
        // BODY.PEEK[]：不置 \Seen 标记，不污染用户真实邮箱已读状态
        let stream = session
            .uid_fetch(uid.to_string(), "(BODY.PEEK[])")
            .await
            .map_err(|e| {
                err!(
                    Internal,
                    "邮件 IMAP 拉取失败 email={} uid={}: {}",
                    credentials.email_address,
                    uid,
                    e
                )
            })?;
        let mut stream = stream;
        while let Some(fetch) = stream.next().await {
            let fetch = fetch.map_err(|e| {
                err!(
                    Internal,
                    "邮件 IMAP 拉取流中断 email={} uid={}: {}",
                    credentials.email_address,
                    uid,
                    e
                )
            })?;
            let Some(raw) = fetch.body() else {
                log_warn!(
                    "email imap fetch empty body (skip): credential_id={} uid={}",
                    credentials.credential_id,
                    uid
                );
                continue;
            };
            // 解析失败单封跳过（log + 继续），不阻塞同 tick 其余邮件
            match parse_inbound_event(credentials, *uid, raw) {
                Ok(event) => {
                    crate::pkg::aop::registry()
                        .publish(&RequestContext::new_system(), event)
                        .await;
                    published += 1;
                }
                Err(e) => {
                    log_warn!(
                        "email imap parse failed (skip): credential_id={} uid={} err={}",
                        credentials.credential_id,
                        uid,
                        e
                    );
                    // 本地永久无法处理这封（MIME 结构异常）→ 直接确认越过它：
                    // 不确认就会每轮重拉同一封、每轮刷一条 warn。
                    // 语义同消费者侧的 `Discard`：确定性放弃，但留下可审计的 warn 痕迹。
                    cursors.confirm(&credentials.credential_id, *uid);
                }
            }
        }
    }

    // ⚠️ **不在这里推进游标**（P2）：`published` 只代表"已入队"，不代表"已消费"。
    // 推进由消费侧确认后经 `EmailDao::advance_inbound_cursor` 完成（写 `UidCursorStore`）。
    // 解析失败的单封已在上面单独确认（本地永久无法处理，不越过就会每轮重拉 + 刷日志）。
    Ok(published)
}

/// 原始报文 → 入站事件（纯函数包装，可测）
fn parse_inbound_event(
    credentials: &EmailImapCredentials,
    uid: u32,
    raw: &[u8],
) -> Result<EmailInboundEvent> {
    let parsed =
        parse_mail(raw).map_err(|e| err!(Internal, "邮件 MIME 解析失败 uid={}: {}", uid, e))?;
    let from = extract_from(&parsed);
    let subject = extract_subject(&parsed);
    let body = extract_text_content(&parsed);
    let mut parts: Vec<String> = Vec::with_capacity(2);
    if !subject.is_empty() {
        parts.push(subject.clone());
    }
    if !body.is_empty() {
        parts.push(body);
    }
    Ok(EmailInboundEvent {
        credential_id: credentials.credential_id.clone(),
        from,
        message_key: extract_message_key(&parsed, raw),
        subject,
        content: parts.join("\n\n"),
        uid,
    })
}

/// 轮询循环体：收邮件 publish 事件 + 推进内存游标；失败按节奏退避重试
async fn poll_loop(
    credential_id: String,
    credentials: EmailImapCredentials,
    cursors: std::sync::Arc<UidCursorStore>,
) {
    log_info!(
        "email imap poll loop started: credential_id={} email={} host={}:{}",
        credential_id,
        credentials.email_address,
        credentials.imap_host,
        credentials.imap_port
    );
    let mut cursor = PollCursor::default();
    let mut consecutive_failures: u32 = 0;
    loop {
        match poll_once(&credentials, &mut cursor, &cursors).await {
            Err(e) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let delay = if consecutive_failures <= FAIL_FAST_LIMIT {
                    FAIL_RETRY_FAST_MS
                } else {
                    FAIL_RETRY_SLOW_MS
                };
                log_warn!(
                    "email imap poll failed (retry in {}ms): credential_id={} failures={} err={}",
                    delay,
                    credential_id,
                    consecutive_failures,
                    e
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }
            Ok(count) => {
                if consecutive_failures > 0 {
                    log_info!(
                        "email imap poll recovered: credential_id={} failures_reset={} published={}",
                        credential_id,
                        consecutive_failures,
                        count
                    );
                }
                consecutive_failures = 0;
                tokio::time::sleep(Duration::from_millis(POLL_INTERVAL_MS)).await;
            }
        }
    }
}

// ==================== 受管轮询 registry ====================

/// 轮询循环句柄：任务 + 启动时凭证指纹（ensure 幂等 / 凭证变化自动重建）
struct PollLoopHandle {
    join: tokio::task::JoinHandle<()>,
    fingerprint: u64,
}

/// 受管轮询 registry：credential_id 键控（一个代理邮箱 = 一个轮询单元）
///
/// 持有本 registry 全部轮询单元共享的 [`UidCursorStore`]：轮询循环（读作拉取起点）
/// 与消费确认回调（写，经 `EmailDao::advance_inbound_cursor`）操作**同一份**游标。
pub(crate) struct ImapPollRegistry {
    loops: RwLock<HashMap<String, PollLoopHandle>>,
    cursors: std::sync::Arc<UidCursorStore>,
}

impl Default for ImapPollRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ImapPollRegistry {
    pub fn new() -> Self {
        Self {
            loops: RwLock::new(HashMap::new()),
            cursors: std::sync::Arc::new(UidCursorStore::new()),
        }
    }

    /// 游标存储句柄（与轮询循环共享同一份 `Arc`；供 DAO 的消费确认回调写入）
    pub(crate) fn cursors(&self) -> std::sync::Arc<UidCursorStore> {
        std::sync::Arc::clone(&self.cursors)
    }

    /// 确保邮箱凭证的轮询循环以指定凭证运行（幂等）
    ///
    /// - 未运行 → 启动；
    /// - 运行中且凭证指纹相同 → no-op（幂等）；
    /// - 运行中但指纹不同（授权码 / 主机 / 端口 / 账号任一变化）→ 停旧重建。
    pub async fn ensure(&self, credentials: &EmailImapCredentials) -> Result<()> {
        let fingerprint = credentials.fingerprint();
        {
            let loops = self.loops.read().await;
            if let Some(handle) = loops.get(&credentials.credential_id)
                && handle.fingerprint == fingerprint
            {
                return Ok(());
            }
        }
        // 指纹不同或未运行：先移除旧句柄（abort 旧任务），再启动新循环
        let removed = self.stop(&credentials.credential_id).await;

        let credential_id = credentials.credential_id.clone();
        let credentials = credentials.clone();
        let cursors = std::sync::Arc::clone(&self.cursors);
        let join = tokio::spawn(poll_loop(credential_id.clone(), credentials, cursors));
        self.loops
            .write()
            .await
            .insert(credential_id.clone(), PollLoopHandle { join, fingerprint });
        if removed {
            log_info!(
                "email imap poll loop rebuilt (credentials changed): credential_id={}",
                credential_id
            );
        }
        Ok(())
    }

    /// 停止指定邮箱凭证的轮询循环（未运行时幂等返回 false）
    pub async fn stop(&self, credential_id: &str) -> bool {
        let handle = self.loops.write().await.remove(credential_id);
        match handle {
            Some(handle) => {
                handle.join.abort();
                log_info!(
                    "email imap poll loop stopped: credential_id={}",
                    credential_id
                );
                true
            }
            None => false,
        }
    }

    /// 停止全部循环（优雅退出）
    pub async fn stop_all(&self) {
        let handles: Vec<(String, PollLoopHandle)> = self.loops.write().await.drain().collect();
        let stopped = handles.len();
        for (_, handle) in handles {
            handle.join.abort();
        }
        if stopped > 0 {
            log_info!("email imap poll loops stopped, total={}", stopped);
        }
    }

    /// 指定邮箱凭证是否正在轮询
    pub async fn is_running(&self, credential_id: &str) -> bool {
        self.loops.read().await.contains_key(credential_id)
    }
}

// ==================== 单测 ====================
#[cfg(test)]
#[path = "imap_tests.rs"]
mod tests;
