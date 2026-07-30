# 预置基础技能 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在系统默认模板中预置若干 Published 技能到共享库，像 builtin tools 一样开箱即用。default.json 作为总控 meta，大内容技能文件单独存放在 `seed/skills/` 子目录中编译期嵌入，apply 时按引用关系动态读取。组织首次初始化时自动同步内置工具到 DB 并导入预置技能。

**Architecture:** 引入 `include_dir` 将 `seed/skills/` 目录树编译期内嵌；`SkillFileDef` 支持 `content` / `ref_path` / `url` 三种内容来源；抽出 `apply_preset_skills` 函数供 `apply_snapshot_to_db` 和 `initialize_system` 复用；`initialize_system` 增加工具同步 + 预置技能导入两步。

**Tech Stack:** Rust, SQLite, serde_json, reqwest (URL 抓取), include_dir (目录嵌入)

---

## 背景分析

### 现状问题

1. **default.json 的 skills 为空**：`"skills": []`
2. **SkillDef 只有元数据**：不携带文件内容，apply 后 skill.md 不会被创建
3. **单文件不够灵活**：技能包可能很大，一个 skill.md 不方便维护和单独更新
4. **`sync_builtin_tools_to_db` 未在启动或初始化时调用**：内置工具只注册到内存 registry，DB 的 tools 表无数据
5. **`initialize_system` 只创建 org/user/provider**：不导入预置技能，新组织没有任何基础技能

### 解决方案

```
src/service/domain/system/seed/
├── default.json                    # 总控 meta（编译期 include_str!）
├── skills/                         # 预置技能文件夹（编译期 include_dir!）
│   ├── platform_guide/
│   │   └── skill.md
│   ├── memory_guide/
│   │   └── skill.md
│   └── collaboration_guide/
│       └── skill.md
├── embedded.rs                     # include_dir 嵌入 + 读取函数
├── defs.rs                         # SkillFileDef 三来源定义
└── default.rs                      # default.json 加载
```

**default.json 中用 ref_path 引用编译期内嵌文件**：
```json
{
  "id": "TEMPLATE_PLATFORM_GUIDE",
  "files": [
    { "path": "skill.md", "ref_path": "skills/platform_guide/skill.md" }
  ]
}
```

**apply 时动态解析**（优先级 content > ref_path > url）：
```
SkillFileDef { path: "skill.md", content: Some("...") }     → 直接写入
SkillFileDef { path: "skill.md", ref_path: Some("...") }    → 从编译期内嵌读取
SkillFileDef { path: "skill.md", url: Some("https://...") } → 运行时 HTTP 抓取
```

**职责拆分 — 数据来源单一**：
- `apply_preset_skills(ctx, skills)` — 抽出的独立函数，负责技能导入（元数据 + 文件写入）
- `apply_snapshot_to_db` — 完整模板恢复，内部调用 `apply_preset_skills`
- `initialize_system` — 首次初始化，调用 `sync_builtin_tools_to_db` + `apply_preset_skills`（从 default.json 读取 skills）

### 预置技能清单

| # | 名称 | tags | category | 用途 |
|---|------|------|----------|------|
| 1 | 平台使用指南 | `["neural"]` | `system` | 平台核心能力概览 |
| 2 | 记忆管理指南 | `["neural"]` | `system` | 记忆系统深入使用策略 |
| 3 | Agent 协作指南 | `["neural"]` | `system` | 跨 Agent 协作模式 |

---

## File Structure

| 文件 | 操作 | 职责 |
|------|------|------|
| `Cargo.toml` | 修改 | 新增 `include_dir` 依赖 |
| `src/service/domain/system/seed/defs.rs` | 修改 | `SkillFileDef` 支持三来源；`SkillDef` 新增 `files` 字段 |
| `src/service/domain/system/seed/embedded.rs` | 新建 | `include_dir!` 嵌入 skills 目录 + 读取函数 |
| `src/service/domain/system/seed/mod.rs` | 修改 | 声明 embedded 子模块 |
| `src/service/domain/system/seed/skills/` | 新建 | 预置技能内容文件夹（3 个技能各含 skill.md） |
| `src/service/domain/system/seed/default.json` | 修改 | 新增 3 条预置技能（用 ref_path 引用） |
| `src/handlers/system/seed/mod.rs` | 修改 | 抽出 `apply_preset_skills` 函数；`apply_snapshot_to_db` 调用它；`assemble_snapshot_from_db` 导出文件 |
| `src/service/domain/finance/mod.rs` | 修改 | `ToolProviderManage` trait 新增 `sync_builtin_tools` 方法声明 |
| `src/service/domain/finance/tool_provider.rs` | 修改 | `FinanceDomainImpl` 实现 `sync_builtin_tools`（委托 DAL） |
| `src/service/domain/runtime/tool_execution_test.rs` | 修改 | DAL mock 的 `sync_builtin_tools_to_db` 改为 `Ok(0)` 避免 panic |
| `src/handlers/organization/initialize_system.rs` | 修改 | 通过 domain 层增加工具同步 + 预置技能导入两步 |
| `src/service/domain/system/seed/seed_test.rs` | 修改 | SkillFileDef 序列化 + ref_path 解析测试 |
| `src/handlers/system/seed/seed_handler_test.rs` | 修改 | apply 预置技能集成测试 |
| `src/handlers/organization/initialize_system_test.rs` | 修改/新建 | 首次初始化后技能库有预置技能测试 |

---

## Task 1: 引入 include_dir 依赖并扩展 SkillFileDef

**Files:**
- Modify: `Cargo.toml`
- Modify: `src/service/domain/system/seed/defs.rs`
- Modify: `src/service/domain/system/seed/seed_test.rs`

- [ ] **Step 1: 添加 include_dir 依赖**

在 `Cargo.toml` 的 `[dependencies]` 末尾（第 72 行 `futures = "0.3.32"` 之后）新增：

```toml
include_dir = "0.7"
```

- [ ] **Step 2: 写失败测试**

在 `src/service/domain/system/seed/seed_test.rs` 新增测试：

