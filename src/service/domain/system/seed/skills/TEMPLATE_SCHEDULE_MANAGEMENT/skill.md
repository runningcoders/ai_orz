# 日程管理（Schedule Management）

让用户「到点提醒我 / 到点自己去做」的诉求按时发生。四个工具：`schedule_agent_task`（建）、`query_schedule`（查）、`update_schedule`（改）、`cancel_schedule`（删）。

## 何时用

- 「明天提醒我…」「每周一早上…」→ 建日程
- 「这周有什么安排」「下周有没有…」→ 查日程
- 「改到下午三点」「换个人负责」「不用了」→ 改 / 删日程

## 怎么做

1. **建**：`title` 写短标题，`description` 写完整意图（到点原样回灌给 Agent）。时间规则三选一：`cron_expression`（5 字段 cron）、`interval_seconds`、`run_at`（一次性，Unix 毫秒）。默认 `behavior=AsyncAgentWakeup`（唤醒负责 Agent 执行）；只提醒用户用 `AsyncNotify`。`agent_id` 缺省是自己。
2. **查**：`date_term` 直接传 `today` / `tomorrow` / `this_week` / `next_week` 等，服务端按系统时区折算，**不要自己算毫秒时间戳**。返回的是**实例**：每日重复日程查一周得 7 条，按 `trigger_id` 去重。
3. **改**：先查到 `trigger_id` 再 `update_schedule`。`start_at` / `end_at` / `max_runs` 是三态：不传=保持，`null`=清除限制，传值=覆盖；传任一时间规则字段=整体替换时间规则。
4. **删**：`cancel_schedule(trigger_id)` 幂等。只改时间或标题就用改，别删了重建。

## 红线

- 只能查、改、删**归属当前用户**的日程；归属用户、组织、创建者由系统从上下文固化，传参改不了。
- 查到的是展开后的实例而非规则；重复日程的多个实例共享同一个 `trigger_id`。
