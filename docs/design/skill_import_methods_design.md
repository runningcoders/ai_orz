# 技能导入方式扩展与统一导入框架设计

> 🎯 定位：技能系统能力扩展设计——不新增独立 import handler，通过在 Create/Update 请求体中新增可复用 SkillContentInput 子结构体 + 6 个纯方法复用群，将原本「手填内容新建技能」的单一路径扩展为「3 种内容源统一装配 + Domain 层 4 原子共享流水线 + 前后端 Create/Update 同表单」的闭合结构。
>
> 状态：`Draft`
>
> 查阅场景：修改 Skill DTO 结构前读 §1 决策表；改造 Domain 流水线读 §二 架构；Handler 改签名读 §三 涉及文件表；排查导入故障读 §四 安全红线；扩展内容源类型读 §五。
>
> 关联文档：
> - 对应 plan 文档：docs/plan/技能导入方式扩展与统一导入框架落地.md
> - 上层规范：AGENTS.md · docs/CODE_STANDARDS.md · docs/LAYERED_ARCHITECTURE_PRACTICE.md
> - 相关决策：docs/archive/design-archive/skill_design.md · docs/archive/design-archive/skill_system_enhancement_design.md

---

## 一、目标与决策表

### §1.1 目标（7 条）

1. 用户手动显式操作 = 仅在 Create / Update 接口装配内容源；不做自动扫描、不做后台隐式导入、不新增独立 import 路由；3 种内容源的差异完全收敛到 `CreateSkillRequest.content_input` / `UpdateSkillRequest.content_input` 这一个子结构里。
2. 3 种内容源覆盖完整单技能入库路径：方式 1 直接文本 skill.md；方式 2 HTTPS URL（单 md 或单技能 zip，解压后 skill.md 须在根目录）；方式 3 附件散文件装配（attachment_id + target_path 映射）。
3. Create / Update 行为只差「创建还是更新」，内容源处理 100% 共享：Domain 层私有方法 `apply_content_sources(imports, remote_source)` 作为统一 4 原子流水线，两端同一份代码同一份安全校验。核心抽象 `SkillFileImport` = 「(去哪放 target_path) × (从哪来 source_abs_path / content_bytes)」二象性，所有外部适配器（手填文本 / URL 下载 / 附件上传）翻译到这一个结构。
4. `SkillContentInput` 纯方法复用群 = 前后端 / 双接口共享校验逻辑 SSOT：is_empty / classify / validate_url_https_only / validate_files_unique_target 等集中到 common DTO impl，避免双标漂移。
5. 测试阶段直接切割不做兼容层，未来零兼容债：删除 CreateSkillRequest.content + initial_files；删除 UpdateSkillRequest.content + files；不提供 Compat DTO 也不做 From 转换。前端事实锚点：Create 从未用 initial_files（见 frontend/src/pages/hr/skills.rs#L124-L140）；Update 从未用顶层 content/files（见 frontend/src/pages/hr/skill_detail.rs#L122-L131）。
6. Phase 1 后端 + 前端同步改造，Create/Update 共用同一个 SkillForm 组件。前端不延后、不拆分阶段；改 skill.md 主文件不再走独立 update_skill_file_content 文件级接口，统一走 UpdateSkillRequest.content_input.content 一次请求。
7. 多技能批量 / ZIP 上传 / CLI 专用导入器后续单独设计。Phase 1 只做单技能 3 内容源。

### §1.2 决策表（14 问）

