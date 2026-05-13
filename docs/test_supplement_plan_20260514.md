# 测试补充方案 - 2026-05-14

## 一、背景

根据最近对 `tool` 和 `memory` 模块的功能新增，需要补充相应的单元测试，确保功能正确性和防止回归。

**主要修改来源 commit：**
- `05ef2f0` - 简化内置工具机制并添加 Builtin 保护
- `1242ea0` - 更新测试适配记忆系统重构

---

## 二、新增功能清单

### 2.1 Tool 模块新增功能

| 功能 | 函数/方法 | 说明 |
|------|----------|------|
| Builtin 工具保护 | `update_tool` / `delete_tool` | 不允许修改/删除 Builtin 协议的工具 |
| 同步内置工具 | `sync_builtin_tools_to_db` | 从 ToolRegistry 同步 Builtin 工具到数据库 |
| 默认值填充 | `ToolPo::fill_defaults_for_builtin` | 为 Builtin 工具填充默认字段 |
| 通用查询 | `ToolQuery` / `query` | 灵活的查询参数组合 |
| 统一搜索 | `ToolSearch` / `search` | 关键词 + 过滤统一入口 |

### 2.2 Memory 模块新增功能

| 功能 | 函数/方法 | 说明 |
|------|----------|------|
| Daily JSONL | `append_trace` / `batch_append_traces` | 每日文件记忆追加 |
| 记忆引用读取 | `read_memory_reference` | 从 date_path + line_number 读取原始记忆 |
| 通用查询 | `MemoryQuery` / `query_short_term` | 灵活查询短期记忆索引 |
| 关键词搜索 | `MemorySearch` / `search_short_term` | SQLite FTS 全文搜索 |
| 软删除机制 | `forget_short_term_index` | 标记为 Forgotten，不物理删除 |
| 完整知识图谱 CRUD | 各种方法 | 知识节点、关系、引用完整操作 |

---

## 三、现有测试覆盖度检查

### 3.1 Tool 模块现有测试

**文件**: `src/service/dao/tool/sqlite_test.rs`

| 已有测试 | 覆盖功能 |
|---------|--------|
| `test_create_and_get_tool_full` | 创建和查询工具 |
| `test_add_tool_to_agent_and_list` | Agent 绑定和查询 |
| `test_remove_tool_from_agent` | Agent 解绑 |
| `test_list_enabled` | 启用状态过滤 |
| `test_get_by_name` | 按名称查询 |
| `test_update_tool` | 非内置工具更新 |
| `test_update_builtin_tool_protected` | 内置工具更新保护 ✅ |
| `test_delete_builtin_tool_protected` | 内置工具删除保护 ✅ |
| `test_find_not_exists` | 不存在的工具处理 |

**结论**: Builtin 工具保护机制测试已覆盖，其他新增功能需要补充。

### 3.2 Memory 模块现有测试

**文件**: `src/service/dao/memory/sqlite_test.rs`

| 已有测试 | 覆盖功能 |
|---------|--------|
| `test_append_trace_and_create_short_term_index` | 单个 trace 追加 + 索引创建 |
| `test_create_knowledge_node` | 知识节点创建和查询 |
| `test_add_knowledge_relation` | 知识关系添加 |
| `test_add_knowledge_reference` | 知识引用添加 |
| `test_memory_trace_id_is_content_hash` | Trace ID hash 验证 |
| `test_memory_trace_to_markdown` | Markdown 格式化 |

**结论**: 记忆系统重构后的大量新功能尚未覆盖。

---

## 四、测试补充计划

### 4.1 Tool 模块测试补充

**文件**: `src/service/dao/tool/sqlite_test.rs`

| 优先级 | 测试用例 | 测试目标 | 验证点 |
|------|---------|--------|-------|
| P0 | `test_sync_builtin_tools_to_db` | 验证同步内置工具功能正常 | 1. 工具成功插入<br>2. 重复同步幂等性<br>3. 默认字段正确填充 |
| P0 | `test_tool_query` | 验证通用查询功能 | 1. ID 批量查询<br>2. 关键词搜索<br>3. enabled 过滤<br>4. limit 限制<br>5. agent_id 过滤 |
| P1 | `test_tool_search` | 验证统一搜索功能 | 1. 关键词搜索<br>2. 过滤条件叠加<br>3. limit 生效 |

