# Workspace 第二期：Task DAG 分层布局 Plan

**Goal**: TaskListItem 补 dependencies 字段，前端实现 Kahn 拓扑排序分层布局，ProjectDetail 视图升级为真正 DAG。

**Tasks**:
1. 后端 TaskListItem 加 dependencies 字段 + to_list_item 补齐映射
2. CanvasNode 加 layer 字段 + 同步所有构造点
3. 新建 layered_layout.rs（Kahn 拓扑排序 + 分层均分 + 7 个单元测试）
4. force_layout 增加 layer y 方向约束力
5. workspace_graph 的 build_project_detail_view 集成 DAG 分层 + Task→Task 依赖边
6. 验证编译 + 后端测试 + 推送

**关键文件**:
- common/src/api/task.rs（改 TaskListItem）
- src/handlers/project/task/response.rs（改 to_list_item）
- frontend/src/components/canvas_scene.rs（CanvasNode 加 layer）
- frontend/src/components/layered_layout.rs（新建）
- frontend/src/components/force_layout.rs（加分层约束力）
- frontend/src/components/workspace_graph.rs（集成 DAG）

**核心算法**:
- Kahn 拓扑排序：入度=0 入队 → 取出后继入度-1 → 入度变 0 入队并 layer+1
- 环检测：未处理节点强制放最底层
- 同层水平均分
- force_layout 对有 layer 的节点施加 y 方向弹簧力