| # | Q | 方案 | 原因 |
|---|---|------|------|
| 1.1 | 字段布局：3 种内容源顶层 vs 子结构包裹 | 统一 SkillContentInput 子结构（content / url / files 三字段） | 复用核心；6 个纯方法必须挂靠该子结构 |
| 1.2 | 旧字段（content/files/initial_files）兼容层？ | 直接删除；无转换；无 flatten；无 Compat DTO | 前端从未使用；测试阶段无外部消费者 |
| 1.3 | URL 放顶层还是放 SkillContentInput | 放 SkillContentInput.url | classify / validate 必须同时看到 3 字段 |
| 1.4 | classify 6 变体 + 冲突优先级 | files > content > url；content 与 url 同时 Some 为 MixedContentOverridesUrl（提示用户忽略 URL）；files+content 同时 Some 为 MixedTextAttachments；都 None 为 None | 实内容优先于需抓取的虚内容 |
| 1.5 | 前端 Create/Update 独立表单 vs 共用 SkillForm | 共用 SkillForm；通过 mode=Create/Update{skill_id} 切换 | Meta 字段完全一致；净减少约 180 行；避免漂移 |
| 1.6 | 3 种内容源前端 UI 形态 | DaisyUI 3 Tab：Tab0 文本 / Tab1 URL / Tab2 附件装配 Table | Tab 天然强制互斥；附件 Table 视图直观 |
| 1.7 | 附件上传：create/update 内 multipart 还是先 upload_attachment | 先调 upload_attachment 拿 id，再组装 SkillFileInput JSON；完全复用 chat.rs#L419-L471 现有链路 | 维持 DTO JSON 契约；附件存储职责不放到 create/update handler |
| 1.8 | URL 抓取实现：Seed 与 Skill 共用？ | 从 Seed defs.rs 抽出 pkg/fetch_remote_content.rs（HTTPS/30s/1MB/SSRF/HTML→text），双端调用 | 防抓取规则漂移 |
| 1.9 | zip 解压 + frontmatter 合并放哪层 | Domain 层 apply_content_sources 内部：原子 2 Remote（zip 解包）、原子 4 Frontmatter（元信息覆盖）；Handler 只传字符串不做业务处理 | Adapter 层只能 DTO↔Command 转换 |
| 1.10 | HTTPS 校验 + SSRF：一处 or 两处 | 两处：(a) common impl 只做前缀 https:// 字符串校验（纯函数前后端共享，即时反馈）；(b) pkg fetch 内部 DNS 解析后 SSRF IP 黑名单（后端独有强防护） | 体验 + 安全双重保障 |
| 1.11 | target_path 重复 / 路径穿越校验放哪层 | 两层：(a) common validate_files_unique_target pure fn（双端共享 unique + `..`/`/`/`\` 基础拒绝）；(b) Domain 原子 3 做 S6 白名单正则 + S4 10MB 累积 + S5 100 文件累积 | unique 是纯逻辑；大小/数量必须读 bytes 后在后端算 |
| 1.12 | lark-cli 自带技能处理 | 本方案配套放开 stderr 提示（T0 已完成 apply_cli_env）。用户手动从 CLI 提示定位文件 → 回到技能库用方式 1/3 手动注入；二期再设计 import/cli-lark | 保持手动显式原则 |
| 1.13 | Phase 2 Zero-Copy 优化会破坏 Phase 1 契约？ | 不会。Phase 2 纯内部：合并 old file_imports + file_sources 为单一 `SkillFileImport`（target_path × 来源二象性）；原子 2/3 收编为单 `process_skill_package` 纯函数；DTO / Handler / Frontend 零改动 | 对外契约稳定；内部抽象统一，更易扩展 |
| 1.15 | target_path=None 时怎么办？ | 5 级降级推断链：① suggested_name(附件 original_name) ② source.file_name() ③ zip/md 内容 magic 识别 ④ 特定场景默认组织(图片→assets/、脚本→scripts/、单 md→skill.md) ⑤ 全失败才报错 | 覆盖 4 种高频场景用户不用手填 target |
| 1.16 | SkillFileImport 多来源字段的优先级？ | source_abs_path 路径优先 > content_bytes 内存内容；都有则忽略 bytes 走路径（0 拷贝优先） | Phase 2 Zero-Copy 核心目标 |
| 1.17 | zip 包内多 md 无 skill.md 的报错方向？ | "zip 包根目录未找到 skill.md，且含多个 Markdown 文件。请将主技能文件重命名为 skill.md 后重新上传" | 宁报错不导错，指引明确 |
| 1.14 | 未来多技能批量接口与本方案关系 | Phase 3 单独设计；外层 for-loop 套壳，每个技能的内容处理内核 100% 复用 apply_content_sources | 不允许批量接口重写一份内容源处理逻辑 |

---

## 二、架构与统一流水线

**前端 1 组件 + 后端 2 接口对称 + Domain 1 共享流水线：**

- SkillForm（共用组件）= SkillMetaEditor（name/desc/tags/category/status）+ SkillContentInputEditor（3 Tab：文本 skill.md、HTTPS URL、附件装配 Table）
  - Tab2 附件上传：复用 chat.rs 现有 upload_attachment 流程，拿到 attachment_id 后填入 SkillFileInput
  - 输出：SkillFormOutput → CreateSkillRequest 或 UpdateSkillRequest（content_input 子结构统一）
- Handler 层（create_skill.rs / update_skill.rs 对称）
  - classify 预检 → validate(HTTPS + unique_target) → 附件读 finance_domain().get_attachment(**Phase 1: include_content=true 读 bytes；Phase 2: include_content=false 只读路径元信息**) → URL 走 pkg fetch → 组装 CreateSkillParams / UpdateSkillParams
- Domain 层私有 apply_content_sources（统一流水线，两端同一份代码）：
  - **Phase 1（已完成）**：4 原子分离——① DirectText Write ② Remote URL Fetch + ZIP Unpack ③ Attachments Bytes Write ④ Frontmatter Meta Merge
  - **Phase 2（本阶段）**：合并原子 1/2/3 为单一 `process_skill_package` 纯函数，统一 `SkillFileImport` 结构（target_path × 来源二象性）；附件走 source_abs_path 路径优先 → same_fs rename 零拷贝，手填文本走 content_bytes 通路省临时文件；原子 4 Frontmatter 永远读最终磁盘 skill.md
  - 16 处 create_skill 非 skill 库调用点：兼容走 `CreateSkillParams::from_skill(&skill)`（imports: vec![]），无需传新字段
- lark-cli 配套（T0 已完成）：apply_cli_env 统一 4 处 env 注入，LARKSUITE_CLI_NO_SKILLS_NOTIFIER 从 1 改为 0，放开 stderr 技能提示

### §2.1 Phase 2 统一 SkillFileImport 结构

```
SkillFileImport = (去哪放) × (从哪来)
├ target_path: Option<String>         — 最终在技能目录里的相对路径；None 时走 5 级推断链
├ source_abs_path: Option<PathBuf>    — 来源 A：磁盘绝对路径（优先级高，可 0 拷贝 rename）
├ content_bytes: Option<Vec<u8>>      — 来源 B：内存 bytes（次优先级；无路径时用）
└ suggested_name: Option<String>     — 弱线索名（附件 original_name；target=None 时推断用）
```

**来源优先级**：source_abs_path > content_bytes（路径优先 = 0 拷贝优先）

**target_path=None 时的 5 级降级推断链**：
1. suggested_name（附件 original_name）→ 直接用
2. source_abs_path.file_name() → 用文件名
3. 内容 magic 识别 → zip（PK\x03\x04）解包 / md（`---` YAML 头或 `# ` 开头）默认 skill.md
4. 特定场景默认组织 → 图片(png/jpg/gif/webp/svg) → `assets/{name}`；脚本(py/js/sh) → `scripts/{name}`；单 md → `skill.md`
5. 全失败 → 报错「无法自动推断目标路径，请为该文件填写 target_path」

