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

use crate::models::agent::Agent;
use crate::models::skill::Skill;
use crate::pkg::RequestContext;
use crate::service::dal::agent as agent_dal;
use crate::service::dal::agent::AgentDal;
use crate::service::dal::skill as skill_dal;
use crate::service::dal::skill::SkillDal;
use crate::service::dal::tool as tool_dal;
use crate::service::dal::tool::ToolDal;
use crate::service::dao::agent::AgentQuery;
use crate::service::dao::skill::{SkillQuery, SkillSearch};
use common::enums::{AgentStatus, SkillStatus};
use common::error::Result;
use std::sync::{Arc, OnceLock};

// ==================== 常量 ====================

/// 飞书前台 Agent 的角色标签
pub const FEISHU_RECEPTION_ROLE: &str = "feishu_reception";

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

#[async_trait::async_trait]
impl HrDomain for HrDomainImpl {
    fn agent_manage(&self) -> &dyn AgentManage {
        self
    }
    fn skill_manage(&self) -> &dyn SkillManage {
        self
    }

    /// 解析当前可用的前台 Agent（统一路由方法）
    ///
    /// 路由优先级：
    /// 1. 带 `feishu_reception` 角色的 Onboarded Agent
    /// 2. 任意 Onboarded Agent
    async fn resolve_agent(&self, ctx: RequestContext) -> Result<Option<Agent>> {
        // 优先按 feishu_reception 角色查找
        let query = AgentQuery {
            roles: Some(vec![FEISHU_RECEPTION_ROLE.to_string()]),
            status: Some(AgentStatus::Onboarded),
            pagination: common::api::PaginationParams {
                limit: Some(1),
                offset: None,
            },
            ..Default::default()
        };
        let agents = self.agent_dal.query(ctx.clone(), query).await?;
        if let Some(agent) = agents.items.into_iter().next() {
            return Ok(Some(agent));
        }

        // fallback：任意 Onboarded Agent
        let query = AgentQuery {
            status: Some(AgentStatus::Onboarded),
            pagination: common::api::PaginationParams {
                limit: Some(1),
                offset: None,
            },
            ..Default::default()
        };
        let agents = self.agent_dal.query(ctx, query).await?;
        Ok(agents.items.into_iter().next())
    }
}

// ==================== traits 定义 ====================

/// HR Domain 总 trait
///
/// 聚合人力资源领域所有子功能 trait
#[async_trait::async_trait]
pub trait HrDomain: Send + Sync {
    /// Agent 管理能力
    fn agent_manage(&self) -> &dyn AgentManage;
    /// Skill 管理能力
    fn skill_manage(&self) -> &dyn SkillManage;

    /// 解析当前可用的前台 Agent（统一路由方法）
    ///
    /// **只接受 ctx，不感知 project**：agent 与 project 是两个维度，
    /// 不在 hr domain 中融合，由上层（handler 层）按需组合。
    ///
    /// 路由优先级：
    /// 1. 带 `feishu_reception` 角色的 Onboarded Agent
    /// 2. 任意 Onboarded Agent
    ///
    /// 返回 None 表示无可用前台 Agent。
    async fn resolve_agent(&self, ctx: RequestContext) -> Result<Option<Agent>>;
}

/// Agent 管理 trait
///
/// 定义 Agent 相关的业务接口
#[async_trait::async_trait]
pub trait AgentManage: Send + Sync {
    /// 创建 Agent
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 获取 Agent
    async fn get_agent(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::agent::AgentFetchOptions,
    ) -> Result<Option<Agent>>;

