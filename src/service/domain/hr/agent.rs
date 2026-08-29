//! Agent 管理具体方法实现

use crate::models::agent::Agent;
use crate::models::skill::Skill;
use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::domain::hr::{AgentManage, HrDomainImpl};
use common::api::{SkillListItem, ToolListItem};
use common::enums::AgentStatus;
use common::error::{Result, bail_err, err};

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
    /// - 允许 Local Agent 暂不指定 model_provider_id：缺模型时用 Interviewing 状态表达
    ///   "尚未就绪"，用户可在模型管理中补配并入职后使用（不是错误，是生命周期状态）
    /// - 强制校验：创建后状态固定为 Interviewing（统一从面试开始）
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        // 强制校验：状态必须是 Interviewing
        if agent.po.status != AgentStatus::Interviewing {
            bail_err!(InvalidRequest, "新建 Agent 状态必须为 Interviewing");
        }

        self.agent_dal.create(ctx, agent).await
    }

    /// 获取 Agent
    ///
    /// 基础操作：根据 ID 查询 Agent
    /// - with_tools=true：加载绑定工具 + tag 匹配工具
    /// - with_skills=true：加载 Agent 已安装的技能副本（author_id = agent_id，排除 Expired）
    /// 写入 Agent 实体供后续 wake/awaken 使用。
    async fn get_agent(
        &self,
        ctx: RequestContext,
        id: &str,
        options: crate::service::dal::agent::AgentFetchOptions,
    ) -> Result<Option<Agent>> {
        let with_tools = options.with_tools.unwrap_or(false);
        let with_skills = options.with_skills.unwrap_or(false);
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
                // 过滤 internal 标签工具：内部系统工具不可暴露给 Agent
                // （如 request_tool_call / send_tool_call_message 仅由 ToolDal 内部转发）
                let mut seen_ids = std::collections::HashSet::new();
                let all_tools: Vec<Tool> = bound_tools
                    .into_iter()
                    .chain(tag_tools.items)
                    .filter(|t| seen_ids.insert(t.po.id.clone()))
                    .filter(|t| !t.po.get_tags().iter().any(|tag| tag == "internal"))
                    .collect();
                agent.set_tools(all_tools);
            }
            if with_skills {
                // 技能只在 Agent 已安装的副本范围内查询（author_id = agent_id）
                // 技能讲究"安装且自进化"，即便神经技能也需安装到自身目录才能使用
                let skills = self
                    .skill_dal
                    .query(
                        ctx.clone(),
                        crate::service::dao::skill::SkillQuery {
                            author_id: Some(id.to_string()),
                            exclude_status: Some(common::enums::SkillStatus::Expired),
                            ..Default::default()
                        },
                    )
                    .await?;
                agent.set_skills(skills.items);
            }
        }

        Ok(agent)
    }

    /// 通用综合查询
    ///
    /// Domain 层可以添加业务逻辑：权限校验、数据过滤、业务规则验证
    async fn query(
        &self,
        ctx: RequestContext,
        query: AgentQuery,
    ) -> Result<common::api::PagedResult<Agent>> {
        self.agent_dal.query(ctx, query).await
    }

    /// 统计符合查询条件的 Agent 数量（透传 DAL count）
    async fn count_agents(&self, ctx: RequestContext, query: AgentQuery) -> Result<u64> {
        self.agent_dal.count(ctx, query).await
    }

    /// 列出所有 Agent
    ///
    /// 语法糖：调用通用查询，默认排除已删除状态
    async fn list_agents(&self, ctx: RequestContext) -> Result<Vec<Agent>> {
        Ok(self
            .query(
                ctx,
                AgentQuery {
                    exclude_status: Some(AgentStatus::Deleted),
                    ..Default::default()
                },
            )
            .await?
            .items)
    }

    async fn search_agents(
        &self,
        ctx: RequestContext,
        search: crate::service::dao::agent::AgentSearch,
    ) -> Result<common::api::PagedResult<Agent>> {
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

        let current_status = agent.po.status;

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
            bail_err!(
                InvalidRequest,
                "非法状态流转：{:?} → {:?}",
                current_status,
                target_status
            );
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
    async fn validate_onboard_readiness(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        let agent_id = agent.po.id.as_str();

        // 1. 校验状态必须是 PendingOnboard
        if agent.po.status != AgentStatus::PendingOnboard {
            bail_err!(
                InvalidRequest,
                "Agent 状态必须是 PendingOnboard 才能入职，当前状态：{:?}",
                agent.po.status
            );
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

        // 神经工具天生拥有，无需作为工具包安装；显式拒绝避免冗余和展示歧义
        if tag == "neural" {
            bail_err!(
                InvalidRequest,
                "neural 是系统保留标签，所有 Agent 天生拥有神经工具，无需通过工具包安装"
            );
        }

        // 幂等：已安装则跳过
        if agent.po.get_runtime_config().has_tag(tag) {
            log_info!(
                ctx,
                "install_tool_pack",
                "agent_id={}, tag={} 已安装，跳过",
                agent_id,
                tag
            );
            return Ok(());
        }

        agent.po.install_tag(tag);
        self.agent_dal.update(ctx.clone(), &agent).await?;

        log_info!(
            ctx,
            "install_tool_pack",
            "agent_id={}, tag={} 安装成功",
            agent_id,
            tag
        );
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
            log_info!(
                ctx,
                "uninstall_tool_pack",
                "agent_id={}, tag={} 未安装，跳过",
                agent_id,
                tag
            );
            return Ok(());
        }

        agent.po.uninstall_tag(tag);
        self.agent_dal.update(ctx.clone(), &agent).await?;

        log_info!(
            ctx,
            "uninstall_tool_pack",
            "agent_id={}, tag={} 卸载成功",
            agent_id,
            tag
        );
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
        let skills = self
            .skill_dal
            .list_published_by_tag(ctx.clone(), tag)
            .await?;

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
    /// 当 delete_copies=true 时，同时删除该 tag 下 Agent 的技能副本。
    /// 幂等：未安装则跳过。
    async fn uninstall_skill_pack(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        tag: &str,
        delete_copies: bool,
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

        // 可选：删除该 tag 下 Agent 的技能副本
        if delete_copies {
            let copies = self
                .skill_dal
                .query(
                    ctx.clone(),
                    crate::service::dao::skill::SkillQuery {
                        author_id: Some(agent_id.to_string()),
                        has_parent: Some(true), // 只查副本
                        tags: Some(vec![tag.to_string()]),
                        ..Default::default()
                    },
                )
                .await?;
            for skill in copies.items {
                let _ = self.skill_dal.delete(ctx.clone(), &skill.po.id).await;
            }
        }

        log_info!(
            ctx,
            "uninstall_skill_pack",
            "agent_id={}, tag={} 卸载成功（delete_copies={}）",
            agent_id,
            tag,
            delete_copies
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
        let source_skills = self
            .skill_dal
            .list_published_by_tag(ctx.clone(), tag)
            .await?;

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
        let parent_ids: Vec<String> = source_skills.iter().map(|s| s.po.id.clone()).collect();
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
                self.overwrite_skill_copy(ctx.clone(), source, copy).await?;
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

    async fn get_agent_association_view(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        with_tools: bool,
        with_skills: bool,
    ) -> Result<(
        Option<common::api::AgentToolsOverview>,
        Option<common::api::AgentSkillsOverview>,
    )> {
        use common::api::{AgentSkillsOverview, AgentToolPackGroup, AgentToolsOverview};

        const NEURAL_TAG: &str = "neural";
        const INTERNAL_TAG: &str = "internal";

        let runtime_cfg = agent.po.get_runtime_config();
        let installed_tool_tags: Vec<String> = runtime_cfg
            .installed_tags
            .iter()
            .filter(|t| t.as_str() != NEURAL_TAG)
            .cloned()
            .collect();
        let installed_skill_packs: Vec<String> = runtime_cfg
            .installed_skill_packs
            .iter()
            .filter(|t| t.as_str() != NEURAL_TAG)
            .cloned()
            .collect();

        // 两侧开关都关闭时无需任何查询，直接短路
        if !with_tools && !with_skills {
            return Ok((None, None));
        }

        // ======================== 工具分组 ========================
        let tools_overview: Option<AgentToolsOverview> = if with_tools {
            // 1. 神经工具：tags 含 neural 且 不含 internal 的全部启用工具
            let neural_candidates = self
                .tool_dal
                .query(
                    ctx.clone(),
                    crate::service::dao::tool::ToolQuery {
                        tags: Some(vec![NEURAL_TAG.to_string()]),
                        enabled_only: Some(true),
                        ..Default::default()
                    },
                )
                .await?;
            let neural_tools_map: std::collections::BTreeMap<String, Tool> = neural_candidates
                .items
                .into_iter()
                .filter(|t| {
                    let tags = t.po.get_tags();
                    tags.iter().any(|x| x == NEURAL_TAG) && !tags.iter().any(|x| x == INTERNAL_TAG)
                })
                .map(|t| (t.po.id.clone(), t))
                .collect();

            // 2. 直接绑定工具：agent_tools 关联的启用工具（去重已在神经组）
            let bound_candidates = self
                .tool_dal
                .list_tools_for_agent_full(ctx.clone(), &agent.po.id)
                .await?;
            let mut bound_tools_map: std::collections::BTreeMap<String, Tool> =
                std::collections::BTreeMap::new();
            for t in bound_candidates {
                if neural_tools_map.contains_key(&t.po.id) {
                    continue;
                }
                let tags = t.po.get_tags();
                if tags.iter().any(|x| x == INTERNAL_TAG) {
                    continue;
                }
                bound_tools_map.insert(t.po.id.clone(), t);
            }

            // 3. 工具包分组：按每个 tag 展开（跳过 neural，且不重复前两组）
            let mut pack_groups: Vec<AgentToolPackGroup> = Vec::new();
            for tag in &installed_tool_tags {
                let candidates = self
                    .tool_dal
                    .query(
                        ctx.clone(),
                        crate::service::dao::tool::ToolQuery {
                            tags: Some(vec![tag.clone()]),
                            enabled_only: Some(true),
                            ..Default::default()
                        },
                    )
                    .await?;
                let mut pack_tools: Vec<Tool> = Vec::new();
                for t in candidates.items {
                    let tid = t.po.id.clone();
                    if neural_tools_map.contains_key(&tid) || bound_tools_map.contains_key(&tid) {
                        continue;
                    }
                    let tags = t.po.get_tags();
                    if tags.iter().any(|x| x == INTERNAL_TAG) {
                        continue;
                    }
                    if !tags.iter().any(|x| x == tag) {
                        continue;
                    }
                    pack_tools.push(t);
                }
                pack_tools.sort_by(|a, b| a.po.id.cmp(&b.po.id));
                pack_groups.push(AgentToolPackGroup {
                    tag: tag.clone(),
                    tools: pack_tools.iter().map(to_tool_list_item_default).collect(),
                });
            }

            // 汇总 bound_tools（需要已填充排除 pack 后）：上面先按 id 做去重
            let mut bound_sorted: Vec<&Tool> = bound_tools_map.values().collect();
            bound_sorted.sort_by(|a, b| a.po.id.cmp(&b.po.id));
            let bound_tools_list: Vec<common::api::ToolListItem> = bound_sorted
                .into_iter()
                .map(to_tool_list_item_default)
                .collect();

            let mut neural_sorted: Vec<&Tool> = neural_tools_map.values().collect();
            neural_sorted.sort_by(|a, b| a.po.id.cmp(&b.po.id));
            let neural_tools_list: Vec<common::api::ToolListItem> = neural_sorted
                .into_iter()
                .map(to_tool_list_item_default)
                .collect();

            Some(AgentToolsOverview {
                neural_tools: neural_tools_list,
                bound_tools: bound_tools_list,
                pack_groups,
            })
        } else {
            None
        };

        // ======================== 技能分组 ========================
        let skills_overview: Option<AgentSkillsOverview> = if with_skills {
            // 所有 Agent 自有技能副本（author_id = agent_id，exclude Expired）
            let all_skills = self
                .skill_dal
                .list_for_agent(ctx.clone(), &agent.po.id)
                .await?;

            // 第一趟：分配神经技能（优先级最高）
            let mut neural_skill_ids: std::collections::BTreeSet<String> =
                std::collections::BTreeSet::new();
            let mut neural_skills: Vec<common::api::SkillListItem> = all_skills
                .iter()
                .filter(|s| {
                    let tags = s.po.parse_tags();
                    tags.iter().any(|x| x == NEURAL_TAG)
                })
                .map(to_skill_list_item)
                .collect();
            // 记录神经技能 id，供第二/三趟排除（与上面的 filter 保持同一判定）
            for s in all_skills.iter() {
                if s.po.parse_tags().iter().any(|x| x == NEURAL_TAG) {
                    neural_skill_ids.insert(s.po.id.clone());
                }
            }
            neural_skills.sort_by(|a, b| a.id.cmp(&b.id));

            // 第二趟：按 installed_skill_packs 顺序分配（跳过已在神经组）
            use common::api::AgentSkillPackGroup;
            let mut pack_groups_skills: Vec<AgentSkillPackGroup> = Vec::new();
            for tag in &installed_skill_packs {
                let mut members: Vec<common::api::SkillListItem> = all_skills
                    .iter()
                    .filter(|s| {
                        if neural_skill_ids.contains(&s.po.id) {
                            return false;
                        }
                        let tags = s.po.parse_tags();
                        tags.iter().any(|x| x == tag)
                    })
                    .map(to_skill_list_item)
                    .collect();
                members.sort_by(|a, b| a.id.cmp(&b.id));
                pack_groups_skills.push(AgentSkillPackGroup {
                    tag: tag.clone(),
                    skills: members,
                });
            }

            // 第三趟：独立技能（不在神经组、不在任一 pack）
            let mut standalone: Vec<common::api::SkillListItem> = all_skills
                .iter()
                .filter(|s| {
                    if neural_skill_ids.contains(&s.po.id) {
                        return false;
                    }
                    // tags 只解析一次，避免在 any 闭包内对每个 tag 重复 parse
                    let tags = s.po.parse_tags();
                    let in_any_pack = installed_skill_packs
                        .iter()
                        .any(|tag| tags.iter().any(|x| x == tag));
                    !in_any_pack
                })
                .map(to_skill_list_item)
                .collect();
            standalone.sort_by(|a, b| a.id.cmp(&b.id));

            Some(AgentSkillsOverview {
                neural_skills,
                pack_groups: pack_groups_skills,
                standalone_skills: standalone,
            })
        } else {
            None
        };

        Ok((tools_overview, skills_overview))
    }
}

// ======================== DTO 转换辅助（默认 runtime_ready=Unknown） ========================

fn to_tool_list_item_default(tool: &Tool) -> ToolListItem {
    use common::enums::ToolStatus;
    ToolListItem {
        id: tool.po.id.clone(),
        name: tool.po.name.clone(),
        description: Some(tool.po.description.clone()),
        protocol: tool.po.protocol,
        control_mode: tool.po.control_mode,
        parameters_schema: tool.po.parameters_schema.clone(),
        tags: tool.po.get_tags(),
        status: tool.po.status,
        has_config: has_tool_config(&tool.po.config),
        enabled: matches!(tool.po.status, ToolStatus::Enabled),
        created_by: tool.po.created_by.clone().unwrap_or_default(),
        created_at: tool.po.created_at,
        updated_at: tool.po.updated_at,
        runtime_ready: common::api::RuntimeReady::Unknown,
    }
}

fn has_tool_config(config: &serde_json::Value) -> bool {
    match config {
        serde_json::Value::Null => false,
        serde_json::Value::Object(m) if m.is_empty() => false,
        _ => true,
    }
}

fn to_skill_list_item(skill: &Skill) -> SkillListItem {
    SkillListItem {
        id: skill.po.id.clone(),
        name: skill.po.name.clone(),
        description: skill.po.description.clone(),
        tags: skill.po.parse_tags(),
        category: skill.po.category.clone(),
        parent_skill_id: skill.po.parent_skill_id.clone(),
        author_id: skill.po.author_id.clone(),
        author_type: skill.po.author_type,
        status: skill.po.status,
        created_at: skill.po.created_at,
        updated_at: skill.po.updated_at,
    }
}