### §2.2 Phase 2 process_skill_package 纯函数流程

```
对每个 SkillFileImport：
  ① 来源归一：source_abs_path → PATH 分支 / content_bytes → BYTES 分支 / 都没 → Err
  ② zip magic 检测（PATH 分支读前 4B；BYTES 分支读前 4B）
     ├ 是 zip → ZipArchive 解包：skill.md 必须在根目录；多 md 无 skill.md → 报错指引
     │         每个条目逐一 validate_skill_import_target_path → write_file_bytes
     └ 非 zip → 单文件处理
  ③ target 推断（target_path=None 时走 §2.1 的 5 级链）
  ④ 落盘策略：
     ├ PATH 分支 + same_fs → fs::rename（0 拷贝）
     ├ PATH 分支 + 跨 fs → fs::copy（降级）
     └ BYTES 分支 → write_file_bytes
  ⑤ 路径穿越校验 validate_skill_import_target_path（所有非 zip 条目）
```

---

## 三、涉及文件清单（按分层）

| 分层 | 路径 | 操作 | 改动内容 |
|------|------|------|---------|
| common DTO | common/src/api/skill.rs | Modify | ① 新增 SkillContentInput + 6 纯方法 + SkillContentKind 枚举；② 删除 CreateSkillRequest.content + initial_files → 加 content_input；③ 删除 UpdateSkillRequest.content + files → 加 content_input: Option<_>；④ 同步改 UpdateSkillRequestOriginal |
| Adapter Handler | src/handlers/hr/skill/create_skill.rs | Modify | 删 initial_files；完全复用 update_skill.rs 现有附件读取流程装 SkillFileImport；加 classify + validate 预检；组装 CreateSkillParams 调 domain |
| Adapter Handler | src/handlers/hr/skill/update_skill.rs | Modify | 删硬写 content:None/files:None 空代码；若 content_input.is_some() 走 classify + validate + finance 附件读 + pkg URL；组装 UpdateSkillParams |
| Domain Trait | src/service/domain/hr/mod.rs | Modify | Phase 1：create_skill 签名换 CreateSkillParams + from_skill 兼容 16 调用点。Phase 2：SkillFileImport 合并 old file_imports + file_writes → 统一结构（target_path × source_abs_path × content_bytes × suggested_name）；CreateSkillParams/UpdateSkillParams 删 file_writes + file_imports → 单一 imports: Vec<SkillFileImport> |
| Domain impl | src/service/domain/hr/skill.rs | Modify | Phase 1：apply_content_sources 4 原子。Phase 2：原子 1/2/3 合并为 `process_skill_package` 纯函数（5 级 target 推断 + zip 解包 + same_fs rename/copy/write_bytes 三通路）；原子 4 Frontmatter 保留读最终磁盘 |
| DAL trait | src/service/dal/skill.rs | Modify（Phase 2） | skill_dir / file_abs_path 从 private 升 pub trait 方法（供 Domain 拼目标绝对路径做 rename） |
| DAO impl | src/service/dao/skill/sqlite.rs | Modify（Phase 2） | skill_dir / file_path 升 pub + 重命名 file_abs_path |
| pkg 注册 | src/pkg/mod.rs | Modify | pub mod fetch_remote_content |
| pkg lark_cli | src/pkg/tool_registry/lark_cli.rs#L200-L212 | ✅已完成 | apply_cli_env + NO_SKILLS_NOTIFIER=0，统一 4 处 env |
| pkg lark_integration | src/pkg/lark_integration.rs#L96-L102 | ✅已完成 | 复用 apply_cli_env，移除分散 env 写 |
| Seed 共用抓取 | src/service/domain/system/seed/defs.rs | Modify | resolve_skill_file_content 内联 reqwest 抓取替换为 pkg::fetch_remote_content；对外签名 100% 不变 |
| 前端新组件 | frontend/src/components/hr/skill_form.rs | Create | SkillForm 共用组件：mode + on_submit callback + initial_value；内嵌 SkillMetaEditor + SkillContentInputEditor（3 Tab） |
| 前端列表页 | frontend/src/pages/hr/skills.rs | Modify | show_add_modal 删除手写 name/desc/tags/category/content use_signal → 换 SkillForm mode=Create on_submit 组装 CreateSkillRequest |
| 前端详情页 | frontend/src/pages/hr/skill_detail.rs | Modify | show_edit_modal 删除手写 edit_name/desc/tags/category → 换 SkillForm mode=Update{skill_id} initial_value=current；元信息 + 内容源合并一次请求提交 |
| 前端 API 客户端 | frontend/src/api/hr.rs | Modify（小） | 同步请求结构字段：旧的 content/initial_files 改成 content_input；不新增独立 import API |
| 后端测试 | src/service/domain/hr/tests/skill_content_input_test.rs | Create | ① SkillContentInput 6 方法单测（6 分支全 hit / HTTPS / unique_target 重复）；② SSRF UT（S1：127.x / 10.x / 169.254.169.254）；③ S2 路径穿越 UT（target_path 含 `..`、`/` 开头、`\` → 拒绝） |
| 种子测试 | 现有 seed tests | Modify（调用面不变） | defs.rs 内部委托改完后 PASS：cargo test seed |

---

## 四、边界与行为红线

### §4.1 安全红线（S1–S8，Phase 1 启用 S1–S6/S8）

| ID | 红线 | 所在位置 |
|----|------|---------|
| S1 | URL 仅 HTTPS；后端 DNS 解析后拦截内网/回环/元数据 IP（10.x、172.16-31.x、192.168.x、127.x、169.254.169.254、fd00::/8、::1）；IPv4 映射 IPv6 先解映射 | 两处：common impl 字符串预检 + pkg fetch DNS+IP 强防护 |
| S2 | target_path 禁止 `..`、绝对路径、`/` 开头、`\`、控制字符 | 两处：common validate_files_unique_target + Domain 原子 3 |
| S3 | SSRF IP 黑名单 = S1 的后端侧（同 S1 实现在 pkg fetch） | pkg fetch_remote_content.is_ip_forbidden() |
| S4 | 单技能合计大小 ≤ 10MB（文本 + URL 包 + 附件累积） | Domain apply_content_sources 入口累加 |
| S5 | 单技能文件数 ≤ 100（文本 / URL 解包 / 附件合计） | Domain apply_content_sources 开头计数 |
| S6 | 文件名 / target_path 仅允许 `[a-zA-Z0-9._-/]`；不允许 `\`、控制字符、`:*?"<>|` | Domain 原子 3 正则校验 |
| S7 | Phase 3 批量接口用（预留编号不启用） | Phase 1 跳过 |
| S8 | 技能内容只写 content_path/skills/{id}/；content_path.join(sanitized_path) 禁止字符串拼接 | SkillDao 路径构造处 |

