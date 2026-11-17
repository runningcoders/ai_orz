//! 系统内统一 ID 管理。
//!
//! 系统里存在多种 ID 格式，本模块把它们收拢到一处：**字符串格式不变，只用新类型约束生成与解析**。
//!
//! | 类型 | 字符串格式 | 典型用途 |
//! | --- | --- | --- |
//! | [`EntityId`] | `{prefix}_{uuid_simple}` | 内部实体（知识、任务、消息等） |
//! | [`UserId`] | 16 位 `[A-Z0-9]` | 用户 |
//! | [`OrgId`] | 12 位 `[A-Z0-9]` | 组织 |
//! | [`UuidId`] | 32 位无横杠 UUID | 配对码、组织链接、联邦合约 |
//! | [`TimeId`] | `YYYYMMDDHHMMSS` + 18 位十六进制 | 事件、请求、会话、SSE 连接、进度任务 |
//! | [`A2aTaskId`] | 32 位无横杠 UUID，或对端的不透明字符串 | A2A 协议任务 |
//! | [`ExternalId`] | 原样保留 | 飞书 open_id、企微 client_id 等第三方 ID |
//!
//! [`IdValue`] 是上述类型的统一泛型边界。
//!
//! 设计目标：
//! - **格式集中**：新增 ID 格式只改本模块，调用方不再各自拼随机串；
//! - **前缀即类型**：一种实体一个 [`IdPrefix`]，看 ID 就知道是什么实体；
//! - **单一事实来源**：新增/修改实体前缀只改 [`IdPrefix`] 一处；
//! - **可生成、可解析**：[`EntityId`] 可以从枚举生成，也可以从字符串解析回枚举 + UUID；
//!   日常生成优先使用 `new_id!` 宏，其它类型使用各自的 `generate()`；
//! - **对 Agent 友好**：本系统生成的 UUID 统一使用 `simple()`（32 位十六进制、无横杠），
//!   避免横杠被 Agent 当作分隔符导致参数被拆分；解析侧仍兼容带横杠的旧值；
//! - **时间可索引**：事件、请求等带时间属性的内部 ID 使用 [`TimeId`]，
//!   本地时间 `YYYYMMDDHHMMSS` 开头，可直接解析出日期，与按天 JSONL 分片对齐；
//! - **对外以协议为准**：需要发给外部系统的字段（如企微 iLink 的 `client_id`）
//!   按对方文档输出，必要时用 [`UuidId::to_hyphenated`] 取标准带横杠文本，
//!   不在内部强制归一化；
//! - **动态扩展**：未登记的前缀会落到 [`IdPrefix::Custom`]，
//!   也可以通过 [`EntityId::generate_custom`] 生成。
//!
//! 实体 ID 格式固定为 `{prefix}_{uuid}`，例如 `kn_0192abcd...`。
//! 解析使用「最后一个下划线」切分，因此前缀本身可以包含下划线（如 `ont_cls`）。
//! 新生成的 UUIDv7 自带时间戳，同一前缀下的 ID 字典序即时间序。
//!
//! # 兼容性
//!
//! 历史数据不迁移：旧哈希 ID / 带横杠 UUID 仍以字符串形式存在于数据库，
//! 读取侧继续按 `String` 处理。类型化 ID 只约束**新生成**的值，并保持原有字符串格式；
//! 旧哈希 ID 不是 UUID，[`EntityId::parse`] 无法解析。

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id_prefixes {
    (
        $(
            $(#[$meta:meta])*
            $variant:ident => $prefix:literal,
        )*
    ) => {
        /// 实体 ID 前缀（实体种类）。
        ///
        /// 新增实体时只在本宏调用里加一行，前缀字符串、`ALL` 和解析映射会自动生成。
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum IdPrefix {
            $(
                $(#[$meta])*
                $variant,
            )*
            /// 动态前缀（未登记的实体类型）。
            ///
            /// 优先通过 [`EntityId::generate_custom`] 构造，它会校验空前缀和非法字符。
            Custom(String),
        }

        impl IdPrefix {
            /// 所有已登记的前缀（不包含 [`Self::Custom`]）。
            pub const ALL: &'static [IdPrefix] = &[
                $( Self::$variant, )*
            ];

            /// 前缀字符串。
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $prefix, )*
                    Self::Custom(prefix) => prefix,
                }
            }

            /// 根据前缀字符串还原实体种类；未登记的前缀落到 [`Self::Custom`]。
            pub fn from_prefix(prefix: &str) -> Self {
                match prefix {
                    $( $prefix => Self::$variant, )*
                    other => Self::Custom(other.to_string()),
                }
            }
        }
    };
}

