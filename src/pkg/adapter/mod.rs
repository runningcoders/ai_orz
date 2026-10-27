//! 外部消息适配者注册中心
//!
//! 作为基础设施层，提供统一的适配者注册与获取能力。
//! 各渠道 DAL（如 `LarkDalImpl`）在 init 时向注册中心注册自己，
//! producer 层通过注册中心获取适配者，运行适配得到内部消息后投递。
//!
//! 设计原则：
//! - pkg/adapter 不依赖任何 DAO/DAL 层类型（纯基础设施）
//! - 适配者以 `Arc<dyn Any + Send + Sync>` 存储，producer 按渠道 downcast 取用
//! - `AdaptedMessage` 为 owned 转换结果，producer 据此构造 `SendToAgentCommand`

pub mod message;

use common::enums::ChannelType;
use common::error::{Result, err};
use once_cell::sync::Lazy;
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// ==================== 适配结果 ====================

/// 渠道入站附件单文件大小上限（10MB）
///
/// 与运行时视觉注入的单图准入上限（`awakening` 的 `MAX_IMAGE_BYTES`）保持一致：
/// 超过该值的图片即使落库也不会被注入模型，提前在下载侧拒绝可省内存与磁盘。
pub const MAX_INBOUND_ATTACHMENT_BYTES: usize = 10 * 1024 * 1024;

/// 单条入站消息携带图片数上限（4 张）
///
/// 与运行时视觉注入的「单条 ≤ 4 张」准入一致：图文混排消息（飞书 `post`）
/// 可能一次带很多图，超出部分只下载前 4 张，其余留日志丢弃。
pub const MAX_INBOUND_IMAGES_PER_MESSAGE: usize = 4;

/// 按图片 MIME 给出文件扩展名（不含点；未知类型按 `jpg`）
///
/// 渠道侧拿不到原始文件名，落库前需按真实 MIME 生成带正确扩展名的文件名，
/// 否则附件会以错误扩展名存储（下游按文件名推断类型时失真）。
pub fn image_extension_for_mime(mime_type: &str) -> &'static str {
    match mime_type.split(';').next().unwrap_or_default().trim() {
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        _ => "jpg",
    }
}

/// 渠道入站附件（图片等二进制资源）
///
/// 渠道 DAL 完成协议侧下载后填入 [`AdaptedMessage::attachments`]；producer 负责
/// 落库为内部 Attachment 资产，再以 `attachment_ids` 交给消息投递链路。
/// 本结构只承载字节与元数据，不含任何渠道协议字段（保持 pkg 层纯净）。
#[derive(Debug, Clone)]
pub struct AdaptedAttachment {
    /// 原始文件名（渠道未提供时由适配层按 MIME 推断生成）
    pub file_name: String,
    /// MIME 类型（如 `image/jpeg`）
    pub mime_type: String,
    /// 文件字节
    pub bytes: Vec<u8>,
}

/// 适配后的内部消息（owned）
///
/// 由各渠道 DAL 的 adapt 方法返回，consumer 据此构造 `SendToAgentCommand`
/// 并调用 `MessageDomain::send_to_agent` 完成发送。
#[derive(Debug, Clone)]
pub struct AdaptedMessage {
    /// 发送者 ID（外部用户在内部系统的映射 ID）
    pub from_id: String,
    /// 发送者角色（外部消息一般为 User）
    pub from_role: common::enums::MessageRole,
    /// 目标 Agent ID
    ///
    /// 渠道已绑定 agent_id 时直接填充；未绑定时为 `None`，由 consumer 层
    /// 通过角色路由策略（如 `feishu_reception` tag）决定。
    pub to_agent_id: Option<String>,
    /// 来源渠道类型（producer 据此组装接待角色档位链；不填会误路由到默认渠道）
    pub channel_type: ChannelType,
    /// 消息内容（纯文本）
    pub content: String,
    /// 关联项目 ID（可选）
    pub project_id: Option<String>,
    /// 关联任务 ID（可选）
    pub task_id: Option<String>,
    /// 引用的父消息 ID（可选，支持消息链）
    pub reply_to_id: Option<String>,
    /// 外部渠道消息键（可选，形如 `"lark:om_xxx"` / `"wechat:{message_id}"`，渠道前缀防撞键）
    ///
    /// 渠道消息自身的平台侧 ID，随消息落库到 `messages.external_key`。
    ///
    /// 命名沿用了最初用途（**飞书**入站回复按平台 `parent_id`/`root_id` 反查父消息、
    /// 贯通消息链）；微信侧同样填该字段，承载的是**「渠道消息平台 ID 通用存档」**这一
    /// 扩展语义——iLink 协议没有线程/回复字段，故**不承担反查父消息**职责。
    /// 该列是普通索引、无唯一约束，新增写入无需 DDL。
    pub external_key: Option<String>,
    /// 外部渠道话题键（可选，形如 `"omt_xxx"`，飞书「话题」唯一标识）
    ///
    /// 渠道自身的「话题/线程」平台侧 ID，随消息落库到 `messages.thread_id`。
    /// 同一话题内所有消息共享同一值；普通私信与非话题回复为 `None`。
    pub thread_id: Option<String>,
    /// 渠道入站附件（图片等），由 producer 落库后随文本消息一并投递
    ///
    /// 无附件时为**空 Vec**（不设 `Option`：消费侧遍历即可，无额外分支）。
    pub attachments: Vec<AdaptedAttachment>,
}