    /// 通用综合查询
    ///
    /// 支持组合查询条件，所有字段都是 Option
    async fn query(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::agent::AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>>;

    /// 统计符合查询条件的 Agent 数量（透传 DAL count）
    async fn count_agents(
        &self,
        ctx: RequestContext,
        query: crate::service::dao::agent::AgentQuery,
    ) -> Result<u64>;

    /// 列出所有 Agent
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>>;

    /// 搜索 Agent（关键词 + 向量语义混合搜索）
    ///
    /// 自动根据参数选择搜索策略：
    /// - keyword 存在 → 走 FTS5 全文检索
    /// - query_vector 存在 → 走向量语义搜索
    /// - 两者都有 → 混合搜索，合并结果（三态匹配 + 综合排序）
    ///
    /// 返回分页结果，支持 runtime_state 内存过滤。
    async fn search_agents(
        &self,
        ctx: RequestContext,
        search: crate::service::dao::agent::AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>>;

    /// 更新 Agent
    async fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 删除 Agent
    async fn delete_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 状态流转
    ///
    /// 校验状态流转合法性，更新状态并持久化
    async fn transition_status(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        target_status: AgentStatus,
    ) -> Result<()>;

    /// 校验入职就绪状态
    ///
    /// 检查工具绑定、技能安装等完整性条件
    async fn validate_onboard_readiness(&self, ctx: RequestContext, agent: &Agent) -> Result<()>;

    /// 安装工具包（按 tag）
    ///
    /// 将指定 tag 的工具包安装到 Agent 的 runtime_config.installed_tags 中。
    /// 幂等：已安装则跳过。
    async fn install_tool_pack(&self, ctx: RequestContext, agent_id: &str, tag: &str)
    -> Result<()>;

    /// 卸载工具包（按 tag）
    ///
    /// 从 Agent 的 runtime_config.installed_tags 中移除指定 tag。
    /// 幂等：未安装则跳过。
    async fn uninstall_tool_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
    ) -> Result<()>;

    /// 列出已安装的工具包 tags
    async fn list_installed_tool_packs(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>>;

    /// 安装技能包（按 tag）
    ///
    /// 查询指定 tag 的已发布技能，批量安装到 Agent 目录。
    /// 幂等：tag 已安装则跳过，返回 0。
    /// 返回成功安装的技能数量。
    async fn install_skill_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
    ) -> Result<usize>;

    /// 卸载技能包（按 tag）
    ///
    /// 从 Agent 的 runtime_config.installed_skill_packs 中移除指定 tag。
    /// 当 delete_copies=true 时，同时删除该 tag 下 Agent 的技能副本。
    /// 幂等：未安装则跳过。
    async fn uninstall_skill_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
        delete_copies: bool,
    ) -> Result<()>;

    /// 重新安装技能包（按 tag）
    ///
    /// 获取最新 Published 技能列表，覆盖已有副本（更新文件 + 元数据），
    /// 没有副本的则新建安装。返回处理的技能数量。
    async fn reinstall_skill_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
    ) -> Result<usize>;

    /// 列出已安装的技能包 tags
    async fn list_installed_skill_packs(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>>;
}

/// Skill 文件导入统一结构 = (去哪放) × (从哪来)。
///
/// 所有外部来源（手填文本 / URL 下载 / 附件上传）由 Handler 适配翻译为此结构，
/// Domain 层 `process_skill_package` 纯函数统一处理。
///
/// 来源优先级：source_abs_path 路径优先 > content_bytes 内存内容（路径优先 = 0 拷贝优先）
#[derive(Debug, Clone, Default)]
pub struct SkillFileImport {
    /// 最终在技能目录里的相对路径；None 时走 5 级降级推断链
    pub target_path: Option<String>,
    /// 来源 A：磁盘绝对路径（优先级高，可 0 拷贝 rename）
    pub source_abs_path: Option<std::path::PathBuf>,
    /// 来源 B：内存内容 bytes（次优先级；无路径时用，省临时文件）
    pub content_bytes: Option<Vec<u8>>,
    /// 弱线索名（附件 original_name；target=None 时推断用）
    pub suggested_name: Option<String>,
}

/// 技能创建复合参数
///
/// 封装技能元数据 + 内容源处理所需的文件导入信息。
/// 老调用点可通过 `CreateSkillParams::from_skill(&skill)` 1 行兼容构造。
#[derive(Debug, Clone)]
pub struct CreateSkillParams<'a> {
    /// 技能实体（包含元数据）
    pub skill: &'a Skill,
    /// 统一文件导入列表（手填文本 / 附件 / URL 下载 tmp 全走这里）
    pub imports: Vec<SkillFileImport>,
    /// 远程内容源 URL（HTTPS，由 Handler 传入；Domain 内部 download 到 tmp 后加入 imports）
    pub remote_source: Option<&'a str>,
}