define_id_prefixes! {
    /// 短期记忆，前缀 `st`
    ShortTerm => "st",
    /// 长期知识节点，前缀 `kn`
    KnowledgeNode => "kn",
    /// 知识关系边，前缀 `krel`
    KnowledgeRelation => "krel",
    /// 知识引用，前缀 `kref`
    KnowledgeReference => "kref",
    /// 知识属性，前缀 `kp`
    KnowledgeProperty => "kp",
    /// 思考轨迹，前缀 `trace`
    Trace => "trace",
    /// 项目，前缀 `prj`
    Project => "prj",
    /// 任务，前缀 `task`
    Task => "task",
    /// 消息，前缀 `msg`
    Message => "msg",
    /// 消息渠道，前缀 `chan`
    MessageChannel => "chan",
    /// 消息渠道会话绑定，前缀 `mcc`
    MessageChannelConversation => "mcc",
    /// 附件，前缀 `att`
    Attachment => "att",
    /// 交付物，前缀 `art`
    Artifact => "art",
    /// 定时触发器，前缀 `cron`
    CronTrigger => "cron",
    /// 本体实体类，前缀 `ont_cls`
    OntologyClass => "ont_cls",
    /// 本体属性，前缀 `ont_prop`
    OntologyProperty => "ont_prop",
    /// 本体关系类型，前缀 `ont_rel`
    OntologyRelationType => "ont_rel",
    /// 本体类关系（三元组），前缀 `ont_crel`
    OntologyClassRelation => "ont_crel",
    /// 本体同义映射，前缀 `ont_syn`
    OntologySynonym => "ont_syn",
    /// 工具，前缀 `tool`
    Tool => "tool",
    /// MCP Server，前缀 `mcps`
    McpServer => "mcps",
    /// 技能，前缀 `skill`
    Skill => "skill",
    /// Agent，前缀 `agent`
    Agent => "agent",
    /// 模型 Provider，前缀 `prov`
    ModelProvider => "prov",
    /// 金融工具授权，前缀 `auth`
    FinanceAuthorization => "auth",
    /// 金融工具授权授予，前缀 `grant`
    FinanceGrant => "grant",
    /// 用户凭据，前缀 `cred`
    UserCredential => "cred",
    /// 工具调用兜底 ID，前缀 `tc`
    ToolCall => "tc",
}

impl IdPrefix {
    /// 用当前前缀生成一个新的 UUIDv7 实体 ID。
    pub fn generate(&self) -> EntityId {
        EntityId::generate(self.clone())
    }

    /// 是否为未登记的动态前缀。
    pub fn is_custom(&self) -> bool {
        matches!(self, Self::Custom(_))
    }
}

impl fmt::Display for IdPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 类型化实体 ID：前缀（实体种类）+ UUIDv7。
///
/// 字符串形式为 `{prefix}_{uuid_simple}`，例如 `st_0192abcd...`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EntityId {
    prefix: IdPrefix,
    uuid: Uuid,
}

impl EntityId {
    /// 用指定实体种类生成新的 UUIDv7 ID。
    ///
    /// # Panics
    ///
    /// 如果传入的是 [`IdPrefix::Custom`] 且前缀非法（空或含非法字符）会 panic。
    /// 动态前缀请优先使用 [`Self::generate_custom`]，它返回 `Result`。
    pub fn generate(prefix: IdPrefix) -> Self {
        let prefix = match prefix {
            IdPrefix::Custom(custom) => {
                if let Err(err) = validate_prefix(&custom) {
                    panic!("{err}");
                }
                // 已知前缀被误装成 Custom 时归一化，保证「生成 → 解析」结果一致。
                IdPrefix::from_prefix(&custom)
            }
            known => known,
        };
        Self {
            prefix,
            uuid: Uuid::now_v7(),
        }
    }

