//! tests 单元测试（拆分自 ws.rs）
//!
//! 文件瘦身：原 999 行 → 711 行，测试体 289 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

fn test_http() -> reqwest::Client {
    crate::pkg::http::presets::outbound()
        .build()
        .expect("构建测试 HTTP 客户端失败")
}

fn test_adapter() -> LarkWsAdapter {
    LarkWsAdapter::new(test_http(), "cli_test".to_string(), "secret".to_string())
}

/// 单片（`sum=1, seq=0`，官方未分片事件形态）
#[test]
fn merge_single_fragment() {
    let mut cache = FragmentCache::default();
    let out = merge_fragment(&mut cache, "m1", Some("1"), Some("0"), b"hello");
    assert_eq!(out, MergeOutcome::Complete(b"hello".to_vec()));
    assert!(cache.slots.is_empty(), "合并完成后应释放槽位");
}

/// 未带分片元数据 → 视为单片（比官方更宽容，避免整帧事件被丢）
#[test]
fn merge_without_metadata_treated_as_single() {
    let mut cache = FragmentCache::default();
    let out = merge_fragment(&mut cache, "", None, None, b"whole");
    assert_eq!(out, MergeOutcome::Complete(b"whole".to_vec()));
}

/// 多片乱序到达 → 按 seq 顺序拼接
#[test]
fn merge_multi_fragment_out_of_order() {
    let mut cache = FragmentCache::default();
    assert_eq!(
        merge_fragment(&mut cache, "m2", Some("3"), Some("2"), b"C"),
        MergeOutcome::Pending
    );
    assert_eq!(
        merge_fragment(&mut cache, "m2", Some("3"), Some("0"), b"A"),
        MergeOutcome::Pending
    );
    assert_eq!(
        merge_fragment(&mut cache, "m2", Some("3"), Some("1"), b"B"),
        MergeOutcome::Complete(b"ABC".to_vec())
    );
    assert!(cache.slots.is_empty());
}

/// 非法元数据：只带一个 / 非数字 / sum=0 / seq 越界 / sum 不一致 / 超上限
#[test]
fn merge_invalid_metadata() {
    let mut cache = FragmentCache::default();
    assert_eq!(
        merge_fragment(&mut cache, "m", Some("2"), None, b"x"),
        MergeOutcome::Invalid
    );
    assert_eq!(
        merge_fragment(&mut cache, "m", Some("abc"), Some("0"), b"x"),
        MergeOutcome::Invalid
    );
    assert_eq!(
        merge_fragment(&mut cache, "m", Some("0"), Some("0"), b"x"),
        MergeOutcome::Invalid
    );
    assert_eq!(
        merge_fragment(&mut cache, "m", Some("2"), Some("2"), b"x"),
        MergeOutcome::Invalid
    );
    assert_eq!(
        merge_fragment(
            &mut cache,
            "m",
            Some(&(MAX_FRAGMENT_COUNT + 1).to_string()),
            Some("0"),
            b"x"
        ),
        MergeOutcome::Invalid
    );
    // 首片建槽后，后续 sum 不一致 → Invalid
    assert_eq!(
        merge_fragment(&mut cache, "m3", Some("2"), Some("0"), b"x"),
        MergeOutcome::Pending
    );
    assert_eq!(
        merge_fragment(&mut cache, "m3", Some("3"), Some("1"), b"x"),
        MergeOutcome::Invalid
    );
    // 多片但缺分片键 → Invalid
    assert_eq!(
        merge_fragment(&mut cache, "", Some("2"), Some("0"), b"x"),
        MergeOutcome::Invalid
    );
}

/// 连接 URL query 解析出 service_id / device_id
#[test]
fn parse_query_extracts_ids() {
    let (sid, did) = parse_conn_query("wss://gw.example.com/ws?device_id=dev-1&service_id=42&x=1");
    assert_eq!(sid, 42);
    assert_eq!(did, "dev-1");
    // 无 query / 无相关字段
    assert_eq!(
        parse_conn_query("wss://gw.example.com/ws"),
        (0, String::new())
    );
    assert_eq!(
        parse_conn_query("wss://gw.example.com/ws?a=b"),
        (0, String::new())
    );
}

/// app_id 形态：合规不告警，可疑只告警不拒绝
#[test]
fn app_id_form_check() {
    assert!(check_app_id("cli_0123456789abcdef").is_none());
    assert!(check_app_id("cli_0123456789ABCDEF").is_none());
    assert!(check_app_id("ou_0123456789abcdef").is_some());
    assert!(check_app_id("cli_short").is_some());
    assert!(check_app_id("cli_0123456789abcdeg").is_some());
}

/// 服务端下发参数覆盖兜底默认值（`ReconnectCount = -1` ⇒ 无限）
#[test]
fn params_apply_config_overrides() {
    let mut params = WsParams::default();
    assert_eq!(params.ping_interval_secs, DEFAULT_PING_INTERVAL_SECS);
    assert_eq!(params.reconnect_count, None);
    params.apply_config(&ClientConfig {
        ping_interval: Some(30),
        reconnect_interval: Some(10),
        reconnect_count: Some(5),
        reconnect_nonce: Some(3),
    });
    assert_eq!(params.ping_interval_secs, 30);
    assert_eq!(params.reconnect_interval_secs, 10);
    assert_eq!(params.reconnect_nonce_secs, 3);
    assert_eq!(params.reconnect_count, Some(5));
    // 负数（官方 -1）= 无限
    params.apply_config(&ClientConfig {
        reconnect_count: Some(-1),
        ..Default::default()
    });
    assert_eq!(params.reconnect_count, None);
    // 缺失字段保留原值
    params.apply_config(&ClientConfig::default());
    assert_eq!(params.ping_interval_secs, 30);
}