### §4.2 行为红线（B1–B7）

B1：冲突优先级 files > content > url；content+url 同时 Some 时提示用户「已用文本，URL 忽略」，不静默吞意图；B2：create_skill 作者始终 = ctx.uid() User，不切换 System；B3：HTML→文本用轻量去标签脚本（不引 pulldown-cmark / scraper）；B4：Frontmatter 解析失败不 500，逐级降级最终按"无 frontmatter"处理；B5：content_input.is_empty() 且只改 Meta → 合法，直接走原 Meta 更新路径，不触发 apply_content_sources；B6：Update 模式下 Meta 空输入 → None=不覆盖；不支持显式清空；B7：附件上传 purpose 固定 `"skill"`（chat 用 "chat"），分类统计/清理时不混。

---

## 五、扩展路径

1. Phase 2 Zero-Copy + 统一结构（**进行中**）：合并 old file_imports + file_writes 为单一 `SkillFileImport`（target_path × 来源二象性）；Handler include_content=false 只读附件路径元信息；Domain `process_skill_package` 纯函数统一处理 zip 解包 / 5 级 target 推断 / same_fs rename 0 拷贝 / 跨 fs copy 降级 / bytes write 兜底；DTO/Handler JSON/Frontend 零改动。
2. Phase 3 批量接口：`POST /api/v1/skills/batch` 外层 for-loop 套壳；每项复用 create_skill(CreateSkillParams) + apply_content_sources；禁止重写内容处理逻辑。
3. CLI 专用导入器：`POST /api/v1/skills/import/cli-lark` spawn lark-cli → 组装 N 个 SkillContentInput → 走批量接口循环。
4. 导出→导入闭环：`POST /api/v1/skills/{id}/export` zip；导入走方式 2（URL zip 直链）或方式 3（附件 zip）；原子 2 已内建 zip 解包。
5. 第 4 内容源：追加 SkillContentInput.git: Option<SkillGitInput>；原子 2 Remote 加新分支 `git clone --depth 1`；不影响现有 3 字段语义。

