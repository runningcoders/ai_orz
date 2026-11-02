<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 02-Agent运行时

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 42 篇：A 设计 8 · B 落地方案 15 · C 知识 19。

## A 设计（design）

- [Agent 循环驱动引擎设计](<A-设计/agent_loop_engine_design.md>) — `已废弃`
- [决策型策略设计（策略引擎承载小脑场景）](<A-设计/cerebellum_decision_strategy_design.md>) — `现行`
- [意图感知两阶段唤醒设计](<A-设计/intent_aware_two_stage_awaken_design.md>) — `已废弃`
- [Runtime Domain 设计](<A-设计/runtime_design.md>) — `现行`
- [Runtime Domain 推进路线图](<A-设计/runtime_domain_roadmap.md>) — `已废弃`
- [思考生命周期设计（Think Lifecycle）](<A-设计/think_lifecycle_design.md>) — `现行`
- [思考图引擎改造设计（Awaken 主思考路径节点化）](<A-设计/thinking_graph_engine_design.md>) — `落地中`
- [Agent 思考运行时 + 策略引擎设计](<A-设计/thinking_task_policy_engine_design.md>) — `现行`

## B 落地方案（plan）

- [Agent Loop Alignment + AOP Sync Hooks Implementation Plan](<B-落地方案/Agent Loop Alignment + AOP Sync Hooks（执行蓝图）.md>) — `已完成`
- [Agent 循环驱动引擎 Implementation Plan](<B-落地方案/Agent 循环驱动引擎实施计划.md>) — `已完成`
- [AgentRuntimeInfo 业务上下文扩展 Implementation Plan](<B-落地方案/AgentRuntimeInfo 业务上下文扩展（执行蓝图）.md>) — `已完成`
- [Agent运行时上下文扩展](<B-落地方案/Agent运行时上下文扩展.md>) — `已完成`
- [Rig 0.34 → 0.41 Upgrade Implementation Plan](<B-落地方案/Rig 0.34 → 0.41 Upgrade（执行蓝图）.md>) — `已完成`
- [Runtime API + exit_reason 持久化 Implementation Plan](<B-落地方案/Runtime API + exit_reason 持久化（执行蓝图）.md>) — `已完成`
- [两阶段唤醒：IntentAnalyze+Awaken正式执行](<B-落地方案/两阶段唤醒：IntentAnalyze+Awaken正式执行.md>) — `已完成`
- [唤醒上下文注入 + 沉睡沉淀约束改造](<B-落地方案/唤醒上下文与睡眠约束.md>) — `已完成`
- [思考图引擎改造实施计划](<B-落地方案/思考图引擎改造.md>) — `进行中`
- [意图感知两阶段唤醒 Implementation Plan](<B-落地方案/意图感知两阶段唤醒（执行蓝图）.md>) — `已完成`
- [移除 rig 依赖与 Cortex 层扁平化重构](<B-落地方案/移除rig依赖与向量存储后端解耦.md>) — `已完成`
- [策略引擎 + AgentThinkRuntime Implementation Plan](<B-落地方案/策略引擎 + AgentThinkRuntime（执行蓝图）.md>) — `已完成`
- [策略引擎与思考运行时策略增强](<B-落地方案/策略引擎与思考运行时策略增强.md>) — `已完成`
- [运行时API可观测与退出原因字段](<B-落地方案/运行时API可观测与退出原因字段.md>) — `已完成`
- [运行时问题修复](<B-落地方案/运行时问题修复.md>) — `已完成`

## C 知识（wiki）

- [A2A Client + 外部 Agent Runtime（出站调用 + 内部桥接）](<C-知识/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送/A2A Client + 外部 Agent Runtime：A2aRuntimeDao HTTP 调用 + ExternalCortexDao 桥接 + A2aCallbackDao Push 推送.md>)
- [AI Agent 管理](<C-知识/AI Agent 管理.md>)
- [Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报](<C-知识/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报/Agent 思考运行时 AgentThinkRuntime：挂载清理取消与每轮快照上报.md>)
- [Agent 搜索与推荐](<C-知识/Agent 搜索与推荐.md>)
- [Agent 生命周期管理](<C-知识/Agent 生命周期管理.md>)
- [Agent处理器](<C-知识/Agent处理器.md>)
- [Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2](<C-知识/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联/Intent 感知两阶段唤醒：IntentAnalyze Phase1 七字段意图分析 + 6 级 JSON 降级兜底 + Awaken Phase2 正式执行串联.md>)
- [ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板](<C-知识/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板/ThinkTrace 轮级轨迹：JSONL 落盘 + query_think_traces 作用域守门 + 轮次时间线与轨迹面板.md>)
- [小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类](<C-知识/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类/小脑快判断（Cerebellum）：System One 协议 DTO + SystemOneCerebellumDao client + 默认小脑获取 + 运行时快判断路由三分类.md>)
- [小脑快判断（System One）](<C-知识/小脑快判断（System One）.md>)
- [工具绑定管理](<C-知识/工具绑定管理.md>)
- [思考循环异常退出兜底总结与错误上报结构化](<C-知识/思考循环异常退出兜底总结与错误上报结构化/思考循环异常退出兜底总结与错误上报结构化.md>)
- [思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件](<C-知识/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件/思考运行时前端观测：runtime-status cancel-thinking runtime-list 接口与 runtime_panel 组件.md>)
- [技能包管理](<C-知识/技能包管理.md>)
- [策略引擎与 Shell 拦截层](<C-知识/策略引擎与 Shell 拦截层.md>)
- [策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层 + PolicyAction 动作上浮 + Shell 拦截层](<C-知识/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层/策略引擎：Policy trait + PolicyGroup 嵌套组合 + policy_set! 宏声明式写法 + PolicyAction 动作上浮 + Shell 拦截层.md>)
- [记忆系统管理](<C-知识/记忆系统管理.md>)
- [运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化](<C-知识/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化/运行时诊断工具注册为 Agent 可调用工具：runtime-status cancel-thinking runtime-list 三接口工具化.md>)
- [运行时领域](<C-知识/运行时领域.md>)
