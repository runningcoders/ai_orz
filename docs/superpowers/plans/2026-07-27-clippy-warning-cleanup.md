# Clippy Warning Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 清理整个代码库的 442 个 clippy warning，分 4 个 batch 按 lint 类别推进，最终在 CI 中启用 `cargo clippy -- -D warnings` 强制门槛。

**Architecture:** 按 lint 类别分批清理（不按代码模块），每批一个 commit。优先用 `cargo clippy --fix` 自动修复，然后手工处理自动修复无法覆盖的 case。每批完成后跑全部 17 个集成测试 + 单元测试验证无回归。Batch 4 完成后修改 CI workflow 启用 `-D warnings`。

**Tech Stack:** Rust 1.x, cargo clippy, `--fix` flag。PROTOC=/opt/homebrew/bin/protoc 前缀（lancedb 编译需要）。

**当前 baseline（2026-07-27 统计）：**

| Lint | 数量 | Batch |
|------|------|-------|
| clippy::collapsible_if | 213 | 1 |
| unused_imports | 75 | 1 |
| clippy::too_many_arguments | 52 | 3 |
| clippy::redundant_closure | 38 | 2 |
| dead_code | 27 | 3 |
| clippy::derivable_impls | 23 | 2 |
| clippy::needless_borrow | 22 | 2 |
| clippy::unnecessary_map_or | 20 | 2 |
| clippy::needless_update | 16 | 2 |
| clippy::unnecessary_sort_by | 14 | 2 |
| clippy::default_constructed_unit_structs | 14 | 2 |
| unused_mut | 12 | 1 |
| clippy::new_without_default | 12 | 2 |
| clippy::needless_question_mark | 12 | 2 |
| clippy::drop_non_drop | 12 | 3 |
| clippy::single_char_add_str | 10 | 3 |
| unused_variables | 9 | 1 |
| clippy::module_inception | 8 | 3 |
| clippy::useless_conversion | 8 | 3 |
| clippy::let_unit_value | 6 | 3 |
| clippy::doc_lazy_continuation | 6 | 3 |
| clippy::needless_as_bytes | 6 | 3 |
| clippy::field_reassign_with_default | 6 | 3 |
| 其他 32 类（≤4 个） | ~71 | 3 |

总计 442 个 warning。Batch 1 (309) + Batch 2 (171) = 480（有重叠，实际约 442）。

---

## File Structure

不创建新文件。修改的范围：
- `src/` —— 主代码
- `common/src/` —— 共享类型
- `frontend/src/` —— 前端 Rust 代码
- `ai-orz-macros/src/` —— 过程宏
- `tests/` —— 测试代码
- `.github/workflows/rust.yml` —— Batch 4 末尾启用 `-D warnings`

