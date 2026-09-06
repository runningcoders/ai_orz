//! 入站运行状态（通用模型）
//!
//! 一个实体的全部「入站运行时信息」——动态游标 + 动态会话——合成一列 JSON 存取，
//! 与用户静态配置（如 `message_channels.config_json`）物理隔离：
//! 运行时循环只写本列，管理后台只写 config，互不覆盖。
//!
//! 与具体协议解耦：iLink 的 `get_updates_buf` / `context_token`、企微的序号游标 /
//! 会话票据都能装进来。未来新增动态信息直接加字段，零 DDL；任何实体需要增量拉取
//! 时，加同名同类型的 `inbound_state TEXT` 列即可复用。

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// 入站运行状态：动态、由运行时循环独占读写。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct InboundState {
    /// 动态游标：增量拉取进度
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<InboundCursor>,
    /// 动态会话态：按对端组织，内含滚动刷新的动态令牌
    #[serde(default)]
    pub sessions: InboundSessions,
}

impl InboundState {
    /// 序列化为 JSON 文本（落列用；`Default` 序列化为 `"{}"`，同样合法可解析）
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// 从 JSON 文本解析；解析失败返回 `None`，等价于"无状态，从头开始"
    /// （fail-open 而非 panic：损坏的运行态不应阻断入站链路）。
    pub fn from_json(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.is_empty() {
            return None;
        }
        serde_json::from_str(raw).ok()
    }
}

/// 游标语义：决定 value 如何解释、能否比较、能否安全回退
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CursorKind {
    /// 不透明字符串：只能原样回传，不可比较大小、不可回退。
    /// iLink 的 get_updates_buf 属此类。
    #[default]
    Opaque,
    /// 单调递增序号：可比较，回退安全
    Sequence,
    /// 毫秒时间戳：可比较
    Timestamp,
    /// 数值偏移：可比较
    Offset,
}

/// 通用增量拉取游标（与具体协议解耦）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct InboundCursor {
    /// 游标内容（序列化为 UTF-8 文本存储；不透明类型由协议自解释）
    pub value: String,
    /// 游标语义
    #[serde(default)]
    pub kind: CursorKind,
    /// 产生游标的来源标识（如 `ilink`、`wecom_bot`），排障与迁移用
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// 最近更新时间（毫秒时间戳），用于判定陈旧 / 决定是否重置
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at_ms: Option<i64>,
}

impl InboundCursor {
    /// 不透明游标（iLink 的 get_updates_buf 等）
    pub fn opaque(value: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            kind: CursorKind::Opaque,
            source: Some(source.into()),
            updated_at_ms: None,
        }
    }

    /// 单调序号游标
    pub fn sequence(value: i64, source: impl Into<String>) -> Self {
        Self {
            value: value.to_string(),
            kind: CursorKind::Sequence,
            source: Some(source.into()),
            updated_at_ms: None,
        }
    }

    /// 游标内容是否为空（空内容 = 无有效进度）
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// 距最近更新的毫秒数（无更新时间返回 `None`）
    pub fn age_ms(&self, now_ms: i64) -> Option<i64> {
        self.updated_at_ms.map(|t| now_ms.saturating_sub(t))
    }
}

/// 入站会话上下文（以对端为单位）
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct InboundSession {
    /// 对端标识（协议侧原值：iLink 为 from_user_id，企微为 userid）
    pub peer_id: String,
    /// 会话令牌：滚动刷新的动态令牌，回消息时须回传（iLink 的 context_token）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_token: Option<String>,
    /// 最近一次入站的消息 ID，排障用，也可与 AOP 幂等键对照
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_id: Option<String>,
    /// 最近更新时间（毫秒）：挑"最新会话"与判断陈旧都靠它
    #[serde(default)]
    pub updated_at_ms: i64,
}

/// 入站会话集合：一个渠道/实体一份，整体 JSON 落列
///
/// 内部用 `Vec` 而不是 `HashMap`：map 的 key 会与 `peer_id` 字段冗余；
/// peer 数量是个位数，线性查找开销可忽略；Vec 顺序稳定，日志与排障更直观。
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct InboundSessions {
    /// 会话列表（按插入顺序，最新活跃的 upsert 会移到末尾）
    #[serde(default)]
    pub sessions: Vec<InboundSession>,
}

/// 会话数上限（防多 peer 长期运行无限膨胀；超出时丢弃最旧的）
const SESSIONS_RETAIN_LIMIT: usize = 100;

impl InboundSessions {
    /// 按 peer 查找（线性，peer 数个位数量级）
    pub fn get(&self, peer_id: &str) -> Option<&InboundSession> {
        self.sessions.iter().find(|s| s.peer_id == peer_id)
    }

