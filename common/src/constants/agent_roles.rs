//! Agent 预设角色标签常量
//!
//! roles 字段是开放的 Vec<String> 数组：既可以是本模块定义的系统预设常量，
//! 也支持用户完全自定义的任意字符串。本模块的常量仅用于：
//! - 前端角色输入框的预设 chip 列表
//! - resolve_agent 打分匹配时的默认传参（web 前台、飞书前台等场景）
//!
//! 匹配逻辑由 service/domain/hr/mod.rs `resolve_agent` 负责，
//! 预设常量只是便于前后端共享同一份字符串，避免 "feishu_reception" 这类魔法字符串散落在各处。

/// Web 对话框前台 Agent（默认对话入口）
pub const ROLE_RECEPTION: &str = "reception";

/// 飞书入站前台 Agent（通过飞书 WebSocket/消息渠道进入的消息默认路由到此角色）
pub const ROLE_FEISHU_RECEPTION: &str = "feishu_reception";

/// A2A 网关 Agent（处理外部 Agent 发来的任务或订阅请求）
pub const ROLE_A2A_GATEWAY: &str = "a2a_gateway";

/// 项目经理 Agent（可作为 Project 的 owner_agent）
pub const ROLE_PROJECT_OWNER: &str = "project_owner";

/// 执行员工 Agent（普通 worker，被 PM 指派任务）
pub const ROLE_WORKER: &str = "worker";

/// 代码开发 Agent（擅长写代码）
pub const ROLE_CODER: &str = "coder";

/// 数据分析 Agent（擅长处理数据）
pub const ROLE_DATA_ANALYST: &str = "data_analyst";

/// 客服 / 接待通用角色（作为通用 reception fallback 可命中）
pub const ROLE_SERVICE: &str = "service";

/// 返回所有系统预设角色的列表（用于前端展示预设 chip 下拉）
pub const SYSTEM_PRESET_ROLES: &[&str] = &[
    ROLE_RECEPTION,
    ROLE_FEISHU_RECEPTION,
    ROLE_A2A_GATEWAY,
    ROLE_PROJECT_OWNER,
    ROLE_WORKER,
    ROLE_CODER,
    ROLE_DATA_ANALYST,
    ROLE_SERVICE,
];

/// 预设角色 → 展示名称（中文标签，用于前端 chip 显示）
pub fn preset_role_display(role: &str) -> Option<&'static str> {
    match role {
        ROLE_RECEPTION => Some("Web 前台"),
        ROLE_FEISHU_RECEPTION => Some("飞书前台"),
        ROLE_A2A_GATEWAY => Some("A2A 网关"),
        ROLE_PROJECT_OWNER => Some("项目经理"),
        ROLE_WORKER => Some("执行员工"),
        ROLE_CODER => Some("代码开发"),
        ROLE_DATA_ANALYST => Some("数据分析"),
        ROLE_SERVICE => Some("客服接待"),
        _ => None,
    }
}

/// 角色是否属于系统预设（决定前端 chip 是否高亮成 badge-primary）
pub fn is_preset_role(role: &str) -> bool {
    preset_role_display(role).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_preset_roles_have_display() {
        for r in SYSTEM_PRESET_ROLES {
            assert!(
                preset_role_display(r).is_some(),
                "预设角色 {r} 缺少 display 映射"
            );
            assert!(is_preset_role(r));
        }
    }

    #[test]
    fn custom_role_is_not_preset() {
        assert!(!is_preset_role("my_custom_role"));
        assert!(preset_role_display("my_custom_role").is_none());
    }
}
