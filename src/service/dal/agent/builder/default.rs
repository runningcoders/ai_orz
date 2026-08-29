//! DefaultPromptBuilder——Local Agent 默认 Prompt 构建器
//!
//! 拆分自原 agent.rs（本次文件重构）：承载 [`super::super::AgentDal::prompt_builder`]
//! 默认实现使用的 Prompt 拼装逻辑。统一注入 skills，build() 时按 tag 自动分块：
//!
//! 1. 【Agent 人设】        ← 最稳定
//! 2. 【神经技能】          ← tags 含 "neural"，所有 Agent 必加载
//! 3. 【必加载技能】        ← tags 不含 "neural" 但与 agent match_keys 有交集
//! 4. 【用户画像】          ← 随用户变化，对话中相对稳定
//! 5. 【项目上下文】+【任务上下文】 ← 业务上下文，随消息变化
//! 6. 【历史对话】          ← 随对话增长
//! 7. 【工具失败警告】      ← 实时变化
//! 8. 【trace_id + 当前消息】← 每次变化
//!
//! 所有区块拼装方法（`build_skills_sections` / `build_common_context_sections` /
//! `render_intent_analysis_section` / `build_final_response_guidance` /
//! `push_settled_reference` / `awaken_system_part` / `awaken_user_part`）
//! 均定义在 [`crate::models::prompt_builder::PromptBuilder`] trait 中，
//! 本实现为其完整实现；其他 Builder（如 FlatPromptBuilder）用不上的方法走 trait 默认空实现。

use crate::models::agent::Agent;
use crate::models::cortex_types::ChatMessage;
use crate::models::memory::Memory;
use crate::models::message::Message;
use crate::models::skill::SkillPo;
use crate::models::user::UserPo;
// ==================== Prompt Builder（Local Agent 默认实现） ====================

/// neural tag 常量：标记为神经级别的工具/技能，所有 Agent 必加载
const NEURAL_TAG: &str = "neural";

/// 默认 Prompt 构建器（Local Agent 使用）
///
/// 统一注入 skills，build() 时按 tag 自动分块拼装：
///
/// 1. 【Agent 人设】        ← 最稳定
/// 2. 【神经技能】          ← tags 含 "neural"，所有 Agent 必加载
/// 3. 【必加载技能】        ← tags 不含 "neural" 但与 agent match_keys 有交集
/// 4. 【用户画像】          ← 随用户变化，对话中相对稳定
/// 5. 【项目上下文】+【任务上下文】 ← 业务上下文，随消息变化
/// 6. 【历史对话】          ← 随对话增长
/// 7. 【工具失败警告】      ← 实时变化
/// 8. 【trace_id + 当前消息】← 每次变化
///
/// match_keys = agent.roles ∪ agent.installed_tags
///
/// 工具信息传递路径：
/// - 工具列表（name/description/parameters）→ OpenAI tools API 字段（协议层）
/// - Prompt 文本层不再包含任何工具描述（工具调用对模型透明，由 awakening 层根据 control_mode 分发）
///
/// build_sleep_prompt() 与 build() 对称，复用 1-6 区块（跳过 tool_failures 和 current_message），
/// 加上沉淀约束章节 + 待沉淀记忆摘要，用于 sleep_and_settle 场景。
#[derive(Debug, Clone, Default)]
pub struct DefaultPromptBuilder {
    /// 本次思考的 Trace ID
    current_trace_id: Option<String>,
    /// Agent 人设 / System Prompt
    system_prompt: Option<String>,
    /// 匹配键：agent.roles ∪ agent.installed_tags（system_prompt 时缓存）
    match_keys: Vec<String>,
    /// 用户画像信息（仅客服类 Agent 使用）
    user_profile: Option<String>,
    /// 项目上下文摘要（消息关联的项目实体，有值即拼装）
    project_context: Option<String>,
    /// 任务上下文摘要（消息关联的任务实体，有值即拼装）
    task_context: Option<String>,
    /// 历史对话记忆
    history: Vec<String>,
    /// 近期已沉淀记忆参考条目（仅沉淀场景，已完成沉淀，不作为待处理对象）
    settled_reference: Vec<String>,
    /// 上一轮工作压缩结果（仅 awaken 场景，压缩后由框架直接注入）
    compacted_context: Option<String>,
    /// 更早的记忆参考条目（仅 awaken 场景，压缩后的轮次补充连续性）
    past_memories: Vec<String>,
    /// 当前用户消息
    current_message: Option<String>,
    /// 技能（全量，build 时按 tag 分块）
    skills: Vec<SkillPo>,
    /// 工具失败统计：(工具名称, 失败次数)
    tool_failures: Vec<(String, u64)>,
    /// 意图分析结果（IntentAnalyze 阶段产出），供 build() 时渲染参考区块使用
    pub intent_analysis: Option<crate::service::domain::runtime::awakening::IntentAnalysis>,
    /// 工作空间上下文：默认工作目录
    workspace_default: Option<String>,
    /// 工作空间上下文：用户 HOME 目录
    workspace_user_home: Option<String>,
    /// 工作空间上下文：用户级共享区
    workspace_user_shared: Option<String>,
    /// 工作空间上下文：Agent 为该用户工作的默认落盘目录
    workspace_user_agent: Option<String>,
    /// 工作空间上下文：Agent 自身工作区
    workspace_agent: Option<String>,
    /// 工作空间上下文：当前项目协作工作区（逻辑型 Project 为 None）
    workspace_project: Option<String>,
}

impl DefaultPromptBuilder {
    /// 创建空的 Builder
    pub fn new() -> Self {
        Self::default()
    }

    /// 技能是否为神经技能（tags 含 "neural"）
    fn is_neural_skill(skill: &SkillPo) -> bool {
        skill.parse_tags().iter().any(|t| t == NEURAL_TAG)
    }

    /// 工具/技能的 tags 是否与 match_keys 有交集
    fn tags_match(tags: &[String], match_keys: &[String]) -> bool {
        tags.iter().any(|t| match_keys.contains(t))
    }

    /// 构建技能区块字符串
    fn build_skills_section(title: &str, skills: &[&SkillPo]) -> String {
        if skills.is_empty() {
            return String::new();
        }
        let mut s = format!("【{}】\n", title);
        for skill in skills {
            s.push_str(&skill.to_prompt_summary());
            s.push('\n');
        }
        s.push('\n');
        s
    }
}

/// 实现 PromptBuilder trait
impl crate::models::prompt_builder::PromptBuilder for DefaultPromptBuilder {
    fn current_trace_id(&mut self, trace_id: &str) {
        self.current_trace_id = Some(trace_id.to_string());
    }

    fn system_prompt(&mut self, agent: &Agent) {
        self.system_prompt = Some(agent.to_system_prompt());
        // 缓存匹配键：roles ∪ installed_tags
        let mut keys = agent.po.get_roles();
        keys.extend(agent.po.get_installed_tags());
        keys.sort();
        keys.dedup();
        self.match_keys = keys;
    }

    fn history(&mut self, memories: &[Memory]) {
        for memory in memories {
            if let Some(summary) = memory.to_prompt_summary() {
                self.history.push(summary);
            }
        }
    }

    fn settled_reference(&mut self, items: &[String]) {
        self.settled_reference.extend_from_slice(items);
    }