    /// 用动态前缀生成新的 UUIDv7 ID。
    ///
    /// 前缀必须非空，且只能包含 ASCII 字母、数字或下划线。
    pub fn generate_custom(prefix: impl Into<String>) -> Result<Self, IdParseError> {
        let prefix = prefix.into();
        validate_prefix(&prefix)?;
        Ok(Self {
            // 已知前缀会被规范化回枚举，保证「生成 → 解析」结果一致。
            prefix: IdPrefix::from_prefix(&prefix),
            uuid: Uuid::now_v7(),
        })
    }

    /// 从字符串解析：`{prefix}_{uuid}`。
    ///
    /// - 用最后一个下划线切分，允许前缀含下划线；
    /// - UUID 同时接受 simple（无横杠）和标准带横杠形式，输出统一为 simple；
    /// - 未登记的前缀会解析为 [`IdPrefix::Custom`]；
    /// - 旧哈希 ID（如 `st_{sha256}`）不是 UUID，会返回 [`IdParseError::InvalidUuid`]。
    pub fn parse(input: &str) -> Result<Self, IdParseError> {
        let (prefix, raw_uuid) = input
            .rsplit_once('_')
            .ok_or(IdParseError::MissingSeparator)?;
        validate_prefix(prefix)?;
        let uuid = Uuid::parse_str(raw_uuid)
            .map_err(|_| IdParseError::InvalidUuid(raw_uuid.to_string()))?;
        Ok(Self {
            prefix: IdPrefix::from_prefix(prefix),
            uuid,
        })
    }

    /// 实体种类前缀。
    pub fn prefix(&self) -> &IdPrefix {
        &self.prefix
    }

    /// 底层 UUID。
    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    /// 是否属于指定实体种类。
    pub fn has_prefix(&self, prefix: &IdPrefix) -> bool {
        &self.prefix == prefix
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}_{}", self.prefix.as_str(), self.uuid.simple())
    }
}