impl<'a> CreateSkillParams<'a> {
    /// 从 Skill 实体兼容构造（老调用点：从 skill.files 提取内容组装 imports）。
    pub fn from_skill(skill: &'a Skill) -> Self {
        let imports = skill
            .files
            .iter()
            .filter_map(|f| {
                f.content.as_ref().map(|c| SkillFileImport {
                    target_path: Some(f.filename.clone()),
                    source_abs_path: None,
                    content_bytes: Some(c.as_bytes().to_vec()),
                    suggested_name: None,
                })
            })
            .collect();
        Self {
            skill,
            imports,
            remote_source: None,
        }
    }
}

/// 技能更新复合参数
#[derive(Debug, Clone)]
pub struct UpdateSkillParams<'a> {
    /// 技能实体（包含要更新的元数据）
    pub skill: &'a Skill,
    /// 统一文件导入列表（手填文本 / 附件 / URL 下载 tmp 全走这里）
    pub imports: Vec<SkillFileImport>,
    /// 文件删除操作列表（文件名）
    pub file_deletes: Vec<&'a str>,
    /// 远程内容源 URL（HTTPS）
    pub remote_source: Option<&'a str>,
}

/// Skill 管理 trait
///
/// 定义技能管理相关的业务接口
#[async_trait::async_trait]
pub trait SkillManage: Send + Sync {
    // A. 技能基础管理（CRUD）
    async fn create_skill(&self, ctx: RequestContext, params: CreateSkillParams<'_>) -> Result<()>;
    async fn get_skill(&self, ctx: RequestContext, id: &str) -> Result<Option<Skill>>;
    async fn update_skill(&self, ctx: RequestContext, params: UpdateSkillParams<'_>) -> Result<()>;
    async fn delete_skill(&self, ctx: RequestContext, id: &str) -> Result<()>;

    // B. 技能查询与搜索
    async fn query_skills(
        &self,
        ctx: RequestContext,
        query: SkillQuery,
    ) -> Result<common::api::PagedResult<Skill>>;
    async fn list_by_status(&self, ctx: RequestContext, status: SkillStatus) -> Result<Vec<Skill>>;
    async fn list_by_category(&self, ctx: RequestContext, category: &str) -> Result<Vec<Skill>>;
    async fn list_by_author(&self, ctx: RequestContext, author_id: &str) -> Result<Vec<Skill>>;
    async fn list_for_agent(&self, ctx: RequestContext, agent_id: &str) -> Result<Vec<Skill>>;
    async fn search_skills(
        &self,
        ctx: RequestContext,
        search: SkillSearch,
    ) -> Result<common::api::PagedResult<Skill>>;

    /// 列出所有已发布技能的 distinct tags（用于前端技能包安装下拉框数据源）
    async fn list_skill_tags(&self, ctx: RequestContext) -> Result<Vec<String>>;

    // C. Agent 技能安装
    async fn install_to_agent(
        &self,
        ctx: RequestContext,
        source_skill_id: &str,
        agent_id: &str,
    ) -> Result<Skill>;

    /// 从 Agent 目录卸载技能副本（删除 DB 记录 + 文件目录）
    ///
    /// 仅限通过 install_to_agent 安装的副本（parent_skill_id 不为空）。
    /// 校验技能归属于指定 Agent，且是安装副本而非原始技能。
    async fn uninstall_from_agent(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        agent_id: &str,
    ) -> Result<()>;

    // D. Skill 文件独立操作
    /// 列出 Skill 所有文件，返回文件列表摘要
    async fn list_skill_files(
        &self,
        ctx: RequestContext,
        skill_id: &str,
    ) -> Result<Option<Vec<crate::models::skill::SkillFile>>>;

    /// 读取 Skill 指定文件内容，返回 UTF-8 文本
    async fn get_skill_file_content(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        filename: &str,
    ) -> Result<Option<String>>;

    /// 创建或更新 Skill 指定文件内容
    /// 如果 skill 不存在返回 NotFound，如果乐观锁不匹配返回 Conflict
    async fn update_skill_file_content(
        &self,
        ctx: RequestContext,
        skill_id: &str,
        filename: &str,
        content: &str,
        expected_updated_at: Option<i64>,
    ) -> Result<()>;
}
