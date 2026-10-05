//! tests 单元测试（拆分自 mod.rs）
//!
//! 文件瘦身：原 1349 行 → 828 行，测试体 522 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use tokio_tungstenite::accept_async;

/// 回显测试 adapter：收集收到的文本帧；端点指向测试监听地址
struct EchoTestAdapter {
    url: String,
    received: Arc<tokio::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl WsClientAdapter for EchoTestAdapter {
    fn name(&self) -> &str {
        "test"
    }
    async fn endpoint(&self) -> Result<String> {
        Ok(self.url.clone())
    }
    async fn on_frame(&self, text: String) -> FrameAction {
        self.received.lock().await.push(text);
        FrameAction::Continue
    }
}

/// 回显测试 adapter 收到指定文本帧即请求重连
struct CloseOnFrameAdapter {
    url: String,
    trigger: String,
    received: Arc<tokio::sync::Mutex<Vec<String>>>,
}

#[async_trait]
impl WsClientAdapter for CloseOnFrameAdapter {
    fn name(&self) -> &str {
        "test-close"
    }
    async fn endpoint(&self) -> Result<String> {
        Ok(self.url.clone())
    }
    async fn on_frame(&self, text: String) -> FrameAction {
        self.received.lock().await.push(text.clone());
        if text == self.trigger {
            FrameAction::Reconnect
        } else {
            FrameAction::Continue
        }
    }
}

/// 起一个本地 WS 测试服务端：每次 accept 后先推一条文本帧，然后保持连接
async fn spawn_test_server(greeting: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let Ok(ws) = accept_async(stream).await else {
                continue;
            };
            let (mut write, mut read) = ws.split();
            // 建连即推一条问候帧
            if write
                .send(Message::Text(greeting.to_string()))
                .await
                .is_err()
            {
                continue;
            }
            // 保持连接直到对端关闭
            while let Some(Ok(msg)) = read.next().await {
                let _ = msg;
            }
            let _ = write.close().await;
        }
    });
    (format!("ws://{}", addr), handle)
}

