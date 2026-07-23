//! Agent 管理具体方法实现

use crate::models::agent::Agent;
use crate::models::skill::Skill;
use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::domain::hr::{AgentManage, HrDomainImpl};
use common::enums::AgentStatus;
use common::error::{Result, err, bail_err};

use crate::enrich_ctx;

impl HrDomainImpl {
    /// 用源技能的最新内容覆盖 Agent 已有的技能副本
    ///
    /// 覆盖策略：
    /// - 将源技能的所有文件写入副本目录（覆盖同名文件）
    /// - 更新副本的 SkillPo 元数据（name/description/tags）
    /// - 持久化更新
    async fn overwrite_skill_copy(
        &self,
        ctx: RequestContext,
        source: &Skill,
        copy: &Skill,
    ) -> Result<()> {
        // 1. 将源技能的所有文件写入副本目录
        for file in &source.files {
            if let Some(content) = &file.content {
                self.skill_dal
                    .write_file(&copy.po, &file.filename, content)?;
            } else {
                // 大文件按需读取后写入
                let bytes = self.skill_dal.read_file(&source.po, &file.filename)?;
                self.skill_dal
                    .write_file(&copy.po, &file.filename, &bytes)?;
            }
        }

        // 2. 更新副本的 SkillPo 元数据
        let mut updated_po = copy.po.clone();
        updated_po.name = source.po.name.clone();
        updated_po.description = source.po.description.clone();
        updated_po.tags = source.po.tags.clone();

        // 3. 持久化更新
        self.skill_dal
            .update(
                ctx,
                &Skill {
                    po: updated_po,
                    files: Vec::new(),
                    search_match: None,
                },
            )
            .await
    }
}

#[async_trait::async_trait]
impl AgentManage for HrDomainImpl {
    /// 创建 Agent
    ///
    /// 基础操作：将 Agent 持久化到存储
    /// 强制校验：
    /// - Local Agent 必须指定 model_provider_id（外部 Agent 不需要）
    /// - 创建后状态固定为 Interviewing
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        // 强制校验：Local agent 必须指定 model_provider_id
        // 外部 agent（Cli/Remote）使用外部运行时，不需要本地 model provider
        if agent.po.kind.is_local() && agent.po.model_provider_id.is_empty() {
            bail_err!(InvalidRequest, "创建 Local Agent 必须指定 model_provider_id");
        }

        // 强制校验：状态必须是 Interviewing
        if agent.po.status != AgentStatus::Interviewing {
            bail_err!(InvalidRequest, "新建 Agent 状态必须为 Interviewing");
        }

        self.agent_dal.create(ctx, agent).await
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    /// 当 with_tools=true 时，额外加载 Agent 可用的工具（绑定工具 + tag 匹配工具），
    /// 写入 Agent 实体供后续 wake/awaken 使用。
    async fn get_agent(&self, ctx: RequestContext, id: &str, options: crate::service::dal::agent::AgentFetchOptions) -> Result<Option<Agent>> {
        let with_tools = options.with_tools.unwrap_or(false);
        let mut agent = self.agent_dal.get_agent(ctx.clone(), id, options).await?;

        if let Some(ref mut agent) = agent {
            if with_tools {
                // 绑定工具（通过 agent_tools 关联表）
                let bound_tools = self
                    .tool_dal
                    .list_tools_for_agent_full(ctx.clone(), id)
                    .await?;
                // tag 匹配工具（neural + installed_tags）
                let mut tag_filter = vec!["neural".to_string()];
                tag_filter.extend(agent.po.get_installed_tags());
                let tag_tools = self
                    .tool_dal
                    .query(
                        ctx.clone(),
                        crate::service::dao::tool::ToolQuery {
                            tags: Some(tag_filter),
                            enabled_only: Some(true),
                            status: Some(common::enums::ToolStatus::Enabled),
                            ..Default::default()
                        },
                    )
                    .await?;
                // 合并去重（绑定工具和 tag 工具可能有交集）
                let mut seen_ids = std::collections::HashSet::new();
                let all_tools: Vec<Tool> = bound_tools
                    .into_iter()
                    .chain(tag_tools)
                    .filter(|t| seen_ids.insert(t.po.id.clone()))
                    .collect();
                agent.set_tools(all_tools);
            }
        }

        Ok(agent)
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(&self, ctx: RequestContext, query: AgentQuery) -> Result<Vec<Agent>> {
        self.agent_dal.query(ctx, query).await
    }

    /// 列出所有 Agent
    ///
    /// 语法糖：调用通用查询，默认排除已删除状态
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        self.query(
            ctx,
            AgentQuery {
                exclude_status: Some(AgentStatus::Deleted),
                ..Default::default()
            },
        )
        .await
    }

    async fn search_agents(
        &self,
        ctx: RequestContext,
        search: crate::service::dao::agent::AgentSearch,
    ) -> Result<Vec<Agent>> {
        self.agent_dal.search(ctx, search).await
    }