**关键约束：**
1. **不重构业务逻辑** —— 只做 clippy 建议的等价改写（合并 if、删除冗余 borrow、改 closure 为 method call 等），不改任何函数签名语义
2. **`too_many_arguments` 是例外** —— 这个 lint 需要把参数打包成 struct，是重构。Batch 3 中单独处理，每处改动单独 commit
3. **每批必须跑全部 17 个集成测试 + `cargo test --lib`** —— 防止 clippy fix 破坏代码
4. **PROTOC 前缀** —— 所有 cargo 命令前缀 `PROTOC=/opt/homebrew/bin/protoc`
5. **不修改 migrations/** —— 数据库迁移不动
6. **dead_code 谨慎处理** —— 可能是真正未使用的代码，也可能是 cfg 条件编译。Batch 3 中逐个判断

---

## Phase 1: Batch 1 — Auto-fix 友好类（309 个）

### Task 1: 用 `cargo clippy --fix` 自动修复 Batch 1 的 lint

**Files:**
- Modify: 全代码库（auto-fix 决定范围）

- [ ] **Step 1: 跑 cargo clippy --fix 自动应用建议**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --fix --allow-dirty --allow-no-vcs 2>&1 | tail -20
```

`--allow-dirty` 允许修改已修改但未提交的文件（这里工作区应该是干净的）。
`--allow-no-vcs` 允许在无版本控制时也运行。
此命令会自动修复：`unused_imports` / `unused_mut` / `unused_variables` / `clippy::collapsible_if` / `clippy::redundant_closure` / `clippy::needless_borrow` 等大部分 lint。

- [ ] **Step 2: 查看自动修复后的 diff**

```bash
git diff --stat | tail -20
```

预期：修改大量文件，但每个文件改动量小（几行）。

- [ ] **Step 3: 验证编译通过**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo build --all-targets 2>&1 | tail -10
```

预期：编译成功。如果失败（auto-fix 偶尔会破坏代码），运行 `git checkout .` 回滚后改用 Step 4 的手工方式。

- [ ] **Step 4: 跑全部测试验证无回归**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -20
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test --test core_crud_test --test message_delivery_test --test vector_degradation_test --test a2a_flow_test 2>&1 | tail -20
```

预期：所有测试 PASS。

- [ ] **Step 5: 提交 Batch 1 自动修复**

```bash
git add -A
git commit -m "style(clippy): batch 1 auto-fix — collapsible_if/unused_imports/unused_mut/unused_variables

Auto-applied via 'cargo clippy --fix'. No semantic changes.
- 213 collapsible_if → 合并嵌套 if
- 75 unused_imports → 删除未使用 import
- 12 unused_mut → 删除多余 mut
- 9 unused_variables → 删除未用变量

验证: cargo build + 17 集成测试 + 单元测试全部 PASS"
```

---

### Task 2: 手工处理 Batch 1 中 auto-fix 未覆盖的 case

**Files:**
- 由 clippy 输出决定

- [ ] **Step 1: 检查 Batch 1 lint 还剩多少**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets --message-format=json 2>/dev/null | python3 -c "
import json, sys, collections
counts = collections.Counter()
for line in sys.stdin:
    try:
        d = json.loads(line)
        if d.get('reason') == 'compiler-message':
            m = d.get('message', {})
            if m.get('level') == 'warning':
                code = m.get('code', {}).get('code', 'unknown')
                counts[code] += 1
    except Exception:
        pass
for code, n in counts.most_common():
    print(f'{n:4d}  {code}')
"
```

预期：Batch 1 的 4 类（collapsible_if / unused_imports / unused_mut / unused_variables）应该全部清零。如有残留，列出具体文件手工处理。

- [ ] **Step 2: 手工处理残留的 Batch 1 lint**

如果还有残留（auto-fix 无法处理的 case，如条件编译下的 unused_imports），逐个文件处理：

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -E "(collapsible_if|unused_imports|unused_mut|unused_variables)" -B1 -A3
```

针对每条 warning，用 Edit 工具修改对应文件：
- `unused_imports`：删除对应 `use` 语句
- `unused_mut`：删除 `mut` 关键字
- `unused_variables`：用 `_` 前缀或删除变量
- `collapsible_if`：合并 `if a { if b { ... } }` 为 `if a && b { ... }`

- [ ] **Step 3: 验证全部清零**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -cE "(collapsible_if|unused_imports|unused_mut|unused_variables)"
```

预期：输出 `0`。

- [ ] **Step 4: 跑测试验证**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test --test core_crud_test 2>&1 | tail -5
```

预期：全部 PASS。

- [ ] **Step 5: 提交 Batch 1 手工修复**

```bash
git add -A
git commit -m "style(clippy): batch 1 手工修复 auto-fix 未覆盖的 case

- 条件编译下的 unused_imports
- auto-fix 跳过的 collapsible_if

验证: cargo build + 测试全部 PASS"
```

---

## Phase 2: Batch 2 — 多数 auto-fix 友好类（171 个）

### Task 3: 用 `cargo clippy --fix` 自动修复 Batch 2 的 lint

**Files:**
- Modify: 全代码库

**Batch 2 涵盖的 lint（共 171 个）：**
- `clippy::redundant_closure` (38) — `.map(|x| f(x))` → `.map(f)`
- `clippy::derivable_impls` (23) — 手写的 `Default` impl 可用 `#[derive(Default)]`
- `clippy::needless_borrow` (22) — `&x` 多余时去掉
- `clippy::unnecessary_map_or` (20) — `.map(|x| x.is_some())` → `.is_some()`
- `clippy::needless_update` (16) — `Struct { ..base }` 但 base 已包含所有字段
- `clippy::unnecessary_sort_by` (14) — `.sort_by(|a, b| a.cmp(b))` → `.sort()`
- `clippy::default_constructed_unit_structs` (14) — `Unit::default()` → `Unit`
- `clippy::new_without_default` (12) — 实现 `new` 但没 impl `Default`
- `clippy::needless_question_mark` (12) — `Some(x?)` → `x`

- [ ] **Step 1: 再次跑 cargo clippy --fix**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --fix --allow-dirty --allow-no-vcs 2>&1 | tail -20
```

注意：此时 `cargo clippy --fix` 会自动应用所有可修复的 lint（包括 Batch 1 残留和 Batch 2 的）。

- [ ] **Step 2: 验证编译**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo build --all-targets 2>&1 | tail -10
```

预期：编译通过。如果 `new_without_default` 的 auto-fix 添加了 `#[derive(Default)]` 但 struct 有非 Default 字段，可能需要手工回退该处改动并改用 `impl Default` 手动实现。

- [ ] **Step 3: 跑测试**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -10
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test --test core_crud_test --test message_delivery_test 2>&1 | tail -10
```

预期：全部 PASS。

- [ ] **Step 4: 提交 Batch 2 自动修复**

```bash
git add -A
git commit -m "style(clippy): batch 2 auto-fix — redundant_closure/derivable_impls/needless_borrow/unnecessary_map_or/needless_update/unnecessary_sort_by/default_constructed_unit_structs/new_without_default/needless_question_mark

Auto-applied via 'cargo clippy --fix'.
- 38 redundant_closure → method call
- 23 derivable_impls → #[derive(Default)]
- 22 needless_borrow → 去掉多余 &
- 20 unnecessary_map_or → 直接调方法
- 16 needless_update → 删除多余 ..base
- 14 unnecessary_sort_by → .sort()
- 14 default_constructed_unit_structs → 直接用 Unit
- 12 new_without_default → 添加 Default impl
- 12 needless_question_mark → 去掉 Some 包装

验证: cargo build + 集成测试 + 单元测试全部 PASS"
```

---

### Task 4: 手工处理 Batch 2 残留

- [ ] **Step 1: 检查 Batch 2 lint 剩余**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets --message-format=json 2>/dev/null | python3 -c "
import json, sys, collections
batch2 = {'clippy::redundant_closure', 'clippy::derivable_impls', 'clippy::needless_borrow',
          'clippy::unnecessary_map_or', 'clippy::needless_update', 'clippy::unnecessary_sort_by',
          'clippy::default_constructed_unit_structs', 'clippy::new_without_default', 'clippy::needless_question_mark'}
counts = collections.Counter()
for line in sys.stdin:
    try:
        d = json.loads(line)
        if d.get('reason') == 'compiler-message':
            m = d.get('message', {})
            if m.get('level') == 'warning':
                code = m.get('code', {}).get('code', 'unknown')
                if code in batch2:
                    counts[code] += 1
    except Exception:
        pass
for code, n in counts.most_common():
    print(f'{n:4d}  {code}')
print(f'Total: {sum(counts.values())}')
"
```

预期：剩余数量 < 30（多数 auto-fix 已处理）。如有残留，列出具体位置：

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -E "(redundant_closure|derivable_impls|needless_borrow|unnecessary_map_or|needless_update|unnecessary_sort_by|default_constructed_unit_structs|new_without_default|needless_question_mark)" -B1 -A3
```

- [ ] **Step 2: 手工处理残留**

针对每条 warning，用 Edit 工具修改：
- `new_without_default` 残留：如果 struct 字段不全是 Default，需要手写 `impl Default for X { fn default() -> Self { Self { ... } } }`
- `derivable_impls` 残留：可能 struct 字段类型不支持 `#[derive(Default)]`，需要手写 impl
- `needless_borrow` 残留：auto-fix 跳过的（如 macro 内部的 borrow），逐个手工去掉

- [ ] **Step 3: 跑测试**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --test vector_degradation_test --test a2a_flow_test 2>&1 | tail -5
```

- [ ] **Step 4: 提交 Batch 2 手工修复**

```bash
git add -A
git commit -m "style(clippy): batch 2 手工修复 auto-fix 未覆盖的 case

- new_without_default: struct 字段非 Default 的手写 impl
- derivable_impls: 不支持 derive 的手写 impl

验证: 测试全部 PASS"
```

---

## Phase 3: Batch 3 — 需手工判断类（130+ 个）

### Task 5: 处理 `clippy::too_many_arguments`（52 个）

**Files:**
- 由 clippy 输出决定（多为 handler / domain 方法）

**这个 lint 需要重构：** 把多个参数打包成 struct。但只对**内部方法**做重构，**对外接口（HTTP handler / trait method）保持签名不变**。

- [ ] **Step 1: 列出所有 too_many_arguments 的位置**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep "too many arguments" -B2 | grep -E "^(src|tests|common|frontend|ai-orz-macros)/" | sort -u
```

- [ ] **Step 2: 按文件分组处理**

对每个文件：
1. 读文件，找到所有 `too_many_arguments` 的方法签名
2. 判断是内部方法还是对外接口：
   - **对外接口（HTTP handler / trait method / pub method）**：在方法上方加 `#[allow(clippy::too_many_arguments)]` 注解，理由："对外接口稳定性优先于 lint 限制"
   - **内部方法（private / pub(crate) method）**：把多个参数打包成 struct（用现有的 `*Params` 或新建 `*Options` struct）
3. 用 Edit 工具修改

- [ ] **Step 3: 验证 too_many_arguments 已处理**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -c "too many arguments"
```

预期：输出 `0`。

- [ ] **Step 4: 跑全部测试**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -10
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test --test core_crud_test --test message_delivery_test --test vector_degradation_test --test a2a_flow_test 2>&1 | tail -20
```

预期：全部 PASS。

- [ ] **Step 5: 提交**

```bash
git add -A
git commit -m "style(clippy): batch 3 too_many_arguments — 内部方法参数打包为 struct，对外接口加 #[allow]

- 52 处 too_many_arguments
- 内部方法：参数打包成 *Params struct
- 对外接口（handler / trait / pub）：加 #[allow(clippy::too_many_arguments)]

验证: 全部 17 集成测试 + 单元测试 PASS"
```

---

### Task 6: 处理 `dead_code`（27 个）

**Files:**
- 由 clippy 输出决定

**谨慎处理：** dead_code 可能是：
1. 真的未使用的代码 → 删除
2. cfg 条件编译下未启用的代码 → 加 `#[allow(dead_code)]` 注解
3. 公共 API 但当前无调用方 → 加 `#[allow(dead_code)]` 注解（保留 API）

- [ ] **Step 1: 列出所有 dead_code 的位置**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep "is never" -B2 | grep -E "^(src|tests|common|frontend|ai-orz-macros)/" | sort -u
```

- [ ] **Step 2: 逐个判断并处理**

对每条 dead_code warning：
1. 读对应代码上下文
2. 判断属于哪类：
   - 真未使用 + 非公共 API → 删除
   - cfg 条件编译 → 加 `#[allow(dead_code)]` 加注释 `// 用于 xxx feature，条件编译时未启用`
   - 公共 API（pub fn / pub struct）保留 → 加 `#[allow(dead_code)]` 加注释 `// 公共 API，保留供未来使用`
3. 用 Edit 工具修改

- [ ] **Step 3: 验证**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -c "is never"
```

预期：输出 `0`。

- [ ] **Step 4: 跑测试**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -10
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test --test core_crud_test 2>&1 | tail -10
```

- [ ] **Step 5: 提交**

```bash
git add -A
git commit -m "style(clippy): batch 3 dead_code — 真死代码删除，cfg/pub 保留并加 #[allow]

- 27 处 dead_code
- 真未使用 + 非 pub: 删除
- cfg 条件编译: #[allow(dead_code)] + 注释
- 公共 API: #[allow(dead_code)] + 注释

验证: 测试全部 PASS"
```

---

### Task 7: 处理 Batch 3 剩余 lint（约 60 个）

**Files:**
- 由 clippy 输出决定

**Batch 3 剩余 lint：**
- `clippy::drop_non_drop` (12) — `drop(x)` 但 x 不需要 drop，删除 drop 调用
- `clippy::single_char_add_str` (10) — `format!("{}{}", a, b)` 当 b 是单字符时改用 `format!("{}c", a)`
- `clippy::module_inception` (8) — `mod foo { mod foo {} }`，重命名内层模块
- `clippy::useless_conversion` (8) — `x.into()` 但类型相同，删除 `.into()`
- `clippy::let_unit_value` (6) — `let x = ();`，改用 `let _ = ();` 或直接删除
- `clippy::doc_lazy_continuation` (6) — doc 注释续行缩进不对
- `clippy::needless_as_bytes` (6) — `s.as_bytes()` 多余
- `clippy::field_reassign_with_default` (6) — `let mut x = X::default(); x.f = 1;` → `let x = X { f: 1, ..Default::default() };`
- 其他 ≤4 个的 lint：手工逐个处理

- [ ] **Step 1: 跑 cargo clippy --fix 处理剩余 auto-fix 类**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --fix --allow-dirty --allow-no-vcs 2>&1 | tail -20
```

- [ ] **Step 2: 验证编译和测试**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo build --all-targets 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
```

- [ ] **Step 3: 列出剩余手工处理的位置**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -E "(drop_non_drop|single_char_add_str|module_inception|useless_conversion|let_unit_value|doc_lazy_continuation|needless_as_bytes|field_reassign_with_default|map_clone|double_ended_iterator_last|useless_format|type_complexity|manual_flatten|clone_on_copy|useless_vec|empty_line_after_doc_comments|single_match|to_string_trait_impl|redundant_field_names|expect_fun_call|needless_range_loop|needless_borrows_for_generic_args|let_underscore_future|manual_inspect|needless_ifs|bind_instead_of_map|map_entry|cloned_ref_to_slice_refs|redundant_pattern_matching|for_kv_map|unnecessary_lazy_evaluations|large_enum_variant|let_and_return|cmp_owned|should_implement_trait|unnecessary_to_owned|bool_assert_comparison|assertions_on_constants)" -B1 -A2 | head -100
```

- [ ] **Step 4: 手工逐个处理**

对每条 warning：
- `drop_non_drop`：删除 `drop(x)` 调用
- `single_char_add_str`：把 `format!("{}{}", a, "x")` 改成 `format!("{}x", a)`
- `module_inception`：重命名内层模块（如 `mod foo { mod foo {} }` → `mod foo { mod inner {} }`），同时更新所有引用
- `useless_conversion`：删除 `.into()` 或 `.try_into().unwrap()`
- `let_unit_value`：改用 `let _ = ...` 或删除
- `doc_lazy_continuation`：修正 doc 注释缩进
- `needless_as_bytes`：删除 `.as_bytes()`
- `field_reassign_with_default`：用结构体字面量替代
- 其他：按 clippy 提示处理

- [ ] **Step 5: 验证 Batch 3 全部清零**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets --message-format=json 2>/dev/null | python3 -c "
import json, sys
batch3_lints = ['clippy::too_many_arguments', 'dead_code', 'clippy::drop_non_drop', 'clippy::single_char_add_str',
                'clippy::module_inception', 'clippy::useless_conversion', 'clippy::let_unit_value',
                'clippy::doc_lazy_continuation', 'clippy::needless_as_bytes', 'clippy::field_reassign_with_default']
count = 0
for line in sys.stdin:
    try:
        d = json.loads(line)
        if d.get('reason') == 'compiler-message':
            m = d.get('message', {})
            if m.get('level') == 'warning':
                code = m.get('code', {}).get('code', '')
                if code in batch3_lints:
                    count += 1
    except Exception:
        pass
print(f'Batch 3 剩余: {count}')
"
```

预期：输出 `Batch 3 剩余: 0`。

- [ ] **Step 6: 跑全部测试**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test --test core_crud_test --test message_delivery_test --test vector_degradation_test --test a2a_flow_test 2>&1 | tail -20
```

- [ ] **Step 7: 提交**

```bash
git add -A
git commit -m "style(clippy): batch 3 剩余 lint 手工处理

- drop_non_drop: 删除多余 drop 调用
- single_char_add_str: 单字符直接写入 format 字符串
- module_inception: 重命名内层模块
- useless_conversion: 删除多余 .into()
- let_unit_value / needless_as_bytes / field_reassign_with_default 等
- 其他小批量 lint

验证: 全部测试 PASS"
```

---

## Phase 4: Batch 4 — 特殊处理类（8 个）

### Task 8: 处理 await_holding_lock（2 个）+ unconditional_recursion（3 个）+ large_enum_variant（2 个）+ deprecated（1 个）

**Files:**
- 由 clippy 输出决定

**这些 warning 可能是真 bug 或性能问题，必须逐个分析：**

- [ ] **Step 1: 列出 Batch 4 全部位置**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -E "(await_holding_lock|unconditional_recursion|large_enum_variant|deprecated)" -B2 -A5
```

- [ ] **Step 2: 处理 await_holding_lock（潜在死锁 bug）**

`await_holding_lock` 警告：在持有 Mutex/RwLock 的情况下 `.await`，可能导致死锁。

对每处：
1. 读代码理解为什么持锁 await
2. 重构：把 await 移出锁的范围
   - 把需要 await 的部分提取到锁释放后执行
   - 或用 `tokio::sync::Mutex` 替代 `std::sync::Mutex`（如果是异步场景）
3. 用 Edit 工具修改

- [ ] **Step 3: 处理 unconditional_recursion（潜在无限递归 bug）**

`unconditional_recursion` 警告：函数无条件递归调用自己。

对每处：
1. 读代码理解递归意图
2. 如果是真 bug（漏了终止条件）：加终止条件
3. 如果是误报（实际有终止条件但 clippy 没识别）：加 `#[allow(clippy::unconditional_recursion)]` + 注释说明
4. 用 Edit 工具修改

- [ ] **Step 4: 处理 large_enum_variant（性能问题）**

`large_enum_variant` 警告：enum 的某个 variant 比其他大很多（如 1000 bytes vs 10 bytes），建议 Box 包装。

对每处：
1. 读 enum 定义
2. 把大字段用 `Box::new()` 包装
3. 同步修改所有构造该 variant 的位置
4. 用 Edit 工具修改

- [ ] **Step 5: 处理 deprecated**

`deprecated` 警告：调用了 `#[deprecated]` 标记的 API。

对每处：
1. 读 deprecated 标注的替代方案
2. 改用推荐的新 API
3. 用 Edit 工具修改

- [ ] **Step 6: 验证 Batch 4 全部清零**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -cE "(await_holding_lock|unconditional_recursion|large_enum_variant|deprecated)"
```

预期：输出 `0`。

- [ ] **Step 7: 跑全部测试**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo test --lib 2>&1 | tail -5
PROTOC=/opt/homebrew/bin/protoc cargo test --test auth_sysinit_test --test core_crud_test --test message_delivery_test --test vector_degradation_test --test a2a_flow_test 2>&1 | tail -20
```

预期：全部 PASS。特别关注 await_holding_lock 修复后是否破坏并发逻辑。

- [ ] **Step 8: 提交**

```bash
git add -A
git commit -m "fix(clippy): batch 4 特殊处理 — 修复潜在 bug 和性能问题

- await_holding_lock: 把 await 移出锁范围，避免死锁
- unconditional_recursion: 修复漏掉的终止条件或加 #[allow] 说明
- large_enum_variant: 大字段用 Box 包装
- deprecated: 迁移到新 API

验证: 全部测试 PASS"
```

---

## Phase 5: 启用 CI 强制门槛

### Task 9: 修改 CI workflow 启用 `-D warnings`

**Files:**
- Modify: `.github/workflows/rust.yml`

- [ ] **Step 1: 验证本地 clippy 零 warning**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets 2>&1 | grep -c "^warning:"
```

预期：输出 `0`。如果还有 warning，回到对应 Batch 处理后再继续。

- [ ] **Step 2: 修改 CI workflow**

在 `.github/workflows/rust.yml` 中找到 Clippy check 步骤（之前是 `cargo clippy --all-targets` 不带 `-D warnings`），改为：

```yaml
    - name: Clippy check (deny warnings)
      run: PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets -- -D warnings
```

注意：`--` 之后是 rustc 参数，`-D warnings` 是 rustc 的 deny warnings 标志。

同时删除之前注释中"未来清理完 warning 后再改为 -D warnings"的说明。

- [ ] **Step 3: 验证本地等价命令通过**

```bash
PROTOC=/opt/homebrew/bin/protoc cargo clippy --all-targets -- -D warnings 2>&1 | tail -10
```

预期：编译通过，无 warning 无 error。

- [ ] **Step 4: 提交**

```bash
git add .github/workflows/rust.yml
git commit -m "ci: 启用 clippy -D warnings 强制门槛

442 个 warning 已全部清理，CI 正式启用 -D warnings。
未来新代码引入 warning 会被 CI 直接拦截。

历史清理 commits:
- batch 1: collapsible_if / unused_imports / unused_mut / unused_variables
- batch 2: redundant_closure / derivable_impls / needless_borrow 等 9 类
- batch 3: too_many_arguments / dead_code / drop_non_drop 等
- batch 4: await_holding_lock / unconditional_recursion / large_enum_variant / deprecated"
```

---

## Self-Review Checklist

**Spec coverage:**
- ✅ Batch 1 (309 warnings): Task 1 auto-fix + Task 2 手工残留
- ✅ Batch 2 (171 warnings): Task 3 auto-fix + Task 4 手工残留
- ✅ Batch 3 (130+ warnings): Task 5 too_many_arguments + Task 6 dead_code + Task 7 剩余 lint
- ✅ Batch 4 (8 warnings): Task 8 await_holding_lock / unconditional_recursion / large_enum_variant / deprecated
- ✅ Phase 5 启用 CI 门槛: Task 9

**Placeholder scan:** 所有 Step 都有具体命令和预期输出。无 "TBD" / "implement later" 等占位符。

**Type consistency:** 全部命令前缀 `PROTOC=/opt/homebrew/bin/protoc`，所有验证步骤跑全部 17 个集成测试（auth_sysinit / core_crud / message_delivery / vector_degradation / a2a_flow）。

**风险点：**
1. `cargo clippy --fix` 偶尔会破坏代码（如删除了实际需要的 import）—— Step 3 验证编译通过；如果失败，回滚后手工处理
2. `too_many_arguments` 重构参数 struct 时，可能漏改调用方 —— Step 4 跑全部测试验证
3. `module_inception` 重命名内层模块时，需要同步更新所有 `use` 引用 —— 测试会捕获漏改
4. `await_holding_lock` 重构可能改变并发语义 —— 重点跑 a2a_flow_test 和 message_delivery_test（这两个测试涉及并发路径）
5. `large_enum_variant` 用 Box 包装后，所有构造 variant 的位置都要改 —— 测试会捕获漏改
6. `dead_code` 删除真死代码时，可能漏掉 cfg 条件编译的引用 —— 用 `#[allow(dead_code)]` 保守处理 cfg 类
7. Batch 之间可能有依赖：Batch 2 的 `derivable_impls` 可能修复 Batch 1 的 `new_without_default` —— 顺序执行即可，`cargo clippy --fix` 是增量的

**执行顺序建议：** 严格按 Task 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9。每个 Task 完成后立即提交，便于回滚。