// ==================== 注册中心 ====================

/// 适配者注册中心
///
/// 按 `ChannelType` 注册/获取适配者。
/// 适配者以 `Arc<dyn Any + Send + Sync>` 存储，consumer 获取后 downcast 为具体类型。
pub struct AdapterRegistry {
    adapters: RwLock<HashMap<i32, Arc<dyn Any + Send + Sync>>>,
}

impl AdapterRegistry {
    fn new() -> Self {
        Self {
            adapters: RwLock::new(HashMap::new()),
        }
    }

    /// 注册适配者
    ///
    /// `T` 是具体 DAL 类型（如 `LarkDalImpl`）。
    /// 重复注册同一渠道会返回 `Conflict` 错误。
    pub fn register<T>(&self, channel_type: ChannelType, adapter: Arc<T>) -> Result<()>
    where
        T: Any + Send + Sync + 'static,
    {
        let mut map = self
            .adapters
            .write()
            .map_err(|e| err!(Internal, "adapter registry lock poisoned: {}", e))?;
        let key = channel_type.into();
        if map.contains_key(&key) {
            return Err(err!(
                Conflict,
                "adapter already registered for channel {:?}",
                channel_type
            ));
        }
        map.insert(key, adapter as Arc<dyn Any + Send + Sync>);
        Ok(())
    }

    /// 获取适配者（按具体类型 downcast）
    ///
    /// 返回 `Arc<T>`，类型不匹配返回 `None`。
    pub fn get<T>(&self, channel_type: ChannelType) -> Option<Arc<T>>
    where
        T: Any + Send + Sync + 'static,
    {
        let map = self.adapters.read().ok()?;
        let any = map.get(&i32::from(channel_type))?.clone();
        any.downcast::<T>().ok()
    }

    /// 是否已注册某渠道适配者
    pub fn has(&self, channel_type: ChannelType) -> bool {
        self.adapters
            .read()
            .map(|m| m.contains_key(&i32::from(channel_type)))
            .unwrap_or(false)
    }
}

// ==================== 全局单例 ====================

static REGISTRY: Lazy<AdapterRegistry> = Lazy::new(AdapterRegistry::new);

/// 获取全局注册中心
pub fn registry() -> &'static AdapterRegistry {
    &REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeAdapter {
        name: String,
    }

    #[test]
    fn test_register_and_get() {
        let reg = AdapterRegistry::new();
        let adapter = Arc::new(FakeAdapter {
            name: "lark".to_string(),
        });
        reg.register(ChannelType::Lark, adapter).unwrap();

        let got = reg.get::<FakeAdapter>(ChannelType::Lark).unwrap();
        assert_eq!(got.name, "lark");
    }

    #[test]
    fn test_duplicate_register_returns_conflict() {
        let reg = AdapterRegistry::new();
        let a1 = Arc::new(FakeAdapter {
            name: "a1".to_string(),
        });
        let a2 = Arc::new(FakeAdapter {
            name: "a2".to_string(),
        });
        reg.register(ChannelType::Lark, a1).unwrap();
        let result = reg.register(ChannelType::Lark, a2);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_wrong_type_returns_none() {
        let reg = AdapterRegistry::new();
        let adapter = Arc::new(FakeAdapter {
            name: "lark".to_string(),
        });
        reg.register(ChannelType::Lark, adapter).unwrap();

        struct OtherAdapter;
        assert!(reg.get::<OtherAdapter>(ChannelType::Lark).is_none());
    }

    #[test]
    fn test_get_unregistered_returns_none() {
        let reg = AdapterRegistry::new();
        assert!(reg.get::<FakeAdapter>(ChannelType::Wechat).is_none());
        assert!(!reg.has(ChannelType::Wechat));
    }
}
