//! Agent 管理具体方法实现

use crate::models::agent::Agent;
use crate::models::skill::Skill;
use crate::models::tool::Tool;
use crate::pkg::RequestContext;
use crate::service::dao::agent::AgentQuery;
use crate::service::domain::hr::{AgentManage, HrDomainImpl};
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

    /// 解析 Agent 可见的技能全集（供 wake/awaken 与关联全景共用）。
    /// 解析 Agent 可见的技能全集（供 wake/awaken 与关联全景共用）。
    ///
    /// 仅返回 Agent 自身已安装的副本（author_id = agent_id，排除 Expired）。
    /// 神经技能等基础包在 create_agent 时已显式安装为副本，因此加载侧无需再兜底。
    async fn resolve_agent_skills(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<Vec<Skill>> {
        let skills = self
            .skill_dal
            .query(
                ctx.clone(),
                crate::service::dao::skill::SkillQuery {
                    author_id: Some(agent_id.to_string()),
                    exclude_status: Some(common::enums::SkillStatus::Expired),
                    ..Default::default()
                },
            )
            .await?;
        Ok(skills.items)
    }
}

/// 创建 Agent 时默认安装的基础包 tags（工具包与技能包共用同一集合）。
///
/// 包含 neural / skill_management / tool_management 三个基础包：
///
/// - 显式安装让每个 Agent 都持有一份自己的副本/绑定，无需加载侧再兜底；
/// - 这三个基础包在卸载时受保护（见 uninstall_tool_pack / uninstall_skill_pack）；
/// - 仅当库里已有对应已发布资源时才安装（见 sync_agent_packs 守卫）；
/// - 缺失时可通过 sync_agent_packs（POST /agents/{id}/sync-packs）补装。
const BASE_AGENT_PACKS: &[&str] = &["neural", "skill_management", "tool_management"];

#[async_trait::async_trait]
impl AgentManage for HrDomainImpl {
    /// 创建 Agent（分步骤执行流程，效仿 initialize_system::run_steps）
    ///
    /// 创建被拆成 2 个显式步骤，每步边界清晰、可独立观察：
    /// - Step 1 基础信息：仅持久化 Agent 本体（统一进入 Interviewing 状态）
    /// - Step 2 同步包：补装基础工具包/技能包 + 已安装技能包增量补全（复用 sync_agent_packs）
    ///
    /// 设计原则：基础信息之外的步骤均为「增强」，失败不阻塞创建；
    /// 且都有存在性守卫（无对应已发布资源则不记录脏数据）。
    /// - 允许 Local Agent 暂不指定 model_provider_id：缺模型时用 Interviewing 状态表达
    ///   "尚未就绪"，用户可在模型管理中补配并入职后使用（不是错误，是生命周期状态）
    /// - 强制校验：创建后状态固定为 Interviewing（统一从面试开始）
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        // 强制校验：状态必须是 Interviewing
        if agent.po.status != AgentStatus::Interviewing {
            bail_err!(InvalidRequest, "新建 Agent 状态必须为 Interviewing");
        }

        // ── Step 1/2：基础信息构建 ──
        // 仅持久化 Agent 本体，不做任何额外安装。
        log_info!(ctx, "create_agent", "Step 1/2: 创建 Agent 基础信息");
        self.agent_dal.create(ctx.clone(), agent).await?;

        // ── Step 2/2：同步包 ──
        // 补装基础工具包/技能包（神经/技能/工具管理），并对已安装技能包做增量补全。
        // 仅当库里已有对应已发布资源时才安装，避免无资源环境下留下脏数据；
        // 单个包失败不阻塞创建（sync_agent_packs 内部记 warn 继续）。
        log_info!(ctx, "create_agent", "Step 2/2: 同步基础包与技能包");
        let result = self.sync_agent_packs(ctx.clone(), &agent.po.id).await?;
        log_info!(
            ctx,
            "create_agent",
            "Step 2/2 完成: 补装工具包={:?}, 补装技能包={:?}",
            result.installed_tool_tags,
            result.installed_skill_packs
        );