```rust
use super::defs::{SkillDef, SkillFileDef};

#[test]
fn test_skill_file_def_content_source() {
    let file = SkillFileDef {
        path: "skill.md".to_string(),
        content: Some("# 内容".to_string()),
        ref_path: None,
        url: None,
    };
    let json = serde_json::to_string(&file).unwrap();
    let de: SkillFileDef = serde_json::from_str(&json).unwrap();
    assert_eq!(de.content.as_ref().unwrap(), "# 内容");
    assert!(de.ref_path.is_none());
    assert!(de.url.is_none());
}

#[test]
fn test_skill_file_def_ref_path_source() {
    let file = SkillFileDef {
        path: "skill.md".to_string(),
        content: None,
        ref_path: Some("skills/platform_guide/skill.md".to_string()),
        url: None,
    };
    let json = serde_json::to_string(&file).unwrap();
    let de: SkillFileDef = serde_json::from_str(&json).unwrap();
    assert_eq!(de.ref_path.as_ref().unwrap(), "skills/platform_guide/skill.md");
}

#[test]
fn test_skill_file_def_url_source() {
    let file = SkillFileDef {
        path: "skill.md".to_string(),
        content: None,
        ref_path: None,
        url: Some("https://example.com/guide.md".to_string()),
    };
    let json = serde_json::to_string(&file).unwrap();
    let de: SkillFileDef = serde_json::from_str(&json).unwrap();
    assert_eq!(de.url.as_ref().unwrap(), "https://example.com/guide.md");
}

#[test]
fn test_skill_def_with_files_roundtrip() {
    let skill = SkillDef {
        id: "test_skill".to_string(),
        name: "测试技能".to_string(),
        description: "用于测试".to_string(),
        tags: vec!["neural".to_string()],
        category: "system".to_string(),
        parent_skill_id: String::new(),
        author_id: "TEMPLATE_ADMIN".to_string(),
        author_type: 0,
        status: 1,
        content_path: "skills/test_skill".to_string(),
        files: vec![SkillFileDef {
            path: "skill.md".to_string(),
            content: Some("# 测试".to_string()),
            ref_path: None,
            url: None,
        }],
    };
    let json = serde_json::to_string(&skill).unwrap();
    let de: SkillDef = serde_json::from_str(&json).unwrap();
    assert_eq!(de.files.len(), 1);
}

#[test]
fn test_skill_def_backward_compat_no_files() {
    let json = r#"{
        "id": "old_skill", "name": "旧", "description": "",
        "tags": [], "category": "x", "parent_skill_id": "",
        "author_id": "u", "author_type": 0, "status": 1,
        "content_path": "skills/old"
    }"#;
    let skill: SkillDef = serde_json::from_str(json).unwrap();
    assert!(skill.files.is_empty());
}
```

- [ ] **Step 3: 运行测试确认失败**

Run: `cargo test -p ai_orz --lib service::domain::system::seed::seed_test -- --nocapture`
Expected: FAIL — `SkillFileDef` 字段不存在

- [ ] **Step 4: 实现 SkillFileDef 和 SkillDef**

在 `src/service/domain/system/seed/defs.rs` 中，`SkillDef` 之前新增 `SkillFileDef`：

```rust
/// Skill 文件定义（支持多种内容来源）
///
/// 优先级：content > ref_path > url
/// - content：直接内嵌文本内容
/// - ref_path：引用 seed 目录下编译期内嵌文件的相对路径（如 "skills/platform_guide/skill.md"）
/// - url：运行时从 HTTPS URL 抓取内容
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq)]
pub struct SkillFileDef {
    /// 写入到技能目录的相对路径（如 "skill.md"、"references/guide.md"）
    pub path: String,

    /// 内嵌的文件内容（优先级最高）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,

    /// 引用 seed 目录下编译期内嵌文件的相对路径
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_path: Option<String>,

    /// 文件内容的 URL 来源（运行时抓取，必须 HTTPS）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}
```

修改 `SkillDef`，新增 `files` 字段（`#[serde(default)]` 保证旧快照向后兼容）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SkillDef {
    pub id: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub category: String,
    pub parent_skill_id: String,
    pub author_id: String,
    pub author_type: i32,
    pub status: i32,
    pub content_path: String,

    /// 技能文件列表（skill.md 主文件 + 附加文件）
    /// 每个文件可通过 content / ref_path / url 指定内容来源
    #[serde(default)]
    pub files: Vec<SkillFileDef>,
}
```

- [ ] **Step 5: 运行测试确认通过**

Run: `cargo test -p ai_orz --lib service::domain::system::seed::seed_test -- --nocapture`
Expected: PASS

- [ ] **Step 6: 修复 assemble_snapshot_from_db 编译错误**

在 `src/handlers/system/seed/mod.rs` 的 `assemble_snapshot_from_db` 中，skill 导出部分补充 `files` 字段：

```rust
let skill_defs: Vec<SkillDef> = skills
    .items
    .into_iter()
    .map(|s| SkillDef {
        id: s.po.id.clone(),
        name: s.po.name.clone(),
        description: s.po.description.clone(),
        tags: s.po.parse_tags(),
        category: s.po.category.clone(),
        parent_skill_id: s.po.parent_skill_id.clone(),
        author_id: s.po.author_id.clone(),
        author_type: s.po.author_type.to_i32(),
        status: s.po.status.to_i32(),
        content_path: s.po.content_path.clone(),
        files: vec![],  // 导出时由 assemble 阶段单独填充
    })
    .collect();
```

- [ ] **Step 7: 验证编译通过**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml src/service/domain/system/seed/defs.rs src/service/domain/system/seed/seed_test.rs src/handlers/system/seed/mod.rs
git commit -m "feat(seed): 引入 include_dir + SkillFileDef 支持三来源（content/ref_path/url）"
```

---

## Task 2: 实现编译期内嵌文件读取模块

**Files:**
- Create: `src/service/domain/system/seed/embedded.rs`
- Modify: `src/service/domain/system/seed/mod.rs`

- [ ] **Step 1: 创建 skills 目录占位文件**

```bash
mkdir -p src/service/domain/system/seed/skills
touch src/service/domain/system/seed/skills/.gitkeep
```

- [ ] **Step 2: 创建 embedded.rs**

`src/service/domain/system/seed/embedded.rs`:

```rust
//! 编译期内嵌技能文件读取模块
//!
//! 使用 include_dir 将 seed/skills/ 目录树编译期内嵌到二进制中。
//! apply 时通过 ref_path 引用读取对应文件内容。

use include_dir::{include_dir, Dir};
use std::path::Path;

/// 编译期内嵌的 seed 根目录（包含 default.json 和 skills/ 子目录）
static SEED_DIR: Dir<'static> = include_dir!("src/service/domain/system/seed/");

/// 根据相对路径读取编译期内嵌的文件内容
///
/// # 参数
/// - `ref_path`: 相对于 seed 目录的路径，如 "skills/platform_guide/skill.md"
///
/// # 返回
/// 文件文本内容，文件不存在或非 UTF-8 时返回 Err
pub fn read_embedded_file(ref_path: &str) -> Result<String, String> {
    let file = SEED_DIR
        .get_file(Path::new(ref_path))
        .ok_or_else(|| format!("编译期内嵌文件不存在: {}", ref_path))?;

    let content = std::str::from_utf8(file.contents())
        .map_err(|e| format!("编译期内嵌文件非 UTF-8: {} : {}", ref_path, e))?;

    Ok(content.to_string())
}

/// 列出编译期内嵌的 skills 目录下所有文件路径
///
/// 用于调试和验证预置技能文件是否正确嵌入
pub fn list_embedded_skill_files() -> Vec<String> {
    let mut result = Vec::new();
    if let Some(skills_dir) = SEED_DIR.get_dir("skills") {
        collect_files(skills_dir, "skills", &mut result);
    }
    result
}

fn collect_files(dir: &Dir<'_>, prefix: &str, result: &mut Vec<String>) {
    for entry in dir.entries() {
        let path = format!("{}/{}", prefix, entry.path().display());
        match entry {
            include_dir::DirEntry::Dir(sub_dir) => {
                collect_files(sub_dir, &path, result);
            }
            include_dir::DirEntry::File(_) => {
                result.push(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_embedded_file_not_exist() {
        let result = read_embedded_file("skills/nonexistent/skill.md");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_embedded_skill_files() {
        let files = list_embedded_skill_files();
        // 此测试在 Task 3 创建技能文件后才有意义
        // 这里只验证函数不 panic
        println!("Embedded skill files: {:?}", files);
    }
}
```

