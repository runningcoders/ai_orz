# awaken/sleep 注入上下文 + 沉淀约束 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task.

**Goal:** (1) 新增统一的 `ThinkingOptions` 结构体，awaken 和 sleep_and_settle 都接收此参数，支持注入 project/task 等业务上下文；(2) awaken 流程中，当 message 携带 project_id/task_id 时，查询实体并注入到 prompt；(3) sleep_and_settle 流程中，只加载记忆相关 skill，沉淀 prompt 明确约束"不发消息、不依赖外部信息"。

**Architecture:**
1. 新增 `ThinkingOptions` 结构体（定义在 `src/service/domain/runtime/awakening.rs`），含 `scene: ThinkingScene` + project/task/user_profile 等可选字段，builder 模式构造
2. 新增 `ThinkingScene` 枚举（`Awaken` / `Settle`），用于区分唤醒和沉睡场景
3. `wake_agent_brain` 方法签名增加 `scene: ThinkingScene` 参数，Settle 场景下过滤 Auto 工具（只保留 tags 含 neural 或 memory 的工具注册到 Rig）
4. awaken 和 sleep_and_settle 方法签名增加 `options: &ThinkingOptions` 参数
5. PromptBuilder trait 新增 `project_context` / `task_context` 方法，build 时拼装到【历史对话】之前作为【业务上下文】区块
6. awaken 实现从 options 读取 project/task，传给 builder
7. MessageConsumer 在调用 awaken 前查询 project/task 实体，构造 ThinkingOptions
8. sleep_and_settle 中过滤 Manual 工具（Prompt 展示）和 skill_pos，只保留 tags 含 "neural" 或 "memory" 的
9. PromptBuilder 增加 `build_sleep_prompt` 方法，复用已挂载的 system_prompt/tools/skills/history，拼装沉淀约束章节 + 待沉淀记忆摘要生成最终模板（与 build() 对称，不在 settle_memory.rs 里 format! 完整模板）
10. sleep_and_settle 签名由 `settle_prompt: &str` 改为 `pending_memories_summary: &str`，内部调用 `builder.build_sleep_prompt()`，不再构造虚拟 System Message
11. settle_memory.rs 的 `build_settle_prompt` 简化为 `build_pending_memories_summary`，只查询记忆并生成摘要（约束模板移入 builder）

**工具过滤双层机制**（Settle 场景）：
- **Auto 工具过滤**（wake_agent_brain 中）：只注册记忆相关 Auto 工具到 Rig，避免模型通过 function calling 调用消息类工具
- **Manual 工具过滤**（sleep_and_settle 中）：Prompt 只展示记忆相关 Manual 工具，避免模型手动调用消息类工具
- 两层都过滤，确保 Agent 在沉淀模式下只能接触记忆类工具

---

## Task 1: 新增 ThinkingOptions + PromptBuilder 扩展

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`（新增 ThinkingOptions 结构体）
- Modify: `src/models/prompt_builder.rs`（trait 新增 project_context / task_context 方法）
- Modify: `src/service/dal/agent.rs`（DefaultPromptBuilder 实现 + build 拼装）
- Modify: `src/models/project.rs`（Project 增加 to_prompt_summary）
- Modify: `src/models/task.rs`（Task 增加 to_prompt_summary）

- [ ] **Step 1: 新增 ThinkingScene 枚举 + ThinkingOptions 结构体**

在 `src/service/domain/runtime/awakening.rs` 文件顶部（trait 定义之前）增加：

```rust
/// 思考场景类型
///
/// 用于区分唤醒（awaken）和沉睡（sleep_and_settle）两种场景，
/// wake_agent_brain 根据场景过滤注册到 Rig 的 Auto 工具，
/// sleep_and_settle 根据场景过滤 Prompt 展示的 Manual 工具和 skill。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingScene {
    /// 唤醒场景：响应外部消息，加载全部工具
    #[default]
    Awaken,
    /// 沉睡场景：沉淀记忆，只加载记忆相关工具（neural/memory tag）
    Settle,
}

