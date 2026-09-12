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

    /// 【边：Incubating → Interviewing】职业生涯选择
    ///
    /// 拿 Agent 自己的 `roles ∪ capabilities` 逐 tag 去找资源，命中即装。
    /// 语义是「我因为这些身份/能力标签，所以会这些」——即成长阶段学完的职业技能。
    /// 走完这条边才配进入面试环节。
    async fn apply_career_bindings(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<common::api::TrainAgentResponse> {
        let agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;

        // 匹配源：角色 + 能力关键词，去重后逐 tag 探测资源
        let mut tags: Vec<String> = agent.po.get_roles();
        tags.extend(agent.po.get_capabilities());
        tags.sort();
        tags.dedup();

        let requests = tags
            .into_iter()
            .map(|tag| PackBinding {
                tag,
                tool: true,
                skill: true,
                is_company: false,
            })
            .collect();

        self.bind_packs(ctx, agent_id, requests).await
    }

    /// 【边：PendingOnboard → Onboarded】入职：安装「组织要求你会」的包
    ///
    /// `packs` 为 `None` 时回退组织级配置 `OrganizationConfig.agent_onboard`；
    /// 有传入则以本次为准（组织配置不再叠加）。
    ///
    /// 组织归属取 `ctx.organization_id()`（AgentPo 无 organization_id）；
    /// 取不到或读取失败时按空配置处理 —— 入职状态已经落库，不因此回滚。
    async fn apply_company_bindings(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        packs: Option<&common::api::AgentPackSelection>,
    ) -> Result<common::api::TrainAgentResponse> {
        let selection = match packs {
            Some(p) => p.clone(),
            None => {
                let cfg = match ctx.organization_id() {
                    Some(org_id) => self
                        .org_dal
                        .get_org_config(ctx.clone(), org_id)
                        .await
                        .unwrap_or_default(),
                    None => common::api::OrganizationConfig::default(),
                };
                common::api::AgentPackSelection {
                    tool_packs: cfg.agent_onboard.required_tool_packs,
                    skill_packs: cfg.agent_onboard.required_skill_packs,
                }
            }
        };

        // 工具包与技能包是两条独立的落点，同名包两侧都要装，故取并集后按 tag 归口
        let mut tags: Vec<String> = selection
            .tool_packs
            .iter()
            .cloned()
            .chain(selection.skill_packs.iter().cloned())
            .collect();
        tags.sort();
        tags.dedup();

        let requests = tags
            .into_iter()
            .map(|tag| PackBinding {
                tool: selection.tool_packs.contains(&tag),
                skill: selection.skill_packs.contains(&tag),
                tag,
                is_company: true,
            })
            .collect();

        self.bind_packs(ctx, agent_id, requests).await
    }
}

/// 单个包的绑定意图（`bind_packs` 的输入）
///
/// 工具包与技能包是两条独立落点（`installed_tags` vs `installed_skill_packs` + 技能副本），
/// 所以「装哪一侧」由调用方显式声明，而不是让下游猜。
struct PackBinding {
    /// 包 tag
    tag: String,
    /// 是否安装工具包
    tool: bool,
    /// 是否安装技能包
    skill: bool,
    /// 是否组织指定：`true` = 显式授权声明，无条件写；
    /// `false` = 个人匹配，需先探测到资源才写（避免脏 tag）
    is_company: bool,
}