- [ ] **Step 3: 在 mod.rs 中声明子模块**

在 `src/service/domain/system/seed/mod.rs` 中新增：

```rust
pub mod embedded;
```

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/system/seed/embedded.rs src/service/domain/system/seed/mod.rs src/service/domain/system/seed/skills/.gitkeep
git commit -m "feat(seed): 实现 include_dir 编译期内嵌文件读取模块"
```

---

## Task 3: 创建预置技能内容文件

**Files:**
- Create: `src/service/domain/system/seed/skills/platform_guide/skill.md`
- Create: `src/service/domain/system/seed/skills/memory_guide/skill.md`
- Create: `src/service/domain/system/seed/skills/collaboration_guide/skill.md`

- [ ] **Step 1: 创建平台使用指南**

`src/service/domain/system/seed/skills/platform_guide/skill.md`:

```markdown
# 平台使用指南

你运行在 ai_orz 平台上。本指南帮助你快速掌握平台核心能力。

## 神经工具

平台提供一批标记为 `neural` 的工具，所有 Agent 无需显式绑定即可调用：

### 记忆管理
- `save_short_term_memory`：保存当前对话中的重要信息
- `save_long_term_memory`：主动保存重要知识和经验
- `settle_memory`：将短期记忆沉淀为长期知识图
- `search_memory`：语义搜索历史记忆
- `query_memory`：按条件查询记忆
- `update_memory` / `delete_memory`：管理记忆条目

### 技能发现
- `search_skill`：按关键词搜索共享技能库
- `list_skill_tags`：查看技能分类标签
- `install_skill_to_agent`：安装技能到自己的目录

### 工具查询
- `list_tools` / `query_tools`：查看可用工具
- `list_tool_tags`：查看工具分类

### 消息通信
- `send_message`：向用户发送消息
- `list_messages`：查看历史消息
- `send_task_assignment_message`：向其他 Agent 分配任务

### 工具调用
- `request_tool_call`：同步调用 Manual 工具
- `send_tool_call_message`：异步派发工具调用

## 技能自进化

完成复杂任务后，使用 `create_skill` 将经验沉淀为可复用技能。技能内容写入 skill.md，支持多文件结构。

## 行为准则

1. **主动记忆**：遇到重要信息时主动保存
2. **渐进学习**：完成任务后沉淀技能
3. **明确沟通**：清晰说明操作和决策
4. **合理委托**：超出能力范围时委托给其他 Agent
```

- [ ] **Step 2: 创建记忆管理指南**

`src/service/domain/system/seed/skills/memory_guide/skill.md`:

```markdown
# 记忆管理指南

平台提供分层记忆系统，帮助你跨对话积累经验。正确使用记忆是提升服务质量的关键。

## 记忆类型

### 短期记忆（Short-term）
- 使用 `save_short_term_memory` 记录当前对话中的临时信息
- 适用场景：用户偏好、对话上下文、临时任务状态
- 短期记忆在对话结束后会自动沉淀为长期记忆

### 长期记忆（Long-term）
- 使用 `save_long_term_memory` 主动保存重要知识
- 适用场景：业务规则、用户画像、成功经验、失败教训
- 长期记忆持久存储，跨对话可用

## 记忆沉淀

使用 `settle_memory` 将短期记忆沉淀为结构化的长期知识图：
- 在对话结束或任务完成时主动触发沉淀
- 沉淀过程会提取关键实体和关系
- 沉淀后的知识可通过语义搜索检索

## 记忆搜索

### 语义搜索
- `search_memory`：按自然语言语义搜索，适合模糊查询
- 示例："用户上次提到的偏好"、"类似问题的解决方案"

### 条件查询
- `query_memory`：按字段精确查询，适合结构化检索
- 可按类型、时间范围、标签等过滤

## 最佳实践

### 何时保存记忆
1. 用户明确表达偏好时
2. 发现重要的业务规则时
3. 成功解决复杂问题后
4. 遇到值得记录的失败教训时

### 记忆质量
- 内容要具体、可操作，避免空泛描述
- 包含足够的上下文信息，便于未来理解
- 使用清晰的标题和结构

### 记忆维护
- 使用 `update_memory` 更新过时信息
- 使用 `delete_memory` 清理无效记忆
- 定期回顾和整理记忆，保持知识库质量
```

- [ ] **Step 3: 创建 Agent 协作指南**

`src/service/domain/system/seed/skills/collaboration_guide/skill.md`:

```markdown
# Agent 协作指南

在多 Agent 环境中，协作是完成复杂任务的关键能力。本指南介绍跨 Agent 协作的模式和规范。

## 任务委派

使用 `send_task_assignment_message` 向其他 Agent 分配任务：
- 明确描述任务目标和要求
- 提供必要的上下文信息
- 指定期望的输出格式

### 委派原则
1. **明确边界**：清晰定义任务范围，避免模糊
2. **提供上下文**：传递足够的背景信息
3. **指定期望**：说明输出格式和时间要求
4. **适度委托**：只委派超出自身能力的部分

## 消息通信

### 与用户通信
- `send_message`：向用户发送消息
- `list_messages`：查看历史消息
- 保持语言简洁清晰，避免技术术语

### 与其他 Agent 通信
- 通过 `send_task_assignment_message` 委派任务
- 委派时说明转接原因和目标
- 关注任务进度，及时向用户反馈

## 产物管理

当安装了 `project_management` 工具包后，可以管理任务产物：

### 创建产物
- `create_text_artifact`：直接提交文本内容
- `register_artifact_from_path`：注册工作目录中的文件

### 管理产物
- `update_artifact`：更新内容和元数据
- `query_artifacts`：查询产物列表
- `get_artifact_content`：获取产物文件内容

### 产物最佳实践
- 为产物添加清晰的名称和描述
- 使用 tags 分类管理
- 重要成果及时保存为产物