impl ThinkingScene {
    /// 判断工具是否在此场景可用
    ///
    /// Awaken 场景：全部可用
    /// Settle 场景：只有 tags 含 "neural" 或 "memory" 的工具可用
    pub fn is_tool_allowed(&self, tags: &[String]) -> bool {
        match self {
            ThinkingScene::Awaken => true,
            ThinkingScene::Settle => tags.iter().any(|t| t == "neural" || t == "memory"),
        }
    }
}

/// 唤醒/沉睡的统一选项
///
/// 用于在不同场景传递业务上下文和场景标识，避免频繁修改方法签名。
/// awaken 和 sleep_and_settle 都接收此结构体，wake_agent_brain 接收 scene 字段。
///
/// # 字段说明
/// - `scene`：场景标识（Awaken/Settle），决定工具过滤行为
/// - `project` / `task`：awaken 场景下，消息关联的项目/任务实体，注入 prompt 作为业务上下文
/// - `user_profile`：用户画像（预留，未来扩展）
///
/// # 构造方式
/// ```rust, ignore
/// let options = ThinkingOptions::new()
///     .with_project(project)
///     .with_task(task);
/// // 沉睡场景
/// let options = ThinkingOptions::for_scene(ThinkingScene::Settle);
/// ```
#[derive(Debug, Clone, Default)]
pub struct ThinkingOptions {
    /// 场景标识
    pub scene: ThinkingScene,
    /// 消息关联的项目实体（awaken 场景使用）
    pub project: Option<crate::models::project::Project>,
    /// 消息关联的任务实体（awaken 场景使用）
    pub task: Option<crate::models::task::Task>,
    /// 用户画像（预留，未来扩展）
    pub user_profile: Option<crate::models::user::UserPo>,
}

impl ThinkingOptions {
    /// 创建唤醒场景的选项
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建指定场景的选项
    pub fn for_scene(scene: ThinkingScene) -> Self {
        Self {
            scene,
            ..Default::default()
        }
    }

    /// 设置项目上下文
    pub fn with_project(mut self, project: crate::models::project::Project) -> Self {
        self.project = Some(project);
        self
    }

    /// 设置任务上下文
    pub fn with_task(mut self, task: crate::models::task::Task) -> Self {
        self.task = Some(task);
        self
    }
}
```

- [ ] **Step 2: Project 增加 to_prompt_summary 方法**

在 `src/models/project.rs` 的 `impl Project` 块中增加（ProjectPo 字段：id, name, description, workflow, guidance, status, priority, tags, root_user_id, owner_agent_id 等）：

```rust
/// 生成 Prompt 用的摘要字符串
pub fn to_prompt_summary(&self) -> String {
    let mut s = String::from("【项目上下文】\n");
    s.push_str(&format!("- 项目ID: {}\n", self.po.id));
    s.push_str(&format!("- 项目名称: {}\n", self.po.name));
    if !self.po.description.is_empty() {
        s.push_str(&format!("- 项目描述: {}\n", self.po.description));
    }
    s.push_str(&format!("- 项目状态: {:?}\n", self.po.status));
    if let Some(owner_agent_id) = &self.po.owner_agent_id {
        s.push_str(&format!("- 负责Agent: {}\n", owner_agent_id));
    }
    if let Some(workflow) = &self.po.workflow {
        s.push_str(&format!("- 运作流程: {}\n", workflow));
    }
    if let Some(guidance) = &self.po.guidance {
        s.push_str(&format!("- 指导建议: {}\n", guidance));
    }
    s
}
```

- [ ] **Step 3: Task 增加 to_prompt_summary 方法**

在 `src/models/task.rs` 的 `impl Task` 块中增加（TaskPo 字段：id, title, description, status, priority, assignee_type, assignee_id, project_id, progress 等）：

```rust
/// 生成 Prompt 用的摘要字符串
pub fn to_prompt_summary(&self) -> String {
    let mut s = String::from("【任务上下文】\n");
    s.push_str(&format!("- 任务ID: {}\n", self.po.id));
    s.push_str(&format!("- 任务标题: {}\n", self.po.title));
    if !self.po.description.is_empty() {
        s.push_str(&format!("- 任务描述: {}\n", self.po.description));
    }
    s.push_str(&format!("- 任务状态: {:?}\n", self.po.status));
    s.push_str(&format!("- 分配给: {:?}({})\n", self.po.assignee_type, self.po.assignee_id));
    s.push_str(&format!("- 任务进度: {}%\n", self.po.progress));
    s
}
```

- [ ] **Step 4: PromptBuilder trait 新增方法**

在 `src/models/prompt_builder.rs` 的 trait 定义中，在 `user_profile` 方法之后增加：

```rust
/// 设置项目上下文（消息关联的项目实体摘要）
fn project_context(&mut self, project: &crate::models::project::Project);

