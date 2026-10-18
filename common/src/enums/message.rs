//! Message related enums

use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlx")]
use sqlx::Type;

/// Message role (谁发送的消息)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MessageRole {
    /// User (用户发送)
    #[default]
    User = 0,
    /// Agent (AI 代理发送)
    Agent = 1,
    /// System (系统消息)
    System = 2,
}

impl From<i32> for MessageRole {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageRole::User,
            1 => MessageRole::Agent,
            2 => MessageRole::System,
            _ => MessageRole::default(),
        }
    }
}

impl MessageRole {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<MessageRole> for i32 {
    fn from(r: MessageRole) -> i32 {
        r as i32
    }
}

impl From<i64> for MessageRole {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Message type (消息类型)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MessageType {
    /// Text (纯文本)
    #[default]
    Text = 0,
    /// Image (图片)
    Image = 1,
    /// File (文件)
    File = 2,
    /// Audio (音频)
    Audio = 3,
    /// Video (视频)
    Video = 4,
    /// ToolCallRequest (工具调用请求，Agent→System)
    ToolCallRequest = 5,
    /// ToolCallResult (工具调用结果，System→Agent)
    ToolCallResult = 6,
    /// ConfirmRequest (用户确认请求，Agent→User)
    ConfirmRequest = 7,
    /// ConfirmResponse (用户确认回复，User→Agent)
    ConfirmResponse = 8,
    /// TaskAssignment (任务分配通知，System/User/Agent→Agent)
    TaskAssignment = 9,
    /// 任务调度通知（System→Agent，任务状态变更触发）
    TaskDispatchNotification = 10,
    /// 项目跟进通知（System→Agent，定时补偿触发）
    ProjectFollowupNotification = 11,
    /// Agent 知会通知（Agent→Agent，声明无需回复来源方，防协作乒乓）
    AgentNotify = 12,
    /// 混排消息（content 为多资源混排协议文本，批3 主线启用；批1 仅枚举预留，无生产写入路径）
    Mixed = 13,
}

impl From<i32> for MessageType {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageType::Text,
            1 => MessageType::Image,
            2 => MessageType::File,
            3 => MessageType::Audio,
            4 => MessageType::Video,
            5 => MessageType::ToolCallRequest,
            6 => MessageType::ToolCallResult,
            7 => MessageType::ConfirmRequest,
            8 => MessageType::ConfirmResponse,
            9 => MessageType::TaskAssignment,
            10 => MessageType::TaskDispatchNotification,
            11 => MessageType::ProjectFollowupNotification,
            12 => MessageType::AgentNotify,
            13 => MessageType::Mixed,
            _ => MessageType::default(),
        }
    }
}

impl MessageType {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }
}

impl From<MessageType> for i32 {
    fn from(t: MessageType) -> i32 {
        t as i32
    }
}

impl From<i64> for MessageType {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

/// Message status (处理状态，用于事件总线恢复)
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "sqlx", derive(Type))]
#[cfg_attr(feature = "sqlx", sqlx(type_name = "INTEGER"))]
pub enum MessageStatus {
    /// Recalled (已撤回，逻辑删除，不再显示和处理)
    Recalled = 0,
    /// Pending (待处理)
    #[default]
    Pending = 1,
    /// Processing (处理中)
    Processing = 2,
    /// Processed (处理完成)
    Processed = 3,
    /// Failed (处理失败)
    Failed = 4,
}

impl From<i32> for MessageStatus {
    fn from(v: i32) -> Self {
        match v {
            0 => MessageStatus::Recalled,
            1 => MessageStatus::Pending,
            2 => MessageStatus::Processing,
            3 => MessageStatus::Processed,
            4 => MessageStatus::Failed,
            _ => MessageStatus::default(),
        }
    }
}

impl MessageStatus {
    /// Convert from i32
    pub fn from_i32(v: i32) -> Self {
        v.into()
    }

    /// Convert to i32
    pub fn to_i32(&self) -> i32 {
        (*self).into()
    }

    /// 是否「未完成」：`Pending`（没人领走）或 `Processing`（已被领走、正在跑）
    ///
    /// ⚠️⚠️ **这个判据有两个相反的用途，混用会静默丢是哪一边：**
    ///
    /// | 用途 | 应该用 | 判错方向的后果 |
    /// |---|---|---|
    /// | **去重**（该 Agent / 项目是否已有在途消息） | `is_unfinished()` | 只看 `Pending` ⇒ 正在跑的那条不算「在途」⇒ 重复派发同一任务 |
    /// | **重投**（哪些消息「没人管、要捞回来」） | 仅 `Pending` | 误用 `is_unfinished()` ⇒ 把正在跑的会话又投递一遍，同一条消息并发消费两次 |
    ///
    /// 一句话记法：**去重要「宽」（在飞也算有了），重投要「窄」（只捡没人管的）。**
    pub fn is_unfinished(&self) -> bool {
        matches!(self, MessageStatus::Pending | MessageStatus::Processing)
    }

    /// 是否终态：一旦到达就不该再被任何生命周期回调覆盖（`Recalled` / `Processed`）
    ///
    /// ⚠️ `Failed` **不在终态里**：它是可重投的中间态（重投成功后仍会走到 `Processed`）。
    /// 把它误判成终态会让失败消息永久卡死。
    pub fn is_terminal(&self) -> bool {
        matches!(self, MessageStatus::Recalled | MessageStatus::Processed)
    }
}

impl From<MessageStatus> for i32 {
    fn from(s: MessageStatus) -> i32 {
        s as i32
    }
}

impl From<i64> for MessageStatus {
    fn from(v: i64) -> Self {
        (v as i32).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 锁住「去重要宽、重投要窄」这组相反口径 —— 两者搞反都会静默出错：
    /// 去重错 ⇒ 重复派发同一任务；重投错 ⇒ 同一条消息被并发消费两次。
    #[test]
    fn unfinished_covers_both_queued_and_inflight() {
        assert!(MessageStatus::Pending.is_unfinished());
        assert!(MessageStatus::Processing.is_unfinished());

        assert!(!MessageStatus::Processed.is_unfinished());
        assert!(!MessageStatus::Recalled.is_unfinished());
        // Failed 是可被重投的中间态，不在「未完成」里 —— 它由别的路径兜
        assert!(!MessageStatus::Failed.is_unfinished());
    }

    #[test]
    fn failed_is_not_terminal_so_it_can_be_retried() {
        assert!(MessageStatus::Processed.is_terminal());
        assert!(MessageStatus::Recalled.is_terminal());
        assert!(!MessageStatus::Failed.is_terminal());
        assert!(!MessageStatus::Pending.is_terminal());
        assert!(!MessageStatus::Processing.is_terminal());
    }

    /// 状态值不能漂移：`#[repr(i32)]` 的判别值已在库里落地
    #[test]
    fn status_discriminants_are_stable() {
        assert_eq!(MessageStatus::Recalled.to_i32(), 0);
        assert_eq!(MessageStatus::Pending.to_i32(), 1);
        assert_eq!(MessageStatus::Processing.to_i32(), 2);
        assert_eq!(MessageStatus::Processed.to_i32(), 3);
        assert_eq!(MessageStatus::Failed.to_i32(), 4);
    }
}