    /// 按 peer 可变查找
    pub fn get_mut(&mut self, peer_id: &str) -> Option<&mut InboundSession> {
        self.sessions.iter_mut().find(|s| s.peer_id == peer_id)
    }

    /// 插入或更新会话（`None` 字段保留原值；已存在则移到末尾保持最新在后的顺序）
    pub fn upsert(
        &mut self,
        peer_id: impl Into<String>,
        context_token: Option<String>,
        last_message_id: Option<String>,
        updated_at_ms: i64,
    ) {
        let peer_id = peer_id.into();
        if let Some(existing) = self.get_mut(&peer_id) {
            if context_token.is_some() {
                existing.context_token = context_token;
            }
            if last_message_id.is_some() {
                existing.last_message_id = last_message_id;
            }
            existing.updated_at_ms = updated_at_ms;
            return;
        }
        self.sessions.push(InboundSession {
            peer_id,
            context_token,
            last_message_id,
            updated_at_ms,
        });
    }

    /// 取最近活跃的会话（对端未配置时出站兜底用）
    pub fn latest(&self) -> Option<&InboundSession> {
        self.sessions.iter().max_by_key(|s| s.updated_at_ms)
    }

    /// 仅保留最近 `max` 条会话（按 `updated_at_ms` 降序），防无限膨胀
    pub fn retain_latest(&mut self, max: usize) {
        if self.sessions.len() <= max {
            return;
        }
        self.sessions
            .sort_by_key(|s| std::cmp::Reverse(s.updated_at_ms));
        self.sessions.truncate(max);
    }

    /// 默认上限的裁剪（每次写回前调用）
    pub fn retain_default(&mut self) {
        self.retain_latest(SESSIONS_RETAIN_LIMIT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_json_roundtrip() {
        let mut sessions = InboundSessions::default();
        sessions.upsert(
            "peer_1",
            Some("tok_a".to_string()),
            Some("msg_1".to_string()),
            100,
        );
        let state = InboundState {
            cursor: Some(InboundCursor::opaque("abc123", "ilink")),
            sessions,
        };

        let json = state.to_json();
        let parsed = InboundState::from_json(&json).expect("应可解析");
        assert_eq!(parsed, state);
        assert_eq!(parsed.cursor.as_ref().unwrap().value, "abc123");
        assert_eq!(
            parsed
                .sessions
                .get("peer_1")
                .unwrap()
                .context_token
                .as_deref(),
            Some("tok_a")
        );
    }

    #[test]
    fn test_from_json_fail_open() {
        assert!(InboundState::from_json("").is_none());
        assert!(InboundState::from_json("not json").is_none());
        // 空对象 = 合法空状态
        assert_eq!(InboundState::from_json("{}"), Some(InboundState::default()));
        // 未知字段向后兼容（serde 默认忽略）
        assert!(InboundState::from_json(r#"{"future_field":1}"#).is_some());
    }

    #[test]
    fn test_cursor_kinds_and_age() {
        let c = InboundCursor::sequence(42, "wecom_bot");
        assert_eq!(c.kind, CursorKind::Sequence);
        assert!(!c.is_empty());
        assert_eq!(InboundCursor::default().age_ms(1000), None);
        let mut c2 = InboundCursor::opaque("x", "ilink");
        c2.updated_at_ms = Some(900);
        assert_eq!(c2.age_ms(1000), Some(100));
        // serde snake_case 往返
        assert_eq!(
            serde_json::from_str::<InboundCursor>(r#"{"value":"v","kind":"opaque"}"#).unwrap(),
            InboundCursor::opaque("v", "")
        );
    }

    #[test]
    fn test_sessions_upsert_get_latest() {
        let mut s = InboundSessions::default();
        s.upsert("a", Some("t1".to_string()), None, 100);
        s.upsert("b", Some("t2".to_string()), None, 200);
        // 更新已有 peer：None 字段保留原值，token 覆盖
        s.upsert("a", Some("t1_new".to_string()), Some("m9".to_string()), 300);
        let a = s.get("a").unwrap();
        assert_eq!(a.context_token.as_deref(), Some("t1_new"));
        assert_eq!(a.last_message_id.as_deref(), Some("m9"));
        assert_eq!(a.updated_at_ms, 300);
        assert_eq!(s.latest().unwrap().peer_id, "a");
        assert!(s.get("missing").is_none());
    }

    #[test]
    fn test_sessions_retain_latest() {
        let mut s = InboundSessions::default();
        for i in 0..150 {
            s.upsert(format!("peer_{i}"), None, None, i);
        }
        s.retain_latest(100);
        assert_eq!(s.sessions.len(), 100);
        // 最旧的被裁掉，最新的保留
        assert!(s.get("peer_0").is_none());
        assert!(s.get("peer_149").is_some());
    }
}