/// 设置任务上下文（消息关联的任务实体摘要）
fn task_context(&mut self, task: &crate::models::task::Task);

/// 构建沉淀场景的 Prompt（与 build() 对称）
///
/// 复用已挂载的 system_prompt/tools/skills/history，加上沉淀约束章节
/// （不发消息、只用记忆工具）和待沉淀短期记忆摘要，生成最终模板。
/// 不使用 current_message（沉淀场景无用户消息）。
///
/// 默认实现回退到 build()，仅 DefaultPromptBuilder 真正实现沉淀语义
/// （Cli/Remote Agent 不参与沉淀，不会走到此分支）。
fn build_sleep_prompt(&self, pending_memories_summary: &str) -> String {
    let _ = pending_memories_summary;
    self.build()
}
```

- [ ] **Step 5: DefaultPromptBuilder 实现 + build 拼装**

在 `src/service/dal/agent.rs` 的 DefaultPromptBuilder：

1. 结构体增加字段：
```rust
pub struct DefaultPromptBuilder {
    // ... 现有字段 ...
    project_context: Option<String>,
    task_context: Option<String>,
}
```

2. `new()` 方法初始化：`project_context: None, task_context: None`

3. 实现 trait 方法：
```rust
fn project_context(&mut self, project: &crate::models::project::Project) {
    self.project_context = Some(project.to_prompt_summary());
}

