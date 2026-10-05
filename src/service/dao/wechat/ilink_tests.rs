//! tests 单元测试（拆分自 ilink.rs）
//!
//! 文件瘦身：原 1923 行 → 1297 行，测试体 627 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use common::models::CredentialDetail;

fn channel() -> MessageChannel {
    MessageChannel::from_po(crate::models::message_channel::MessageChannelPo::new(
        "ch_wx_1".to_string(),
        "org_1".to_string(),
        "user_1".to_string(),
        None,
        common::enums::ChannelType::Wechat,
        "我的微信".to_string(),
        None,
        None,
        None,
        Default::default(),
        "user_1".to_string(),
    ))
}

fn credential_row(
    bot_token: &str,
    bot_id: &str,
    base_url: &str,
) -> crate::models::user_credential::UserCredentialPo {
    crate::models::user_credential::UserCredentialPo::new(
        "cred_1".to_string(),
        "org_1".to_string(),
        "user_1".to_string(),
        common::models::CredentialKind::WechatIlink,
        "iLink".to_string(),
        // 明文直存：decrypt_channel_secret 对无 enc:v1: 前缀的值透传（不依赖 master_key 配置）
        CredentialDetail::WechatIlink {
            bot_token: bot_token.to_string(),
            bot_id: bot_id.to_string(),
            user_id: None,
            base_url: base_url.to_string(),
        },
        common::models::CredentialVisibility::Private,
        "user_1".to_string(),
    )
}

/// 凭证解析：kind 校验 + bot_token 解密 + base_url 空值回落默认域
#[test]
fn test_resolve_ilink_credentials() {
    let ch = channel();
    let row = credential_row("tok_plain", "bot_1", "https://alt.example.com");
    let resolved = resolve_ilink_credentials(&row, &ch).unwrap();
    assert_eq!(resolved.bot_token, "tok_plain");
    assert_eq!(resolved.bot_id, "bot_1");
    assert_eq!(resolved.base_url, "https://alt.example.com");

    // base_url 空：回落默认接入域
    let row = credential_row("tok_plain", "bot_1", "");
    assert_eq!(
        resolve_ilink_credentials(&row, &ch).unwrap().base_url,
        ILINK_DEFAULT_BASE_URL
    );

    // kind 不匹配：报错
    let mut row = credential_row("tok", "bot_1", "https://x");
    row.kind = common::models::CredentialKind::GithubToken;
    assert!(resolve_ilink_credentials(&row, &ch).is_err());
}

/// 凭证指纹：三要素任一变化即不同；同凭证稳定
#[test]
fn test_credentials_fingerprint() {
    let a = IlinkChannelCredentials {
        bot_token: "tok".into(),
        bot_id: "bot_1".into(),
        base_url: "https://x".into(),
    };
    let same = a.clone();
    assert_eq!(a.fingerprint(), same.fingerprint());

    let mut b = a.clone();
    b.bot_id = "bot_2".into();
    assert_ne!(a.fingerprint(), b.fingerprint());

    let mut c = a.clone();
    c.base_url = "https://y".into();
    assert_ne!(a.fingerprint(), c.fingerprint());

    let mut d = a.clone();
    d.bot_token = "tok2".into();
    assert_ne!(a.fingerprint(), d.fingerprint());
}