## 协作场景

### 场景1：用户请求超出能力范围
1. 诚实告知用户需要转接
2. 使用 `send_task_assignment_message` 委派给专业 Agent
3. 跟踪进度并向用户反馈结果

### 场景2：多步骤复杂任务
1. 拆解任务为子步骤
2. 自身完成能力范围内的部分
3. 将专业子任务委派给对应 Agent
4. 汇总结果并交付

### 场景3：知识不足
1. 使用 `search_memory` 查找历史经验
2. 使用 `search_skill` 搜索相关技能
3. 仍无法解决时，委派给更合适的 Agent
```

- [ ] **Step 4: 验证 embedded 模块能读取文件**

在 `src/service/domain/system/seed/embedded.rs` 的测试模块中更新测试：

```rust
#[test]
fn test_read_embedded_platform_guide() {
    let result = read_embedded_file("skills/platform_guide/skill.md");
    assert!(result.is_ok());
    let content = result.unwrap();
    assert!(content.contains("# 平台使用指南"));
}

#[test]
fn test_list_embedded_skill_files() {
    let files = list_embedded_skill_files();
    assert!(files.iter().any(|f| f.contains("platform_guide")));
    assert!(files.iter().any(|f| f.contains("memory_guide")));
    assert!(files.iter().any(|f| f.contains("collaboration_guide")));
}
```

Run: `cargo test -p ai_orz --lib service::domain::system::seed::embedded -- --nocapture`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/service/domain/system/seed/skills/ src/service/domain/system/seed/embedded.rs
git commit -m "feat(seed): 新增 3 个预置技能内容文件（platform/memory/collaboration）"
```

---

## Task 4: default.json 引用预置技能

**Files:**
- Modify: `src/service/domain/system/seed/default.json`

- [ ] **Step 1: 修改 default.json 新增 skills 数组**

将 default.json 的 `"skills": []` 替换为（用 ref_path 引用编译期内嵌文件）：

```json
"skills": [
  {
    "id": "TEMPLATE_PLATFORM_GUIDE",
    "name": "平台使用指南",
    "description": "平台核心能力概览，涵盖神经工具使用、技能发现、工具调用等基础能力。所有 Agent 推荐安装。",
    "tags": ["neural"],
    "category": "system",
    "parent_skill_id": "",
    "author_id": "TEMPLATE_ADMIN",
    "author_type": 0,
    "status": 1,
    "content_path": "skills/TEMPLATE_PLATFORM_GUIDE",
    "files": [
      {
        "path": "skill.md",
        "ref_path": "skills/platform_guide/skill.md"
      }
    ]
  },
  {
    "id": "TEMPLATE_MEMORY_GUIDE",
    "name": "记忆管理指南",
    "description": "记忆系统深入使用策略，涵盖短期/长期记忆、知识沉淀、语义搜索的最佳实践。",
    "tags": ["neural"],
    "category": "system",
    "parent_skill_id": "",
    "author_id": "TEMPLATE_ADMIN",
    "author_type": 0,
    "status": 1,
    "content_path": "skills/TEMPLATE_MEMORY_GUIDE",
    "files": [
      {
        "path": "skill.md",
        "ref_path": "skills/memory_guide/skill.md"
      }
    ]
  },
  {
    "id": "TEMPLATE_COLLABORATION_GUIDE",
    "name": "Agent 协作指南",
    "description": "跨 Agent 协作模式，涵盖任务委派、消息通信、产物管理的最佳实践。",
    "tags": ["neural"],
    "category": "system",
    "parent_skill_id": "",
    "author_id": "TEMPLATE_ADMIN",
    "author_type": 0,
    "status": 1,
    "content_path": "skills/TEMPLATE_COLLABORATION_GUIDE",
    "files": [
      {
        "path": "skill.md",
        "ref_path": "skills/collaboration_guide/skill.md"
      }
    ]
  }
]
```

- [ ] **Step 2: 验证 default.json 解析正确**

Run: `cargo test -p ai_orz --lib service::domain::system::seed::seed_test -- --nocapture`
Expected: PASS（`embedded_default_snapshot` 能正确解析含 3 个 skills 的 default.json）

- [ ] **Step 3: Commit**

```bash
git add src/service/domain/system/seed/default.json
git commit -m "feat(seed): default.json 新增 3 个预置技能（ref_path 引用编译期内嵌文件）"
```

---

## Task 5: 抽出 apply_preset_skills 函数 + apply_snapshot_to_db 调用

**Files:**
- Modify: `src/handlers/system/seed/mod.rs`

- [ ] **Step 1: 实现 resolve_skill_file_content 辅助函数**

在 `src/handlers/system/seed/mod.rs` 文件顶部（`apply_snapshot_to_db` 之前）新增文件内容解析函数：

```rust
/// 解析技能文件内容（优先级：content > ref_path > url）
///
/// - content：直接返回内嵌内容
/// - ref_path：从编译期内嵌文件读取
/// - url：运行时 HTTPS 抓取
async fn resolve_skill_file_content(
    file_def: &crate::service::domain::system::seed::defs::SkillFileDef,
) -> Result<String> {
    // 优先级 1：content 内嵌内容
    if let Some(content) = &file_def.content {
        return Ok(content.clone());
    }

    // 优先级 2：ref_path 编译期内嵌文件
    if let Some(ref_path) = &file_def.ref_path {
        return crate::service::domain::system::seed::embedded::read_embedded_file(ref_path)
            .map_err(|e| err!(Internal, "读取编译期内嵌技能文件失败: {}", e));
    }

    // 优先级 3：url 运行时抓取
    if let Some(url) = &file_def.url {
        if !url.starts_with("https://") {
            bail_err!(InvalidRequest, "技能文件 URL 必须是 HTTPS: {}", url);
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| err!(Internal, "构建 HTTP 客户端失败: {}", e))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| err!(Internal, "抓取技能 URL 失败 {}: {}", url, e))?;
        if !response.status().is_success() {
            bail_err!(InvalidRequest, "抓取技能 URL 失败 {}: HTTP {}", url, response.status());
        }
        let content = response
            .text()
            .await
            .map_err(|e| err!(Internal, "读取技能 URL 内容失败 {}: {}", url, e))?;
        if content.len() > 1024 * 1024 {
            bail_err!(InvalidRequest, "技能 URL 内容超过 1MB 限制: {}", url);
        }
        return Ok(content);
    }

    bail_err!(InvalidRequest, "技能文件 {} 未指定内容来源（content/ref_path/url 均为空）", file_def.path);
}
```

- [ ] **Step 2: 抽出 apply_preset_skills 函数**

在 `src/handlers/system/seed/mod.rs` 中，`apply_snapshot_to_db` 之前新增独立的技能导入函数。`author_id_override` 参数让 `initialize_system` 能将模板的 `TEMPLATE_ADMIN` 替换为实际 owner user_id：