fn task_context(&mut self, task: &crate::models::task::Task) {
    self.task_context = Some(task.to_prompt_summary());
}
```

4. 提取公共方法 `build_tools_and_skills_sections(&self) -> String`，把 build() 中【神经工具】【神经技能】【常用工具】【必加载技能】四个区块的拼装逻辑抽出来，供 build() 和 build_sleep_prompt() 复用，避免重复。

5. build() 重构：调用 `build_tools_and_skills_sections()` 替换内联的工具/技能区块；把原【用户画像】区块 + 新增的【项目上下文】/【任务上下文】拼装替换为调用 `build_common_context_sections()`（见第 7 步）。这样 build() 中不再有 user_profile/project_context/task_context 的内联 if-let，统一走公共方法。

6. 实现 `build_sleep_prompt(&self, pending_memories_summary: &str) -> String`：
   - 复用 system_prompt + build_tools_and_skills_sections + user_profile + project_context + task_context + history（与 build() 共用所有"有值即拼装"的上下文区块，保证沉淀场景也能感知业务上下文）
   - **保留** user_profile（认知是具身的，Agent 需要知道自己是谁才能形成有效沉淀）
   - **保留** project_context / task_context（场景化沉淀：在什么项目/任务下总结出的经验，沉淀出的知识自带场景标签；仅在 options 携带时才有值，无值不拼装，与 build() 通用逻辑一致）
   - **跳过** tool_failures（沉淀不调外部工具，无失败统计）
   - **跳过** current_message（沉淀无用户消息）
   - 拼装沉淀约束章节 + 待沉淀记忆摘要 + 任务步骤 + 认知要点（模板内聚在 builder，不再散落在 settle_memory.rs）

```rust
fn build_sleep_prompt(&self, pending_memories_summary: &str) -> String {
    let mut result = String::new();

    // 1. System Prompt（Agent 人设）
    if let Some(system) = &self.system_prompt {
        result.push_str(system);
        result.push_str("\n\n");
    }

    // 2. 工具/技能区块（复用 build 的分块逻辑，sleep_and_settle 调用前已过滤只保留记忆相关）
    result.push_str(&self.build_tools_and_skills_sections());

    // 3. 通用上下文区块（用户画像 + 项目上下文 + 任务上下文，与 build() 共用，有值即拼装）
    //    认知是具身的 → 保留 user_profile
    //    场景化沉淀 → 保留 project/task_context，沉淀出的经验自带场景标签
    result.push_str(&self.build_common_context_sections());

    // 4. 历史对话记忆
    if !self.history.is_empty() {
        result.push_str("【历史对话】\n");
        for h in &self.history {
            result.push_str(h);
            result.push('\n');
        }
        result.push('\n');
    }

    // 5. Trace ID
    if let Some(trace_id) = &self.current_trace_id {
        result.push_str(&format!("【思考 Trace ID】{}\n\n", trace_id));
    }

    // 6. 沉淀约束 + 待沉淀记忆 + 任务步骤（模板内聚在 builder）
    result.push_str("【沉淀工作模式触发】\n\n");
    result.push_str("你收到这个消息是因为触发了沉淀流程（类似人脑的睡眠整理记忆）。请进入沉淀工作模式，对以下未沉淀的短期记忆进行归纳整理：\n\n");
    result.push_str(&format!("## 待沉淀的短期记忆\n{}\n\n", pending_memories_summary));
    result.push_str("## 沉淀约束（重要）\n\n");
    result.push_str("- **不要发送消息**：睡觉是对自身知识的沉淀积累，不应依赖外部信息\n");
    result.push_str("- **不要调用消息类工具**（send_message / send_task_assignment_message 等），避免触发消息流程导致异步唤醒自己\n");
    result.push_str("- **只使用记忆类工具**：search_memory / save_long_term_memory / update_memory / query_memory\n");
    result.push_str("- 这是一个内循环：你与自己的记忆对话，不是与外部世界交互\n\n");
    result.push_str("## 你的任务\n\n");
    result.push_str("请用已有工具自主完成沉淀：\n\n");
    result.push_str("1. **归纳总结**：对上述短期记忆进行归纳，提炼核心概念、抽象经验、可复用模式（不要记具体细节）\n");
    result.push_str("2. **查询已有图谱**：用 search_memory 检查是否已有相关知识点（避免重复节点）\n");
    result.push_str("3. **创建/更新节点**：\n");
    result.push_str("   - 新知识 → save_long_term_memory 创建节点\n");
    result.push_str("   - 已有相似节点 → update_memory 更新节点内容\n");
    result.push_str("   - 过大且可拆分的旧节点 → 拆分为子节点 + 概述父节点 + contains 关系\n");
    result.push_str("4. **建立关系**：用 save_long_term_memory 的 relations 参数建立节点间关系（related/contains/depends 等）\n");
    result.push_str("5. **评估共享**：判断哪些节点对蜂巢有共享价值，用 update_memory 的 node_tags 字段加 'published' 标签\n");
    result.push_str("6. **标记完成**：每条短期记忆沉淀完成后，用 update_memory 把它的 status 改为 'settled'\n\n");
    result.push_str("## 认知要点\n\n");
    result.push_str("- 图谱是活的，每次沉淀都是迭代优化，不是机械合并\n");
    result.push_str("- 记抽象不记细节，可复用模式才沉淀\n");
    result.push_str("- 新老知识交替不是覆盖是迭代，推翻时用 opposite 关系保留痕迹\n");
    result.push_str("- published 标签让节点全局共享，通过共享节点作为桥梁发现跨 Agent 的知识网络\n");
    result.push_str("- 详见\"记忆认知\"技能的沉淀机制和新老知识交替章节\n\n");
    result.push_str("开始沉淀吧。");

    result
}
```

7. 提取 `build_common_context_sections(&self) -> String` 私有方法，把【用户画像】+【项目上下文】+【任务上下文】三个"有值即拼装"的区块抽出，供 build() 和 build_sleep_prompt() 复用，避免重复（user_profile / project_context / task_context 都是在 builder 中通用的上下文区块，无论唤醒还是沉睡场景都应一致拼装）。

```rust
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
    }
    s
}
```

build() 和 build_sleep_prompt() 都改为调用 `result.push_str(&self.build_common_context_sections())`，删除各自内联的拼装代码。

- [ ] **Step 6: 验证编译通过**

Run: `cargo check -p ai_orz --lib`

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(runtime): add ThinkingOptions, project/task context, build_sleep_prompt to PromptBuilder"
```