/// 心跳帧形状：pbbp2 控制帧 `type=ping` + `service_id`；间隔取自运行参数
#[test]
fn heartbeat_frame_shape() {
    let adapter = test_adapter();
    if let Ok(mut params) = adapter.params.write() {
        params.service_id = 7;
        params.ping_interval_secs = 15;
    }
    let hb = adapter.heartbeat().expect("lark uses app-layer ping");
    assert_eq!(hb.interval, Duration::from_secs(15));
    let Some(WsOutFrame::Binary(bytes)) = hb.frame else {
        panic!("expected binary ping frame");
    };
    let frame = Frame::decode(bytes.as_slice()).expect("decode ping");
    assert!(frame.is_control());
    assert_eq!(frame.msg_type(), Some(msg_type::PING));
    assert_eq!(frame.service, 7);

    // 服务端下发 0 ⇒ 关闭心跳
    if let Ok(mut params) = adapter.params.write() {
        params.ping_interval_secs = 0;
    }
    assert!(adapter.heartbeat().is_none());
}

/// 重连策略：固定间隔 + 首次抖动 + 次数上限；间隔下限 1s（防忙重连）
#[test]
fn reconnect_policy_from_params() {
    let adapter = test_adapter();
    if let Ok(mut params) = adapter.params.write() {
        params.reconnect_interval_secs = 0;
        params.reconnect_nonce_secs = 30;
        params.reconnect_count = Some(3);
    }
    assert_eq!(
        adapter.reconnect_policy(),
        ReconnectPolicy::Fixed {
            interval: Duration::from_secs(1),
            first_jitter: Some(Duration::from_secs(30)),
            max_attempts: Some(3),
        }
    );
}

/// ACK 形状：复用入帧 + `biz_rt`（官方写负值）+ payload `{"code":200}`
#[test]
fn ack_reply_shape() {
    let adapter = test_adapter();
    let inbound = Frame {
        seq_id: 3,
        log_id: 4,
        service: 5,
        method: pbbp2::frame_type::DATA,
        headers: vec![pbbp2::Header {
            key: header_key::TYPE.to_string(),
            value: msg_type::EVENT.to_string(),
        }],
        payload_encoding: "json".to_string(),
        payload_type: "event".to_string(),
        payload: br#"{"a":1}"#.to_vec(),
        log_id_new: "lg".to_string(),
    };
    let reply = adapter.ack_reply(&inbound, 200, Instant::now());
    let Some(WsOutFrame::Binary(bytes)) = Some(reply) else {
        panic!("expected binary ack frame");
    };
    let ack = Frame::decode(bytes.as_slice()).expect("decode ack");
    assert_eq!(ack.seq_id, 3);
    assert_eq!(ack.log_id, 4);
    assert_eq!(ack.service, 5);
    assert_eq!(ack.method, pbbp2::frame_type::DATA);
    assert_eq!(ack.payload_text(), Some(r#"{"code":200}"#));
    // biz_rt 是负值（官方 `String(startTime - endTime)`）
    let biz_rt = ack
        .header(header_key::BIZ_RT)
        .expect("biz_rt header present")
        .parse::<i64>()
        .expect("biz_rt integer");
    assert!(biz_rt <= 0, "biz_rt should be non-positive, got {}", biz_rt);
}

/// 脏帧/文本帧/超限帧：留痕但不 panic、不回写
#[tokio::test]
async fn malformed_frames_are_not_fatal() {
    let adapter = test_adapter();
    // 乱码二进制
    let out = adapter
        .on_message(WsFrame::Binary(vec![0xff, 0xff, 0xff]))
        .await;
    assert_eq!(out.action, crate::pkg::ws::FrameAction::Continue);
    assert!(out.replies.is_empty());
    // 文本帧
    let out = adapter
        .on_message(WsFrame::Text("{\"type\":\"ping\"}".to_string()))
        .await;
    assert!(out.replies.is_empty());
    // 超限帧（不真正分配 8MiB 以外，只验证判定分支）
    let out = adapter
        .on_message(WsFrame::Binary(vec![0u8; MAX_FRAME_BYTES + 1]))
        .await;
    assert!(out.replies.is_empty());
}

/// 非订阅数据类型（如 card）：留痕且不回 ACK
#[tokio::test]
async fn unsupported_data_type_gets_no_ack() {
    let adapter = test_adapter();
    let frame = Frame {
        seq_id: 1,
        log_id: 1,
        service: 1,
        method: pbbp2::frame_type::DATA,
        headers: vec![pbbp2::Header {
            key: header_key::TYPE.to_string(),
            value: msg_type::CARD.to_string(),
        }],
        payload_encoding: String::new(),
        payload_type: String::new(),
        payload: Vec::new(),
        log_id_new: String::new(),
    };
    let out = adapter
        .on_message(WsFrame::Binary(frame.encode_to_vec()))
        .await;
    assert!(out.replies.is_empty());
}

/// 终局原因只置一次（保留首个根因），取端点成功后清除
#[test]
fn terminal_reason_is_sticky_and_clearable() {
    let adapter = test_adapter();
    assert!(adapter.terminal_reason().is_none());
    adapter.set_terminal("first");
    adapter.set_terminal("second");
    assert_eq!(adapter.terminal_reason().as_deref(), Some("first"));
    if let Ok(mut t) = adapter.terminal.lock() {
        *t = None;
    }
    assert!(adapter.terminal_reason().is_none());
}