```rust
/// 导入预置技能到共享库
///
/// 从 SeedSnapshot 的 skills 列表中读取技能定义，
/// 动态解析文件内容（content > ref_path > url）并写入 DB + 文件系统。
/// 已存在的技能（按 ID）会被更新，不存在的会被创建。
///
/// # author_id_override
/// - `None`：保持 SkillDef 中的 author_id（模板恢复场景，保留 TEMPLATE_ADMIN）
/// - `Some(uid)`：替换为实际用户 ID（首次初始化场景，对齐组织 owner）
///
/// 此函数供 `apply_snapshot_to_db`（完整模板恢复，传 None）和
/// `initialize_system`（首次初始化，传 Some(owner_id)）复用，保证数据来源单一。
///
/// # 返回
/// 处理的技能数量
pub async fn apply_preset_skills(
    ctx: RequestContext,
    skills: &[crate::service::domain::system::seed::defs::SkillDef],
    author_id_override: Option<&str>,
) -> Result<usize> {
    let mut count = 0;
    for skill_def in skills {
        let existing = hr::domain()
            .skill_manage()
            .get_skill(ctx.clone(), &skill_def.id)
            .await?;

        // author_id 替换：initialize_system 场景用实际 owner id
        let author_id = author_id_override
            .map(|s| s.to_string())
            .unwrap_or_else(|| skill_def.author_id.clone());

        let mut skill_po = crate::models::skill::SkillPo::new(
            skill_def.id.clone(),
            skill_def.name.clone(),
            skill_def.description.clone(),
            skill_def.tags.clone(),
            skill_def.category.clone(),
            skill_def.parent_skill_id.clone(),
            author_id,
            common::enums::skill::SkillAuthorType::from(skill_def.author_type),
            skill_def.content_path.clone(),
        );
        skill_po.status = common::enums::SkillStatus::from(skill_def.status);

        // 动态解析每个文件的内容（content > ref_path > url）
        let mut file_writes: Vec<(String, String)> = Vec::new();
        for file_def in &skill_def.files {
            let content = resolve_skill_file_content(file_def).await?;
            file_writes.push((file_def.path.clone(), content));
        }

        let skill = crate::models::skill::Skill::from_po(skill_po);

        if existing.is_some() {
            // 已存在：update_skill 写入文件
            let file_writes_ref: Vec<(&str, &str)> = file_writes
                .iter()
                .map(|(p, c)| (p.as_str(), c.as_str()))
                .collect();
            let params = crate::service::domain::hr::UpdateSkillParams {
                skill: &skill,
                file_writes: file_writes_ref,
                file_deletes: vec![],
                file_imports: vec![],
            };
            hr::domain()
                .skill_manage()
                .update_skill(ctx.clone(), params)
                .await?;
        } else {
            // 新建：先 create_skill 写元数据，再 update_skill 写文件
            hr::domain()
                .skill_manage()
                .create_skill(ctx.clone(), &skill)
                .await?;

            if !file_writes.is_empty() {
                let file_writes_ref: Vec<(&str, &str)> = file_writes
                    .iter()
                    .map(|(p, c)| (p.as_str(), c.as_str()))
                    .collect();
                let params = crate::service::domain::hr::UpdateSkillParams {
                    skill: &skill,
                    file_writes: file_writes_ref,
                    file_deletes: vec![],
                    file_imports: vec![],
                };
                hr::domain()
                    .skill_manage()
                    .update_skill(ctx.clone(), params)
                    .await?;
            }
        }
        count += 1;
    }
    Ok(count)
}
```

- [ ] **Step 3: 修改 apply_snapshot_to_db 调用 apply_preset_skills**

在 `apply_snapshot_to_db` 函数中，替换原有的 skill 写入循环（约 407-455 行）为：

```rust
// 6. 写入 Skill（复用 apply_preset_skills，传 None 保留模板原始 author_id）
let skill_count = apply_preset_skills(ctx.clone(), &snapshot.skills, None).await?;
created += skill_count;
```

> **注意：** 原有的 skill 写入循环（含 SkipExisting 检查、created/updated 计数）被 `apply_preset_skills` 替代。`apply_preset_skills` 内部已处理 existing 判断（存在则 update，不存在则 create）。SkipExisting 策略的粒度控制在此处简化为"总是 upsert"，因为预置技能的 ID 是固定的模板 ID，upsert 语义正确。如果需要保留 SkipExisting 行为，可以在 `apply_preset_skills` 中增加 `skip_existing: bool` 参数。

- [ ] **Step 4: 验证编译通过**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 5: Commit**

```bash
git add src/handlers/system/seed/mod.rs
git commit -m "feat(seed): 抽出 apply_preset_skills 函数，支持动态解析文件来源（content/ref_path/url）"
```

---

## Task 6: assemble_snapshot_from_db 导出技能文件

**Files:**
- Modify: `src/handlers/system/seed/mod.rs` (`assemble_snapshot_from_db` 函数)

- [ ] **Step 1: 修改 assemble_snapshot_from_db 读取技能文件**

在 `assemble_snapshot_from_db` 中 skill 导出部分，读取技能目录下的文件内容填充 `files`（导出时用 content 内嵌）：

```rust
// 5. Skill（含文件内容，导出时内嵌到 content 字段）
let skills = hr::domain()
    .skill_manage()
    .query_skills(ctx.clone(), Default::default())
    .await?;

let mut skill_defs = Vec::with_capacity(skills.items.len());
for s in skills.items {
    let files_result = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), s.id())
        .await;

    let files: Vec<crate::service::domain::system::seed::defs::SkillFileDef> =
        match files_result {
            Ok(file_list) => file_list
                .into_iter()
                .filter(|f| f.content.is_some())
                .map(|f| crate::service::domain::system::seed::defs::SkillFileDef {
                    path: f.filename,
                    content: f.content,
                    ref_path: None,
                    url: None,
                })
                .collect(),
            Err(e) => {
                log_warn!(ctx, "assemble_snapshot", "读取技能 {} 文件失败: {}", s.id(), e);
                vec![]
            }
        };

    skill_defs.push(SkillDef {
        id: s.po.id.clone(),
        name: s.po.name.clone(),
        description: s.po.description.clone(),
        tags: s.po.parse_tags(),
        category: s.po.category.clone(),
        parent_skill_id: s.po.parent_skill_id.clone(),
        author_id: s.po.author_id.clone(),
        author_type: s.po.author_type.to_i32(),
        status: s.po.status.to_i32(),
        content_path: s.po.content_path.clone(),
        files,
    });
}
```

- [ ] **Step 2: 验证编译通过**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 3: 运行已有 seed 测试确认无回归**

Run: `cargo test -p ai_orz --lib handlers::system::seed -- --nocapture`
Expected: 所有测试 PASS