    fn compacted_context(&mut self, summary: &str) {
        self.compacted_context = Some(summary.to_string());
    }

    fn past_memories_reference(&mut self, items: &[String]) {
        self.past_memories.extend_from_slice(items);
    }

    fn current_message(&mut self, message: &Message) {
        let label = match message.po.message_type {
            common::enums::MessageType::ToolCallResult => "【工具执行结果】",
            common::enums::MessageType::ToolCallRequest => "【工具调用请求】",
            common::enums::MessageType::ConfirmRequest => "【确认请求】",
            common::enums::MessageType::ConfirmResponse => "【确认回复】",
            common::enums::MessageType::TaskAssignment => "【任务分配通知】",
            _ => "【当前消息】",
        };
        self.current_message = Some(format!("{}\n{}", label, message.to_prompt()));
    }

    fn skills(&mut self, skills: &[SkillPo]) {
        self.skills.extend_from_slice(skills);
    }

    fn tool_failures(&mut self, failures: &[(String, u64)]) {
        self.tool_failures.extend_from_slice(failures);
    }

    fn user_profile(&mut self, user: &UserPo) {
        self.user_profile = Some(user.to_basic_info_prompt());
    }

    fn project_context(&mut self, project: &crate::models::project::Project) {
        self.project_context = Some(project.to_prompt_summary());
    }

    fn task_context(&mut self, task: &crate::models::task::Task) {
        self.task_context = Some(task.to_prompt_summary());
    }

    fn workspace_context(
        &mut self,
        default_workspace: String,
        user_home: String,
        user_shared_workspace: String,
        user_agent_workspace: Option<String>,
        agent_workspace: Option<String>,
        project_workspace: Option<String>,
    ) {
        self.workspace_default = Some(default_workspace);
        self.workspace_user_home = Some(user_home);
        self.workspace_user_shared = Some(user_shared_workspace);
        self.workspace_user_agent = user_agent_workspace;
        self.workspace_agent = agent_workspace;
        self.workspace_project = project_workspace;
    }

    // ==================== 区块拼装（DefaultPromptBuilder 完整实现；其他 Builder 走 trait 默认空实现）====================

    /// 构建技能区块（神经技能 + 必加载技能）
    ///
    /// 工具列表和调用规范都不再出现在 Prompt 中：
    /// - 工具列表（name/description/parameters）通过 OpenAI tools API 协议层传递
    /// - Manual 工具调用对模型透明（awakening 层根据 control_mode 分发执行）
    ///
    /// 技能仍按 tag 分块展示在 Prompt 中（技能是方法论，无 API 对应）。
    fn build_skills_sections(&self) -> String {
        let mut result = String::new();

        // 神经技能（tags 含 "neural"，所有 Agent 必加载）
        let neural_skills: Vec<_> = self
            .skills
            .iter()
            .filter(|s| Self::is_neural_skill(s))
            .collect();
        result.push_str(&Self::build_skills_section("神经技能", &neural_skills));

        // 必加载技能（tags 不含 "neural" 但与 match_keys 有交集）
        let tagged_skills: Vec<_> = self
            .skills
            .iter()
            .filter(|s| {
                let tags = s.parse_tags();
                !tags.iter().any(|tag| tag == NEURAL_TAG)
                    && Self::tags_match(&tags, &self.match_keys)
            })
            .collect();
        result.push_str(&Self::build_skills_section("必加载技能", &tagged_skills));

        result
    }

    /// 构建通用上下文区块：用户画像 + 项目上下文 + 任务上下文
    ///
    /// 这些字段都是"有值即拼装"，唤醒和沉睡场景逻辑一致：
    /// - user_profile：认知是具身的，Agent 需知道"自己是谁"
    /// - project_context / task_context：场景化上下文，沉淀出的经验自带场景标签
    fn build_common_context_sections(&self) -> String {
        let mut s = String::new();
        if let Some(profile) = &self.user_profile {
            s.push_str("【用户画像】\n");
            s.push_str(profile);
            s.push_str("\n\n");
        }
        if let Some(project) = &self.project_context {
            s.push_str(project);
            s.push('\n');
        }
        if let Some(task) = &self.task_context {
            s.push_str(task);
            s.push('\n');
            // 记忆聚焦提示：当有任务上下文时，提示 Agent 可用 task_id 过滤记忆
            s.push_str("【记忆聚焦提示】\n");
            s.push_str("如需聚焦当前任务的记忆，可用 query_memory / search_memory 的 task_id 参数过滤；默认历史记忆是跨任务全局取最近若干条。\n\n");
        }
        if let Some(workspace_default) = &self.workspace_default {
            s.push_str("【工作空间与路径约定】\n");
            s.push_str("【默认工作目录】\n");
            s.push_str(&format!(
                "- {}（shell_exec / fs_read / fs_write 不传 working_dir 时自动使用此目录）\n\n",
                workspace_default
            ));
            s.push_str("【可用目录（全部为绝对路径）】\n");
            if let Some(user_home) = &self.workspace_user_home {
                s.push_str(&format!("- 用户 HOME 目录：{}（lark-cli/gh-cli/.gitconfig/.ssh 等自动读写处，不要直接写这里的业务文件）\n", user_home));
            }
            if let Some(user_shared) = &self.workspace_user_shared {
                s.push_str(&format!("- 用户共享区：{}（跨 Agent 协作放这里；多 Agent 接力的项目根在 {}/projects/<project_id>/）\n", user_shared, user_shared));
            }
            if let Some(user_agent) = &self.workspace_user_agent {
                s.push_str(&format!("- Agent 默认工作区：{}（无明确项目时的临时工作副本、草稿文件放这里；不要把跨 Agent 共享文件写这里）\n", user_agent));
            }
            if let Some(agent) = &self.workspace_agent {
                s.push_str(&format!("- Agent 自身工作区：{}（无用户上下文的自主行为，如定时任务/记忆沉淀，使用此目录）\n", agent));
            }
            if let Some(project) = &self.workspace_project {
                s.push_str(&format!("- 当前项目协作工作区：{}（克隆仓库、共享编辑文件、多 Agent 同时访问同一项目时优先用这里；逻辑型 Project 可忽略，用 Agent 默认工作区即可）\n", project));
            }
            s.push('\n');
            s.push_str("【fs_read / fs_write / shell_exec 的 path 参数说明】\n");
            s.push_str(&format!("- 相对路径均以 base_data_path（即 {} 的父目录链，结合默认工作目录解释）为锚点解析；建议一律传绝对路径，避免歧义\n", workspace_default));
            if self.workspace_user_agent.is_some() {
                s.push_str("- 传用户的临时请求产物 → 优先写 workspace_user_agent（若有）\n");
            }
            if self.workspace_user_shared.is_some() || self.workspace_project.is_some() {
                s.push_str("- 传跨 Agent 共享/项目级文件 → 写 workspace_user_shared 或 workspace_project（若有）\n");
            }
            s.push_str("- 不要将业务产物写入 workspace_user_home 下的配置子目录（.config/、.ssh/、.lark-cli/ 等）\n");
            s.push('\n');
        }
        s
    }