impl HrDomainImpl {
    /// 【绑定落点】按意图列表批量绑定工具包与技能包
    ///
    /// 「职业选择」与「入职」两条边共用这一个落点，保证两侧规则完全一致，
    /// 不会出现同一件事两处写、语义分叉。
    ///
    /// 为什么两侧都必须装（缺一不可）：
    /// - **工具**：授权 = `neural ∪ (tool.tags ∩ installed_tags)`，**不含 roles**，
    ///   所以不写 `installed_tags` 的工具一律被拒；
    /// - **技能**：`roles` 虽直接进 `match_keys`，但技能必须先存在于 Agent 副本池
    ///   （`author_id = agent_id`）才轮得到判定，所以必须真正建副本。
    ///
    /// 守卫与容错：
    /// - 工具包：**个人匹配**要求库里确有该 tag 的已启用工具才写 `installed_tags`
    ///   （否则 `capabilities` 里的自由关键词 chat / knowledge 会被写成空包脏 tag）；
    ///   **组织指定**是显式授权声明，无条件写入（工具后续上线即生效）；
    /// - 技能包：两侧都必须有已发布技能才装 —— 技能只能由真实技能复制而来；
    /// - 全程幂等（install_* 内部各自判重），单包失败只记 warn 不中断；
    /// - 返回值只统计**本次新增**的包，供调用方（入职/进修自愈）展示。
    async fn bind_packs(
        &self,
        ctx: RequestContext,
        agent_id: &str,
        requests: Vec<PackBinding>,
    ) -> Result<common::api::TrainAgentResponse> {
        let agent = self
            .agent_dal
            .find_by_id(ctx.clone(), agent_id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent_id))?;
        let ctx = enrich_ctx!(&ctx, &agent);

        let mut resp = common::api::TrainAgentResponse {
            agent_id: agent_id.to_string(),
            ..Default::default()
        };
        let runtime_config = agent.po.get_runtime_config();

        for req in requests {
            let source = if req.is_company {
                "组织指定"
            } else {
                "个人匹配"
            };

            // ── 工具包 ──
            if req.tool && !runtime_config.has_tag(&req.tag) {
                // 组织指定 = 显式授权声明 → 无条件写 installed_tags（工具后续上线即生效）；
                // 个人匹配 = 按 tag 探测资源 → 库里没有该 tag 的已启用工具就跳过。
                let has_tools = if req.is_company {
                    true
                } else {
                    !self
                        .tool_dal
                        .query(
                            ctx.clone(),
                            crate::service::dao::tool::ToolQuery {
                                tags: Some(vec![req.tag.clone()]),
                                enabled_only: Some(true),
                                ..Default::default()
                            },
                        )
                        .await?
                        .items
                        .is_empty()
                };
                if has_tools {
                    match self
                        .install_tool_pack(ctx.clone(), agent_id, &req.tag)
                        .await
                    {
                        Ok(()) => {
                            log_info!(
                                ctx.clone(),
                                "bind_packs",
                                "agent_id={}, tag={}, 来源={} 工具包已绑定",
                                agent_id,
                                req.tag,
                                source
                            );
                            resp.installed_tool_tags.push(req.tag.clone());
                        }
                        Err(e) => {
                            log_warn!(
                                ctx.clone(),
                                "bind_packs",
                                "绑定工具包 {tag} 失败（忽略）: {e}",
                                tag = req.tag
                            );
                        }
                    }
                }
            }

            // ── 技能包 ──
            // 技能只能由真实已发布技能复制而来 → 两侧都必须有资源才装；
            // 无资源则跳过且不写 installed_skill_packs，避免空技能包脏数据。
            if req.skill && !runtime_config.has_skill_pack_tag(&req.tag) {
                let Ok(published) = self
                    .skill_dal
                    .list_published_by_tag(ctx.clone(), &req.tag)
                    .await
                else {
                    continue;
                };
                if published.is_empty() {
                    continue;
                }
                match self
                    .install_skill_pack(ctx.clone(), agent_id, &req.tag)
                    .await
                {
                    Ok(n) if n > 0 => {
                        resp.installed_skill_packs.push(req.tag.clone());
                    }
                    Ok(_) => {}
                    Err(e) => {
                        log_warn!(
                            ctx.clone(),
                            "bind_packs",
                            "安装技能包 {tag} 失败（忽略）: {e}",
                            tag = req.tag
                        );
                    }
                }
            }
        }

        Ok(resp)
    }
}