        Ok(())
    }

    /// 同步 Agent 包（通用恢复/同步入口）
    ///
    /// 两阶段执行，全程幂等，单个包失败不阻塞其他包：
    /// - 阶段 1 基础包缺失补装：工具包仅写 installed_tags 关联（无包内补全问题）；
    ///   技能包安装副本后由阶段 2 统一补全。
    /// - 阶段 2 已安装技能包增量补全：对当前所有已安装技能包，检测该 tag 下
    ///   是否有 Agent 尚未拥有的新增已发布技能（按 parent_skill_id 比对），
    ///   有则重装该技能包（reinstall_skill_pack 同时刷新已有副本内容）。
    async fn sync_agent_packs(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<common::api::SyncAgentPacksResponse> {
        let mut resp = common::api::SyncAgentPacksResponse {
            agent_id: agent_id.to_string(),
            ..Default::default()
        };

        // 前置：确认 Agent 存在（后续 install_* 内部也会校验，这里提前给出明确错误）
        let agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;
        let ctx = enrich_ctx!(&ctx, &agent);

        // ══════ 阶段 1/2：基础包缺失补装 ══════
        for &tag in BASE_AGENT_PACKS {
            // 工具包：仅当库里已有对应已启用工具时才写关联，避免脏数据
            let tools = self
                .tool_dal
                .query(
                    ctx.clone(),
                    crate::service::dao::tool::ToolQuery {
                        tags: Some(vec![tag.to_string()]),
                        enabled_only: Some(true),
                        ..Default::default()
                    },
                )
                .await?;
            if tools.items.is_empty() {
                continue;
            }
            if agent.po.get_runtime_config().has_tag(tag) {
                continue;
            }
            match self.install_tool_pack(ctx.clone(), agent_id, tag).await {
                Ok(()) => resp.installed_tool_tags.push(tag.to_string()),
                Err(e) => {
                    log_warn!(
                        ctx.clone(),
                        "sync_agent_packs",
                        "补装工具包 {tag} 失败（忽略）: {e}"
                    );
                }
            }
        }
        for &tag in BASE_AGENT_PACKS {
            // 技能包：仅当库里已有对应已发布技能时才安装，避免空包脏数据
            let published = self
                .skill_dal
                .list_published_by_tag(ctx.clone(), tag)
                .await?;
            if published.is_empty() {
                continue;
            }
            if agent.po.get_runtime_config().has_skill_pack_tag(tag) {
                continue;
            }
            match self.install_skill_pack(ctx.clone(), agent_id, tag).await {
                Ok(_) => resp.installed_skill_packs.push(tag.to_string()),
                Err(e) => {
                    log_warn!(
                        ctx.clone(),
                        "sync_agent_packs",
                        "补装技能包 {tag} 失败（忽略）: {e}"
                    );
                }
            }
        }

        // ══════ 阶段 2/2：已安装技能包增量补全 ══════
        // 以阶段 1 之后的最新 installed_skill_packs 为准（刚补装的包内容全新，
        // 检测也不会有新增，统一纳入检测可少一套排除逻辑）。
        let agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;
        let installed_packs = agent.po.get_installed_skill_packs();
        for tag in installed_packs {
            let Ok(published) = self
                .skill_dal
                .list_published_by_tag(ctx.clone(), &tag)
                .await
            else {
                continue;
            };
            if published.is_empty() {
                continue;
            }

            // 按 parent_skill_id 比对：找出 Agent 尚未拥有副本的新增已发布技能
            let parent_ids: Vec<String> = published.iter().map(|s| s.po.id.clone()).collect();
            let existing_copies = self
                .skill_dal
                .find_agent_skill_copies(ctx.clone(), agent_id, &parent_ids)
                .await?;
            let existing_parents: std::collections::HashSet<&str> = existing_copies
                .iter()
                .map(|s| s.po.parent_skill_id.as_str())
                .collect();
            let has_new = published
                .iter()
                .any(|s| !existing_parents.contains(s.po.id.as_str()));
            if !has_new {
                continue;
            }

            // 重装该技能包：补全新增技能 + 顺带刷新已有副本内容
            match self.reinstall_skill_pack(ctx.clone(), agent_id, &tag).await {
                Ok(count) => {
                    log_info!(
                        ctx,
                        "sync_agent_packs",
                        "agent_id={}, tag={} 检测到新增技能，已重装补全: 处理={}",
                        agent_id,
                        tag,
                        count
                    );
                    resp.refreshed_skill_packs.push(tag);
                }
                Err(e) => {
                    log_warn!(
                        ctx.clone(),
                        "sync_agent_packs",
                        "重装技能包 {tag} 补全失败（忽略）: {e}"
                    );
                }
            }
        }

        log_info!(
            ctx,
            "sync_agent_packs",
            "agent_id={} 同步完成: 补装工具包={:?}, 补装技能包={:?}, 重装补全={:?}",
            agent_id,
            resp.installed_tool_tags,
            resp.installed_skill_packs,
            resp.refreshed_skill_packs
        );
        Ok(resp)
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
                // 优先 Agent 自身副本；副本缺失的神经技能用种子兜底（见 resolve_agent_skills）
                let skills = self.resolve_agent_skills(ctx.clone(), id).await?;
                agent.set_skills(skills);
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

        // 基础包受保护：neural / skill_management / tool_management 不允许卸载
        if BASE_AGENT_PACKS.contains(&tag) {
            bail_err!(InvalidRequest, "{tag} 是基础工具包，不允许卸载");
        }

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

        // 基础包受保护：neural / skill_management / tool_management 不允许卸载
        if BASE_AGENT_PACKS.contains(&tag) {
            bail_err!(InvalidRequest, "{tag} 是基础技能包，不允许卸载");
        }

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

    async fn get_agent_association_groups(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        with_tools: bool,
        with_skills: bool,
    ) -> Result<(
        Option<crate::service::domain::hr::AgentToolGroups>,
        Option<crate::service::domain::hr::AgentSkillGroups>,
    )> {
        use crate::service::domain::hr::{
            AgentSkillGroups, AgentSkillPackIds, AgentToolGroups, AgentToolPackIds,
        };

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
        let tool_groups: Option<AgentToolGroups> = if with_tools {
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
            let mut pack_groups: Vec<AgentToolPackIds> = Vec::new();
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
                pack_groups.push(AgentToolPackIds {
                    tag: tag.clone(),
                    tool_ids: pack_tools.iter().map(|t| t.po.id.clone()).collect(),
                });
            }

            // 汇总（已按 id 排序，保证输出稳定）
            let bound_ids: Vec<String> = bound_tools_map.keys().cloned().collect();
            let neural_ids: Vec<String> = neural_tools_map.keys().cloned().collect();

            Some(AgentToolGroups {
                neural_ids,
                bound_ids,
                pack_groups,
            })
        } else {
            None
        };

        // ======================== 技能分组 ========================
        let skill_groups: Option<AgentSkillGroups> = if with_skills {
            // 优先 Agent 自身副本；副本缺失的神经技能用种子兜底（支持自我演进 + 加载兜底）
            let all_skills = self.resolve_agent_skills(ctx.clone(), &agent.po.id).await?;

            // 第一趟：分配神经技能（优先级最高）
            let neural_skill_ids: std::collections::BTreeSet<String> = all_skills
                .iter()
                .filter(|s| s.po.parse_tags().iter().any(|x| x == NEURAL_TAG))
                .map(|s| s.po.id.clone())
                .collect();

            // 第二趟：按 installed_skill_packs 顺序分配（跳过已在神经组）
            let mut pack_groups: Vec<AgentSkillPackIds> = Vec::new();
            for tag in &installed_skill_packs {
                let mut members: Vec<String> = all_skills
                    .iter()
                    .filter(|s| {
                        if neural_skill_ids.contains(&s.po.id) {
                            return false;
                        }
                        let tags = s.po.parse_tags();
                        tags.iter().any(|x| x == tag)
                    })
                    .map(|s| s.po.id.clone())
                    .collect();
                members.sort();
                pack_groups.push(AgentSkillPackIds {
                    tag: tag.clone(),
                    skill_ids: members,
                });
            }

            // 第三趟：独立技能（不在神经组、不在任一 pack）
            let mut standalone: Vec<String> = all_skills
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
                .map(|s| s.po.id.clone())
                .collect();
            standalone.sort();

            Some(AgentSkillGroups {
                neural_ids: neural_skill_ids.into_iter().collect(),
                pack_groups,
                standalone_ids: standalone,
            })
        } else {
            None
        };

        Ok((tool_groups, skill_groups))
    }
}