### 4.2 Memory 模块测试补充

**文件**: `src/service/dao/memory/sqlite_test.rs`

| 优先级 | 测试用例 | 测试目标 | 验证点 |
|------|---------|--------|-------|
| P0 | `test_batch_append_traces` | 验证批量追加记忆 | 1. 多个 trace 成功写入<br>2. 位置信息正确返回 |
| P0 | `test_read_memory_reference` | 验证从引用读取原始内容 | 1. 写入后可通过引用读回<br>2. 内容完全一致 |
| P0 | `test_memory_query` | 验证通用查询 | 1. agent_id 过滤<br>2. status 过滤<br>3. exclude_status 过滤<br>4. limit 限制<br>5. ids 批量查询 |
| P0 | `test_memory_search` | 验证关键词搜索 | 1. MATCH 查询生效<br>2. 过滤条件叠加 |
| P1 | `test_update_short_term_index` | 验证更新功能 | 1. 字段正确更新<br>2. 不存在返回 NotFound |
| P1 | `test_forget_short_term_index` | 验证遗忘机制 | 1. status 变更正确<br>2. 默认查询排除 |
| P1 | `test_list_short_term_memory_forgotten_not_returned` | 验证已遗忘内容默认不返回 | 1. 默认查询不包含 Forgotten<br>2. 指定 status 可查询 |
| P1 | `test_query_short_term_limit` | 验证 limit 参数 | 1. limit 限制生效<br>2. 返回数量正确 |

### 4.3 DAL 层测试补充（可选，视时间而定）

**文件**: `src/service/dal/tool_test.rs` 和 `src/service/dal/memory_test.rs`

建议：由于 DAO 层已覆盖核心逻辑，DAL 层测试可以简化或复用现有测试，重点验证 DAL 组装逻辑。

---

## 五、实现步骤

### 阶段 1：Tool 模块测试补充（优先级 P0）
1. 补充 `test_sync_builtin_tools_to_db`
2. 补充 `test_tool_query`
3. 补充 `test_tool_search`
4. 运行所有测试，确保通过

### 阶段 2：Memory 模块测试补充（优先级 P0）
1. 补充 `test_batch_append_traces`
2. 补充 `test_read_memory_reference`
3. 补充 `test_memory_query`
4. 补充 `test_memory_search`
5. 运行所有测试，确保通过

### 阶段 3：边界条件测试（优先级 P1）
1. 补充 Memory 模块剩余测试
2. 补充异常路径和边界条件测试
3. 完整运行测试套件

### 阶段 4：文档更新和提交
1. 更新相关设计文档
2. Git 提交

---

## 六、注意事项

### 6.1 Memory 模块测试注意事项
- 需要临时目录配置，使用 `tempfile` 避免影响真实数据
- daily JSONL 文件路径处理要正确
- 测试结束清理临时文件

### 6.2 Tool 模块测试注意事项
- 需要初始化 ToolRegistry，确保 Builtin 工厂已注册
- 测试环境配置正确的 `SQLX_OFFLINE`
- 避免修改全局单例状态影响其他测试

### 6.3 现有测试维护
- 确保新增测试不破坏现有测试
- 复用现有测试的辅助函数
- 保持测试隔离性

---

## 七、验收标准

1. **新增测试通过**：所有补充的测试用例运行通过
2. **完整测试套件通过**：`cargo test` 全量测试 283 个测试全部通过
3. **无回归**：现有功能保持正常
4. **代码质量**：测试代码清晰可维护，复用辅助函数

---

## 附录：相关技能/文档参考

- Rust DAL 单元测试模式（已有 Skill）
- Rust DAO 职责分离重构模式（已有 Skill）
- 项目分层架构设计文档

---

**文档创建时间**: 2026-05-14
**最后更新时间**: 2026-05-14
**下一步**: 按方案实现测试补充