    /// 构建意图分析场景的 Prompt（Task 3：完整实现）
    ///
    /// 与 build()/build_sleep_prompt() 对称：复用 1-8 区块（人设 + 技能 + 上下文 + 历史），
    /// 再追加「意图识别 SOP 五步走 + 严格执行禁令 + JSON Schema 输出约束」的专属指令块，
    /// 最后附上当前消息作为明确靶子。
    fn build_intent_analyze_prompt(&self) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 技能区块（神经技能 + 必加载技能；调用方 analyze_input_intent 会通过
        //    scene=IntentAnalyze 的工具白名单过滤，保证 Prompt 中无执行类技能描述）
        result.push_str(&self.build_skills_sections());

        // 6-7. 通用上下文区块（用户画像 + 项目 + 任务，有值即拼装）
        result.push_str(&self.build_common_context_sections());

        // 8. 历史对话记忆（最近 N 条）
        if !self.history.is_empty() {
            result.push_str("【历史对话】\n");
            for h in &self.history {
                result.push_str(h);
                result.push('\n');
            }
            result.push('\n');
        }

        // 9. Trace ID
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }

        // ==================== 阶段一：输入理解专用指令（核心）====================
        result.push_str("### 阶段一：输入理解专用指令（仅限 IntentAnalyze 场景）\n\n");

        result.push_str("===== 【输入理解阶段】IntentAnalyze 场景约束（非常重要！）=====\n\n");
        result.push_str("## 你的任务：只做理解，不做执行\n\n");
        result.push_str("你当前处于正式干活前的「审题阶段」。本阶段你的唯一目标是产出一份结构化的理解结果，然后就结束本轮思考。\n\n");
        result.push_str("✅ 必须做：\n");
        result.push_str(
            "   1. 在思考中严格按下方「理解 SOP 五步走」执行一遍，每一步都要有实质思考，不要跳过\n",
        );
        result.push_str("   2. 必须执行多步检索：至少调用一次 search_memory + 一次 recommend_seed_nodes 或 traverse_knowledge_graph（100% 全新无历史的闲聊可豁免，但必须在思考中明确说明理由）\n");
        result.push_str("   3. 关键词联想要充分展开，联想扩展词与基础关键词一起写入 key_terms\n");
        result.push_str("   4. 最终输出严格的 JSON 对象，字段完整可被解析\n\n");
        result.push_str("❌ 严格禁止做（任何违反都将导致此阶段结果作废）：\n");
        result.push_str("   1. 严禁执行任何行动/工具调用——禁止调用 send_message / send_task_assignment_message / send_message_to_agent：不准给任何用户/Agent 发消息\n");
        result.push_str("   2. 严禁编造无来源信息——禁止调用 create_task / update_task / create_project / update_project / update_memory 状态写入类工具；不准改动任何系统状态（只有 save_short_term_memory 内部记忆写入是允许的，若你需要临时记录东西）\n");
        result.push_str("   3. 如果信息不足必须 need_clarification=true 并把澄清话术写进 resolutions——禁止做任何外部 API 调用、shell 执行、文件读写类工具；禁止直接回答用户问题（哪怕你 100% 知道答案），不准在 Final 里写对用户的回复\n\n");

        result.push_str("## 理解 SOP 五步走（在思考中严格按此顺序执行）\n\n");
        result.push_str("### Step 1：意图识别\n");
        result.push_str("在思考中先把【当前消息】归类，写出你判断的依据：\n");
        result.push_str("- Question：提问型（要信息/问进度/问规则/请教）\n");
        result.push_str("- TaskRequest：任务型（提需求/安排工作/要产出）\n");
        result.push_str("- Confirm：确认型（同意/否定/选择/拍板）\n");
        result.push_str("- FollowUp：追问型（承接之前某条回答/产出的继续追问）\n");
        result.push_str("- ClarificationResponse：澄清响应型（针对你前面追问的答复）\n");
        result.push_str("- Chat：闲聊型（打招呼/客套/社交礼貌）\n");
        result.push_str("- Mixed：混合型（多类意图，拆分说明）\n");
        result.push_str(
            "意图类型写入 intent_type 字段；置信度 0.0~1.0 自己打分写入 confidence。\n\n",
        );

        result.push_str("### Step 2：指代与上下文消歧\n");
        result.push_str("1. 仔细读【历史对话】+【项目/任务上下文】+【用户画像】\n");
        result.push_str("2. 找【当前消息】中的指代短语：这/那/上次/那个/他/按之前定的来 等\n");
        result.push_str("3. 在思考中把每个指代对应到具体对象（project_id/task_id/message_id/某个人物…），写进 resolutions 数组，每条格式：\"\\\"XXX\\\" → YYY\"\n");
        result.push_str("4. 读完所有上下文仍无法确定 → 写进 need_clarification，不要硬猜\n\n");

        result.push_str("### Step 3：关键词抽取与联想扩展\n");
        result.push_str(
            "这一步不只是提取，更重要的是联想扩展，为后续检索提供丰富的 query 基础。\n\n",
        );
        result.push_str("3.1 基础关键词抽取：\n");
        result.push_str("从【当前消息】+ 消歧后的具体对象中，抽取关键实体和核心短语：\n");
        result.push_str("- 显式实体：项目名/任务名/产品名/人名/专有名词/技术术语\n");
        result.push_str("- 隐式语义：核心动词短语（推进进度→进度查询、对比方案→方案比较）\n");
        result.push_str("- 情感倾向词：急迫/犹豫/不满/期待（影响执行优先级判断）\n\n");
        result.push_str("3.2 关键词联想扩展（在思考中展开，不要跳过）：\n");
        result.push_str("对每个基础关键词，思考它的关联概念并扩展：\n");
        result.push_str("- 同义/近义词：用户说方案A → 也搜索 proposal A / 备选方案A\n");
        result.push_str("- 上下游概念：用户说部署 → 关联测试/回滚/监控/配置变更\n");
        result.push_str("- 时间关联：用户说上次 → 思考上次是什么时候 → 搜索对应时间段的记忆\n");
        result.push_str("- 因果关联：用户说为什么失败 → 关联错误日志/最近变更/依赖状态\n\n");
        result.push_str("3.3 把基础关键词 + 联想扩展词都写进 key_terms 数组（5~12 个），\n");
        result.push_str("这些词将直接用于 Step 4 的多角度检索，越丰富检索越全面。\n\n");

        result.push_str(
            "### Step 4：多步语义检索与知识图谱关联分析（强制执行，本阶段核心价值所在）\n",
        );
        result.push_str(
            "本步直接决定后续执行阶段的信息完备性。宁可多检索一步，不要遗漏关键上下文。\n",
        );
        result.push_str("你的检索策略应该是有层次的，不是随机调工具：\n\n");
        result.push_str("4.1 短期记忆检索（search_memory）——第一轮：\n");
        result.push_str("- 用 Step 3 的核心关键词组合成 query，调用 search_memory\n");
        result
            .push_str("- 如果第一批结果不够相关，换一组关键词组合再搜一轮（不要一次不中就放弃）\n");
        result.push_str(
            "- 示例：用户问上次那个方案进度 → 先搜「方案 进度」，再搜「方案A 项目X」\n\n",
        );
        result.push_str(
            "4.2 知识图谱探索（recommend_seed_nodes + traverse_knowledge_graph）——第二轮：\n",
        );
        result.push_str(
            "- 调用 recommend_seed_nodes 获取与当前 project/task/agent 相关的图谱种子节点\n",
        );
        result
            .push_str("- 从种子节点出发，调用 traverse_knowledge_graph 走 1~2 跳，探索关联知识\n");
        result.push_str(
            "- 重点关注：用户偏好节点（user_preference tag）、历史决策节点、相关项目/任务节点\n",
        );
        result.push_str("- 知识图谱中的关系链路本身就是信息：A 依赖 B、A 衍生自 C、A 取代了 D\n\n");
        result.push_str("4.3 历史对话补充（list_messages，可选第三轮）：\n");
        result.push_str("- 如果短期记忆和知识图谱都不够，调用 list_messages 上拉最近对话记录\n");
        result.push_str("- 特别关注：用户最近提过的类似需求、Agent 之前给过的承诺或结论\n\n");
        result.push_str("4.4 检索结果整理：\n");
        result.push_str(
            "- 把所有检索命中的高相关内容**你自己概括为短摘要**（1~2 句每条，不要贴原始 JSON）\n",
        );
        result.push_str("- 每条摘要注明来源类型：[记忆]/[图谱]/[对话]\n");
        result.push_str("- 按相关度排序，最相关的放前面\n");
        result.push_str("- 写进 retrieved_context 数组\n\n");
        result.push_str("如果跳过了 4.1 或 4.2，必须在思考中明确说明理由（如：100%全新话题，无历史可检索）。\n\n");

        result.push_str("### Step 5：综合研判与总结\n");
        result.push_str("5.1 信息完备性检查：\n");
        result.push_str("- 回顾 Step 1~4 的全部产出，检查是否有信息缺口\n");
        result.push_str("- 如果消歧失败 / 混合型意图优先级不清 / 需求边界不明 / 需要用户决策\n");
        result.push_str("  → 把要问用户的具体问题逐条写进 need_clarification（问题尽量用选择题形式，不要开放式）\n");
        result.push_str("- 如果理解充分 → need_clarification = []\n\n");
        result.push_str("5.2 形成理解结论：\n");
        result.push_str("- 在思考中用 1~2 句话总结：我理解用户想要 XXX，相关的背景信息有 YYY\n");
        result.push_str("- 这个总结将直接作为下一阶段执行的输入，务必准确、完整、可执行\n");
        result.push_str("- 写进 summary 字段\n\n");

        result.push_str("## 最终输出规范（必须严格遵守）\n\n");
        result.push_str("你输出的【最终 Final 内容】必须严格符合以下格式：\n");
        result.push_str("- Final block MUST start with `--- INTENT_ANALYSIS_START ---` followed by pure JSON and end with `--- INTENT_ANALYSIS_END ---`\n");
        result
            .push_str("- 中间 JSON 对象必须严格符合以下 schema（7 个字段全包含，不要省略）：\n\n");
        result.push_str("JSON Schema 字段说明：\n");
        result.push_str("- intent_type：字符串，取值为 Question | TaskRequest | Confirm | FollowUp | ClarificationResponse | Chat | Mixed\n");
        result.push_str("- confidence：数字，0.0 到 1.0，你对自己意图判断的置信度\n");
        result.push_str("- key_terms：字符串数组，5~12 个关键词（基础抽取 + 联想扩展）\n");
        result.push_str(
            "- resolutions：字符串数组，指代消歧映射结果，每条格式 \"\\\"XXX\\\" → 具体对象\"\n",
        );
        result.push_str("- retrieved_context：字符串数组，search_memory / recommend_seed_nodes 等命中结果的你自己概括的短摘要（不要原始 JSON）\n");
        result.push_str(
            "- need_clarification：字符串数组，需要向用户澄清的具体问题（空列表 = 理解充分）\n",
        );
        result.push_str("- summary：字符串，一句话总结你最终理解的用户需求\n\n");
        result.push_str("示例 JSON（请严格模仿此结构，字段名和类型必须一致）：\n");
        result.push_str("--- INTENT_ANALYSIS_START ---\n");
        result.push_str("{\n");
        result.push_str("  \"intent_type\": \"FollowUp\",\n");
        result.push_str("  \"confidence\": 0.85,\n");
        result.push_str("  \"key_terms\": [\"项目X\", \"方案A\", \"进度\", \"上次那个方案\"],\n");
        result.push_str(
            "  \"resolutions\": [\"\\\"上次那个方案\\\" → project=proj_123, task=task_456\"],\n",
        );
        result.push_str("  \"retrieved_context\": [\"2026-08-10 短期记忆：项目X 方案 A/B 比较，推荐方案 A（相似度 0.88）\"],\n");
        result.push_str("  \"need_clarification\": [],\n");
        result.push_str(
            "  \"summary\": \"用户想知道项目 X 中之前讨论过的方案 A 的当前推进进度与结果\"\n",
        );
        result.push_str("}\n");
        result.push_str("--- INTENT_ANALYSIS_END ---\n\n");

        result.push_str("===== 【输入理解阶段】指令结束 =====\n\n");

        // 10. 当前消息（放在最后，给 Agent 明确的靶子）
        if let Some(msg) = &self.current_message {
            result.push_str("【当前消息】\n");
            result.push_str(msg);
            result.push_str("\n\n现在开始：在思考中走完 Step 1~5，然后按上面的 INTENT_ANALYSIS_START/END 锚点格式输出最终 JSON。\n");
        } else {
            result.push_str("【注意】当前消息为空。请直接输出空 JSON 或说明情况。\n");
        }

        result
    }

    /// 渲染【输入理解结果】参考区块（Task 4：完整实现 + 严格截断规则）
    ///
    /// 姿态：反复强调"仅供参考，以你当下判断为准"，避免 Agent 被前置结论带偏。
    ///
    /// 截断规则（防止 Prompt 过长超 token，CRITICAL）：
    /// - key_terms / resolutions / retrieved_context / need_clarification：
    ///   每项最多 150 字符，每个数组最多 10 项，超出 "... 及 N 项已省略"
    /// - summary：最多 800 字符
    ///
    /// 若 intent_analysis 为 None → 不渲染任何区块，返回 ""
    fn render_intent_analysis_section(&self) -> String {
        let ia = match &self.intent_analysis {
            None => return String::new(),
            Some(ia) if ia.intent_type.is_empty() && ia.summary.is_empty() => return String::new(),
            Some(ia) => ia,
        };

        let trunc_str = |s: &str, max: usize| -> String {
            let chars: Vec<char> = s.chars().collect();
            if chars.len() <= max {
                s.to_string()
            } else {
                let mut out: String = chars.into_iter().take(max).collect();
                out.push('…');
                out
            }
        };

        const MAX_ITEMS: usize = 10;
        const MAX_ITEM_CHARS: usize = 150;
        const MAX_SUMMARY_CHARS: usize = 800;

        let format_array =
            |items: &[String], prefix_icon: &str, _omit_msg: &str| -> (String, bool) {
                let mut s = String::new();
                let display = items.iter().take(MAX_ITEMS);
                let count = items.len();
                for item in display {
                    s.push_str(&format!(
                        "{} {}\n",
                        prefix_icon,
                        trunc_str(item, MAX_ITEM_CHARS)
                    ));
                }
                let omitted = count > MAX_ITEMS;
                if omitted {
                    let n = count - MAX_ITEMS;
                    s.push_str(&format!("... 及 {} 项已省略\n", n));
                }
                (s, omitted)
            };

        let need_clarify = !ia.need_clarification.is_empty();

        let mut s = String::new();
        if need_clarify {
            s.push_str("## 【输入理解结果 · 仅供参考】 ⚠️\n\n");
        } else {
            s.push_str("## 【输入理解结果 · 仅供参考】\n\n");
        }
        s.push_str("> 说明：以下内容是上一阶段「审题阶段」自动预分析得出的理解摘要，仅供你正式执行时参考。\n");
        s.push_str(
            "> 若你当下重新判断后发现不一致，请**以你当下的理解为准**，不要被以下内容束缚。\n\n",
        );

        if !ia.intent_type.is_empty() {
            s.push_str(&format!(
                "🎯 **意图类型**：{}（置信度 {:.2}%）\n\n",
                ia.intent_type,
                ia.confidence * 100.0
            ));
        }

        if !ia.key_terms.is_empty() {
            s.push_str("🔑 **关键词抽取**：\n");
            let (content, _) = format_array(&ia.key_terms, "-", "关键词");
            s.push_str(&content);
            s.push('\n');
        }

        if !ia.resolutions.is_empty() {
            s.push_str("🧩 **指代消歧结果**：\n");
            let (content, _) = format_array(&ia.resolutions, "-", "消歧结果");
            s.push_str(&content);
            s.push('\n');
        }

        if !ia.retrieved_context.is_empty() {
            s.push_str("📚 **检索补充上下文摘要**：\n");
            let (content, _) = format_array(&ia.retrieved_context, "-", "检索摘要");
            s.push_str(&content);
            s.push('\n');
        }

        if need_clarify {
            s.push_str("⚠️ **建议向用户澄清的问题**（上一阶段判断存在歧义）：\n");
            let (content, _) = format_array(&ia.need_clarification, "❓", "澄清问题");
            s.push_str(&content);
            s.push('\n');
        }

        if !ia.summary.is_empty() {
            s.push_str(&format!(
                "💡 **一句话理解总结**：{}\n\n",
                trunc_str(&ia.summary, MAX_SUMMARY_CHARS)
            ));
        }

        s.push_str("---\n\n");
        s
    }

    // ==================== P0-a：最终回复指引（注入 System 消息，防止工具调用死循环）====================

    /// 生成【回复规则指引】段落——直接拼入 System 消息末尾，
    /// 明确告诉 Agent 何时用 Final 文本直接回复，何时才用 send_message 工具。
    ///
    /// 这是解决"简单消息 → 强制检索 → 空结果 → 重试检索 → 365 轮死循环"的核心修复。
    fn build_final_response_guidance(&self) -> String {
        let mut s = String::new();
        s.push_str(
            "================================================================================\n",
        );
        s.push_str("【回复规则（必须严格遵守）】\n");
        s.push_str(
            "================================================================================\n\n",
        );

        // ---------------- §0：审题 SOP（原两阶段唤醒的 Phase 1 简化版，3 步压缩进 System）----------------
        // 旧 Phase 1 会独立跑一轮 think_loop 做结构化理解（意图归类/消歧/抽关键词/检索/完备性判断）。
        // 移除 Phase 1 后，把核心 SOP 压缩成 3 步放在回复规则最前面，模型自然会执行。
        s.push_str("§0. 先审题再回答（3 步必走，用时 <1% 注意力）：\n");
        s.push_str("   ① 理解意图 + 消歧：判断用户当前这句话在问什么/想做什么。\n");
        s.push_str(
            "      若出现「这、它、那个、上次、之前」这类代词，去【历史对话】/【上下文】区块里找对应的具体对象，\n",
        );
        s.push_str("      没找到就问用户澄清，不要猜。\n");
        s.push_str(
            "   ② 判断信息是否充足：结合人设、能力清单、上下文/历史，你自己是否能直接给出正确回答？\n",
        );
        s.push_str(
            "      若还缺资料 → 只做 1 次检索（见 §4）；若够了 → 直接跳到第三步输出回复。\n",
        );
        s.push_str(
            "   ③ 直接输出：把最终回复文本按正常说话方式写出，不要包任何 JSON、不要用工具。\n\n",
        );

        s.push_str("§1. 何时直接回复用户（核心规则！）：\n");
        s.push_str("   - 当你有足够信息回答当前用户的问题/消息时，**直接输出最终文本回复即可，不要调用任何工具**。\n");
        s.push_str(
            "   - 你的最终文本会自动发送给当前对话中的用户，**不需要**调用 send_message 工具。\n\n",
        );

        s.push_str("§2. send_message 的正确用途（不要滥用！）：\n");
        s.push_str("   - ✅ 仅用于：向「不在当前对话中的用户/Agent」发送异步通知\n");
        s.push_str("   - ✅ 仅用于：跨 Agent 协作时的特殊消息通道（当前对话外的人）\n");
        s.push_str("   - ❌ 严禁：用 send_message 回复当前用户 → 请用最终文本直接回复\n\n");

        s.push_str("§3. 闲聊/简单消息的检索豁免：\n");
        s.push_str("   - 寒暄/客套/问候/纯确认类消息（如 你好、测试、OK、收到、在吗 等）属于 Chat 闲聊型\n");
        s.push_str(
            "   - Chat 型消息 **不需要强制性检索**，可跳过 search_memory / query_memory 等工具\n",
        );
        s.push_str("   - 直接给出友好自然的回复即可\n\n");

        s.push_str("§4. 检索工具空结果时的处理（防死循环！）：\n");
        s.push_str("   - 调用 search_memory / query_memory 返回空结果，意味着系统暂无相关知识\n");
        s.push_str("   - ❌ 不要反复换参数、换工具重试（同一语义的检索只需 1 次即可）\n");
        s.push_str("   - ❌ 若已检索多次（≥2 次）都没有找到有用信息，在没有出现新的有用输入\n");
        s.push_str("     （用户补充了说明、或其他工具返回了新线索）之前，**不要再搜索**。\n");
        s.push_str("     继续搜索不会有新发现，只会浪费轮次。已有信息够多少就先用多少。\n");
        s.push_str(
            "   - ✅ 空结果后直接基于已有信息答复用户；确实不知道就坦诚说「没有相关记录」\n\n",
        );

        s.push_str("§5. 禁止无意义工具调用（假忙）：\n");
        s.push_str("   - 自问：如果没有任何工具，我能不能直接回答？\n");
        s.push_str("   - 若能 → 直接用 Final 文本回复，不要「为了调用工具而调用工具」\n\n");

        s.push_str(
            "================================================================================\n\n",
        );
        s
    }

    // ==================== 角色拆分辅助方法（System / User 双消息）====================

    /// 渲染【近期已沉淀记忆】参考区块（沉淀场景专用）
    ///
    /// 这些条目**已完成沉淀**，只作为延续/补充关系的参考线索，不是待处理对象：
    /// 主要价值是衔接上次沉淀被预算截断的情况，让 Agent 能看到「上一段沉淀到哪了」。
    /// 需要更完整的上下文时，Agent 应自行 `search_memory` 检索。
    fn push_settled_reference(&self, out: &mut String) {
        if self.settled_reference.is_empty() {
            return;
        }
        out.push_str(&format!(
            "【近期已沉淀记忆（仅供参考）】\n以下是最近 {} 条**已完成沉淀**的短期记忆，\
             供你判断是否存在延续或补充关系。\n它们无需再次处理；需要更多上下文时用 \
             `search_memory` 自行检索。\n\n",
            self.settled_reference.len()
        ));
        for item in &self.settled_reference {
            out.push_str("- ");
            out.push_str(item);
            out.push('\n');
        }
        out.push('\n');
    }

    /// Awaken 场景的 System 部分：人设 + 技能方法论 + 回复规则指引
    fn awaken_system_part(&self) -> String {
        let mut s = String::new();
        if let Some(system) = &self.system_prompt {
            s.push_str(system);
            s.push_str("\n\n");
        }
        s.push_str(&self.build_skills_sections());
        s.push_str(&self.build_final_response_guidance());
        s
    }

    /// Awaken 场景的 User 部分：会话上下文 + 历史 + 当前消息
    fn awaken_user_part(&self) -> String {
        let mut s = String::new();
        s.push_str(&self.build_common_context_sections());
        // 压缩结果与【历史对话】互斥：有压缩结果时调用方不会装配 history
        // （内容都已在摘要里，再查一遍纯属重复占用预算）
        if let Some(summary) = &self.compacted_context {
            s.push_str("【上一轮工作压缩结果】\n");
            s.push_str(
                "以下是你在上一轮思考中完成的工作的压缩摘要。\
                 它已包含你之前的所有进展，直接在此基础上继续即可，\
                 无需再回顾其它历史记忆。\n\
                 如需更早期的记忆，用 `search_memory` 自行检索。\n\n",
            );
            s.push_str(summary.trim());
            s.push_str("\n\n");
        }
        // 压缩后补少量「更早的记忆」：只作连续性线索，必须说清是过去的、
        // 不是当前待办，避免模型把它们当成要继续处理的事情
        if !self.past_memories.is_empty() {
            s.push_str(&format!(
                "【更早的记忆（仅供参考，非当前工作）】\n\
                 以下是**更早之前**的 {} 条记忆，属于历史背景，**不是**你当前要处理的事情，\n\
                 也**不要**据此重复已经做过的动作。仅当你需要连续性线索时参考。\n\
                 如需更多，请用 `search_memory` 检索。\n\n",
                self.past_memories.len()
            ));
            for item in &self.past_memories {
                s.push_str("- ");
                s.push_str(item);
                s.push('\n');
            }
            s.push('\n');
        }
        if !self.history.is_empty() {
            s.push_str("【历史对话】\n");
            for h in &self.history {
                s.push_str(h);
                s.push('\n');
            }
            s.push('\n');
        }
        if !self.tool_failures.is_empty() {
            s.push_str("【工具失败警告】\n");
            s.push_str("以下工具近期失败次数较多，请谨慎使用或考虑替代方案：\n");
            for (tool_name, fail_count) in &self.tool_failures {
                s.push_str(&format!("- {}：失败 {} 次\n", tool_name, fail_count));
            }
            s.push('\n');
        }
        if let Some(trace_id) = &self.current_trace_id {
            s.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }
        let intent_section = self.render_intent_analysis_section();
        if !intent_section.is_empty() {
            s.push_str(&intent_section);
        }
        if let Some(msg) = &self.current_message {
            s.push_str(msg);
            s.push_str("\n\n请回复：");
        }
        s
    }

    fn build(&self) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 技能区块（神经技能/必加载技能）
        // 工具列表和调用规范都不在 Prompt 中（工具通过 API 协议层传递，调用对模型透明）
        result.push_str(&self.build_skills_sections());

        // 6-7. 通用上下文区块（用户画像 + 项目上下文 + 任务上下文，有值即拼装）
        result.push_str(&self.build_common_context_sections());

        // 8. 历史对话记忆
        if !self.history.is_empty() {
            result.push_str("【历史对话】\n");
            for h in &self.history {
                result.push_str(h);
                result.push('\n');
            }
            result.push('\n');
        }

        // 9. 工具失败警告（有失败工具时才显示）
        if !self.tool_failures.is_empty() {
            result.push_str("【工具失败警告】\n");
            result.push_str("以下工具近期失败次数较多，请谨慎使用或考虑替代方案：\n");
            for (tool_name, fail_count) in &self.tool_failures {
                result.push_str(&format!("- {}：失败 {} 次\n", tool_name, fail_count));
            }
            result.push('\n');
        }

        // 10. 本次思考的 Trace ID
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }

        // 10.5 【输入理解结果】区块（Phase 1 IntentAnalyze 阶段产出，Task 4：A+ P3 串联）
        // 位置：严格在 Trace ID 之后、当前消息之前；若 intent_analysis 为 None 则无输出
        let intent_section = self.render_intent_analysis_section();
        if !intent_section.is_empty() {
            result.push_str(&intent_section);
        }

        // 11. 当前用户消息
        if let Some(msg) = &self.current_message {
            result.push_str(msg);
            result.push_str("\n\n请回复：");
        }

        result
    }

    fn build_sleep_prompt(&self, pending_memories_summary: &str, trace_ids: &[String]) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 技能区块（sleep_and_settle 调用前已过滤只保留记忆相关）
        result.push_str(&self.build_skills_sections());

        // 6-7. 通用上下文区块（用户画像 + 项目上下文 + 任务上下文）
        // 认知是具身的 → 保留 user_profile
        // 场景化沉淀 → 保留 project/task_context，沉淀出的经验自带场景标签
        result.push_str(&self.build_common_context_sections());

        // 8. 近期已沉淀记忆（少量参考，不是【历史对话】全量）
        //
        // 沉淀场景**不放**【历史对话】：它与下面的【待沉淀的短期记忆】大面积重复
        // （Active ≤ 20 条时完全重复），白白吃掉上下文预算。
        // 图谱里已有什么、曾经沉淀过什么，技能已要求 Agent 用 search_memory 按需检索，
        // 这里只给少量「顺手可见」的线索，用于衔接上次被截断的沉淀。
        //
        // 注意：沉淀场景不使用 current_message —— 睡觉是对自身记忆的整理，
        // 没有「触发本次沉淀的用户消息」。主循环上下文压缩走 compact_context()，
        // 它直接复用主循环的完整对话，也不需要这个区块。
        self.push_settled_reference(&mut result);

        // 9. Trace ID
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }

        // 10. 沉淀约束 + 待沉淀记忆 + 任务步骤（模板内聚在 builder）
        // 跳过 tool_failures（沉淀不调外部工具）
        result.push_str("【沉淀工作模式触发】\n\n");
        result.push_str("你收到这个消息是因为触发了沉淀流程（类似人脑的睡眠整理记忆）。请进入沉淀工作模式，对以下未沉淀的短期记忆进行归纳整理：\n\n");
        result.push_str(&format!(
            "## 待沉淀的短期记忆\n{}\n\n",
            pending_memories_summary
        ));
        result.push_str("## 沉淀约束（重要）\n\n");
        result.push_str("- **不要发送消息**：睡觉是对自身知识的沉淀积累，不应依赖外部信息\n");
        result.push_str("- **不要调用消息类工具**（send_message / send_task_assignment_message 等），避免触发消息流程导致异步唤醒自己\n");
        result.push_str("- **只使用记忆类工具**：search_memory / save_long_term_memory / update_memory / query_memory / save_short_term_memory\n");
        result.push_str("- 这是一个内循环：你与自己的记忆对话，不是与外部世界交互\n\n");
        result.push_str("## 你的任务\n\n");
        result.push_str("请用已有工具自主完成沉淀：\n\n");
        result.push_str("1. **归纳总结**：对上述短期记忆进行归纳，提炼核心概念、抽象经验、可复用模式（不要记具体细节）\n");
        result.push_str(
            "2. **查询已有图谱**：用 search_memory 检查是否已有相关知识点（避免重复节点）\n",
        );
        result.push_str("3. **创建/更新节点**：\n");
        result.push_str("   - 新知识 → save_long_term_memory 创建节点\n");
        result.push_str("   - 已有相似节点 → update_memory 更新节点内容\n");
        result.push_str("   - 过大且可拆分的旧节点 → 拆分为子节点 + 概述父节点 + contains 关系\n");
        result.push_str("4. **建立关系**：用 save_long_term_memory 的 relations 参数建立节点间关系（related/contains/depends 等）\n");
        result.push_str("5. **评估共享**：判断哪些节点对蜂巢有共享价值，用 update_memory 的 node_tags 字段加 'published' 标签\n");
        // 状态闭环由框架负责：不再要求 Agent 自己改 status
        //
        // 背景：原步骤 6 要求 Agent 调 update_memory 把短期记忆 status 改为 settled，
        // 但模型经常漏调，导致同一批记忆在下次沉淀时被反复处理。现改为沉淀结束后
        // 由框架统一置位（见 MemoryDal::mark_short_term_settled）。
        //
        // 注意：技能文档里的同类说明**不改**——技能可能被其它框架复用，那里未必有
        // 框架兜底，仍然需要 Agent 自行处理状态；只有在 AI Orz 这套沉淀 prompt 下
        // 才由框架接管。
        result.push_str(
            "6. **状态无需处理**：本次待沉淀短期记忆的状态变更由框架在沉淀结束后统一完成，\n",
        );
        result.push_str(
            "   **你不要**调用 update_memory 去修改它们的 status（重复操作没有意义）。\n",
        );
        result.push_str("   你只需专注把内容整合进知识图谱；若某条确实不适合沉淀，忽略它即可。\n");
        result.push_str("7. **强制写入沉淀摘要**（必须执行）：沉淀完成后，**必须**调用 save_short_term_memory 将本次沉淀的摘要写入短期记忆，参数要求：\n");
        result.push_str("   - `summary`：本次沉淀提炼的核心经验摘要（不是细节流水账）\n");
        result.push_str("   - `content`：详细内容（可选，记录沉淀出的关键知识点列表）\n");
        result.push_str("   - `tags`：标签列表（如 `[\"settled\", \"consolidation\"]`）\n");
        result.push_str(&format!(
            "   - `trace_ids`：**必须填入** `[{}]`（本次沉淀依赖的 trace 列表，用于记忆追溯）\n\n",
            trace_ids
                .iter()
                .map(|t| format!("\"{}\"", t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        result.push_str("## 认知要点\n\n");
        result.push_str("- 图谱是活的，每次沉淀都是迭代优化，不是机械合并\n");
        result.push_str("- 记抽象不记细节，可复用模式才沉淀\n");
        result.push_str("- 新老知识交替不是覆盖是迭代，推翻时用 opposite 关系保留痕迹\n");
        result.push_str(
            "- published 标签让节点全局共享，通过共享节点作为桥梁发现跨 Agent 的知识网络\n",
        );
        result.push_str("- 详见\"记忆认知\"技能的沉淀机制和新老知识交替章节\n\n");
        result.push_str("开始沉淀吧。");

        result
    }

    fn build_summary_prompt(
        &self,
        work_summary: &str,
        total_rounds: usize,
        trace_ids: &[String],
    ) -> String {
        let mut result = String::new();

        // 1. System Prompt（Agent 人设）
        if let Some(system) = &self.system_prompt {
            result.push_str(system);
            result.push_str("\n\n");
        }

        // 2-5. 技能区块（Summary 场景已过滤，只保留 neural/memory/messaging/project_management）
        result.push_str(&self.build_skills_sections());

        // 6-7. 通用上下文区块（保留 project/task_context 帮助 Agent 理解任务背景）
        result.push_str(&self.build_common_context_sections());

        // 8. 历史对话记忆
        if !self.history.is_empty() {
            result.push_str("【历史对话】\n");
            for h in &self.history {
                result.push_str(h);
                result.push('\n');
            }
            result.push('\n');
        }

        // 9. Trace ID
        if let Some(trace_id) = &self.current_trace_id {
            result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }

        // 10. 总结退出指令
        result.push_str("【总结退出模式触发】\n\n");
        result.push_str(&format!(
            "你已连续思考 {} 轮仍未完成任务，现在需要总结当前工作进展并退出。\n\n",
            total_rounds
        ));
        result.push_str("## 当前工作对话摘要\n\n");
        result.push_str(work_summary);
        result.push_str("\n\n");
        result.push_str("## 你的任务\n\n");
        result.push_str("1. **总结进展**：梳理当前已完成的工作、取得的阶段性成果\n");
        result.push_str("2. **记录问题**：列出未解决的问题、遇到的障碍、下一步建议\n");
        result.push_str("3. **发送通知**：\n");
        result.push_str("   - 如果有消息源（用户/Agent），用 send_message 将总结发送给对方\n");
        result.push_str("   - 如果关联了 task，用 update_task_progress 更新任务进度和状态\n");
        result.push_str("4. **强制写入短期记忆**（必须执行）：总结完成后，**必须**调用 save_short_term_memory 将本次工作总结写入短期记忆，参数要求：\n");
        result.push_str("   - `summary`：本次工作总结摘要（核心进展 + 问题 + 下一步）\n");
        result.push_str("   - `content`：详细内容（可选，记录完整总结）\n");
        result.push_str("   - `tags`：标签列表（如 `[\"work_summary\", \"max_rounds\"]`）\n");
        result.push_str(&format!(
            "   - `trace_ids`：**必须填入** `[{}]`（本次总结依赖的 trace 列表，用于记忆追溯）\n",
            trace_ids
                .iter()
                .map(|t| format!("\"{}\"", t))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        result.push_str("5. **保持简洁**：总结应聚焦关键信息，避免冗长\n\n");
        result.push_str("## 约束\n\n");
        result.push_str("- 这是退出流程，完成总结后直接回复最终文本即可\n");
        result.push_str("- 不要尝试继续执行原任务，聚焦于总结和通知\n");
        result.push_str("- 如果无法发送消息（无目标），直接输出总结文本\n");
        result.push_str("- save_short_term_memory 是必须执行的操作，不要遗漏\n\n");
        result.push_str("开始总结吧。");

        result
    }

    fn intent_analysis(
        &mut self,
        analysis: &crate::service::domain::runtime::awakening::IntentAnalysis,
    ) {
        self.intent_analysis = Some(analysis.clone());
    }

    // ==================== 角色分离版初始消息（覆盖 trait 默认实现）====================

    /// Awaken 场景：真正的 System + User 角色拆分
    /// System = 人设 + 技能方法论 + 【最终回复指引】（P0 核心修复）
    /// User   = 用户画像/项目/任务 + 历史对话 + 警告 + Trace ID + 意图理解 + 当前消息
    fn build_initial_messages(&self) -> Vec<ChatMessage> {
        let system = self.awaken_system_part();
        let user = self.awaken_user_part();
        vec![ChatMessage::system(system), ChatMessage::user(user)]
    }

    /// 沉淀场景：System（人设+技能+沉淀规范简版指引）
    /// + User（上下文+历史+本轮用户原始消息+待沉淀摘要+Trace）
    fn build_sleep_initial_messages(
        &self,
        pending_memories_summary: &str,
        trace_ids: &[String],
    ) -> Vec<ChatMessage> {
        let mut system = String::new();
        if let Some(s) = &self.system_prompt {
            system.push_str(s);
            system.push_str("\n\n");
        }
        system.push_str(&self.build_skills_sections());
        system.push_str("【沉淀场景补充规则】\n");
        system.push_str("- 你的任务是：调用 save_short_term_memory 把对话摘要写入短期记忆\n");
        system.push_str(
            "- 不需要给用户发消息；写完记忆后直接输出「沉淀完成」类 Final 文本结束即可\n",
        );
        system.push_str("- 同一语义的写入工具不要重复调用超过 1 次\n\n");

        let mut user = String::new();
        user.push_str(&self.build_common_context_sections());
        // 沉淀场景**不放**【历史对话】：它与【待沉淀的短期记忆摘要】大面积重复
        // （Active ≤ 20 条时完全重复），白白吃掉上下文预算。
        // 图谱里已有什么由 Agent 用 search_memory 按需检索，这里只给少量参考线索。
        self.push_settled_reference(&mut user);
        if let Some(trace_id) = &self.current_trace_id {
            user.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }
        user.push_str("【待沉淀的短期记忆摘要】\n");
        user.push_str(pending_memories_summary);
        user.push('\n');
        if !trace_ids.is_empty() {
            user.push_str(
                "【依赖的 Trace ID 列表】（调用 save_short_term_memory 时填入 trace_ids 字段）\n",
            );
            for tid in trace_ids {
                user.push_str(&format!("- {}\n", tid));
            }
            user.push('\n');
        }
        user.push_str("请开始沉淀工作：");

        vec![ChatMessage::system(system), ChatMessage::user(user)]
    }

    /// 总结场景：System（人设+技能+总结规范简版指引）+ User（上下文+摘要+轮次+Trace）
    fn build_summary_initial_messages(
        &self,
        work_summary: &str,
        total_rounds: usize,
        trace_ids: &[String],
    ) -> Vec<ChatMessage> {
        let mut system = String::new();
        if let Some(s) = &self.system_prompt {
            system.push_str(s);
            system.push_str("\n\n");
        }
        system.push_str(&self.build_skills_sections());
        system.push_str("【总结场景补充规则】\n");
        system.push_str("- 总结工作后直接输出 Final 文本，不要无意义循环调工具\n");
        system.push_str("- save_short_term_memory 必须调用 1 次即可，不要多次重复\n");
        system.push_str("- 如果确实无目标可发送消息，用 Final 文本写出总结内容即可结束\n\n");

        let mut user = String::new();
        user.push_str(&self.build_common_context_sections());
        if !self.history.is_empty() {
            user.push_str("【历史对话】\n");
            for h in &self.history {
                user.push_str(h);
                user.push('\n');
            }
            user.push('\n');
        }
        if let Some(trace_id) = &self.current_trace_id {
            user.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }
        user.push_str(&format!(
            "【累计思考轮次】本次工作共消耗 {} 轮思考，现在进入总结阶段\n\n",
            total_rounds
        ));
        user.push_str("【本次工作对话摘要】\n");
        user.push_str(work_summary);
        user.push('\n');
        if !trace_ids.is_empty() {
            user.push_str("【依赖的 Trace ID 列表】（调用 save_short_term_memory 时填入）\n");
            for tid in trace_ids {
                user.push_str(&format!("- {}\n", tid));
            }
            user.push('\n');
        }
        user.push_str("请执行总结流程：");

        vec![ChatMessage::system(system), ChatMessage::user(user)]
    }

    /// 意图分析场景：System（人设+技能+理解规范简版指引+原始专用指令块核心）
    /// + User（上下文+历史+Trace+当前消息靶子）
    fn build_intent_analyze_initial_messages(&self) -> Vec<ChatMessage> {
        let mut system = String::new();
        if let Some(s) = &self.system_prompt {
            system.push_str(s);
            system.push_str("\n\n");
        }
        system.push_str(&self.build_skills_sections());
        // 追加意图分析专属指令（直接复用 build_intent_analyze_prompt 的指令部分——从
        // "阶段一：输入理解专用指令"这一段锚点之后的内容，通过完整 build 再切片不优雅，
        // 直接重新渲染指令块）。
        system.push_str("### 阶段一：输入理解专用指令（仅限 IntentAnalyze 场景）\n\n");
        system.push_str("===== 【输入理解阶段】IntentAnalyze 场景约束（非常重要！）=====\n\n");
        system.push_str("## 你的任务：只做理解，不做执行\n\n");
        system.push_str("你当前处于正式干活前的「审题阶段」。本阶段你的唯一目标是产出一份结构化的理解结果（JSON），然后就结束本轮思考。\n\n");
        system.push_str("✅ 必须做：\n");
        system.push_str("   1. 按「理解 SOP 五步走」的方法理解当前消息\n");
        system.push_str("   2. 需要检索时调用 search_memory / recommend_seed_nodes / traverse_knowledge_graph（纯闲聊 Chat 型且无历史时可直接豁免，不必为了检索而检索）\n");
        system.push_str("   3. 最终输出严格的 JSON 对象（含 intent_type / confidence / summary / key_terms / resolutions / need_clarification / suggested_tools 等字段），不要附加无关废话\n\n");
        system.push_str("❌ 严格禁止：\n");
        system.push_str("   1. 严禁给任何用户/Agent 发消息（禁止 send_message 类工具）\n");
        system.push_str("   2. 严禁修改系统状态（禁止 create_task / update_project / update_memory 等写入类工具）\n");
        system.push_str("   3. 信息不足时设置 need_clarification=true，不要硬猜答案\n");
        system.push_str("   4. 同一语义的检索只需 1 次即可，不要空结果后反复换参数重试\n\n");

        let mut user = String::new();
        user.push_str(&self.build_common_context_sections());
        if !self.history.is_empty() {
            user.push_str("【历史对话】\n");
            for h in &self.history {
                user.push_str(h);
                user.push('\n');
            }
            user.push('\n');
        }
        if let Some(trace_id) = &self.current_trace_id {
            user.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
        }
        if let Some(msg) = &self.current_message {
            user.push_str("【当前消息（作为理解靶子）】\n");
            user.push_str(msg);
            user.push('\n');
        }
        user.push_str("请开始理解并输出最终 JSON：");

        vec![ChatMessage::system(system), ChatMessage::user(user)]
    }
}
