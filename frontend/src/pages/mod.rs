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

use dioxus::prelude::*;

// 导入路由组件函数到当前作用域，供 Routable 宏使用
use crate::pages::finance::attachments::FinanceAttachments;
use crate::pages::finance::message_channels::FinanceMessageChannels;
use crate::pages::finance::mcp_servers::FinanceMcpServers;
use crate::pages::finance::model_providers::FinanceModelProviders;
use crate::pages::finance::tools::FinanceTools;
use crate::pages::hr::agent_detail::HrAgentDetail;
use crate::pages::hr::agents::HrAgents;
use crate::pages::hr::skills::HrSkills;
use crate::pages::message::chat::MessageChat;
use crate::pages::organization::info::OrganizationInfo;
use crate::pages::organization::users::OrganizationUsers;
use crate::pages::project::artifacts::ProjectArtifacts;
use crate::pages::project::project_detail::ProjectDetail;
use crate::pages::project::projects::ProjectList;
use crate::pages::reception::Reception;
use crate::pages::settings::Settings;
use crate::pages::system::health::SystemHealth;
use crate::pages::system::triggers::SystemTriggers;
use crate::pages::user::profile::UserProfile;

/// 全局路由枚举
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    // 前台接待（登录/初始化）
    #[route("/login")]
    Reception {},

    // 对话首页
    #[route("/")]
    MessageChat {},

    // 组织模块
    #[route("/organization")]
    OrganizationInfo {},
    #[route("/organization/users")]
    OrganizationUsers {},

    // HR 模块
    #[route("/hr/agents")]
    HrAgents {},
    #[route("/hr/agents/:id")]
    HrAgentDetail { id: String },
    #[route("/hr/skills")]
    HrSkills {},

    // Finance 模块
    #[route("/finance/model-providers")]
    FinanceModelProviders {},
    #[route("/finance/tools")]
    FinanceTools {},
    #[route("/finance/message-channels")]
    FinanceMessageChannels {},
    #[route("/finance/mcp-servers")]
    FinanceMcpServers {},
    #[route("/finance/attachments")]
    FinanceAttachments {},

    // Project 模块
    #[route("/projects")]
    ProjectList {},
    #[route("/projects/:id")]
    ProjectDetail { id: String },
    #[route("/projects/artifacts")]
    ProjectArtifacts {},

    // System 模块
    #[route("/system/triggers")]
    SystemTriggers {},
    #[route("/system/health")]
    SystemHealth {},

    // 用户
    #[route("/user/profile")]
    UserProfile {},

    // 设置
    #[route("/settings")]
    Settings {},
}
