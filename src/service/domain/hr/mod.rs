//! HR (Human Resources) Domain 模块
//!
//! 人力资源模块，管理：
//! - Agent - AI 智能体
//! - Employee - 人类员工
//! - Skill - 技能管理

pub mod agent;
pub mod skill;

#[cfg(test)]
mod agent_test;
#[cfg(test)]
mod skill_test;

use crate::error::AppError;
use crate::models::agent::Agent;
use crate::models::skill::Skill;
use crate::pkg::RequestContext;
use crate::service::dal::agent as agent_dal;
use crate::service::dal::agent::AgentDal;
use crate::service::dal::skill as skill_dal;
use crate::service::dal::skill::SkillDal;
use crate::service::dal::tool as tool_dal;
use crate::service::dal::tool::ToolDal;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use common::enums::{AgentStatus, SkillStatus};
use std::sync::{Arc, OnceLock};

// ==================== 单例 ====================

static HR_DOMAIN: OnceLock<Arc<dyn HrDomain>> = OnceLock::new();

/// 获取 HR Domain 单例
pub fn domain() -> Arc<dyn HrDomain> {
    HR_DOMAIN.get().cloned().unwrap()
}

/// 初始化 HR Domain
pub fn init() {
    let _ = HR_DOMAIN.set(new(agent_dal::dal(), tool_dal::dal(), skill_dal::dal()));
}

/// 创建 HR Domain 实例（测试可注入隔离依赖）。
pub fn new(
    agent_dal: Arc<dyn AgentDal>,
    tool_dal: Arc<dyn ToolDal>,
    skill_dal: Arc<dyn SkillDal>,
) -> Arc<dyn HrDomain> {
    Arc::new(HrDomainImpl::new(agent_dal, tool_dal, skill_dal))
}

// ==================== 实现 ====================

/// HR Domain 实现
///
/// 聚合所有人力资源子功能实现
struct HrDomainImpl {
    agent_dal: Arc<dyn AgentDal>,
    tool_dal: Arc<dyn ToolDal>,
    skill_dal: Arc<dyn SkillDal>,
}

impl HrDomainImpl {
    /// 创建 Domain 实例
    fn new(
        agent_dal: Arc<dyn AgentDal>,
        tool_dal: Arc<dyn ToolDal>,
        skill_dal: Arc<dyn SkillDal>,
    ) -> Self {
        Self {
            agent_dal,
            tool_dal,
            skill_dal,
        }
    }
}

impl HrDomain for HrDomainImpl {
    fn agent_manage(&self) -> &dyn AgentManage {
        self
    }
    fn skill_manage(&self) -> &dyn SkillManage {
        self
    }
}

// ==================== traits 定义 ====================

/// HR Domain 总 trait
///
/// 聚合人力资源领域所有子功能 trait
pub trait HrDomain: Send + Sync {
    /// Agent 管理能力
    fn agent_manage(&self) -> &dyn AgentManage;
    /// Skill 管理能力
    fn skill_manage(&self) -> &dyn SkillManage;
}

/// Agent 管理 trait
///
/// 定义 Agent 相关的业务接口
#[async_trait::async_trait]
pub trait AgentManage: Send + Sync {
    /// 创建 Agent
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 获取 Agent
    async fn get_agent(&self, ctx: RequestContext, id: &str) -> Result<Option<Agent>, AppError>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::agent::AgentQuery,
    ) -> Result<Vec<Agent>, AppError>;

    /// 列出所有 Agent
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>, AppError>;

    /// 更新 Agent
    async fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 删除 Agent
    async fn delete_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<(), AppError>;

    /// 状态流转
    ///
    /// 校验状态流转合法性，更新状态并持久化
    async fn transition_status(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        target_status: AgentStatus,
    ) -> Result<(), AppError>;

    /// 校验入职就绪状态
    ///
    /// 检查工具绑定、技能安装等完整性条件
    async fn validate_onboard_readiness(
        &self,
        ctx: RequestContext,
        agent: &Agent,
    ) -> Result<(), AppError>;
}

/// Skill 附加文件导入数据。
///
/// Handler 负责将 Finance Attachment 转换为该领域输入，避免 HR Domain 泄漏 attachment_id 等 Finance 概念。
#[derive(Debug, Clone)]
pub struct SkillFileImport {
    /// 导入到 Skill 内容目录内的相对目标路径。
    pub target_path: String,
    /// 文件 bytes。
    pub bytes: Vec<u8>,
}

/// 技能更新复合参数
#[derive(Debug, Clone)]
pub struct UpdateSkillParams<'a> {
    /// 技能实体（包含要更新的元数据）
    pub skill: &'a Skill,
    /// 文件写入操作列表（文件名 -> 内容）
    pub file_writes: Vec<(&'a str, &'a str)>,
    /// 文件删除操作列表（文件名）
    pub file_deletes: Vec<&'a str>,
    /// 附加文件导入列表（目标路径 -> bytes）
    pub file_imports: Vec<SkillFileImport>,
}

/// Skill 管理 trait
///
/// 定义技能管理相关的业务接口
#[async_trait::async_trait]
pub trait SkillManage: Send + Sync {
    // A. 技能基础管理（CRUD）
    async fn create_skill(&self, ctx: RequestContext, skill: &Skill) -> Result<(), AppError>;
    async fn get_skill(&self, ctx: RequestContext, id: &str) -> Result<Option<Skill>, AppError>;
    async fn update_skill(
        &self,
        ctx: RequestContext,
        params: UpdateSkillParams<'_>,
    ) -> Result<(), AppError>;
    async fn delete_skill(&self, ctx: RequestContext, id: &str) -> Result<(), AppError>;

    // B. 技能查询与搜索
    async fn query_skills(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<Vec<Skill>, AppError>;
    async fn list_by_status(
        &self,
        ctx: RequestContext,
        status: SkillStatus,
    ) -> Result<Vec<Skill>, AppError>;
    async fn list_by_category(
        &self,
        ctx: RequestContext,
        category: &str,
    ) -> Result<Vec<Skill>, AppError>;
    async fn list_by_author(
        &self,
        ctx: RequestContext,
        author_id: &str,
    ) -> Result<Vec<Skill>, AppError>;
    async fn list_for_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>, AppError>;
    async fn search_skills(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<Vec<Skill>, AppError>;

    // C. Agent 技能安装
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill, AppError>;
}