impl FromStr for EntityId {
    type Err = IdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for EntityId {
    type Error = IdParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl TryFrom<&str> for EntityId {
    type Error = IdParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl From<EntityId> for String {
    fn from(id: EntityId) -> Self {
        id.to_string()
    }
}

impl From<EntityId> for Uuid {
    fn from(id: EntityId) -> Self {
        id.uuid
    }
}

impl PartialOrd for EntityId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for EntityId {
    /// 先按 UUIDv7 时间排序，再按前缀兜底，保证混合实体列表也能按时间排列。
    ///
    /// 注意：这与 `to_string()` 的字典序不同（字符串会先按前缀分组）。
    /// 需要按实体类型分组时，先按 [`Self::prefix`] 的字符串排序。
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.uuid
            .cmp(&other.uuid)
            .then_with(|| self.prefix.as_str().cmp(other.prefix.as_str()))
    }
}

/// 所有类型化 ID 的公共约束。
///
/// 各 ID 的字符串格式不同（带前缀的实体 ID、业务 ID、UUID、外部不透明 ID 等），
/// 但都需要可克隆、可比较、可哈希、可展示、可从字符串解析，并可按字符串序列化。
/// 泛型函数用本 trait 作为统一边界，避免为每种 ID 重复写 `where` 约束。
pub trait IdValue:
    fmt::Debug
    + Clone
    + PartialEq
    + Eq
    + std::hash::Hash
    + fmt::Display
    + FromStr<Err = IdParseError>
    + Serialize
    + for<'de> Deserialize<'de>
{
}

impl IdValue for EntityId {}

/// 用户 ID 长度（历史格式：16 位大写字母或数字）。
pub const USER_ID_LEN: usize = 16;

/// 组织 ID 长度（历史格式：12 位大写字母或数字）。
pub const ORG_ID_LEN: usize = 12;

/// 36 进制大写字母表，与历史用户/组织 ID 的字符集保持一致。
const ALNUM_UPPER: &[u8; 36] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// 用 UUIDv7 的随机字节生成指定长度的 `[A-Z0-9]` 字符串。
///
/// 长度小于 16 时取 UUID 尾部字节，尽量避开时间戳前缀以保留随机性；
/// 取模带来的偏差可忽略。长度上限为 16（一个 UUID 的字节数）。
fn random_alnum(len: usize) -> String {
    debug_assert!(len <= 16, "随机 ID 长度不能超过 UUID 字节数");
    let bytes = Uuid::now_v7().into_bytes();
    let offset = bytes.len().saturating_sub(len);
    bytes[offset..]
        .iter()
        .map(|b| ALNUM_UPPER[(*b as usize) % ALNUM_UPPER.len()] as char)
        .collect()
}

/// 校验固定长度的大写字母/数字 ID。
fn is_upper_alnum(input: &str, len: usize) -> bool {
    input.len() == len
        && input
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// 为 `String` 内部的 ID 新类型批量实现展示、解析与 [`IdValue`]。
/// 序列化由新类型上的 `serde(try_from/into = "String")` 负责。
macro_rules! impl_string_newtype_id {
    ($name:ident) => {
        impl $name {
            /// 字符串形式（原样保留）。
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdParseError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(&value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdParseError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl IdValue for $name {}
    };
}

/// 定义以 32 位无横杠 UUID 为字符串形式的 ID 新类型。
///
/// 解析同时接受带横杠与无横杠形式，展示统一为无横杠形式。
macro_rules! define_uuid_id {
    (
        $(#[$meta:meta])*
        $name:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name {
            uuid: Uuid,
        }

        impl $name {
            /// 生成新的 UUIDv7 ID（展示为 32 位无横杠格式）。
            pub fn generate() -> Self {
                Self {
                    uuid: Uuid::now_v7(),
                }
            }

            /// 从字符串解析，兼容带横杠与无横杠两种 UUID 形式，统一规范化为无横杠。
            pub fn parse(input: &str) -> Result<Self, IdParseError> {
                let uuid = Uuid::parse_str(input)
                    .map_err(|_| IdParseError::InvalidUuid(input.to_string()))?;
                Ok(Self { uuid })
            }

            /// 底层 UUID。
            pub fn uuid(&self) -> Uuid {
                self.uuid
            }

            /// 标准带横杠 UUID 文本（36 位）。
            ///
            /// 仅用于**外部协议明确要求带横杠**的场景：内部默认格式始终是无横杠，
            /// 对外字段以对方要求为准，不做强制归一化。
            pub fn to_hyphenated(&self) -> String {
                self.uuid.hyphenated().to_string()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.uuid.simple())
            }
        }

        impl FromStr for $name {
            type Err = IdParseError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Self::parse(s)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdParseError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(&value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = IdParseError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.uuid.simple().to_string()
            }
        }

        impl IdValue for $name {}
    };
}

/// 用户 ID：历史格式为 16 位大写字母或数字（`A-Z0-9`）。
///
/// 生成逻辑集中在本模块，避免各 handler 重复拼随机串。
/// 历史数据（包括测试里构造的 UUID 形式用户 ID）仍按 `String` 读取，不经过本类型。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct UserId(String);

impl UserId {
    /// 生成新的用户 ID（16 位大写字母或数字）。
    ///
    /// 随机源复用 UUIDv7 的随机字节，不额外引入 `rand` 依赖。
    pub fn generate() -> Self {
        Self(random_alnum(USER_ID_LEN))
    }

    /// 解析并校验用户 ID 格式。
    pub fn parse(input: &str) -> Result<Self, IdParseError> {
        if is_upper_alnum(input, USER_ID_LEN) {
            Ok(Self(input.to_string()))
        } else {
            Err(IdParseError::InvalidUserId(input.to_string()))
        }
    }
}

impl_string_newtype_id!(UserId);

/// 组织 ID：历史格式为 12 位大写字母或数字（`A-Z0-9`）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OrgId(String);

impl OrgId {
    /// 生成新的组织 ID（12 位大写字母或数字）。
    pub fn generate() -> Self {
        Self(random_alnum(ORG_ID_LEN))
    }

    /// 解析并校验组织 ID 格式。
    pub fn parse(input: &str) -> Result<Self, IdParseError> {
        if is_upper_alnum(input, ORG_ID_LEN) {
            Ok(Self(input.to_string()))
        } else {
            Err(IdParseError::InvalidOrgId(input.to_string()))
        }
    }
}

impl_string_newtype_id!(OrgId);

define_uuid_id! {
    /// 32 位无横杠 UUID 形式的持久化 ID。
    ///
    /// 用于组织配对码、组织链接、联邦合约这类「无实体前缀、直接存 UUID」的记录。
    /// 解析兼容带横杠形式并统一规范化为无横杠；外部协议要求标准 UUID 文本时，
    /// 用 [`UuidId::to_hyphenated`] 输出带横杠形式。
    UuidId
}

/// 时间前缀 ID：本地时间 `YYYYMMDDHHMMSS` + 18 位十六进制后缀。
///
/// 字符串长度固定 32：前 14 位为本地时间（与按天 JSONL 分片使用的时区一致），
/// 可直接从 ID 解析出日期/时间用于索引；后缀保证同一秒内唯一且无分隔符，
/// 便于 Agent 原样传递。用于事件、请求、会话、SSE 连接、进度任务等运行时标识。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct TimeId(String);

/// 时间前缀格式（本地时间）。
const TIME_ID_TIME_FORMAT: &str = "%Y%m%d%H%M%S";
/// 时间前缀长度（`YYYYMMDDHHMMSS`）。
const TIME_ID_TIME_LEN: usize = 14;
/// 文件分片日期长度（`YYYYMMDD`）。
const TIME_ID_DATE_LEN: usize = 8;
/// 唯一后缀长度（十六进制）。
const TIME_ID_SUFFIX_LEN: usize = 18;
/// TimeId 总长度。
const TIME_ID_LEN: usize = TIME_ID_TIME_LEN + TIME_ID_SUFFIX_LEN;

impl TimeId {
    /// 按当前本地时间生成新 ID。
    ///
    /// 时间取自本地时区，保证 `date_part()` 与每日 JSONL 分片目录一致；
    /// 后缀取 UUIDv7 尾部的随机十六进制，同一秒内仍保持唯一。
    pub fn generate() -> Self {
        let time = chrono::Local::now().format(TIME_ID_TIME_FORMAT).to_string();
        let uuid = Uuid::now_v7().simple().to_string();
        let suffix = &uuid[uuid.len() - TIME_ID_SUFFIX_LEN..];
        Self(format!("{time}{suffix}"))
    }

    /// 解析并校验：14 位合法本地时间 + 18 位小写十六进制。
    pub fn parse(input: &str) -> Result<Self, IdParseError> {
        // 先确认纯 ASCII，避免下面按字节切分时落在多字节字符中间导致 panic。
        if input.len() != TIME_ID_LEN || !input.is_ascii() {
            return Err(IdParseError::InvalidTimeId(input.to_string()));
        }
        let (time, suffix) = input.split_at(TIME_ID_TIME_LEN);
        if chrono::NaiveDateTime::parse_from_str(time, TIME_ID_TIME_FORMAT).is_err()
            || !suffix
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(IdParseError::InvalidTimeId(input.to_string()));
        }
        Ok(Self(input.to_string()))
    }

    /// 时间前缀（本地时间 `YYYYMMDDHHMMSS`）。
    pub fn time_part(&self) -> &str {
        &self.0[..TIME_ID_TIME_LEN]
    }

    /// 文件分片日期（本地时间 `YYYYMMDD`），与按天 JSONL 目录一致。
    pub fn date_part(&self) -> &str {
        &self.0[..TIME_ID_DATE_LEN]
    }

    /// 解析出的本地时间。
    pub fn local_datetime(&self) -> Option<chrono::NaiveDateTime> {
        chrono::NaiveDateTime::parse_from_str(self.time_part(), TIME_ID_TIME_FORMAT).ok()
    }
}

impl_string_newtype_id!(TimeId);

/// A2A 协议任务 ID。
///
/// 本端生成时使用 32 位无横杠 UUIDv7；对端返回的任务 ID 可能是不透明字符串，
/// 因此解析只要求非空白、不改写原始值。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct A2aTaskId(String);

impl A2aTaskId {
    /// 本端生成新的 A2A 任务 ID（32 位无横杠 UUIDv7）。
    pub fn generate() -> Self {
        Self(Uuid::now_v7().simple().to_string())
    }

    /// 包装一个 A2A 任务 ID（本端或对端），空或纯空白会被拒绝。
    pub fn parse(input: &str) -> Result<Self, IdParseError> {
        if input.trim().is_empty() {
            return Err(IdParseError::EmptyId);
        }
        Ok(Self(input.to_string()))
    }
}

impl_string_newtype_id!(A2aTaskId);

/// 外部/第三方系统提供的 ID，如飞书 `open_id`、企微 `client_id`、Provider `tool_call_id`。
///
/// 这些 ID 由外部生成，本系统只透传：不校验具体格式、不改写原始值，只要求非空白。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExternalId(String);

impl ExternalId {
    /// 包装一个外部 ID；空或纯空白会被拒绝。
    pub fn parse(input: &str) -> Result<Self, IdParseError> {
        if input.trim().is_empty() {
            return Err(IdParseError::EmptyId);
        }
        Ok(Self(input.to_string()))
    }
}

impl_string_newtype_id!(ExternalId);

/// ID 解析错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdParseError {
    /// 字符串中没有 `_` 分隔符。
    MissingSeparator,
    /// 前缀为空。
    EmptyPrefix,
    /// 前缀包含非法字符（只允许 ASCII 字母、数字、下划线）。
    InvalidPrefix(String),
    /// UUID 无法解析。
    InvalidUuid(String),
    /// 用户 ID 格式非法（应为 16 位大写字母或数字）。
    InvalidUserId(String),
    /// 组织 ID 格式非法（应为 12 位大写字母或数字）。
    InvalidOrgId(String),
    /// 时间 ID 格式非法（应为 `YYYYMMDDHHMMSS` + 18 位小写十六进制）。
    InvalidTimeId(String),
    /// 外部/协议 ID 不能为空或纯空白。
    EmptyId,
}

impl fmt::Display for IdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingSeparator => write!(f, "ID 缺少 `{{prefix}}_{{uuid}}` 分隔符"),
            Self::EmptyPrefix => write!(f, "ID 前缀不能为空"),
            Self::InvalidPrefix(prefix) => {
                write!(f, "ID 前缀只能包含 ASCII 字母、数字或下划线: {prefix}")
            }
            Self::InvalidUuid(raw) => write!(f, "UUID 无法解析: {raw}"),
            Self::InvalidUserId(id) => write!(f, "用户 ID 应为 16 位大写字母或数字: {id}"),
            Self::InvalidOrgId(id) => write!(f, "组织 ID 应为 12 位大写字母或数字: {id}"),
            Self::InvalidTimeId(id) => {
                write!(f, "时间 ID 应为 YYYYMMDDHHMMSS + 18 位十六进制: {id}")
            }
            Self::EmptyId => write!(f, "ID 不能为空"),
        }
    }
}

