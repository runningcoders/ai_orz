//! Shared enumerations used by both backend and frontend

pub mod agent;
pub mod agent_kind;
pub mod artifact;
pub mod caller_type;
pub mod cron_trigger;
pub mod file;
pub mod mcp_server;
pub mod memory;
pub mod message;
pub mod message_channel;
pub mod organization;
pub mod project;
pub mod provider;
pub mod skill;
pub mod task;
/// 思考场景枚举（awaken/settle/summary/intent-analyze）
pub mod thinking_scene;
pub mod tool;
pub mod user;

pub use agent::{AgentRuntimeState, AgentStatus, ModelProviderStatus};
pub use agent_kind::AgentKind;
pub use artifact::ArtifactSourceType;
pub use caller_type::CallerType;
pub use cron_trigger::TriggerType;
pub use file::FileType;
pub use mcp_server::{McpServerStatus, McpTransport};
pub use memory::{KnowledgeRelationType, MemoryRole, MemoryStatus, MemoryType};
pub use message::{MessageRole, MessageStatus, MessageType};
pub use message_channel::{ChannelStatus, ChannelType};
pub use organization::{OrganizationScope, OrganizationStatus};
pub use project::ProjectStatus;
pub use provider::{ModelCapability, ProviderType};
pub use skill::{SkillAuthorType, SkillStatus};
pub use task::{AssigneeType, TaskStatus};
pub use thinking_scene::ThinkingScene;
pub use tool::{ControlMode, ToolProtocol, ToolStatus};
pub use user::{UserRole, UserStatus};