## 六、Phase 2.5 设计补充：file_deletes 闭环 + seed 运行时本地路径

### 6.1 file_deletes 端到端

**问题**：Phase 1+2 完成后 `update_skill` 已支持增（imports 走 PATH/BYTES 写入）+ 改（同名覆盖）+ **缺删**。`UpdateSkillParams.file_deletes: Vec<&str>` 字段已存在但 Domain `// TODO: 实现文件删除` 空循环；DTO `UpdateSkillRequest` 也未暴露此字段。

**端到端方案**：

| 层 | 改动 |
|----|------|
| Common DTO | `UpdateSkillRequest` 加 `file_deletes: Option<Vec<String>>`；`SkillContentInput` 的路径安全校验逻辑拆出独立 `validate_filenames_path_safety(names: &[String])` 公共函数供复用 |
| Handler | 读 `params.file_deletes` → 调 `validate_filenames_path_safety` → 转 `Vec<&str>` 传 `UpdateSkillParams.file_deletes` |
| Domain | 替换 `// TODO: 实现文件删除` 空循环为 for-loop 调 `self.skill_dal.delete_file(po, filename)` |
| SkillDal trait | 加 `fn delete_file(skill: &SkillPo, filename: &str) -> Result<()>` |
| SkillDao sqlite | 实现 `delete_file`：禁删 `skill.md`（业务规则：删了主文件等于让技能变空壳，应用 content 覆盖而非删）+ 路径双重防线（不含 `..` / 不以 `/` 开头 / 不含 `\` + canonicalize 后 starts_with skill_dir）+ fs::remove_file 不存在视为成功 |

### 6.2 seed SkillFileDef 第 4 字段 local_path

**现状**：seed `SkillFileDef` 有 3 字段 content / ref_path / url；优先级 content > ref_path > url。其中 ref_path 是**编译期 `include_str!`**（5 个预置 TEMPLATE_SKILL 文件），不是运行时本地路径。

**新场景**：seed snapshot 由管理员手动维护或外部工具生成时，需要指定运行时磁盘上的某个文件路径（如 `/path/to/my/skill.md`），直接读取导入。区别于 ref_path 的编译期固定路径。

**方案**：

```rust
pub struct SkillFileDef {
    pub path: String,
    pub content: Option<String>,
    /// 运行时本地文件绝对路径（区别于 ref_path 编译期 embedded）。
    /// 指定时跳过内容读取，走 SkillFileImport.source_abs_path 0 拷贝。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    pub ref_path: Option<String>,
    pub url: Option<String>,
}
```

**优先级调整**：content > local_path > ref_path > url
- content：内嵌文本（最高优先级，自包含）
- local_path：运行时本地路径 → 走 `SkillFileImport.source_abs_path` 0 拷贝（Phase 2 已具备）
- ref_path：编译期 embedded 必读 bytes（无法走路径，二进制已内嵌）
- url：运行时 HTTPS 抓取

**seed 改造**：`resolve_skill_file_content` 重构为 `resolve_skill_file_import`，返回 `SkillFileImport` 而不是 `String`，避免"先读 bytes 再塞 content_bytes"的反模式。`apply_preset_skills` 直接用组装好的 imports 数组，删除中间的 `file_contents: Vec<(String, String)>` 缓冲。

### 6.3 安全边界

- **seed 是可信源**：snapshot 由管理员或种子模板生成，不像用户输入那样需严防。但 `local_path` 仍需校验文件存在性（避免指向不存在的路径）。
- **target_path 仍受 SkillContentInput 路径安全校验**：禁止 `..` / 绝对路径 / `\`，避免穿越到技能目录外。
- **file_deletes 双防线**：路径字符串层校验 + canonicalize 物理层校验，即使字符串校验被绕过也能挡住。

### 6.4 不引入的复杂度

- 不引入 `local_path` 的目录扫描（递归导入整个目录）：超出 seed 单文件维度语义，留给 Phase 3 批量接口处理。
- 不引入 `local_path` 的 zip 解包：seed 文件维度是单文件，zip 是 skill 维度（remote_source 通路）。
