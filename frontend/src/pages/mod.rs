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

use dioxus_router::prelude::*;

/// 全局路由枚举
#[derive(Clone, Routable, Debug, PartialEq)]
pub enum Route {
    // 前台接待（登录/初始化）
    #[route("/")]
    Reception {},

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

    // Project 模块
    #[route("/projects")]
    ProjectList {},
    #[route("/projects/:id")]
    ProjectDetail { id: String },

    // Message 模块
    #[route("/messages/chat")]
    MessageChat {},

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