---

## Task 2: awaken/sleep_and_settle/wake_agent_brain 签名增加参数 + 工具过滤

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`（trait 定义 + impl）
- Modify: `src/service/domain/runtime/mod.rs`（如果有 trait re-export）
- Modify: `src/consumer/message.rs`（MessageConsumer 查询实体并传入）
- Modify: `src/handlers/hr/agent/settle_memory.rs`（settle_memory 传入 Settle 场景 options）
- Modify: `src/service/domain/runtime/awakening_test.rs` 或其他调用方（如果有测试调用 awaken）

- [ ] **Step 1: wake_agent_brain trait + impl 签名增加 scene 参数**

在 `src/service/domain/runtime/awakening.rs` 的 RuntimeAwakening trait 中：

```rust
async fn wake_agent_brain(
    &self,
    ctx: RequestContext,
    agent: &mut Agent,
    scene: ThinkingScene,
) -> Result<RequestContext>;
```

impl 中，在分离 Auto 工具时增加场景过滤（Settle 场景只保留记忆相关 Auto 工具）：

```rust
let rig_tools = if agent.po.kind.is_local() {
    let all_tools = std::mem::take(&mut agent.tools);
    let (auto, manual): (Vec<_>, Vec<_>) = all_tools
        .into_iter()
        .partition(|t| matches!(t.po.control_mode, common::enums::ControlMode::Auto));
    // Settle 场景过滤 Auto 工具：只保留记忆相关（neural/memory tag）
    let auto = match scene {
        ThinkingScene::Awaken => auto,
        ThinkingScene::Settle => auto
            .into_iter()
            .filter(|t| {
                let tags = t.po.get_tags();
                scene.is_tool_allowed(&tags)
            })
            .collect(),
    };
    agent.tools = manual;
    auto
} else {
    Vec::new()
};
```

- [ ] **Step 2: awaken trait 签名增加参数**

```rust
async fn awaken(
    &self,
    ctx: RequestContext,
    agent: &Agent,
    message: &Message,
    options: &ThinkingOptions,
) -> Result<AwakeningResult>;
```

- [ ] **Step 3: awaken impl 增加参数并注入 builder**

在 impl 中：
1. 签名增加 `options: &ThinkingOptions` 参数
2. 在 builder 使用处增加（在 `builder.current_message(message)` 之前）：
```rust
if let Some(project) = &options.project {
    builder.project_context(project);
}
if let Some(task) = &options.task {
    builder.task_context(task);
}
```

- [ ] **Step 4: sleep_and_settle trait + impl 签名增加参数**

签名变更：`settle_prompt: &str` → `pending_memories_summary: &str`（只传待沉淀记忆摘要，约束模板由 builder.build_sleep_prompt 内聚）

```rust
async fn sleep_and_settle(
    &self,
    ctx: RequestContext,
    agent: &Agent,
    pending_memories_summary: &str,
    options: &ThinkingOptions,
) -> Result<AwakeningResult>;
```

impl 中目前不使用 options 的 project/task 字段（sleep 不需要），但 options.scene 用于工具过滤（在 Task 3 实现）。impl 内部不再构造虚拟 System Message，改为调用 `builder.build_sleep_prompt(pending_memories_summary)`（在 Task 3 Step 2 实现）。

- [ ] **Step 5: 搜索并修改所有调用 wake_agent_brain / awaken / sleep_and_settle 的地方**

用 Grep 搜索所有调用 `.wake_agent_brain(` / `.awaken(` / `.sleep_and_settle(` 的地方：

1. **`src/consumer/message.rs`** 的 `handle_agent_message`：
   - wake_agent_brain 传入 `ThinkingScene::Awaken`
   - 查询 project/task 实体
   - 构造 ThinkingOptions
   - 传入 awaken

```rust
// wake_agent_brain
let enriched_ctx = self
    .runtime_domain
    .awakening()
    .wake_agent_brain(ctx, &mut agent, ThinkingScene::Awaken)
    .await?;
ctx = enriched_ctx;

// 查询消息关联的 project/task 实体（如果消息携带了 ID）
let mut thinking_options = ThinkingOptions::new();
if let Some(project_id) = &message.po.project_id {
    if let Ok(Some(project)) = self.project_domain.get(ctx.clone(), project_id).await {
        thinking_options = thinking_options.with_project(project);
    }
}
if let Some(task_id) = &message.po.task_id {
    let task_fetch_options = crate::service::dal::task::TaskFetchOptions::default();
    if let Ok(Some(task)) = self.project_domain.task().get_task(ctx.clone(), task_id, task_fetch_options).await {
        thinking_options = thinking_options.with_task(task);
    }
}

let awaken_result = self
    .runtime_domain
    .awakening()
    .awaken(ctx.clone(), &agent, message, &thinking_options)
    .await?;
```

注意：需要 import `ThinkingOptions` 和 `ThinkingScene`。确认 `self.project_domain.task()` 的调用路径。

2. **`src/handlers/hr/agent/settle_memory.rs`** 的 `load_and_settle`：
```rust
// 沉睡场景：wake_agent_brain 传入 Settle（过滤 Auto 工具）
let ctx = runtime_domain()
    .awakening()
    .wake_agent_brain(ctx, &mut agent, ThinkingScene::Settle)
    .await?;

// build_pending_memories_summary 只返回待沉淀记忆摘要（不含约束模板，模板已内聚到 builder）
let summary = match build_pending_memories_summary(&ctx, &agent.po.id, SETTLE_BATCH_LIMIT).await? {
    Some((summary, count)) => {
        log_info!(&ctx, "settle_memory", "待沉淀短期记忆 {} 条", count);
        summary
    }
    None => return Ok(SettleMemoryResponse::skipped()),
};

// 沉睡场景 options，传入摘要（非完整 prompt）
let options = ThinkingOptions::for_scene(ThinkingScene::Settle);
runtime_domain()
    .awakening()
    .sleep_and_settle(ctx, &agent, &summary, &options)
    .await?;
```

3. **测试代码**：如果有测试直接调用 wake_agent_brain / awaken / sleep_and_settle，传入 `ThinkingScene::Awaken` / `&ThinkingOptions::default()`。

- [ ] **Step 6: 验证编译通过**

Run: `cargo check -p ai_orz --lib`

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(runtime): add ThinkingOptions/ThinkingScene to signatures, filter Auto tools in Settle scene"
```

---

## Task 3: sleep_and_settle 复用 builder + 只加载记忆 skill + 不发消息约束

**Files:**
- Modify: `src/service/domain/runtime/awakening.rs`（sleep_and_settle 实现：过滤 skill + 调用 build_sleep_prompt）
- Modify: `src/handlers/hr/agent/settle_memory.rs`（build_settle_prompt → build_pending_memories_summary，只生成摘要）

- [ ] **Step 1: sleep_and_settle 过滤 skill_pos + Manual 工具**

在 `src/service/domain/runtime/awakening.rs` 的 sleep_and_settle 实现中：

1. skill_pos 过滤（只保留记忆相关 skill）：

```rust
// 沉淀模式只加载记忆相关 skill（tags 含 neural 或 memory），避免外部依赖
// 睡觉是对自身知识的沉淀积累，不应触发消息流程导致异步唤醒自己
let skill_pos: Vec<crate::models::skill::SkillPo> = agent
    .skills()
    .iter()
    .filter(|s| {
        let tags = s.po.parse_tags();
        tags.iter().any(|t| t == "neural" || t == "memory")
    })
    .map(|s| s.po.clone())
    .collect();
```

2. all_tools 同样过滤（Manual 工具也只保留记忆相关，与 wake_agent_brain 的 Auto 工具过滤对称）：

```rust
let all_tools: Vec<crate::models::tool::ToolPo> = agent
    .tools()
    .iter()
    .filter(|t| {
        let tags = t.po.get_tags();
        tags.iter().any(|tag| tag == "neural" || tag == "memory")
    })
    .map(|t| t.po.clone())
    .collect();
```

注意：确认 SkillPo 的 parse_tags 方法是否存在。如果方法名不同（如 get_tags），需要调整。

- [ ] **Step 2: sleep_and_settle impl 改为调用 builder.build_sleep_prompt**

把当前 sleep_and_settle impl 中"构造虚拟 System Message + builder.current_message + builder.build()"的三步，替换为直接调用 `builder.build_sleep_prompt(pending_memories_summary)`：

删除这段（不再需要虚拟 Message）：
```rust
// 删除：构造虚拟 System Message
let settle_message = Message::new_with_context(...);

let mut builder = self.prompt_builder(agent);
builder.current_trace_id(&trace_id);
builder.system_prompt(agent);
builder.tools(&all_tools);
builder.skills(&skill_pos);
builder.history(&recent_memories);
builder.current_message(&settle_message);  // 删除

let prompt = builder.build();  // 改为 build_sleep_prompt
```

改为：
```rust
let mut builder = self.prompt_builder(agent);
builder.current_trace_id(&trace_id);
builder.system_prompt(agent);
builder.tools(&all_tools);      // 已过滤为记忆相关
builder.skills(&skill_pos);     // 已过滤为记忆相关
builder.history(&recent_memories);

// 复用 builder 挂载链路，生成沉淀场景 prompt（约束模板内聚在 builder）
let prompt = builder.build_sleep_prompt(pending_memories_summary);
```

好处：
- 不再构造虚拟 System Message（沉淀不是"收到消息"，语义更准确）
- 约束模板与 build() 对称，内聚在 builder，不散落在 settle_memory.rs
- 复用 system_prompt/tools/skills/history 挂载，按需生成

- [ ] **Step 3: settle_memory.rs 的 build_settle_prompt 简化为 build_pending_memories_summary**

把 `build_settle_prompt` 重命名为 `build_pending_memories_summary`，删除所有 `format!` 模板代码（约束章节已移入 builder），只保留：
1. 查询未沉淀短期记忆
2. 拼接记忆摘要字符串（编号列表，供 builder 注入"## 待沉淀的短期记忆"区块）

```rust
/// 构建待沉淀短期记忆的摘要（不含约束模板，模板已内聚到 PromptBuilder.build_sleep_prompt）
///
/// # 返回
/// - `Ok(None)` 表示无未沉淀记忆（调用方应跳过）
/// - `Ok(Some((summary, count)))` 表示有待沉淀记忆，summary 为编号摘要字符串
pub(crate) async fn build_pending_memories_summary(
    ctx: &RequestContext,
    agent_id: &str,
    limit: usize,
) -> Result<Option<(String, usize)>> {
    let short_term_memories = memory_dao()
        .query_short_term(
            ctx.clone(),
            MemoryQuery {
                agent_id: Some(agent_id.to_string()),
                status: Some(MemoryStatus::Active),
                memory_type: Some(MemoryType::ShortTerm),
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await?;

    let count = short_term_memories.len();
    if count == 0 {
        return Ok(None);
    }

    // 拼接编号摘要（约束模板由 builder.build_sleep_prompt 注入）
    let mut summary = String::new();
    for (i, mem) in short_term_memories.iter().enumerate() {
        summary.push_str(&format!("{}. [id={}] {}\n", i + 1, mem.po.id, mem.po.content));
    }
    Ok(Some((summary, count)))
}
```

同时更新 `load_and_settle` 调用处（见 Task 2 Step 5 的调用方代码）。

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p ai_orz --lib`

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(runtime): sleep_and_settle reuses builder.build_sleep_prompt, only memory skills/tools, forbid messaging"
```

---

## Task 4: 最终验证

- [ ] **Step 1: cargo fmt --all**
- [ ] **Step 2: cargo clippy -p ai_orz --lib -- -D warnings**
- [ ] **Step 3: cargo test -p ai_orz --lib memory**
- [ ] **Step 4: cargo test -p ai_orz --lib runtime**（如果有 runtime 测试）
- [ ] **Step 5: 如有修复，提交**

---

## Self-Review

### 关键设计决策
1. **ThinkingOptions 命名**：用 "thinking" 更通用，awaken 和 sleep_and_settle 都是"思考"流程的不同场景
2. **ThinkingOptions 位置**：定义在 `src/service/domain/runtime/awakening.rs`（与 trait 同文件），与 runtime 语义强绑定
3. **project/task 实体在 consumer 层查询**：遵守 domain 间不直接调用的约束。MessageConsumer 持有 project_domain 引用，查询后构造 ThinkingOptions 传入
4. **awaken/sleep_and_settle 签名用 &ThinkingOptions**：引用类型，避免所有权转移；default() 时为空选项，向后兼容
5. **PromptBuilder 新增 project_context / task_context 方法**：独立方法，与 user_profile 语义分离
6. **build 拼装位置**：业务上下文在【用户画像】之后、【历史对话】之前——业务上下文比用户画像更随消息变化，但在历史之前提供背景
7. **sleep_and_settle 过滤 skill + Manual 工具**：只保留 tags 含 neural 或 memory 的（记忆认知技能 tags = ["neural", "memory"]），与 wake_agent_brain 的 Auto 工具过滤对称
8. **沉淀 prompt 约束**：明确禁止消息类工具，强调内循环语义
9. **build_sleep_prompt 复用 builder 挂载链路**：与 build() 对称，沉淀约束模板内聚在 PromptBuilder（不再散落在 settle_memory.rs 的 format!）；提取两个私有方法供 build() 和 build_sleep_prompt() 复用——`build_tools_and_skills_sections`（工具/技能区块）和 `build_common_context_sections`（用户画像 + 项目/任务上下文，有值即拼装的通用区块）；保留 user_profile（认知是具身的）和 project/task_context（场景化沉淀，沉淀出的经验自带场景标签），仅跳过 tool_failures 和 current_message
10. **sleep_and_settle 签名改为 pending_memories_summary**：只传待沉淀记忆摘要（编号列表），约束模板由 builder.build_sleep_prompt 内聚生成；不再构造虚拟 System Message（沉淀不是"收到消息"，语义更准确）
11. **build_sleep_prompt trait 默认实现回退 build()**：Cli/Remote Agent 不参与沉淀，不会走到此分支；仅 DefaultPromptBuilder 真正实现沉淀语义

### 风险点
1. **awaken/sleep_and_settle 签名变更影响面**：所有调用的地方都要加 options 参数。需要搜索全部调用点（consumer/message.rs, settle_memory.rs, 测试代码）
2. **project_domain.task() 调用路径**：需要确认 ProjectDomain trait 中 task 查询的方法名和路径
3. **SkillPo.parse_tags() 方法**：需要确认方法名是否正确
4. **TaskFetchOptions import**：MessageConsumer 中查询 task 需要 TaskFetchOptions，确认 import 路径
