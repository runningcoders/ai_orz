<!-- 本文件由 tools/wiki_index.py 自动生成，请勿手工编辑；改内容请改源文件后跑 `make wiki-index` -->

# 10-异步基建(AOP)

> 模块索引。A = 设计（为什么）/ B = 落地方案（要去哪）/ C = 知识（是什么）。
>
> 本文件自动生成：改内容请改源文件后跑 `make wiki-index`。

本模块共 33 篇：A 设计 1 · C 知识 32。

## A 设计（design）

- [AOP 生产者-消费者契约重构设计](<A-设计/aop_producer_consumer_contract_design.md>) — `现行`

## C 知识（wiki）

- [AOP 事件系统（系统管理层）](<C-知识/AOP 事件系统.md>)
- [AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册](<C-知识/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册/AOP 生产消费事件中心：纯框架零业务 + pkg/aop/core 6 Trait + Registry 全局单例 + 8 类业务消费者注册.md>)
- [Agent 循环消费者（代码落地层）](<C-知识/Agent 循环消费者.md>)
- [Cron 任务调度与系统启动顺序：5 字段 cron 解析 + ensure_system_cron_triggers 2 条注入 + CronTriggerProducer](<C-知识/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离/Cron 任务调度与系统启动顺序：5字段cron解析 + ensure_system_cron_triggers 2条注入 + CronTriggerConsumer 事件 + 两阶段init/aop严格分离.md>)
- [Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅](<C-知识/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅/Domain 内部事件与消费者全链路：8 类 DomainEvent 枚举 + 8 类 Consumer 业务消费 + AOP Producer 投递入口 + Registry 订阅.md>)
- [事件持久化](<C-知识/事件持久化.md>)
- [事件消费者](<C-知识/事件消费者.md>)
- [事件生产者](<C-知识/事件生产者.md>)
- [事件系统核心](<C-知识/事件系统核心.md>)
- [事件调度器](<C-知识/事件调度器.md>)
- [事件队列管理](<C-知识/事件队列管理.md>)
- [事件队列系统](<C-知识/事件队列系统.md>)
- [任务事件消费者（框架层）](<C-知识/任务事件消费者.md>)
- [唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发](<C-知识/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发/唤醒前置门闩与依赖补发闭环：wake_gate_policy 策略化前置校验 + TaskEventConsumer 依赖就绪自动重发.md>)
- [AOP 事件系统（框架层）](<C-知识/基础设施/AOP 事件系统/AOP 事件系统.md>)
- [定时任务和事件系统](<C-知识/定时任务和事件系统.md>)
- [定时任务消费者](<C-知识/定时任务消费者.md>)
- [定时触发生产者](<C-知识/定时触发生产者.md>)
- [工具执行消费者](<C-知识/工具执行消费者.md>)
- [思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路](<C-知识/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路/思考退出原因 exit_reason 统计与 ThinkRoundEvent AOP 事件链路.md>)
- [核心概念与架构](<C-知识/核心概念与架构.md>)
- [AOP 事件系统（代码落地层）](<C-知识/核心模块/AOP 事件系统/AOP 事件系统.md>)
- [任务事件消费者（代码落地层）](<C-知识/核心模块/AOP 事件系统/消费者框架/任务事件消费者.md>)
- [注册中心与调度器](<C-知识/注册中心与调度器.md>)
- [消息消费者（代码落地层）](<C-知识/消息消费者.md>)
- [消费者接口规范](<C-知识/消费者接口规范.md>)
- [消费者框架](<C-知识/消费者框架.md>)
- [生产者机制](<C-知识/生产者机制.md>)
- [生产者消费者模式](<C-知识/生产者消费者模式.md>)
- [监控与统计集成](<C-知识/监控与统计集成.md>)
- [调度器消费者](<C-知识/调度器消费者.md>)
- [队列实现](<C-知识/队列实现.md>)