/// 正常路径：建连 → 服务端推帧投递 adapter → shutdown 优雅退出
#[tokio::test(flavor = "multi_thread")]
async fn client_receives_frames_and_shutdown() {
    let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
    let (url, server) = spawn_test_server("hello").await;

    let adapter = Arc::new(EchoTestAdapter {
        url: url.clone(),
        received: received.clone(),
    });
    let state = start_client(adapter).await.unwrap();

    // 等待建连成功
    for _ in 0..50 {
        let snap = state.conn_state_snapshot().await;
        if snap.phase == WsConnPhase::Connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let snap = state.conn_state_snapshot().await;
    assert_eq!(snap.phase, WsConnPhase::Connected);
    assert_eq!(snap.reconnect_count, 0);
    assert!(snap.last_connected_at.is_some());

    // 服务端推的 "hello" 应已投递到 adapter
    for _ in 0..50 {
        if !received.lock().await.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(*received.lock().await, vec!["hello".to_string()]);

    stop_client(state).await;
    server.abort();
}

/// on_frame 返回 Reconnect → 结束本次连接并自动重连（reconnect_count 增长）
#[tokio::test(flavor = "multi_thread")]
async fn reconnect_on_frame_action() {
    let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
    let (url, server) = spawn_test_server("bye").await;

    let adapter = Arc::new(CloseOnFrameAdapter {
        url: url.clone(),
        trigger: "bye".to_string(),
        received: received.clone(),
    });
    let state = start_client(adapter).await.unwrap();
    // 每次建连服务端都推 "bye" → 触发 Reconnect → supervisor 自动重连。
    // 首次建连不计入 reconnect_count，需等待完整退避（约 2s）+ 第二次建连完成。
    tokio::time::sleep(Duration::from_secs(4)).await;
    let snap = state.conn_state_snapshot().await;
    assert!(
        snap.reconnect_count >= 1,
        "expected at least one reconnect, snapshot: {:?}",
        snap
    );
    assert!(!received.lock().await.is_empty());

    stop_client(state).await;
    server.abort();
}

/// 退避序列：从 1s 起倍增，封顶 60s，±20% 抖动范围内
#[test]
fn next_backoff_grows_and_caps() {
    // 首次退避基准 2s（1s 倍增），允许 ±20% 抖动
    for _ in 0..20 {
        let d = next_backoff(None);
        assert!(d >= Duration::from_millis(1600) && d <= Duration::from_millis(2400));
    }
    // 连续倍增不超过封顶（含抖动后二次封顶）
    let mut current = Some(Duration::from_secs(1));
    for _ in 0..10 {
        current = Some(next_backoff(current));
        assert!(current.unwrap() <= BACKOFF_MAX);
    }
    // 已达封顶后仍维持在封顶以内
    let d = next_backoff(Some(BACKOFF_MAX));
    assert!(d >= Duration::from_secs(48) && d <= BACKOFF_MAX);
}

/// WsConnPhase 字符串表示稳定（监控快照消费）
#[test]
fn ws_conn_phase_as_str() {
    assert_eq!(WsConnPhase::Connecting.as_str(), "connecting");
    assert_eq!(WsConnPhase::Connected.as_str(), "connected");
    assert_eq!(WsConnPhase::Reconnecting.as_str(), "reconnecting");
    assert_eq!(WsConnPhase::Failed.as_str(), "failed");
    assert!(WsConnPhase::Failed.is_terminal());
    assert!(!WsConnPhase::Connected.is_terminal());
}

// ==================== S1 新增能力测试 ====================

/// 二进制 + 回写 adapter：记录所有入站帧；二进制帧回写 ack
struct BinaryAckAdapter {
    url: String,
    received: Arc<tokio::sync::Mutex<Vec<WsFrame>>>,
}

#[async_trait]
impl WsClientAdapter for BinaryAckAdapter {
    fn name(&self) -> &str {
        "test-binary"
    }
    async fn endpoint(&self) -> Result<String> {
        Ok(self.url.clone())
    }
    async fn on_frame(&self, text: String) -> FrameAction {
        self.received.lock().await.push(WsFrame::Text(text));
        FrameAction::Continue
    }
    async fn on_message(&self, frame: WsFrame) -> FrameOutcome {
        self.received.lock().await.push(frame.clone());
        match frame {
            WsFrame::Binary(bytes) => FrameOutcome::cont().with_reply(WsOutFrame::Binary(
                format!("ack:{}", bytes.len()).into_bytes(),
            )),
            WsFrame::Text(_) => FrameOutcome::cont(),
        }
    }
}

/// 测试服务端：建连后推一帧二进制，收集对端回帧（首个文本/二进制后停止）
async fn spawn_binary_server(
    payload: Vec<u8>,
) -> (
    String,
    Arc<tokio::sync::Mutex<Vec<Message>>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let got: Arc<tokio::sync::Mutex<Vec<Message>>> = Arc::default();
    let got_server = got.clone();
    let handle = tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(ws) = accept_async(stream).await else {
            return;
        };
        let (mut write, mut read) = ws.split();
        if write.send(Message::Binary(payload)).await.is_err() {
            return;
        }
        while let Some(Ok(msg)) = read.next().await {
            let stop = msg.is_binary() || msg.is_text();
            got_server.lock().await.push(msg);
            if stop {
                break;
            }
        }
    });
    (format!("ws://{}", addr), got, handle)
}