/// getupdates 解析：游标 + 消息列表 + 超时协商；脏消息跳过但不中断整批
#[test]
fn test_parse_updates() {
    // 官方形态：数字枚举 + `text` 字段
    let body = r#"{
        "ret": 0,
        "get_updates_buf": "cur_abc",
        "longpolling_timeout_ms": 35000,
        "msgs": [
            {"from_user_id":"p1","message_id":"9001","client_id":"c1","message_type":1,"message_state":2,"context_token":"t1","item_list":[{"type":1,"text_item":{"text":"hi"}}]},
            {"item_list": 1}
        ]
    }"#;
    let updates = parse_updates(body).unwrap();
    assert_eq!(updates.cursor.as_deref(), Some("cur_abc"));
    assert_eq!(updates.messages.len(), 1, "脏消息应被跳过而非中断整批");
    assert_eq!(updates.messages[0].from_user_id, "p1");
    assert_eq!(updates.messages[0].text(), Some("hi".to_string()));
    assert!(!updates.is_error());
    assert_eq!(updates.longpolling_timeout_ms, Some(35_000));

    // 早期形态兼容：`msg_list` 字段名 + 字符串枚举 + `content` 文本字段
    let legacy = r#"{"msg_list":[{"from_user_id":"p2","client_id":"c2","message_type":"USER","message_state":"FINISH","item_list":[{"type":1,"text_item":{"content":"旧"}}]}]}"#;
    let updates = parse_updates(legacy).unwrap();
    assert_eq!(updates.cursor, None);
    assert_eq!(updates.messages.len(), 1);
    assert_eq!(updates.messages[0].message_key(), "c2");
    assert_eq!(updates.messages[0].text(), Some("旧".to_string()));

    // 空响应（服务端 hold 到期）
    let updates = parse_updates(r#"{"ret":0}"#).unwrap();
    assert!(updates.messages.is_empty());
    assert_eq!(updates.cursor, None);
    assert_eq!(updates.longpolling_timeout_ms, None);
}

