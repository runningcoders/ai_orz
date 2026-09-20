//! 飞书长连接私有协议帧（`pbbp2`，protobuf）
//!
//! # 协议依据（唯一 spec）
//!
//! 官方 Node SDK `@larksuiteoapi/node-sdk@1.74.0`（`npm pack` 解包后 `package/lib/index.js`），
//! 与官方 Go SDK `larksuite/oapi-sdk-go@v3_main`（`ws/pbbp2.pb.go`）互证：
//!
//! | 锚点 | 行号 | 内容 |
//! |---|---|---|
//! | `pbbp2.Header` | L101282 | 帧头（`key` / `value`） |
//! | `pbbp2.Frame` | L101509 | 帧（9 字段） |
//! | `ErrorCode` | L102101 | 服务端业务码表 |
//! | `FrameType` | L102108 | 帧类型（control=0 / data=1） |
//! | `HeaderKey` | L102116 | 帧内 header key 字面量 |
//! | `MessageType` | L102128 | 帧内 `type` 取值（event/card/ping/pong） |
//! | `WSConfig` | L101963 | 参数默认值（兜底） |
//! | `pullConnectConfig` | L102306 | 取连接地址请求（POST + `{AppID, AppSecret}`） |
//! | `pingLoop` | L102578 | 心跳帧构造 |
//! | `handleControlData` | L102633 | 控制帧（pong 覆盖参数） |
//! | `handleEventData` | L102665 | 数据帧 + ACK 构造 |
//!
//! ## 字段 tag 表（线协议只看 tag；字段名仅 Rust 侧可读性）
//!
//! ```text
//! message Header {          // pbbp2.Header
//!   string key   = 1;
//!   string value = 2;
//! }
//! message Frame {           // pbbp2.Frame（proto2 required：1/2/3/4）
//!   uint64           SeqID           = 1;
//!   uint64           LogID           = 2;
//!   int32            service         = 3;
//!   int32            method          = 4;   // FrameType
//!   repeated Header  headers         = 5;
//!   string           payloadEncoding = 6;
//!   string           payloadType     = 7;
//!   bytes            payload         = 8;
//!   string           LogIDNew        = 9;
//! }
//! ```
//!
//! > 官方 `Frame.decode` 对 `SeqID`/`LogID`/`service`/`method` 做 required 校验（缺失即抛
//! > `ProtocolError`）。我方**不**做同等强校验——服务端发来的帧必然带全，而一旦强校验失败
//! > 会丢掉整帧且只剩一行错误日志，风险高于收益；缺字段按默认值处理并在解码层留痕（见调用方）。
//!
//! ## ACK 形状（官方 `handleEventData`）
//!
//! 服务端 **3 秒**内未收到 ACK 即重推事件（官方文档：超时按未处理处理并重推）。ACK 形状是
//! 「**原样复用入帧**的 SeqID/LogID/service/method/payloadEncoding/payloadType/LogIDNew」+
//! 「headers 追加 `biz_rt`（= 处理耗时的**负值**，官方写 `String(startTime - endTime)`）」+
//! 「payload 换成 `{"code":200}`（业务处理失败为 `500`）」。
//!
//! ## 参数下发（心跳 / 重连）
//!
//! 取连接地址响应 `data.ClientConfig` 下发 `PingInterval` / `ReconnectCount` /
//! `ReconnectInterval` / `ReconnectNonce`（秒）；控制帧 `type=pong` 的 payload 会**再覆盖一次**
//! （同名四字段）。官方兜底默认：`pingInterval=120s`、`reconnectInterval=120s`、
//! `reconnectNonce=30s`、`reconnectCount=-1`（无限）、`pingTimeout=0`（不启用无帧判死）。
//! 注意是**固定间隔重连**（首次叠加 `ReconnectNonce * random()` 抖动），**不是指数退避**。

use prost::Message;

// ==================== 消息定义 ====================

/// 帧头（proto: `pbbp2.Header`）
#[derive(Clone, PartialEq, Message)]
pub struct Header {
    /// proto `key`（tag 1）
    #[prost(string, tag = "1")]
    pub key: String,
    /// proto `value`（tag 2）
    #[prost(string, tag = "2")]
    pub value: String,
}