impl std::error::Error for IdParseError {}

/// 校验前缀：非空且只含 `[A-Za-z0-9_]`。
fn validate_prefix(prefix: &str) -> Result<(), IdParseError> {
    if prefix.is_empty() {
        return Err(IdParseError::EmptyPrefix);
    }
    if !prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err(IdParseError::InvalidPrefix(prefix.to_string()));
    }
    Ok(())
}

/// 生成带前缀的 UUIDv7 字符串 ID。
///
/// - 已知实体：`new_id!(KnowledgeNode)` → `kn_...`
/// - 动态前缀：`new_id!("tenant")` → `tenant_...`
/// - 字符串若恰好是已知前缀（如 `"st"`），会归一化为对应已知类型
///
/// 动态前缀非法（空或含非标识符字符）会 panic；需要 `Result` 时请用
/// [`EntityId::generate_custom`]。
///
/// # Examples
///
/// ```
/// assert!(common::new_id!(KnowledgeNode).starts_with("kn_"));
/// assert!(common::new_id!("tenant").starts_with("tenant_"));
/// assert!(common::new_id!("st").starts_with("st_"));
/// ```
#[macro_export]
macro_rules! new_id {
    ($kind:ident $(,)?) => {
        $crate::id::IdPrefix::$kind.generate().to_string()
    };
    ($prefix:expr $(,)?) => {
        $crate::id::EntityId::generate_custom(($prefix).to_string())
            .unwrap_or_else(|e| panic!("{e}"))
            .to_string()
    };
}

#[cfg(test)]
#[path = "id_tests.rs"]
mod tests;