/// 【第一阶段：出生自带】创建 Agent 时默认安装的基础包 tags（工具包与技能包共用同一集合）。
///
/// 对应「天生就会」的能力：与角色、岗位无关，所有 Agent 出生即持有。
///
/// - `neural`：神经工具/技能（思考场景白名单 + 无条件进 Prompt 的方法论）；
/// - `skill_management` / `tool_management`：Agent 自治骨架，能管理自己的技能与工具。
///
/// 显式安装让每个 Agent 都持有一份自己的副本/绑定，无需加载侧再兜底；
/// 这三个包在卸载时受保护（见 uninstall_tool_pack / uninstall_skill_pack）；
/// 仅当库里已有对应已发布资源时才安装（见 train_agent 守卫）；
/// 缺失时可通过 train_agent（POST /agents/{id}/train）补装。
const BASE_AGENT_PACKS: &[&str] = &["neural", "skill_management", "tool_management"];

/// 【第三阶段：组织指定】入职时「组织要求你会」的包 tags。
///
/// **不再硬编码在代码里** —— 包名属于组织决策，移到组织级配置
/// `OrganizationConfig.agent_onboard`（`organizations.config` JSON 列），
/// 由组织管理员在组织信息页维护。状态机因此不再耦合任何具体业务包名。
///
/// 保留这条注释说明「同名双重身份」这个坑：像 `project_management` 这样的包，
/// 工具包与技能包两个字段都必须配置：
/// - 工具包：项目/任务/产物工具挂着该 tag，须写入 `installed_tags` 才被授权
///   （授权 = neural ∪ (tool.tags ∩ installed_tags)，见 runtime/tool_execution.rs），
///   同时它参与技能必加载的 `match_keys`，也是技能进 Prompt 的必要条件；
/// - 技能包：写入 `installed_skill_packs` 并真正建技能副本。
///
/// 组织未配置任何包时，入职只做状态流转，不装额外能力（合法，不是错误）。

#[async_trait::async_trait]
impl AgentManage for HrDomainImpl {
    /// 创建 Agent（分步骤执行流程，效仿 initialize_system::run_steps）
    ///
    /// 创建被拆成 2 个显式步骤，每步边界清晰、可独立观察：
    /// - Step 1 基础信息：仅持久化 Agent 本体（统一进入 Incubating 状态）
    /// - Step 2 补装基础包：出生自带的神经/技能/工具管理包补装（复用 train_agent，
    ///   新生 Agent 停在 Incubating，进修只会命中基础课，不会提前获得职业/组织能力）
    ///
    /// 设计原则：基础信息之外的步骤均为「增强」，失败不阻塞创建；
    /// 且都有存在性守卫（无对应已发布资源则不记录脏数据）。
    /// - 允许 Local Agent 暂不指定 model_provider_id：缺模型时用 Incubating 状态表达
    ///   "尚未就绪"，用户可在模型管理中补配并入职后使用（不是错误，是生命周期状态）
    /// - 强制校验：创建后状态固定为 Incubating（出生，只有神经能力；角色相关的职业
    ///   技能要等走完 `Incubating → Interviewing` 这条边才获得）
    async fn create_agent(&self, ctx: RequestContext, agent: &Agent) -> Result<()> {
        // 强制校验：状态必须是 Incubating
        if agent.po.status != AgentStatus::Incubating {
            bail_err!(InvalidRequest, "新建 Agent 状态必须为 Incubating");
        }

        // ── Step 1/2：基础信息构建 ──
        // 仅持久化 Agent 本体，不做任何额外安装。
        log_info!(ctx, "create_agent", "Step 1/2: 创建 Agent 基础信息");
        self.agent_dal.create(ctx.clone(), agent).await?;

        // ── Step 2/2：补装基础包 ──
        // 出生自带的神经/技能/工具管理包补装（进修语义下的"基础课"）。
        // 仅当库里已有对应已发布资源时才安装，避免无资源环境下留下脏数据；
        // 单个包失败不阻塞创建（train_agent 内部记 warn 继续）。
        log_info!(ctx, "create_agent", "Step 2/2: 补装基础包");
        let result = self.train_agent(ctx.clone(), &agent.po.id).await?;
        log_info!(
            ctx,
            "create_agent",
            "Step 2/2 完成: 补装工具包={:?}, 补装技能包={:?}",
            result.installed_tool_tags,
            result.installed_skill_packs
        );

        Ok(())
    }

