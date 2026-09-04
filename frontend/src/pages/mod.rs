//! 页面模块 - 按业务域分组

pub mod finance;
pub mod hr;
pub mod message;
pub mod organization;
pub mod project;
pub mod reception;
pub mod settings;
pub mod system;
pub mod user;
pub mod workspace;

use dioxus::prelude::*;

// 导入路由组件函数到当前作用域，供 Routable 宏使用
use crate::pages::finance::attachment_detail::FinanceAttachmentDetail;
use crate::pages::finance::attachments::FinanceAttachments;
use crate::pages::finance::identity::FinanceIdentity;
use crate::pages::finance::mcp_server_detail::FinanceMcpServerDetail;
use crate::pages::finance::mcp_servers::FinanceMcpServers;
use crate::pages::finance::message_channel_detail::FinanceMessageChannelDetail;
use crate::pages::finance::message_channels::FinanceMessageChannels;
use crate::pages::finance::model_provider_detail::FinanceModelProviderDetail;
use crate::pages::finance::model_providers::FinanceModelProviders;
use crate::pages::finance::tool_call_entries::FinanceToolCallEntries;
use crate::pages::finance::tool_detail::FinanceToolDetail;
use crate::pages::finance::tools::FinanceTools;
use crate::pages::hr::agent_detail::HrAgentDetail;
use crate::pages::hr::agents::HrAgents;
use crate::pages::hr::knowledge_graph::HrKnowledgeGraph;
use crate::pages::hr::memory_search::HrMemorySearch;
use crate::pages::hr::skill_detail::HrSkillDetail;
use crate::pages::hr::skills::HrSkills;
use crate::pages::message::chat::MessageChat;
use crate::pages::message::search::MessageSearch;
use crate::pages::organization::info::OrganizationInfo;
use crate::pages::organization::links::OrganizationLinks;
use crate::pages::organization::users::OrganizationUsers;
use crate::pages::project::artifact_detail::ProjectArtifactDetail;
use crate::pages::project::artifacts::ProjectArtifacts;
use crate::pages::project::project_detail::ProjectDetail;
use crate::pages::project::projects::ProjectList;
use crate::pages::project::task_detail::TaskDetail;
use crate::pages::project::tasks::TaskList;
use crate::pages::reception::Reception;
use crate::pages::settings::Settings;
use crate::pages::system::aop::SystemAop;
use crate::pages::system::backup::SystemBackup;
use crate::pages::system::docs::SystemDocs;
use crate::pages::system::health::SystemHealth;
use crate::pages::system::logs::SystemLogs;
use crate::pages::system::processes::SystemProcesses;
use crate::pages::system::seed::SystemSeed;
use crate::pages::system::tasks::SystemTasks;
use crate::pages::system::triggers::SystemTriggers;
use crate::pages::user::profile::UserProfile;
use crate::pages::workspace::Workspace;

/// 全局路由枚举
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    // 前台接待（登录/初始化）
    #[route("/login")]
    Reception {},

    // 对话首页
    #[route("/")]
    MessageChat {},
    #[route("/messages/search")]
    MessageSearch {},

    // 组织模块
    #[route("/organization")]
    OrganizationInfo {},
    #[route("/organization/users")]
    OrganizationUsers {},
    #[route("/organization/links")]
    OrganizationLinks {},

    // HR 模块
    #[route("/hr/agents")]
    HrAgents {},
    #[route("/hr/agents/:id")]
    HrAgentDetail { id: String },
    #[route("/hr/skills")]
    HrSkills {},
    #[route("/hr/skills/:id")]
    HrSkillDetail { id: String },
    #[route("/hr/memory-search")]
    HrMemorySearch {},
    #[route("/hr/knowledge-graph")]
    HrKnowledgeGraph {},

    // Finance 模块
    #[route("/finance/model-providers")]
    FinanceModelProviders {},
    #[route("/finance/model-providers/:id")]
    FinanceModelProviderDetail { id: String },
    #[route("/finance/tools")]
    FinanceTools {},
    #[route("/finance/tools/:id")]
    FinanceToolDetail { id: String },
    #[route("/finance/tool-call-entries")]
    FinanceToolCallEntries {},
    #[route("/finance/identity")]
    FinanceIdentity {},
    #[route("/finance/message-channels")]
    FinanceMessageChannels {},
    #[route("/finance/message-channels/:id")]
    FinanceMessageChannelDetail { id: String },
    #[route("/finance/mcp-servers")]
    FinanceMcpServers {},
    #[route("/finance/mcp-servers/:id")]
    FinanceMcpServerDetail { id: String },
    #[route("/finance/attachments")]
    FinanceAttachments {},
    #[route("/finance/attachments/:id")]
    FinanceAttachmentDetail { id: String },

    // Project 模块
    #[route("/projects")]
    ProjectList {},
    #[route("/projects/:id")]
    ProjectDetail { id: String },
    #[route("/projects/artifacts")]
    ProjectArtifacts {},
    #[route("/projects/artifacts/:id")]
    ProjectArtifactDetail { id: String },
    #[route("/tasks")]
    TaskList {},
    #[route("/tasks/:id")]
    TaskDetail { id: String },

    // System 模块
    #[route("/system/triggers")]
    SystemTriggers {},
    #[route("/system/health")]
    SystemHealth {},
    #[route("/system/docs")]
    SystemDocs {},
    #[route("/system/logs")]
    SystemLogs {},
    #[route("/system/backup")]
    SystemBackup {},
    #[route("/system/processes")]
    SystemProcesses {},
    #[route("/system/aop")]
    SystemAop {},
    #[route("/system/seed")]
    SystemSeed {},
    #[route("/system/tasks")]
    SystemTasks {},

    // 用户
    #[route("/user/profile")]
    UserProfile {},

    // 工作台（Canvas 试点）
    #[route("/workspace")]
    Workspace {},

    // 设置
    #[route("/settings")]
    Settings {},
}