/// 长连接帧（proto: `pbbp2.Frame`）
#[derive(Clone, PartialEq, Message)]
pub struct Frame {
    /// proto `SeqID`（tag 1）—— ping 帧为 0，事件帧由服务端赋值
    #[prost(uint64, tag = "1")]
    pub seq_id: u64,
    /// proto `LogID`（tag 2）
    #[prost(uint64, tag = "2")]
    pub log_id: u64,
    /// proto `service`（tag 3）—— 连接 URL query 里的 `service_id`
    #[prost(int32, tag = "3")]
    pub service: i32,
    /// proto `method`（tag 4）—— 见 [`frame_type`]
    #[prost(int32, tag = "4")]
    pub method: i32,
    /// proto `headers`（tag 5）
    #[prost(message, repeated, tag = "5")]
    pub headers: Vec<Header>,
    /// proto `payloadEncoding`（tag 6）
    #[prost(string, tag = "6")]
    pub payload_encoding: String,
    /// proto `payloadType`（tag 7）
    #[prost(string, tag = "7")]
    pub payload_type: String,
    /// proto `payload`（tag 8）—— 事件为 JSON 字节；ACK 为 `{"code":200}`
    #[prost(bytes = "vec", tag = "8")]
    pub payload: Vec<u8>,
    /// proto `LogIDNew`（tag 9）
    #[prost(string, tag = "9")]
    pub log_id_new: String,
}

// ==================== 常量（字面量 SSOT） ====================

/// 帧类型（proto: `Frame.method`）
pub mod frame_type {
    /// 控制帧（握手 / ping / pong）
    pub const CONTROL: i32 = 0;
    /// 数据帧（事件 / ACK）
    pub const DATA: i32 = 1;
}

/// 帧内 header `type` 的取值（proto: `Header.value`）
pub mod msg_type {
    /// 事件
    pub const EVENT: &str = "event";
    /// 卡片回调
    pub const CARD: &str = "card";
    /// 心跳
    pub const PING: &str = "ping";
    /// 心跳响应
    pub const PONG: &str = "pong";
}

/// 帧内 header key（proto: `Header.key`）
pub mod header_key {
    /// 消息类型（取值见 [`super::pbbp2::msg_type`]）
    pub const TYPE: &str = "type";
    /// 事件聚合 ID（分片重组键）
    pub const MESSAGE_ID: &str = "message_id";
    /// 分片总数
    pub const SUM: &str = "sum";
    /// 分片序号（0-based）
    pub const SEQ: &str = "seq";
    /// 链路追踪 ID
    pub const TRACE_ID: &str = "trace_id";
    /// ACK 耗时（官方写处理耗时的负值）
    pub const BIZ_RT: &str = "biz_rt";
    /// 握手状态（服务端下发，仅留痕）
    pub const HANDSHAKE_STATUS: &str = "handshake-status";
    /// 握手消息（服务端下发，仅留痕）
    pub const HANDSHAKE_MSG: &str = "handshake-msg";
    /// 握手鉴权错误码（服务端下发，仅留痕）
    pub const HANDSHAKE_AUTHERRCODE: &str = "handshake-autherrcode";
}

/// 服务端业务码（proto: 取连接地址响应 `code` / 控制帧错误）
pub mod error_code {
    /// 成功
    pub const OK: i32 = 0;
    /// 系统繁忙
    pub const SYSTEM_BUSY: i32 = 1;
    /// 无权限
    pub const FORBIDDEN: i32 = 403;
    /// 鉴权失败（AppID / AppSecret 不匹配）
    pub const AUTH_FAILED: i32 = 514;
    /// 服务端内部错误（唯一被官方判定为**可重试**的业务码）
    pub const INTERNAL_ERROR: i32 = 1_000_040_343;
    /// 连接数超限（同应用活跃连接数达上限；**不可重试**）
    pub const EXCEED_CONN_LIMIT: i32 = 1_000_040_350;
}

/// 取连接地址失败的处置分类
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnConfigError {
    /// 可重试（网络抖动 / `internal_error`）
    Retryable(String),
    /// 不可重试（终局：需人工介入换凭据或排查连接冲突）
    Terminal(String),
}