    /// Agent 进修（在职学习入口）
    ///
    /// 语义：职业/组织又有了新的工具与技能，Agent 需要学习 —— 把自己的能力
    /// 补齐到当前职业与组织要求的最新状态。三阶段执行，全程幂等，
    /// 单个包失败不阻塞其他包：
    /// - 阶段 1 补修基础课（天生包缺失补装）：工具包仅写 installed_tags 关联
    ///   （无包内补全问题）；技能包安装副本后由阶段 2 统一补全。
    /// - 阶段 2 学习技能更新：对当前所有已安装技能包，检测该 tag 下
    ///   是否有 Agent 尚未拥有的新增已发布技能（按 parent_skill_id 比对），
    ///   有则重装该技能包（reinstall_skill_pack 同时刷新已有副本内容）。
    /// - 阶段 3 补学新课（绑定自愈，按已走过的状态机边）：重跑个人匹配
    ///   （roles ∪ capabilities，非初创）与组织指定的包（仅已入职，
    ///   回退 OrganizationConfig.agent_onboard）。自愈不替代状态机准入：
    ///   仍停在 Incubating 的不提前发岗位能力，未入职的不装组织包。
    async fn train_agent(
        &self,
        ctx: RequestContext,
        agent_id: &str,
    ) -> Result<common::api::TrainAgentResponse> {
        let mut resp = common::api::TrainAgentResponse {
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
                        "train_agent",
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
                Ok(n) if n > 0 => resp.installed_skill_packs.push(tag.to_string()),
                Ok(_) => {
                    // 没有实际安装任何技能（如该 tag 暂无已发布技能）：不记为已安装包
                }
                Err(e) => {
                    log_warn!(
                        ctx.clone(),
                        "train_agent",
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
                        "train_agent",
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
                        "train_agent",
                        "重装技能包 {tag} 补全失败（忽略）: {e}"
                    );
                }
            }
        }

        // ══════ 阶段 3/3：绑定自愈（按 Agent 已走过的边补齐）══════
        // 包可能是「Agent 走过那条边之后才发布」的，只有重跑匹配才能补上。
        // 两条边各自独立判断，未走到的边就跳过 —— 自愈不能替代状态机的准入语义。
        //
        // (a) 职业选择自愈：过了初创期（即已走完 Incubating → Interviewing）才补。
        //     仍停在 Incubating 的 Agent 还没择业，补装等于绕过状态机提前发岗位能力。
        if agent.po.status != AgentStatus::Incubating && agent.po.status != AgentStatus::Deleted {
            match self.apply_career_bindings(ctx.clone(), agent_id).await {
                Ok(bound) => {
                    resp.installed_tool_tags.extend(bound.installed_tool_tags);
                    resp.installed_skill_packs
                        .extend(bound.installed_skill_packs);
                }
                Err(e) => {
                    log_warn!(ctx.clone(), "train_agent", "职业选择自愈失败（忽略）: {e}");
                }
            }
        }
        //
        // (b) 入职自愈：仅已入职 Agent 补组织要求的包（回退组织级配置）。
        if agent.po.status == AgentStatus::Onboarded {
            match self
                .apply_company_bindings(ctx.clone(), agent_id, None)
                .await
            {
                Ok(bound) => {
                    resp.installed_tool_tags.extend(bound.installed_tool_tags);
                    resp.installed_skill_packs
                        .extend(bound.installed_skill_packs);
                }
                Err(e) => {
                    log_warn!(ctx.clone(), "train_agent", "入职绑定自愈失败（忽略）: {e}");
                }
            }
        }

