# 代码分支工作流（Git Branch Workflow）

一种「隔离改动 + 可追溯 + 产物化」的代码工作模式：把对共享仓库的修改放在独立分支，把仓库/分支信息写回任务，把最终的 MR/PR 作为产物（artifact）沉淀。这是**可选工作模式之一**，Agent 应按任务性质自行决定是否采用，不要无差别套用。

## 适用场景

- 任务涉及改动一个**共享仓库**（多人/多 Agent 协作的代码库）。
- 你希望：改动相互隔离（不污染主干）、过程可追溯（哪次任务改了哪个仓库哪条分支）、成果可交付（MR 作为产物留档）。

**不适用**：纯本地一次性脚本、不入库的临时文件、无共享仓库的 exploratory / 文档调研任务。

## 核心思路

```
共享仓库 ──clone──► 你的工作区(users/{uid}/agents/{aid}/work)
        ──checkout -b feature/xxx──► 隔离分支
        ──edit + commit──► 本地提交
        ──push──► 远端分支
        ──(gh pr create | 推分支留待合并)──► MR/PR
        ──mark_artifact──► MR 存为任务产物
任务记录: repo_url + base_branch + feature_branch（写回 execution_plan / description）
```

## 步骤

### 1. 拉取并开分支
用 `shell_exec` 在你的默认工作区（已是 `users/{uid}/agents/{aid}/work`）操作：
```bash
git clone <repo_url> <local_dir>          # 支持本地路径 / file:// / SSH / HTTPS
cd <local_dir>
git checkout -b feature/<task-id>-<slug>  # 分支名含 task id 便于追溯
```
- 远端可以是 GitHub，也可以是**纯本地 git**（局域网 bare 库、共享文件系统上的仓库、`file://` 路径）——本模式不依赖 GitHub。
- 鉴权：GitHub 走 `gh_cli`（需用户 PAT 凭据）；本地 git 走 SSH key 或共享文件系统权限，无需 `gh`。

### 2. 把仓库/分支信息写回任务
立即调用 `update_task`（或 `update_task_progress`）把代码上下文固化进任务，便于追溯与交接。推荐写在 `execution_plan` 的 `## 代码工作上下文` 区块（前端按 Markdown 渲染）：
```markdown
## 代码工作上下文
- 仓库: <repo_url>
- 基分支: <base_branch>（如 main）
- 工作分支: feature/<task-id>-<slug>
- 工作区: <local_dir>
```
这样未来任何人（或你自己）接手任务，一眼就能定位改了哪个仓库、哪条分支。

### 3. 编辑、提交、推送
```bash
# 编辑文件（fs_write / shell_exec）
git add -A
git commit -m "<任务简述>"
git push -u origin feature/<task-id>-<slug>
```

### 4. 生成 MR/PR（两路）
- **有 GitHub**：`gh pr create --title ... --body ...`（需 `gh_cli` 已鉴权）。记下返回的 PR URL。
- **纯本地 git（无 GitHub）**：没有原生 PR 概念。改为「推送分支 + 留待合并」：分支推上共享远端后，把**分支引用**（仓库 + 分支名 + 远端）作为交付物即可；由人工或合并控制器（merge controller）合回基分支。

### 5. 把 MR 存为任务产物（关键闭环）
取第 4 步 `shell_exec` / `gh_cli` 工具结果里的 `call_id`，调用 `mark_artifact`：
```json
{
  "call_id": "<第4步工具的 call_id>",
  "task_id": "<当前任务 id>",
  "name": "MR: <repo> feature/<branch>",
  "description": "任务 <task-id> 的代码改动已生成 MR/分支：<PR URL 或 branch ref>"
}
```
`mark_artifact` 会把该次工具运行的完整输出（含 PR URL / 分支引用 / diff 摘要）复制晋升为**项目产物**并关联到本任务。产物在产物中心可见、可下载，构成「任务 → 代码改动 → MR」的完整链路。

## 现有原语映射

| 本模式动作 | 平台原语 |
|-----------|---------|
| 跑 git 命令 | `shell_exec`（默认工作区即 `users/{uid}/agents/{aid}/work`） |
| 创建 GitHub PR | `gh_cli`（需用户 PAT 凭据） |
| 写回仓库/分支 | `update_task` / `update_task_progress`（落 `execution_plan`/`description`） |
| 存 MR 为产物 | `mark_artifact`（持 `call_id` + `task_id`） |