/// 二进制帧投递 adapter；`FrameOutcome.replies` 在读循环内被立即回写
#[tokio::test(flavor = "multi_thread")]
async fn binary_frame_delivery_and_reply() {
    let received: Arc<tokio::sync::Mutex<Vec<WsFrame>>> = Arc::default();
    let (url, server_got, server) = spawn_binary_server(vec![1, 2, 3]).await;

    let adapter = Arc::new(BinaryAckAdapter {
        url,
        received: received.clone(),
    });
    let state = start_client(adapter).await.unwrap();

    for _ in 0..50 {
        if !received.lock().await.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(
        received.lock().await.clone(),
        vec![WsFrame::Binary(vec![1, 2, 3])]
    );

    // adapter 声明的回写帧应被读循环立即发出
    for _ in 0..50 {
        if !server_got.lock().await.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let frames = server_got.lock().await.clone();
    assert!(
        frames
            .iter()
            .any(|m| matches!(m, Message::Binary(b) if b == b"ack:3")),
        "expected ack reply frame, got {:?}",
        frames
    );

    // 收帧证据：计数与时间戳（判活字段）
    let snap = state.conn_state_snapshot().await;
    assert!(snap.frames_received >= 1, "snapshot: {:?}", snap);
    assert!(snap.last_frame_at_ms.is_some());

    stop_client(state).await;
    server.abort();
}

/// 默认实现下二进制帧不投 `on_frame`，但仍计入判活证据（不静默丢弃）
#[tokio::test(flavor = "multi_thread")]
async fn default_adapter_ignores_binary_but_counts_liveness() {
    let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
    let (url, _got, server) = spawn_binary_server(vec![9, 9]).await;
    let adapter = Arc::new(EchoTestAdapter {
        url,
        received: received.clone(),
    });
    let state = start_client(adapter).await.unwrap();

    for _ in 0..50 {
        if state.conn_state_snapshot().await.frames_received > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let snap = state.conn_state_snapshot().await;
    assert_eq!(snap.phase, WsConnPhase::Connected);
    assert!(snap.frames_received >= 1, "snapshot: {:?}", snap);
    // 联邦默认路径：文本通路不被二进制污染（on_frame 未收到任何东西）
    assert!(received.lock().await.is_empty());

    stop_client(state).await;
    server.abort();
}

/// 服务端 close 帧的 code/reason 落进快照（区分正常轮换与被顶/冲突的唯一证据）
#[tokio::test(flavor = "multi_thread")]
async fn close_frame_recorded_in_snapshot() {
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(ws) = accept_async(stream).await else {
            return;
        };
        let (mut write, _read) = ws.split();
        let _ = write
            .send(Message::Close(Some(CloseFrame {
                // 4001 = 应用自定义「被顶/踢出」：`CloseCode` 是私有 re-export，
                // 用 `.into()` 交给类型推断（目标类型由 CloseFrame.code 决定）
                code: 4001u16.into(),
                reason: std::borrow::Cow::Borrowed("kicked"),
            })))
            .await;
    });

    let received: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
    let adapter = Arc::new(EchoTestAdapter {
        url: format!("ws://{}", addr),
        received,
    });
    let state = start_client(adapter).await.unwrap();

    for _ in 0..50 {
        if state.conn_state_snapshot().await.last_close_code.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let snap = state.conn_state_snapshot().await;
    assert_eq!(snap.last_close_code, Some(4001));
    assert_eq!(snap.last_close_reason.as_deref(), Some("kicked"));

    stop_client(state).await;
    server.abort();
}

/// 终局停机 adapter：端点永远失败，且自报终局原因
struct TerminalAdapter {
    terminal: Arc<std::sync::Mutex<Option<String>>>,
}

#[async_trait]
impl WsClientAdapter for TerminalAdapter {
    fn name(&self) -> &str {
        "test-terminal"
    }
    async fn endpoint(&self) -> Result<String> {
        Err(err!(ThirdPartyError, "endpoint unavailable"))
    }
    async fn on_frame(&self, _text: String) -> FrameAction {
        FrameAction::Continue
    }
    fn terminal_reason(&self) -> Option<String> {
        self.terminal.lock().ok().and_then(|g| g.clone())
    }
}

/// `terminal_reason` 置位 → supervisor 停机（`Failed`）且不再重连
#[tokio::test(flavor = "multi_thread")]
async fn terminal_reason_stops_reconnect() {
    let adapter = Arc::new(TerminalAdapter {
        terminal: Arc::new(std::sync::Mutex::new(Some("exceed_conn_limit".to_string()))),
    });
    let state = start_client(adapter).await.unwrap();

    for _ in 0..50 {
        if state.conn_state_snapshot().await.phase == WsConnPhase::Failed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let snap = state.conn_state_snapshot().await;
    assert_eq!(snap.phase, WsConnPhase::Failed, "snapshot: {:?}", snap);
    assert_eq!(snap.terminal_reason.as_deref(), Some("exceed_conn_limit"));

    // 停机不再重连：等待远超指数退避首轮（约 2s），状态保持 Failed
    tokio::time::sleep(Duration::from_millis(2500)).await;
    let snap = state.conn_state_snapshot().await;
    assert_eq!(snap.phase, WsConnPhase::Failed);
    assert_eq!(snap.reconnect_count, 0);

    stop_client(state).await;
}

/// 心跳间隔热变更 adapter（运行期可改，模拟服务端下发新参数）
struct TuningHeartbeatAdapter {
    url: String,
    interval_ms: Arc<std::sync::atomic::AtomicU64>,
}

#[async_trait]
impl WsClientAdapter for TuningHeartbeatAdapter {
    fn name(&self) -> &str {
        "test-heartbeat"
    }
    async fn endpoint(&self) -> Result<String> {
        Ok(self.url.clone())
    }
    async fn on_frame(&self, _text: String) -> FrameAction {
        FrameAction::Continue
    }
    fn heartbeat(&self) -> Option<Heartbeat> {
        Some(Heartbeat {
            interval: Duration::from_millis(self.interval_ms.load(Ordering::SeqCst)),
            frame: Some(WsOutFrame::Text("hb".to_string())),
        })
    }
}

/// 等待接收计数稳定（排空服务端积压），返回稳定后的计数。
///
/// 修 `heartbeat_interval_takes_effect_live` 的 flaky 根因：`mark` 若取自
/// 「服务端已处理帧数」且此时有积压，观测窗口内服务端补处理积压帧会被
/// 误判成「客户端还在按旧间隔发帧」。机器负载越高越容易复现——全量串行
/// 跑必挂、单独跑必过。
///
/// 稳定性判定用「连续 N 次读数不变」，并设总次数上限：若心跳真的没停
/// （功能失效），计数会持续增长，函数在上限后返回当前值，让后续断言照常
/// 失败——排空辅助不该把真正的失败伪装成超时卡死。
async fn wait_count_stable(got: &Arc<tokio::sync::Mutex<Vec<String>>>) -> usize {
    const POLL: Duration = Duration::from_millis(50);
    const STABLE_HITS: usize = 3;
    const MAX_POLLS: usize = 40; // 上限 2s

    let mut last = got.lock().await.len();
    let mut stable_hits = 0;
    for _ in 0..MAX_POLLS {
        tokio::time::sleep(POLL).await;
        let now = got.lock().await.len();
        if now == last {
            stable_hits += 1;
            if stable_hits >= STABLE_HITS {
                return now;
            }
        } else {
            stable_hits = 0;
            last = now;
        }
    }
    got.lock().await.len()
}

/// 心跳间隔由 adapter 每 tick 提供 ⇒ 运行期改变当轮生效
#[tokio::test(flavor = "multi_thread")]
async fn heartbeat_interval_takes_effect_live() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let got: Arc<tokio::sync::Mutex<Vec<String>>> = Arc::default();
    let got_server = got.clone();
    let server = tokio::spawn(async move {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let Ok(ws) = accept_async(stream).await else {
            return;
        };
        let (_write, mut read) = ws.split();
        while let Some(Ok(msg)) = read.next().await {
            if let Message::Text(text) = msg {
                got_server.lock().await.push(text);
            }
        }
    });

    let interval_ms = Arc::new(std::sync::atomic::AtomicU64::new(30));
    let adapter = Arc::new(TuningHeartbeatAdapter {
        url: format!("ws://{}", addr),
        interval_ms: interval_ms.clone(),
    });
    let state = start_client(adapter).await.unwrap();

    // 30ms 间隔：300ms 内应收到多帧
    tokio::time::sleep(Duration::from_millis(300)).await;
    let fast = got.lock().await.len();
    assert!(
        fast >= 4,
        "expected several heartbeats at 30ms, got {}",
        fast
    );

    // 改为 1000ms：当轮生效（正在 sleep 的旧 tick 最多再补一帧）
    interval_ms.store(1000, Ordering::SeqCst);
    // 取 mark 前先排空接收侧积压，否则窗口内补处理的积压帧会被误判成旧节奏
    let mark = wait_count_stable(&got).await;
    // 观测窗 400ms：对 30ms 旧节奏有 13 倍余量，同时对 tokio timer 在高负载
    // 下的漂移留足容忍（250ms 偏紧，全量跑时 timer 漂移能突破）
    tokio::time::sleep(Duration::from_millis(400)).await;
    let after = got.lock().await.len();
    assert!(
        after - mark <= 1,
        "heartbeat interval change not effective: {} -> {}",
        mark,
        after
    );

    stop_client(state).await;
    server.abort();
}