        log_info!(
            ctx,
            "train_agent",
            "agent_id={} 进修完成: 补修基础工具包={:?}, 补修基础技能包={:?}, 学习技能更新={:?}",
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
    /// 校验状态流转合法性，更新状态并持久化，然后**按边分发副作用**。
    ///
    /// 副作用只挂在「边」上（状态本身只是结果）：
    /// - `Incubating → Interviewing`：职业生涯选择（按 roles/capabilities 个人匹配）
    /// - `PendingOnboard → Onboarded`：入职（安装组织要求的包，packs 为 None 时回退组织配置）
    /// - 其余边：无副作用（如 `Interviewing → PendingOnboard`，预留给后续扩展）
    ///
    /// `packs` 只在入职这条边上被读取，其余状态忽略。
    async fn transition_status(
        &self,
        ctx: RequestContext,
        agent: &mut Agent,
        target_status: AgentStatus,
        packs: Option<common::api::AgentPackSelection>,
    ) -> Result<()> {
        // 补充 Agent 上下文
        let ctx = enrich_ctx!(&ctx, &*agent);

        let current_status = agent.po.status;

        // 状态机校验：定义合法的流转路径
        let is_valid_transition = match (&current_status, &target_status) {
            // 初创 → 面试中（学完了，去面试）
            (AgentStatus::Incubating, AgentStatus::Interviewing) => true,
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

        // 更新状态：以 DB 最新快照为基底只改 status，再整行写回。
        //
        // ⚠️ 不能直接写调用方持有的 `agent` —— 它的 runtime_config 可能落后于
        // 上一条边的副作用（install_* 是「重读 → 改 runtime_config → 整行写回」），
        // 直接覆盖会把上一条边刚装好的包抹掉（前台 Agent 三连转时 reception 包
        // 曾因此被 PendingOnboard 这条边的过期快照冲掉）。
        let mut fresh = self
            .agent_dal
            .find_by_id(ctx.clone(), &agent.po.id)
            .await?
            .ok_or_else(|| err!(NotFound, "Agent {} 不存在", agent.po.id))?;
        fresh.po.status = target_status;
        self.agent_dal.update(ctx.clone(), &fresh).await?;
        // 同步调用方视图：状态更新，其余字段保持调用方原值（调用方如需最新
        // runtime_config 应重新 get_agent）。
        agent.po.status = target_status;

        // 入职 = 安装组织要求的能力；职业选择 = 安装个人匹配的能力。
        // 二者都属「增强」步骤：单包失败已在内部容忍，这里再兜一层，
        // 确保任何异常都不会让已经落库的状态流转回滚。
        match (current_status, target_status) {
            // 初创 → 面试中：职业生涯选择（个人匹配 roles ∪ capabilities）
            (AgentStatus::Incubating, AgentStatus::Interviewing) => {
                match self.apply_career_bindings(ctx.clone(), &agent.po.id).await {
                    Ok(bound) => log_info!(
                        ctx,
                        "select_agent_career",
                        "职业选择完成: 工具包={:?}, 技能包={:?}",
                        bound.installed_tool_tags,
                        bound.installed_skill_packs
                    ),
                    Err(e) => log_warn!(
                        ctx,
                        "select_agent_career",
                        "职业选择失败（忽略，状态已流转）: {e}"
                    ),
                }
            }
            // 待入职 → 已入职：真正的入职流程（组织要求的包）
            (AgentStatus::PendingOnboard, AgentStatus::Onboarded) => {
                match self
                    .apply_company_bindings(ctx.clone(), &agent.po.id, packs.as_ref())
                    .await
                {
                    Ok(bound) => log_info!(
                        ctx,
                        "onboard_agent",
                        "入职绑定完成: 工具包={:?}, 技能包={:?}",
                        bound.installed_tool_tags,
                        bound.installed_skill_packs
                    ),
                    Err(e) => log_warn!(
                        ctx,
                        "onboard_agent",
                        "入职绑定失败（忽略，状态已流转）: {e}"
                    ),
                }
            }
            // 其余边无副作用
            _ => {}
        }

        Ok(())
    }

    /// 拉取远端 A2A Agent 的任务快照
    async fn fetch_remote_task(
        &self,
        ctx: RequestContext,
        agent: &Agent,
        remote_task_id: &str,
    ) -> Result<common::api::a2a::A2aTask> {
        self.runtime_dal
            .fetch_remote_task(ctx, agent, remote_task_id)
            .await
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

        // 真正把包内工具写入关系表 agent_tools（ON CONFLICT 去重），
        // 使工具成为 Agent 的显式关联，便于做防重复 & 下游统一聚合展示。
        let pack_tools = self
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
        for t in pack_tools.items {
            let tid = t.po.id.clone();
            if let Err(e) = self
                .tool_dal
                .add_tool_to_agent(ctx.clone(), agent_id, &tid, None)
                .await
            {
                log_warn!(
                    ctx.clone(),
                    "install_tool_pack",
                    "写入关系表失败（忽略）: tag={}, tool_id={}, err={}",
                    tag,
                    tid,
                    e
                );
            }
        }

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

        // 从关系表移除该包内的工具：仅当工具不再命中任何其它仍安装的包 tag 时才移除，
        // 避免误删被多个包共享/显式绑定的工具。
        let remaining_tags: Vec<String> = agent.po.get_runtime_config().installed_tags.clone();
        let pack_tools = self
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
        for t in pack_tools.items {
            let ttags = t.po.get_tags();
            let still_needed = ttags
                .iter()
                .any(|x| x != tag && remaining_tags.iter().any(|r| r == x));
            if !still_needed {
                let _ = self
                    .tool_dal
                    .remove_tool_from_agent(ctx.clone(), agent_id, &t.po.id)
                    .await;
            }
        }

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
            // 没有任何可安装的已发布技能：直接返回错误，绝不记录 tag ——
            // 避免产生「空技能包」（刷新后残留空包但无任何实际技能）。
            log_warn!(
                ctx,
                "install_skill_pack",
                "agent_id={}, tag={} 没有可安装的已发布技能",
                agent_id,
                tag
            );
            bail_err!(InvalidRequest, "技能包 [{}] 没有可安装的已发布技能", tag);
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

        // 没有任何技能真正安装成功（全部失败）：不创建空包，直接返回错误，
        // 避免刷新后残留「空技能包」。此时 tag 不写入 runtime_config。
        if success_count == 0 {
            bail_err!(
                InvalidRequest,
                "技能包 [{}] 安装失败（失败数={}），未创建空技能包",
                tag,
                fail_count
            );
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

    async fn get_agent_tool_list_ids(
        &self,
        ctx: RequestContext,
        agent: &Agent,
    ) -> Result<Vec<String>> {
        const INTERNAL_TAG: &str = "internal";

        let runtime_cfg = agent.po.get_runtime_config();
        let installed_tool_tags: Vec<String> = runtime_cfg.installed_tags.to_vec();

        let mut ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        // 1. 直接绑定（agent_tools 关联表，含安装工具包时写入的工具）
        let bound = self
            .tool_dal
            .list_tools_for_agent_full(ctx.clone(), &agent.po.id)
            .await?;
        for t in bound {
            let tags = t.po.get_tags();
            if tags.iter().any(|x| x == INTERNAL_TAG) {
                continue;
            }
            ids.insert(t.po.id.clone());
        }

        // 2. 已安装工具包 tag 展开（含 neural，作为普通 tag）。
        //    与直接绑定取并集后按 id 去重——一个工具命中多个包时列表唯一，
        //    显示层（前端）可重复归类到各包分组。
        for tag in &installed_tool_tags {
            if tag == INTERNAL_TAG {
                continue;
            }
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
            for t in candidates.items {
                let tags = t.po.get_tags();
                if tags.iter().any(|x| x == INTERNAL_TAG) {
                    continue;
                }
                if !tags.iter().any(|x| x == tag) {
                    continue;
                }
                ids.insert(t.po.id.clone());
            }
        }

        Ok(ids.into_iter().collect())
    }
}