    /// 更新 Agent
    ///
    /// 基础操作：更新 Agent 信息
    async fn update_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        self.agent_dal.update(ctx, agent).await
    }

    /// 删除 Agent
    ///
    /// 基础操作：软删除 Agent（标记为已删除）
    async fn delete_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let ctx = enrich_ctx!(&ctx, agent);
        self.agent_dal.delete(ctx, agent).await
    }

    /// 状态流转
    ///
    /// 校验状态流转合法性，更新状态并持久化
    async fn transition_status(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        target_status: AgentStatus,
    ) -> Result<()> {
        // 补充 Agent 上下文
        let ctx = enrich_ctx!(&ctx, &*agent);

        let current_status = agent.po.status.clone();

        // 状态机校验：定义合法的流转路径
        let is_valid_transition = match (&current_status, &target_status) {
            // 面试中 → 待入职
            (AgentStatus::Interviewing, AgentStatus::PendingOnboard) => true,
            // 待入职 → 已入职
            (AgentStatus::PendingOnboard, AgentStatus::Onboarded) => true,
            // 已入职 → 待离职
            (AgentStatus::Onboarded, AgentStatus::PendingOffboard) => true,
            // 待离职 → 已离职
            (AgentStatus::PendingOffboard, AgentStatus::Offboarded) => true,
            // 任意状态 → 已删除
            (_, AgentStatus::Deleted) => true,
            // 同状态跳转：允许幂等
            (a, b) if a == b => true,
            // 其他情况：非法
            _ => false,
        };

        if !is_valid_transition {
            bail_err!(InvalidRequest, "非法状态流转：{:?} → {:?}", current_status, target_status);
        }

        // 幂等：状态相同直接返回
        if current_status == target_status {
            return Ok(());
        }

        // 更新状态
        agent.po.status = target_status;

        // 入职时自动安装 project_management 工具包
        // Agent 入职后天生具备项目管理能力，无需逐个绑定工具
        if target_status == AgentStatus::Onboarded {
            agent.po.install_tag("project_management");
        }

        // 持久化
        self.agent_dal.update(ctx, agent).await
    }

    /// 校验入职就绪状态
    ///
    /// 检查工具绑定、技能安装等完整性条件
    async fn validate_onboard_readiness(
        &self,
        ctx: RequestContext,
        agent: &Agent,
    ) -> Result<()> {
        let agent_id = agent.po.id.as_str();

        // 1. 校验状态必须是 PendingOnboard
        if agent.po.status != AgentStatus::PendingOnboard {
            bail_err!(InvalidRequest, "Agent 状态必须是 PendingOnboard 才能入职，当前状态：{:?}", agent.po.status);
        }

        // 补充 Agent 上下文到 ctx，后续调用链可复用
        let ctx = enrich_ctx!(&ctx, agent);

        // 2. 校验至少绑定了 1 个工具
        let tools = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), agent_id)
            .await?;
        if tools.is_empty() {
            bail_err!(InvalidRequest, "Agent 至少绑定 1 个工具才能入职");
        }

        // 3. 校验技能：没有技能只告警，不阻止入职
        let skills = self.skill_dal.list_for_agent(ctx.clone(), agent_id).await?;
        if skills.is_empty() {
            log_warn!(
                ctx.clone(),
                "onboard_agent",
                "Agent {} 未安装任何技能",
                agent_id
            );
        }

        Ok(())
    }

    /// 安装工具包（按 tag）
    ///
    /// 将指定 tag 的工具包安装到 Agent 的 runtime_config.installed_tags 中。
    /// 幂等：已安装则跳过。
    async fn install_tool_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
    ) -> Result<()> {
        let mut agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        let ctx = enrich_ctx!(&ctx, &agent);

        // 幂等：已安装则跳过
        if agent.po.get_runtime_config().has_tag(tag) {
            log_info!(ctx, "install_tool_pack", "agent_id={}, tag={} 已安装，跳过", agent_id, tag);
            return Ok(());
        }

        agent.po.install_tag(tag);
        self.agent_dal.update(ctx.clone(), &agent).await?;

        log_info!(ctx, "install_tool_pack", "agent_id={}, tag={} 安装成功", agent_id, tag);
        Ok(())
    }

    /// 卸载工具包（按 tag）
    ///
    /// 从 Agent 的 runtime_config.installed_tags 中移除指定 tag。
    /// 幂等：未安装则跳过。
    async fn uninstall_tool_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
    ) -> Result<()> {
        let mut agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        let ctx = enrich_ctx!(&ctx, &agent);

        // 幂等：未安装则跳过
        if !agent.po.get_runtime_config().has_tag(tag) {
            log_info!(ctx, "uninstall_tool_pack", "agent_id={}, tag={} 未安装，跳过", agent_id, tag);
            return Ok(());
        }

        agent.po.uninstall_tag(tag);
        self.agent_dal.update(ctx.clone(), &agent).await?;

        log_info!(ctx, "uninstall_tool_pack", "agent_id={}, tag={} 卸载成功", agent_id, tag);
        Ok(())
    }

    /// 列出已安装的工具包 tags
    async fn list_installed_tool_packs(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>> {
        let agent = self
            .agent_dal
            .find_by_id(ctx, agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        Ok(agent.po.get_installed_tags())
    }

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
    ) -> Result<usize> {
        let mut agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        let ctx = enrich_ctx!(&ctx, &agent);

        // 幂等：tag 已安装则跳过
        if agent.po.get_runtime_config().has_skill_pack_tag(tag) {
            log_info!(
                ctx,
                "install_skill_pack",
                "agent_id={}, tag={} 已安装，跳过",
                agent_id,
                tag
            );
            return Ok(0);
        }

        // 查询该 tag 的已发布技能
        let skills = self.skill_dal.list_published_by_tag(ctx.clone(), tag).await?;

        if skills.is_empty() {
            log_warn!(
                ctx,
                "install_skill_pack",
                "agent_id={}, tag={} 没有已发布技能",
                agent_id,
                tag
            );
        }

        let mut success_count = 0usize;
        let mut fail_count = 0usize;
        for skill in &skills {
            match self
                .skill_dal
                .install_to_agent(ctx.clone(), &skill.po.id, agent_id)
                .await
            {
                Ok(_) => success_count += 1,
                Err(e) => {
                    fail_count += 1;
                    log_warn!(
                        ctx.clone(),
                        "install_skill_pack",
                        "安装技能失败: skill_id={}, agent_id={}, tag={}, err={}",
                        skill.po.id,
                        agent_id,
                        tag,
                        e
                    );
                }
            }
        }

        // 记录 tag 到 Agent 的 installed_skill_packs
        agent.po.install_skill_pack_tag(tag);
        self.agent_dal.update(ctx.clone(), &agent).await?;

        log_info!(
            ctx,
            "install_skill_pack",
            "agent_id={}, tag={} 安装完成: 成功={}, 失败={}",
            agent_id,
            tag,
            success_count,
            fail_count
        );
        Ok(success_count)
    }

    /// 卸载技能包（按 tag）
    ///
    /// 从 Agent 的 runtime_config.installed_skill_packs 中移除指定 tag。
    /// 不删除已安装的技能副本。
    /// 幂等：未安装则跳过。
    async fn uninstall_skill_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
    ) -> Result<()> {
        let mut agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        let ctx = enrich_ctx!(&ctx, &agent);

        // 幂等：未安装则跳过
        if !agent.po.get_runtime_config().has_skill_pack_tag(tag) {
            log_info!(
                ctx,
                "uninstall_skill_pack",
                "agent_id={}, tag={} 未安装，跳过",
                agent_id,
                tag
            );
            return Ok(());
        }

        agent.po.uninstall_skill_pack_tag(tag);
        self.agent_dal.update(ctx.clone(), &agent).await?;

        log_info!(
            ctx,
            "uninstall_skill_pack",
            "agent_id={}, tag={} 卸载成功（技能副本保留）",
            agent_id,
            tag
        );
        Ok(())
    }

    /// 重新安装技能包（按 tag）
    ///
    /// 获取最新 Published 技能列表，覆盖已有副本（更新文件 + 元数据），
    /// 没有副本的则新建安装。返回处理的技能数量。
    async fn reinstall_skill_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
    ) -> Result<usize> {
        let agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        let ctx = enrich_ctx!(&ctx, &agent);

        // 查询该 tag 的最新已发布技能
        let source_skills = self.skill_dal.list_published_by_tag(ctx.clone(), tag).await?;

        if source_skills.is_empty() {
            log_warn!(
                ctx,
                "reinstall_skill_pack",
                "agent_id={}, tag={} 没有已发布技能",
                agent_id,
                tag
            );
            return Ok(0);
        }

        // 收集 parent_skill_ids，查询 Agent 已有副本
        let parent_ids: Vec<String> =
            source_skills.iter().map(|s| s.po.id.clone()).collect();
        let existing_copies = self
            .skill_dal
            .find_agent_skill_copies(ctx.clone(), agent_id, &parent_ids)
            .await?;

        // 构建 parent_skill_id → 已有副本 的映射
        let copy_map: std::collections::HashMap<String, crate::models::skill::Skill> =
            existing_copies
                .into_iter()
                .map(|s| (s.po.parent_skill_id.clone(), s))
                .collect();

        let mut processed_count = 0usize;
        for source in &source_skills {
            if let Some(copy) = copy_map.get(&source.po.id) {
                // 已有副本：用源技能内容覆盖副本
                self.overwrite_skill_copy(ctx.clone(), source, copy)
                    .await?;
            } else {
                // 无副本：创建新安装
                self.skill_dal
                    .install_to_agent(ctx.clone(), &source.po.id, agent_id)
                    .await?;
            }
            processed_count += 1;
        }

        log_info!(
            ctx,
            "reinstall_skill_pack",
            "agent_id={}, tag={} 重装完成: 处理={}",
            agent_id,
            tag,
            processed_count
        );
        Ok(processed_count)
    }

    /// 列出已安装的技能包 tags
    async fn list_installed_skill_packs(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<String>> {
        let agent = self
            .agent_dal
            .find_by_id(ctx, agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        Ok(agent.po.get_installed_skill_packs())
    }
}