/// 按官方口径分类「取连接地址」的失败（纯函数，可测）
///
/// 官方 `pullConnectConfig`：`code != 0` 时**只有** `internal_error` 判可重试，其余
/// （含 `system_busy` / `forbidden` / `auth_failed` / `exceed_conn_limit`）一律不可重试；
/// 网络异常（catch 分支）判可重试。
pub fn classify_config_error(code: i32, msg: &str) -> ConnConfigError {
    if code == error_code::OK {
        // 调用方只应在 code != 0 时调用；宽容处理避免误判成终局
        return ConnConfigError::Retryable(String::new());
    }
    let reason = if code == error_code::SYSTEM_BUSY {
        "system busy".to_string()
    } else {
        msg.to_string()
    };
    if code == error_code::INTERNAL_ERROR {
        return ConnConfigError::Retryable(reason);
    }
    ConnConfigError::Terminal(format!("code={} msg={}", code, reason))
}

// ==================== Frame 便捷方法 ====================

impl Frame {
    /// 读取指定 header 的值（同名取首个，与官方 `headers.find()` 一致）
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|h| h.key == key)
            .map(|h| h.value.as_str())
    }

    /// 是否为控制帧
    pub fn is_control(&self) -> bool {
        self.method == frame_type::CONTROL
    }

    /// 是否为数据帧
    pub fn is_data(&self) -> bool {
        self.method == frame_type::DATA
    }

    /// 帧内 `type` 取值（控制帧的 ping/pong、数据帧的 event/card）
    pub fn msg_type(&self) -> Option<&str> {
        self.header(header_key::TYPE)
    }

    /// payload 的 UTF-8 视图（非 UTF-8 返回 None，由调用方留痕）
    pub fn payload_text(&self) -> Option<&str> {
        std::str::from_utf8(&self.payload).ok()
    }

    /// 构造 ACK 帧（官方 `handleEventData` 形状）
    ///
    /// 原样复用入帧的 `SeqID`/`LogID`/`service`/`method`/`payloadEncoding`/`payloadType`/
    /// `LogIDNew`，`headers` 追加 `biz_rt`，`payload` 替换为 `{"code":<code>}`。
    ///
    /// `biz_rt_ms` 是处理耗时字段：官方写的是**负**耗时（`String(startTime - endTime)`），
    /// 调用方按其字面形态传入（这里不替官方「纠错」——线上所有客户端都这么发，
    /// 服务端显然不校验符号；若真机发现异常再改）。
    pub fn ack(&self, code: u16, biz_rt_ms: i64) -> Frame {
        let mut headers = self.headers.clone();
        headers.push(Header {
            key: header_key::BIZ_RT.to_string(),
            value: biz_rt_ms.to_string(),
        });
        Frame {
            seq_id: self.seq_id,
            log_id: self.log_id,
            service: self.service,
            method: self.method,
            headers,
            payload_encoding: self.payload_encoding.clone(),
            payload_type: self.payload_type.clone(),
            payload: serde_json::to_vec(&serde_json::json!({ "code": code })).unwrap_or_default(),
            log_id_new: self.log_id_new.clone(),
        }
    }

    /// 构造 ping 帧（官方 `pingLoop` 形状：`type=ping` + control + `service`）
    pub fn ping(service_id: i32) -> Frame {
        Frame {
            seq_id: 0,
            log_id: 0,
            service: service_id,
            method: frame_type::CONTROL,
            headers: vec![Header {
                key: header_key::TYPE.to_string(),
                value: msg_type::PING.to_string(),
            }],
            payload_encoding: String::new(),
            payload_type: String::new(),
            payload: Vec::new(),
            log_id_new: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 编解码往返：重复 header / 空 payload / 非空 payload
    #[test]
    fn frame_roundtrip() {
        let frame = Frame {
            seq_id: 7,
            log_id: 42,
            service: 3,
            method: frame_type::DATA,
            headers: vec![
                Header {
                    key: header_key::TYPE.to_string(),
                    value: msg_type::EVENT.to_string(),
                },
                Header {
                    key: header_key::MESSAGE_ID.to_string(),
                    value: "m-1".to_string(),
                },
            ],
            payload_encoding: "json".to_string(),
            payload_type: "event".to_string(),
            payload: br#"{"schema":"2.0"}"#.to_vec(),
            log_id_new: "log-new".to_string(),
        };
        let bytes = frame.encode_to_vec();
        let back = Frame::decode(bytes.as_slice()).expect("decode");
        assert_eq!(back, frame);
        assert_eq!(back.msg_type(), Some(msg_type::EVENT));
        assert_eq!(back.payload_text(), Some(r#"{"schema":"2.0"}"#));
    }

    /// 空 payload / 空 headers 往返（ping 帧形态）
    #[test]
    fn ping_frame_roundtrip() {
        let ping = Frame::ping(9);
        assert!(ping.is_control());
        assert_eq!(ping.msg_type(), Some(msg_type::PING));
        assert_eq!(ping.service, 9);
        let back = Frame::decode(ping.encode_to_vec().as_slice()).expect("decode");
        assert_eq!(back, ping);
    }

    /// 超长 payload（256 KiB）往返——服务端长事件/未分片大帧
    #[test]
    fn large_payload_roundtrip() {
        let mut frame = Frame::ping(1);
        frame.method = frame_type::DATA;
        frame.payload = vec![b'x'; 256 * 1024];
        let back = Frame::decode(frame.encode_to_vec().as_slice()).expect("decode");
        assert_eq!(back.payload.len(), 256 * 1024);
    }

    /// ACK 形状：复用入帧 + 追加 biz_rt + payload {"code":200}
    #[test]
    fn ack_shape() {
        let inbound = Frame {
            seq_id: 11,
            log_id: 22,
            service: 5,
            method: frame_type::DATA,
            headers: vec![Header {
                key: header_key::TYPE.to_string(),
                value: msg_type::EVENT.to_string(),
            }],
            payload_encoding: "json".to_string(),
            payload_type: "event".to_string(),
            payload: br#"{"event":"x"}"#.to_vec(),
            log_id_new: "lg".to_string(),
        };
        // 官方 biz_rt 是**负**耗时（`String(startTime - endTime)`），此处按其字面形态传值
        let ack = inbound.ack(200, -17);
        assert_eq!(ack.seq_id, 11);
        assert_eq!(ack.log_id, 22);
        assert_eq!(ack.service, 5);
        assert_eq!(ack.method, frame_type::DATA);
        assert_eq!(ack.log_id_new, "lg");
        assert_eq!(ack.payload_encoding, "json");
        // 原 header 保留 + biz_rt 追加
        assert_eq!(ack.msg_type(), Some(msg_type::EVENT));
        assert_eq!(ack.header(header_key::BIZ_RT), Some("-17"));
        assert_eq!(ack.headers.len(), 2);
        // payload 被替换
        assert_eq!(ack.payload_text(), Some(r#"{"code":200}"#));
    }

    /// 错误码分类：只有 internal_error 可重试
    #[test]
    fn config_error_classification() {
        assert!(matches!(
            classify_config_error(error_code::INTERNAL_ERROR, "internal"),
            ConnConfigError::Retryable(_)
        ));
        for (code, msg) in [
            (error_code::EXCEED_CONN_LIMIT, "exceed"),
            (error_code::AUTH_FAILED, "auth"),
            (error_code::FORBIDDEN, "forbidden"),
            (error_code::SYSTEM_BUSY, "busy"),
        ] {
            assert!(
                matches!(classify_config_error(code, msg), ConnConfigError::Terminal(_)),
                "code {} should be terminal",
                code
            );
        }
    }

    /// 错误码字面量与官方 SDK 一致（防手误改动）
    #[test]
    fn error_code_literals_match_spec() {
        assert_eq!(error_code::OK, 0);
        assert_eq!(error_code::SYSTEM_BUSY, 1);
        assert_eq!(error_code::FORBIDDEN, 403);
        assert_eq!(error_code::AUTH_FAILED, 514);
        assert_eq!(error_code::INTERNAL_ERROR, 1_000_040_343);
        assert_eq!(error_code::EXCEED_CONN_LIMIT, 1_000_040_350);
        assert_eq!(frame_type::CONTROL, 0);
        assert_eq!(frame_type::DATA, 1);
    }
}