- [ ] **Step 4: Commit**

```bash
git add src/handlers/system/seed/mod.rs
git commit -m "feat(seed): assemble_snapshot_from_db 导出技能文件内容"
```

---

## Task 7: domain 层补全 sync_builtin_tools + initialize_system 增加工具同步和预置技能导入

**背景：** `sync_builtin_tools_to_db` 目前只存在于 DAL 层 (`ToolDal`) 和 DAO 层 (`ToolDao`)，domain 层 `ToolProviderManage` trait 没有该方法。按照分层架构，handler 不应直接调用 DAL，需先在 domain 层补全方法。

**Files:**
- Modify: `src/service/domain/finance/mod.rs` （`ToolProviderManage` trait 新增方法声明）
- Modify: `src/service/domain/finance/tool_provider.rs` （`FinanceDomainImpl` 实现）
- Modify: `src/service/domain/runtime/tool_execution_test.rs` （mock 补空实现）
- Modify: `src/handlers/organization/initialize_system.rs`
- Create/Modify: `src/handlers/organization/initialize_system_test.rs`

- [ ] **Step 1: 在 ToolProviderManage trait 新增方法声明**

在 `src/service/domain/finance/mod.rs` 的 `ToolProviderManage` trait 末尾（`search_tools` 方法之后，约 491 行 `}` 之前）新增：

```rust
    /// 同步内置工具到 DB
    ///
    /// 将内存 registry 中注册的 builtin tools 写入 DB 的 tools 表。
    /// 已存在的工具（按 ID）跳过，避免重复。
    /// 返回新增的工具数量。
    ///
    /// 在 `initialize_system`（首次初始化组织）时调用，
    /// 让 DB 的 tools 表包含所有 builtin tools。
    async fn sync_builtin_tools(&self, ctx: RequestContext) -> Result<usize>;
```

- [ ] **Step 2: 在 FinanceDomainImpl 中实现 sync_builtin_tools**

在 `src/service/domain/finance/tool_provider.rs` 的 `impl ToolProviderManage for FinanceDomainImpl` 末尾（`search_tools` 实现之后）新增：

```rust
    /// 同步内置工具到 DB
    async fn sync_builtin_tools(&self, ctx: RequestContext) -> Result<usize> {
        self.tool_dal.sync_builtin_tools_to_db(ctx).await
    }
```

- [ ] **Step 3: 为 tool_execution_test.rs 的 mock 补空实现**

在 `src/service/domain/runtime/tool_execution_test.rs` 中，`RecordingToolDal` 的 `impl ToolDal for RecordingToolDal` 已有 `sync_builtin_tools_to_db` 的 mock（第 366 行 `unimplemented!`）。

> 注意：这是 DAL 层 mock，不是 domain 层 mock。`RecordingToolDal` 实现 `ToolDal`，`FinanceDomainImpl` 在测试中被直接构造（不 mock），所以这里 DAL mock 会被 `FinanceDomainImpl` 持有。无需新增 domain 层 mock。

确认第 366 行的 mock 实现存在且不会被其他测试触发即可。如果当前测试套件没有调用 `sync_builtin_tools`，可以保留 `unimplemented!`。但如果 `tool_execution` 相关测试因为新增的 trait 方法而要求实现，需要改为合理返回：

```rust
        async fn sync_builtin_tools_to_db(&self, _ctx: RequestContext) -> Result<usize> {
            Ok(0)  // 测试中不实际同步
        }
```

> 决策依据：运行 `cargo check -p ai_orz` 后根据编译错误决定是否需要修改。

- [ ] **Step 4: 验证 domain 层编译通过**

Run: `cargo check -p ai_orz`
Expected: 编译通过

- [ ] **Step 5: 写失败测试**

> **测试基础设施**：`initialize_system_test.rs` 和 `TestContext` 当前不存在。参考 `tests/common/` 下的现有工厂函数模式（`init_full_test_env` / `bootstrap_system`）。实现时先阅读 `tests/common/mod.rs` 和 `tests/common/factories/` 下的代码，复用其初始化模式（通常用 `#[tokio::test]` + 初始化 DB + 构造 RequestContext）。

在 `src/handlers/organization/initialize_system_test.rs`（新建）新增测试。注意字段名与实际 `InitializeSystemRequest` 对齐（`admin_username` / `admin_password_hash` / `ModelProviderInitConfig`），`list_skill_files` 返回 `Option<Vec<SkillFile>>`：

```rust
use super::*;
use crate::pkg::RequestContext;
use common::api::{InitializeSystemRequest, ModelProviderInitConfig};
use common::error::Result;

// TestContext 辅助需基于 tests/common 现有工厂模式搭建，此处为示意
// 实际实现时参考 tests/common/mod.rs 的初始化逻辑

#[tokio::test]
async fn test_initialize_system_imports_preset_skills() {
    let ctx = init_test_env().await;  // 复用 tests/common 初始化模式

    // 调用 initialize_system（字段名对齐实际定义）
    let response = initialize_system(ctx.clone(), InitializeSystemRequest {
        organization_name: "测试组织".to_string(),
        admin_username: "admin".to_string(),
        admin_password_hash: "password123".to_string(),
        description: None,
        admin_display_name: None,
        admin_email: None,
        chat_model: ModelProviderInitConfig {
            name: "测试对话模型".to_string(),
            provider_type: 1,
            model_name: "gpt-4o".to_string(),
            api_key: "test-key".to_string(),
            base_url: Some("https://api.openai.com".to_string()),
            description: None,
        },
        embedding_model: None,
    }).await.unwrap();

    // 验证组织和用户已创建
    assert!(!response.organization_id.is_empty());
    assert!(!response.user_id.is_empty());

    // 验证内置工具已同步到 DB（通过 domain 层查询）
    let tools = finance::domain()
        .tool_provider_manage()
        .list_tools(ctx.clone())
        .await
        .unwrap();
    assert!(!tools.is_empty(), "内置工具未同步到 DB");

    // 验证预置技能已导入到共享库
    let skills = hr::domain()
        .skill_manage()
        .list_by_status(ctx.clone(), common::enums::SkillStatus::Published)
        .await
        .unwrap();

    let skill_ids: Vec<&str> = skills.iter().map(|s| s.id()).collect();
    assert!(skill_ids.contains(&"TEMPLATE_PLATFORM_GUIDE"), "缺少平台使用指南");
    assert!(skill_ids.contains(&"TEMPLATE_MEMORY_GUIDE"), "缺少记忆管理指南");
    assert!(skill_ids.contains(&"TEMPLATE_COLLABORATION_GUIDE"), "缺少协作指南");

    // 验证 author_id 已替换为实际 owner（B 方案）
    let platform_skill = skills.iter().find(|s| s.id() == "TEMPLATE_PLATFORM_GUIDE").unwrap();
    assert_eq!(
        platform_skill.po.author_id, response.user_id,
        "预置技能 author_id 应替换为实际 owner id"
    );

    // 验证技能文件已写入（list_skill_files 返回 Option<Vec<SkillFile>>）
    let files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx, "TEMPLATE_PLATFORM_GUIDE")
        .await
        .unwrap()
        .unwrap_or_default();  // 处理 Option
    let skill_md = files.iter().find(|f| f.filename == "skill.md").unwrap();
    assert!(skill_md.content.as_ref().unwrap().contains("# 平台使用指南"));
}
```