/// 错误码：`ret` / `errcode` 任一非 0 即错误；`-14` 单独识别为会话失效
///
/// 回归护栏：此前这两个字段**被完全忽略**，会话过期后表现为「静默空轮次」，
/// 与「没人发消息」在日志上完全同形。
#[test]
fn test_parse_updates_error_codes() {
    let updates = parse_updates(r#"{"ret":-14,"errmsg":"session expired"}"#).unwrap();
    assert!(updates.is_error());
    assert!(updates.is_stale_token());
    assert_eq!(updates.errmsg, "session expired");

    let updates = parse_updates(r#"{"errcode":1001,"errmsg":"boom"}"#).unwrap();
    assert!(updates.is_error());
    assert!(!updates.is_stale_token(), "只有 -14 触达暂停");

    // 0 / 缺失都算正常
    assert!(!parse_updates(r#"{"ret":0}"#).unwrap().is_error());
    assert!(!parse_updates(r#"{}"#).unwrap().is_error());
}

/// 报文脱敏：令牌字段值被替换，其余内容保留（留痕路径不得泄漏会话令牌）
#[test]
fn test_redact_tokens() {
    let body = r#"{"msgs":[{"context_token":"tok_secret_value","item_list":[]}],"bot_token":"bt"}"#;
    let redacted = redact_tokens(body);
    assert!(!redacted.contains("tok_secret_value"));
    assert!(!redacted.contains("\"bt\""));
    assert!(redacted.contains(r#""context_token":"***""#));
    assert!(redacted.contains(r#""bot_token":"***""#));
    // 无令牌的报文原样返回
    let plain = r#"{"ret":0,"msgs":[]}"#;
    assert_eq!(redact_tokens(plain), plain);
}

/// 截断：短串原样、长串截断并附原始长度（按字符切，不切坏多字节）
#[test]
fn test_truncate_for_log() {
    assert_eq!(truncate_for_log("abc", 8), "abc");
    assert_eq!(truncate_for_log("12345678", 8), "12345678");
    assert_eq!(
        truncate_for_log("123456789", 8),
        "12345678…(truncated, 9 chars total)"
    );
    assert!(truncate_for_log("中文测试超长内容", 4).starts_with("中文测试"));
}

/// sendmessage 请求体：from 留空 / to 填对端 / context_token 回传 /
/// 枚举为**数字** / 文本走 `text_item.text` / 不带 `base_info`
///
/// 这条同时是回归护栏：此前 `"BOT"` / `"FINISH"` / `content` 三处都与官方不符。
#[test]
fn test_build_send_body() {
    let body = build_send_body("peer_1", "ctx_tok", "回复内容", "cid_local");
    let msg = body.get("msg").unwrap();
    assert_eq!(msg.get("from_user_id").and_then(Value::as_str), Some(""));
    assert_eq!(
        msg.get("to_user_id").and_then(Value::as_str),
        Some("peer_1")
    );
    assert_eq!(
        msg.get("context_token").and_then(Value::as_str),
        Some("ctx_tok")
    );
    // 官方是数字枚举（`2 = BOT` / `2 = FINISH`）
    assert_eq!(msg.get("message_type").and_then(Value::as_i64), Some(2));
    assert_eq!(msg.get("message_state").and_then(Value::as_i64), Some(2));
    let items = msg.get("item_list").and_then(Value::as_array).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].get("type").and_then(Value::as_i64), Some(1));
    assert_eq!(
        items[0]
            .get("text_item")
            .unwrap()
            .get("text")
            .and_then(Value::as_str),
        Some("回复内容")
    );
    // 官方 `sendmessage` 请求体只有 `msg`，不带 `base_info`
    assert!(body.get("base_info").is_none());
}

/// 出站请求头对齐官方 `buildHeaders`：身份两件套 + 鉴权 + UIN（十进制字符串 base64）
#[test]
fn test_bot_auth_headers_are_aligned() {
    let builder = wechat_ilink::bot_auth_headers(
        crate::pkg::http::presets::outbound()
            .build()
            .unwrap()
            .post("https://example.com"),
        "tok_x",
    );
    let request = builder.body("{}").build().unwrap();
    let headers = request.headers();
    assert_eq!(
        headers.get("iLink-App-Id").and_then(|v| v.to_str().ok()),
        Some("bot")
    );
    assert_eq!(
        headers
            .get("iLink-App-ClientVersion")
            .and_then(|v| v.to_str().ok()),
        Some(wechat_ilink::ILINK_APP_CLIENT_VERSION.to_string().as_str())
    );
    assert_eq!(
        headers
            .get("AuthorizationType")
            .and_then(|v| v.to_str().ok()),
        Some("ilink_bot_token")
    );
    assert_eq!(
        headers.get("Authorization").and_then(|v| v.to_str().ok()),
        Some("Bearer tok_x")
    );
    // X-WECHAT-UIN = base64(十进制字符串)，解出来应是纯数字
    let uin = headers
        .get("X-WECHAT-UIN")
        .and_then(|v| v.to_str().ok())
        .expect("X-WECHAT-UIN 必须存在");
    use base64::Engine as _;
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(uin)
        .expect("UIN 应为合法 base64");
    let decoded = String::from_utf8(decoded).expect("UIN 解出应为 UTF-8");
    assert!(
        !decoded.is_empty() && decoded.chars().all(|c| c.is_ascii_digit()),
        "UIN 解码后应为十进制字符串，实际 {decoded:?}"
    );
}

/// 版本编码：官方 `0x00MMNNPP` 规则（`1.0.11` → 65547）
#[test]
fn test_encode_client_version() {
    assert_eq!(wechat_ilink::encode_client_version("1.0.11"), 0x0001_000B);
    assert_eq!(wechat_ilink::encode_client_version("2.4.9"), 132_105);
    assert_eq!(wechat_ilink::encode_client_version("1.2"), 0x0001_0200);
    assert_eq!(wechat_ilink::encode_client_version("1"), 0x0001_0000);
    // 畸形输入不 panic（缺失段按 0）
    assert_eq!(wechat_ilink::encode_client_version(""), 0);
    assert_eq!(wechat_ilink::encode_client_version("x.y.z"), 0);
}

/// sendmessage 响应解析：ret=0 通过并取 `message_id` / 非 JSON 宽容 / ret!=0 报错
#[test]
fn test_parse_send_response() {
    assert_eq!(parse_send_response(r#"{"ret":0}"#).unwrap(), None);
    assert_eq!(parse_send_response("ok").unwrap(), None);
    // 服务端权威 ID：字符串与数字两种形态都读
    assert_eq!(
        parse_send_response(r#"{"ret":0,"message_id":"m1"}"#).unwrap(),
        Some("m1".to_string())
    );
    assert_eq!(
        parse_send_response(r#"{"ret":0,"message_id":7400000000000000123}"#).unwrap(),
        Some("7400000000000000123".to_string())
    );
    let e = parse_send_response(r#"{"ret":1001,"errmsg":"token expired"}"#).unwrap_err();
    assert!(e.to_string().contains("1001"));
}

/// registry：ensure 幂等（同指纹 no-op）/ 指纹变化重建 / stop 幂等
#[tokio::test]
async fn test_poll_loop_registry_lifecycle() {
    let registry = PollLoopRegistry::new();
    let ch = channel();
    let creds = IlinkChannelCredentials {
        bot_token: "tok".into(),
        bot_id: "bot_1".into(),
        base_url: "https://invalid.test".into(),
    };

    registry
        .ensure(
            &ch,
            &creds,
            None,
            Arc::new(CursorStore::new()),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();
    assert!(registry.is_running("ch_wx_1").await);

    // 同指纹：幂等（不重建）
    registry
        .ensure(
            &ch,
            &creds,
            None,
            Arc::new(CursorStore::new()),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();
    assert!(registry.is_running("ch_wx_1").await);

    // 指纹变化：重建（stop + start）
    let mut creds2 = creds.clone();
    creds2.bot_token = "tok2".into();
    registry
        .ensure(
            &ch,
            &creds2,
            None,
            Arc::new(CursorStore::new()),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();
    assert!(registry.is_running("ch_wx_1").await);

    // stop 幂等
    assert!(registry.stop("ch_wx_1").await);
    assert!(!registry.is_running("ch_wx_1").await);
    assert!(!registry.stop("ch_wx_1").await);

    registry.stop_all().await;
    assert!(!registry.is_running("ch_wx_1").await);
}

/// 轮询阶段判定：`paused` > `degraded` > `polling`
///
/// 暂停优先于降级是刻意的：两者处置完全不同（须重新授权 vs 等一会自愈），
/// 混在一个态里会让运维误判为「等一会就好」。
#[test]
fn test_poll_runtime_stats_state() {
    let mut s = PollRuntimeStats::new("我的微信".to_string(), "bot_1".to_string());
    assert_eq!(s.state(), "polling");
    s.consecutive_failures = 1;
    assert_eq!(s.state(), "degraded");
    s.paused_until_ms = 1;
    assert_eq!(s.state(), "paused");
    s.paused_until_ms = 0;
    s.consecutive_failures = 0;
    assert_eq!(s.state(), "polling");
}

/// 会话暂停表：pause 后进入暂停、clear 立即解除、channel 隔离
#[tokio::test]
async fn test_session_guard_lifecycle() {
    let guard = SessionGuard::new();
    assert!(!guard.is_paused("ch_a").await);
    assert_eq!(guard.remaining_ms("ch_a").await, 0);

    let until = guard.pause("ch_a").await;
    assert!(until > common::constants::utils::current_timestamp_ms());
    assert!(guard.is_paused("ch_a").await);
    assert!(guard.remaining_ms("ch_a").await > 0);
    // channel 隔离：暂停一个渠道不影响另一个
    assert!(!guard.is_paused("ch_b").await);

    guard.clear("ch_a").await;
    assert!(!guard.is_paused("ch_a").await);
}

/// 暂停到期自动清理（否则表会随渠道数无限增长）
#[tokio::test]
async fn test_session_guard_expired_is_cleared() {
    let guard = SessionGuard::new();
    guard.paused_until_ms.write().await.insert(
        "ch_a".to_string(),
        common::constants::utils::current_timestamp_ms() - 1,
    );
    assert_eq!(guard.remaining_ms("ch_a").await, 0);
    assert!(guard.paused_until_ms.read().await.is_empty());
}

/// 运行态快照：无监听 → 空；ensure 后按 channel 暴露建连时快照的展示字段
///
/// 注意此处**不断言** `state` / `last_poll_at_ms`：循环会真的去请求
/// `invalid.test` 并失败，断言初始值会 flaky（阶段判定由上面的纯单测覆盖）。
#[tokio::test]
async fn test_poll_registry_metrics_snapshot() {
    let registry = PollLoopRegistry::new();
    let cursors = Arc::new(CursorStore::new());
    assert!(registry.metrics(&cursors).await.is_empty());

    let ch = channel();
    let creds = IlinkChannelCredentials {
        bot_token: "tok".into(),
        bot_id: "bot_1".into(),
        base_url: "https://invalid.test".into(),
    };
    registry
        .ensure(
            &ch,
            &creds,
            None,
            cursors.clone(),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();

    let metrics = registry.metrics(&cursors).await;
    assert_eq!(metrics.len(), 1);
    let m = &metrics[0];
    assert_eq!(m.channel_id, "ch_wx_1");
    assert_eq!(m.bot_id, "bot_1");
    // 渠道名在建连时快照（`channel()` 构造为「我的微信」）
    assert_eq!(m.channel_name, "我的微信");
    // 尚无已确认消费 → cursor 为 None（而非空串占位）
    assert_eq!(m.cursor, None);

    // 句柄移除后快照同步清空（与 is_running 同源，不残留幽灵行）
    registry.stop_all().await;
    assert!(registry.metrics(&cursors).await.is_empty());
}

/// 监控快照里的游标是**已确认消费**值，且以摘要形式透出（不灌全量 opaque 串）
#[tokio::test]
async fn test_poll_metrics_cursor_is_confirmed_and_shortened() {
    let registry = PollLoopRegistry::new();
    let cursors = Arc::new(CursorStore::new());
    let ch = channel();
    let creds = IlinkChannelCredentials {
        bot_token: "tok".into(),
        bot_id: "bot_1".into(),
        base_url: "https://invalid.test".into(),
    };
    registry
        .ensure(
            &ch,
            &creds,
            None,
            cursors.clone(),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();

    // 消费确认推进游标（模拟 on_consumed 回调）后，快照应反映该值
    cursors.set("ch_wx_1", "abcdefghij").await;
    let metrics = registry.metrics(&cursors).await;
    assert_eq!(metrics[0].cursor.as_deref(), Some("abcdefgh…(10)"));

    registry.stop_all().await;
}

/// P2：已确认游标存储 —— 覆盖语义（opaque 不可比较）/ 空值忽略 / channel 隔离
#[tokio::test]
async fn test_cursor_store_semantics() {
    let store = CursorStore::new();
    assert_eq!(store.get("ch_a").await, None);

    store.set("ch_a", "cur_1").await;
    assert_eq!(store.get("ch_a").await.as_deref(), Some("cur_1"));

    // opaque 值只能"后来的覆盖先前的"（不可比较大小）
    store.set("ch_a", "cur_2").await;
    assert_eq!(store.get("ch_a").await.as_deref(), Some("cur_2"));

    // 空值忽略：服务端未给出新位置 → 保持原位（否则会退回从头拉）
    store.set("ch_a", "").await;
    assert_eq!(store.get("ch_a").await.as_deref(), Some("cur_2"));

    // channel 隔离
    assert_eq!(store.get("ch_b").await, None);
}

/// 日志摘要：短串原样、长串截断附长度、多字节不切坏
#[test]
fn test_short_token() {
    assert_eq!(short_token(""), "");
    assert_eq!(short_token("abc"), "abc");
    assert_eq!(short_token("12345678"), "12345678");
    assert_eq!(short_token("123456789"), "12345678…(9)");
    // 按 char 切（中文游标不会出现半个字符）：前 8 个字符 = 游标一二三四五六
    assert_eq!(
        short_token("游标一二三四五六七八九十"),
        "游标一二三四五六…(12)"
    );
}

/// 收帧键摘要：缺失键标 `<auto>`，超出上限只报数量
#[test]
fn test_brief_message_keys() {
    let mk = |key: &str| IlinkMessage {
        client_id: key.to_string(),
        ..Default::default()
    };
    assert_eq!(brief_message_keys(&[]), "");
    assert_eq!(brief_message_keys(&[mk("c1"), mk("c2")]), "c1, c2");
    // 无 client_id / msg_id → 循环会生成占位 ID，日志里如实标记
    assert_eq!(brief_message_keys(&[IlinkMessage::default()]), "<auto>");
    assert_eq!(
        brief_message_keys(&[mk("c1"), mk("c2"), mk("c3"), mk("c4")]),
        "c1, c2, c3, +1"
    );
}

/// 游标回灌（§5.6）：落库的已确认游标在 ensure 时载入内存 CursorStore
///
/// 不回灌 → 重启首轮以空游标请求，而 iLink 空游标不重放历史 → 停机期间消息丢失。
#[tokio::test]
async fn test_ensure_restores_persisted_cursor() {
    let registry = PollLoopRegistry::new();
    let mut ch = channel();
    ch.po.inbound_state = Some(
        InboundState {
            cursor: Some(common::models::inbound_state::InboundCursor::opaque(
                "cur_persisted",
                "ilink",
            )),
            ..Default::default()
        }
        .to_json(),
    );
    let creds = IlinkChannelCredentials {
        bot_token: "tok".into(),
        bot_id: "bot_1".into(),
        base_url: "https://invalid.test".into(),
    };
    let cursors = Arc::new(CursorStore::new());
    registry
        .ensure(
            &ch,
            &creds,
            None,
            Arc::clone(&cursors),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();
    assert_eq!(
        cursors.get("ch_wx_1").await.as_deref(),
        Some("cur_persisted")
    );
    registry.stop_all().await;
}

/// 无落库游标（或空游标）→ 不回灌，保持"从头拉、由幂等键兜底"的原语义
#[tokio::test]
async fn test_ensure_without_persisted_cursor_keeps_empty() {
    let creds = IlinkChannelCredentials {
        bot_token: "tok".into(),
        bot_id: "bot_1".into(),
        base_url: "https://invalid.test".into(),
    };

    // 全无 inbound_state
    let registry = PollLoopRegistry::new();
    let cursors = Arc::new(CursorStore::new());
    registry
        .ensure(
            &channel(),
            &creds,
            None,
            Arc::clone(&cursors),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();
    assert_eq!(cursors.get("ch_wx_1").await, None);
    registry.stop_all().await;

    // 有状态列但游标为空串
    let registry = PollLoopRegistry::new();
    let mut ch = channel();
    ch.po.inbound_state = Some(
        InboundState {
            cursor: Some(common::models::inbound_state::InboundCursor::opaque(
                "", "ilink",
            )),
            ..Default::default()
        }
        .to_json(),
    );
    let cursors = Arc::new(CursorStore::new());
    registry
        .ensure(
            &ch,
            &creds,
            None,
            Arc::clone(&cursors),
            Arc::new(SessionGuard::new()),
        )
        .await
        .unwrap();
    assert_eq!(cursors.get("ch_wx_1").await, None);
    registry.stop_all().await;
}

/// 内存 InboundStateWriter：写回链路可注入
struct MemWriter(tokio::sync::Mutex<Vec<String>>);

#[async_trait::async_trait]
impl InboundStateWriter for MemWriter {
    async fn save(&self, channel_id: &str, state: &InboundState) {
        self.0
            .lock()
            .await
            .push(format!("{}={}", channel_id, state.to_json()));
    }
}

#[tokio::test]
async fn test_inbound_state_writer_trait_object() {
    let writer: Arc<dyn InboundStateWriter> =
        Arc::new(MemWriter(tokio::sync::Mutex::new(Vec::new())));
    writer.save("ch_1", &InboundState::default()).await;
}