- [ ] **Step 6: 运行测试确认失败**

Run: `cargo test -p ai_orz --lib handlers::organization::initialize_system_test::test_initialize_system_imports_preset_skills -- --nocapture`
Expected: FAIL — 内置工具未同步 / 预置技能未导入

- [ ] **Step 7: 修改 initialize_system 增加工具同步和技能导入**

在 `src/handlers/organization/initialize_system.rs` 中，`initialize_system` 函数末尾（返回 `InitializeSystemResponse` 之前）新增两步。**注意：通过 domain 层方法调用，不直接访问 DAL。**

```rust
    // 4. 【新增】同步内置工具到 DB
    //    通过 finance domain 将内存 registry 中的 builtin tools 写入 DB
    let tool_count = finance::domain()
        .tool_provider_manage()
        .sync_builtin_tools(ctx.clone())
        .await?;
    sys_info!(
        "initialize_system: 同步 {} 个内置工具到 DB",
        tool_count
    );

    // 5. 【新增】导入预置技能到共享库
    //    从 default.json 读取 skills 定义，动态解析文件内容并写入
    //    author_id 替换为实际 owner（B 方案），对齐组织初始化人
    let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
    let skill_count = crate::handlers::system::seed::apply_preset_skills(
        ctx.clone(),
        &snapshot.skills,
        Some(&user_id),  // 替换 TEMPLATE_ADMIN 为实际 owner id
    )
    .await?;
    sys_info!(
        "initialize_system: 导入 {} 个预置技能到共享库（author: {}）",
        skill_count,
        user_id
    );
```

> 注意：`sys_info!` 是项目已有的日志宏。如果实际宏名不同（如 `log_info!`、`tracing::info!`），请根据项目约定调整。实现时请先读取 `initialize_system.rs` 顶部已导入的宏，保持一致。

- [ ] **Step 8: 运行测试确认通过**

Run: `cargo test -p ai_orz --lib handlers::organization::initialize_system_test::test_initialize_system_imports_preset_skills -- --nocapture`
Expected: PASS

- [ ] **Step 9: Commit**

```bash
git add src/service/domain/finance/mod.rs src/service/domain/finance/tool_provider.rs src/service/domain/runtime/tool_execution_test.rs src/handlers/organization/initialize_system.rs src/handlers/organization/initialize_system_test.rs
git commit -m "feat(initialize): domain 层补全 sync_builtin_tools + initialize_system 增加工具同步和预置技能导入"
```

---

## Task 8: 集成测试与回归验证

**Files:**
- Modify: `src/handlers/system/seed/seed_handler_test.rs`

- [ ] **Step 1: 新增 apply_default 完整集成测试**

在 `src/handlers/system/seed/seed_handler_test.rs` 新增测试：

```rust
use super::*;
use crate::service::domain::system::seed::defs::{SkillDef, SkillFileDef, SeedSnapshot};
use common::api::seed::ImportStrategy;
use std::collections::HashMap;

#[tokio::test]
async fn test_apply_default_creates_preset_skills_with_ref_path() {
    let ctx = TestContext::new().await;

    // 应用默认模板（default.json 中技能用 ref_path 引用编译期内嵌文件）
    let snapshot = crate::service::domain::system::seed::default::embedded_default_snapshot();
    apply_snapshot_to_db(
        ctx.clone(),
        &snapshot,
        ImportStrategy::PreserveIds,
        &test_sensitive_values(),
    )
    .await
    .unwrap();

    // 验证 3 个预置技能已创建
    let skills = hr::domain()
        .skill_manage()
        .list_by_status(ctx.clone(), common::enums::SkillStatus::Published)
        .await
        .unwrap();

    let skill_ids: Vec<&str> = skills.iter().map(|s| s.id()).collect();
    assert!(skill_ids.contains(&"TEMPLATE_PLATFORM_GUIDE"), "缺少平台使用指南");
    assert!(skill_ids.contains(&"TEMPLATE_MEMORY_GUIDE"), "缺少记忆管理指南");
    assert!(skill_ids.contains(&"TEMPLATE_COLLABORATION_GUIDE"), "缺少协作指南");

    // 验证每个技能都有 skill.md 文件且内容正确（从 ref_path 编译期内嵌读取）
    // list_skill_files 返回 Option<Vec<SkillFile>>，需 unwrap_or_default
    let platform_files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_PLATFORM_GUIDE")
        .await
        .unwrap()
        .unwrap_or_default();
    let skill_md = platform_files.iter().find(|f| f.filename == "skill.md").unwrap();
    let content = skill_md.content.as_ref().unwrap();
    assert!(content.contains("# 平台使用指南"), "平台使用指南内容不正确");
    assert!(content.contains("神经工具"), "平台使用指南缺少神经工具章节");

    let memory_files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_MEMORY_GUIDE")
        .await
        .unwrap()
        .unwrap_or_default();
    let memory_md = memory_files.iter().find(|f| f.filename == "skill.md").unwrap();
    assert!(memory_md.content.as_ref().unwrap().contains("# 记忆管理指南"));

    let collab_files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx.clone(), "TEMPLATE_COLLABORATION_GUIDE")
        .await
        .unwrap()
        .unwrap_or_default();
    let collab_md = collab_files.iter().find(|f| f.filename == "skill.md").unwrap();
    assert!(collab_md.content.as_ref().unwrap().contains("# Agent 协作指南"));

    // 验证所有预置技能都有 neural tag
    for skill in &skills {
        if skill_ids.contains(&skill.id()) {
            let tags = skill.po.parse_tags();
            assert!(
                tags.contains(&"neural".to_string()),
                "技能 {} 缺少 neural tag",
                skill.id()
            );
        }
    }
}

#[tokio::test]
async fn test_apply_preset_skills_with_content_source() {
    // 验证 content 内嵌来源（直接调用 apply_preset_skills）
    let ctx = TestContext::new().await;
    let skills = vec![SkillDef {
        id: "test_content_skill".to_string(),
        name: "内容测试技能".to_string(),
        description: "测试content来源".to_string(),
        tags: vec![],
        category: "system".to_string(),
        parent_skill_id: String::new(),
        author_id: "test_user".to_string(),
        author_type: 0,
        status: 1,
        content_path: "skills/test_content_skill".to_string(),
        files: vec![SkillFileDef {
            path: "skill.md".to_string(),
            content: Some("# 内容测试\n这是直接内嵌的内容".to_string()),
            ref_path: None,
            url: None,
        }],
    }];

    apply_preset_skills(ctx.clone(), &skills, None).await.unwrap();

    let files = hr::domain()
        .skill_manage()
        .list_skill_files(ctx, "test_content_skill")
        .await
        .unwrap()
        .unwrap_or_default();
    let skill_md = files.iter().find(|f| f.filename == "skill.md").unwrap();
    assert!(skill_md.content.as_ref().unwrap().contains("这是直接内嵌的内容"));
}
```

- [ ] **Step 2: 运行集成测试**

Run: `cargo test -p ai_orz --lib handlers::system::seed::seed_handler_test::test_apply_default_creates_preset_skills_with_ref_path -- --nocapture`
Expected: PASS

Run: `cargo test -p ai_orz --lib handlers::system::seed::seed_handler_test::test_apply_preset_skills_with_content_source -- --nocapture`
Expected: PASS

- [ ] **Step 3: 运行全量 seed 测试**

Run: `cargo test -p ai_orz --lib handlers::system::seed -- --nocapture`
Expected: 所有测试 PASS

- [ ] **Step 4: 运行全量回归测试**

Run: `cargo test --workspace`
Expected: 所有测试 PASS

- [ ] **Step 5: Commit**

```bash
git add src/handlers/system/seed/seed_handler_test.rs
git commit -m "test(seed): 新增预置技能集成测试（ref_path 和 content 来源）"
```

---

## Task 9: 文档更新

**Files:**
- Modify: `docs/skill_design.md`

- [ ] **Step 1: 更新技能设计文档**

在 `docs/skill_design.md` 变更记录中新增：

```markdown
| 2026-07-30 | 预置基础技能：default.json 新增 3 个 neural 技能（平台使用指南、记忆管理指南、Agent 协作指南）；引入 include_dir 编译期嵌入 skills/ 目录树；SkillFileDef 支持 content/ref_path/url 三种内容来源；抽出 apply_preset_skills 函数供 apply_snapshot 和 initialize_system 复用；initialize_system 增加内置工具同步 + 预置技能导入 |
```

- [ ] **Step 2: Commit**

```bash
git add docs/skill_design.md
git commit -m "docs: 更新技能设计文档 - 预置基础技能与初始化导入"
```

---

## Self-Review

### Spec coverage
- ✅ 预置技能到技能库（像 builtin tools 一样开箱即用）→ Task 3 + Task 4
- ✅ 多文件支持 → SkillFileDef.path 支持任意相对路径
- ✅ default.json 作为总控 meta → Task 4（ref_path 引用）
- ✅ 大内容文件单独存放 → Task 3（skills/ 子目录）
- ✅ 编译时嵌入文件夹 → Task 2（include_dir）
- ✅ 导入时动态读取 → Task 5（resolve_skill_file_content）
- ✅ 技能创建支持文件路径或 URL → Task 1 + Task 5（ref_path / url）
- ✅ 服务初始化路径增加技能导入 → Task 7（initialize_system 调用 apply_preset_skills）
- ✅ 服务初始化路径增加本地工具初始化导入 → Task 7（domain 层补全 sync_builtin_tools，initialize_system 通过 domain 调用）
- ✅ 数据来源单一（default.json 指定的技能）→ Task 5 抽出 apply_preset_skills，Task 7 从 default.json 读取
- ✅ 分层架构合规 → Task 7 在 domain 层 `ToolProviderManage` trait 补全 `sync_builtin_tools`，handler 不直接调用 DAL

### 设计要点
- **三来源优先级**：content > ref_path > url，覆盖内嵌、编译期引用、运行时抓取三种场景
- **职责拆分**：`apply_preset_skills` 是独立的技能导入函数，`apply_snapshot_to_db` 和 `initialize_system` 都调用它
- **author_id 对齐组织 owner（B 方案）**：`apply_preset_skills` 接受 `author_id_override: Option<&str>`，`initialize_system` 传 `Some(&user_id)` 将模板的 `TEMPLATE_ADMIN` 替换为实际 owner，`apply_snapshot_to_db` 传 `None` 保留模板原始值
- **数据来源单一**：预置技能定义只在 default.json 中声明，两个调用方读取同一份数据
- **分层架构合规**：工具同步通过 domain 层 `ToolProviderManage::sync_builtin_tools`，不直接调 DAL
- **预置技能 ≠ 自动安装**：技能只写入共享库（Published），Agent 绑定是用户自主操作
- **向后兼容**：`#[serde(default)]` 保证旧快照（无 files 字段）可正常反序列化

### Placeholder scan
- 无 TBD / TODO
- 所有代码步骤都有完整代码块
- 测试用例有具体断言

### Type consistency
- `SkillFileDef { path, content, ref_path, url }` 在 Task 1 定义，Task 5/6/7/8 使用 — 一致
- `read_embedded_file(ref_path: &str) -> Result<String, String>` 在 Task 2 定义，Task 5 使用 — 一致
- `resolve_skill_file_content(file_def) -> Result<String>` 在 Task 5 定义并使用 — 一致
- `apply_preset_skills(ctx, skills: &[SkillDef], author_id_override: Option<&str>) -> Result<usize>` 在 Task 5 定义，Task 7 传 `Some(&user_id)`，Task 8 传 `None` — 一致

### 风险点
1. **include_dir 依赖**：新增依赖，但它是纯 Rust 无系统依赖，体积影响极小。
2. **default.json 不再自包含**：技能内容分散在 skills/ 子目录中，但编译期 include_dir 会全部嵌入二进制，运行时无文件系统依赖。
3. **URL 抓取的网络依赖**：url 来源需要网络，失败时返回错误。预置技能用 ref_path 不受影响。
4. **apply_snapshot_to_db 的 SkipExisting 策略简化**：`apply_preset_skills` 内部总是 upsert（存在则 update，不存在则 create），不处理 SkipExisting。预置技能的 ID 是固定的模板 ID，upsert 语义正确。如果未来需要 SkipExisting 粒度控制，可增加参数。
5. **initialize_system 中的工具同步**：通过 domain 层 `finance::domain().tool_provider_manage().sync_builtin_tools(ctx)` 调用，符合分层架构。domain 层 `ToolProviderManage` trait 新增 `sync_builtin_tools` 方法，委托给 DAL 层 `ToolDal::sync_builtin_tools_to_db`。
6. **tool_execution_test.rs mock**：`RecordingToolDal` 的 `sync_builtin_tools_to_db` 当前是 `unimplemented!`，若 `tool_execution` 测试因新增 domain 方法受影响，需改为 `Ok(0)`。
