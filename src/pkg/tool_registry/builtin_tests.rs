//! tests 单元测试（拆分自 builtin.rs）
//!
//! 文件瘦身：原 367 行 → 77 行，测试体 291 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;

/// 通用内置工具的 tag 分组定义（供工具包按 tag 安装）
#[test]
fn generic_builtin_tools_carry_expected_tags() {
    let expected: Vec<(&str, Vec<&str>)> = vec![
        ("http_fetch", vec!["http"]),
        ("fs_read", vec!["fs"]),
        ("fs_write", vec!["fs"]),
        ("shell_exec", vec!["shell"]),
        ("lark_cli", vec!["lark"]),
        ("gh_cli", vec!["github"]),
        ("tavily_search", vec!["search", "network"]),
        ("doubao_search", vec!["search", "network"]),
        ("browser", vec!["browser", "network"]),
        ("mark_artifact", vec!["artifact"]),
    ];
    for (id, tags) in expected {
        let factory = GENERIC_BUILTIN_TOOLS
            .iter()
            .find(|(fid, _)| fid == id)
            .map(|(_, f)| f)
            .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
        assert_eq!(factory.create_po().get_tags(), tags, "tags of {}", id);
    }
}

/// 护栏：消息类工具必须对**每个 Agent** 可达（带 `neural`）。
///
/// 背景：Agent 的工具面 = 绑定工具 ∪ tag(`neural` ∪ `runtime_config.installed_tags`)
/// （装配点 `hr/agent.rs::get_agent(with_tools)`）。系统提示词（`builder/default.rs`）
/// 与协作沟通技能（`TEMPLATE_COMMUNICATION`）都在按名字推荐这些工具，但它们原先
/// 只挂 `collaboration` / `messaging` tag，而这两个 tag 从未进任何 Agent 的
/// `installed_tags`（也不在 `BASE_AGENT_PACKS`）⇒ 它们其实**不在任何 Agent 的工具
/// 列表里**（`send_message_to_agent` 线上实测 0 次调用）。模型只能退回 `send_message`
/// 并把同伴 Agent 的 ID 塞进 `to_user_id` → 消息落成 `to_role=User + to_id=<Agent>`
/// → 投递全失败 → 重试 8 次后静默丢弃（`search_messages` 同理：技能让模型"必须做一次
/// 语义检索"，工具却不在面板里）。
///
/// 这几个工具是同一条沟通链路的不同出口，可达性必须一致：任一被取消 `neural`
/// 都会让提示词 / 技能的推荐重新变成「做不到的事」，且症状是**静默丢消息**而非报错。
/// `recall_message` 是同一条链路的**反向出口**（撤回已入队但未处理的消息），
/// 与其余消息工具可达性必须一致；漏挂 `neural` 的结果同样是**静默失效**
/// （模型撤不掉过时消息，只能继续往队列里堆新的）。
///
/// `cancel_thinking` 列入同一清单：它是 `recall_message` 的**互补原语** ——
/// `recall_message` 管「这条消息还要不要处理」（状态语义），`cancel_thinking` 管
/// 「现在这一轮停不停」（信号语义）。撤回在飞消息时若调用方就是那个 Agent 自己，
/// 本能力会引导它改用 `cancel_thinking`；后者不可达则该引导变成空话。
/// 护栏：本体**消费侧**工具必须带 `ontology` 主题 tag。
///
/// tag 分两层，与安全模型同构（`install_tool_pack` 按 tag 批量授权）：
/// - `ontology` = **消费侧**：查词表 + 按词写节点/关系/属性 + 按属性检索 +
///   图谱遍历 + 沉淀。装这个包是安全的，任何 Agent 都可以有。
/// - `ontology_management` = **治理侧**：改词表定义本身（create/update/retire/
///   delete），敏感，需单独授权。
///
/// 消费侧跨两处目录（本体 handler 的读工具 + agent handler 的记忆/图谱/沉淀工具），
/// 手工维护极易漏——漏了的表现是 `install_tool_pack("ontology")` 只装到一半，
/// Agent 能查词表却拿不到写记忆的工具（或反之），且**不报错**。
#[test]
fn ontology_consumer_tools_carry_ontology_tag() {
    let registry = crate::pkg::tool_registry::get_registry();
    for id in [
        // 本体 handler 的读侧
        "list_ontology_classes",
        "list_ontology_relation_types",
        "list_ontology_properties",
        "list_ontology_synonyms",
        "list_ontology_lexicon",
        "get_ontology_drift_dashboard",
        "list_ontology_drift_class_details",
        "list_ontology_drift_relation_details",
        // agent handler 的本体语义消费方（词表主消费点 / 属性检索 / 遍历 / 沉淀）
        "save_long_term_memory",
        "update_memory",
        "search_memory",
        "settle_memory",
        "get_knowledge_graph",
    ] {
        let factory = registry
            .get_builtin_factory(id)
            .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
        let tags = factory.create_po().get_tags();
        assert!(
            tags.iter().any(|t| t == "ontology"),
            "{id} 是本体消费侧工具，必须带 ontology tag（否则 install_tool_pack(\"ontology\") \
             装不到它，Agent 拿到的是半套本体能力且不报错）。实际 tags={tags:?}"
        );
        // 顺带挡住 `tags = \"\"` —— 宏按逗号 split 空串会产出一个空字符串 tag
        assert!(
            tags.iter().all(|t| !t.is_empty()),
            "{id} 的 tags 含空字符串（tags = \"\" 被宏 split 成 [\"\"]），实际 tags={tags:?}"
        );
    }
}

/// 护栏：本体**消费侧全集**（读侧 8 + 记忆/图谱/沉淀 5）必须恒带 `neural`。
///
/// 这条直接决定「普通员工要不要装 `ontology` 包」的答案：消费侧 13 个工具全部
/// 出生自带 ⇒ **任何 Agent 不装 ontology 包也能用**，`ontology` 在这里只是主题
/// 归类 tag（`install_tool_pack("ontology")` 的批量授权面），**不是可达性前提**。
///
/// ⚠️ 因此真正的风险是反过来：哪个消费侧工具被摘掉 `neural`，它就退化成
/// 「只有装了 ontology 包的 Agent 才可见」⇒ 其余 Agent **静默失去**写记忆、
/// 查词表的能力——不报错、不告警，是这个项目最难查的一类故障。本护栏守这个不变量。
#[test]
fn ontology_consumer_tools_are_neural_reachable() {
    let registry = crate::pkg::tool_registry::get_registry();
    for id in [
        // 读侧（词表 / 漂移看板）
        "list_ontology_classes",
        "list_ontology_relation_types",
        "list_ontology_properties",
        "list_ontology_synonyms",
        "list_ontology_lexicon",
        "get_ontology_drift_dashboard",
        "list_ontology_drift_class_details",
        "list_ontology_drift_relation_details",
        // 写记忆 / 检索 / 遍历 / 沉淀（本体词表的主消费点）
        "save_long_term_memory",
        "update_memory",
        "search_memory",
        "settle_memory",
        "get_knowledge_graph",
    ] {
        let factory = registry
            .get_builtin_factory(id)
            .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
        let tags = factory.create_po().get_tags();
        assert!(
            tags.iter().any(|t| t == "neural"),
            "{id} 是本体消费侧工具，必须带 neural tag：不带它就只有装了 ontology 包的 \
             Agent 才可见，其余 Agent 会静默失去写记忆/查词表能力。实际 tags={tags:?}"
        );
    }
}

/// 护栏：本体**读侧**工具必须对每个 Agent 可达（带 `neural`）。
///
/// 背景与消息类工具同源（工具面 = 绑定工具 ∪ tag(`neural` ∪ `installed_tags`)）：
/// 19 个本体工具原先一律只挂 `ontology_management`，而该 tag 既不在
/// `BASE_AGENT_PACKS`、也不在组织 onboard 的 required packs（seed 只配了
/// `project_management`）⇒ 它们**不在任何 Agent 的工具面里**。本体论技能会教模型
/// 「写记忆前先查词表、发现漂移先查看板」，读侧不可达 ⇒ 这些引导全是空话，
/// 模型只能凭直觉造词 → 蜂巢共享图谱被同义词反复污染（漂移看板正是为此存在）。
///
/// **只守读侧**是刻意的分层：写侧（`create/update/retire/delete_*`）保留
/// `ontology_management` 包 tag，由管理员 Agent 装包后才有权限。词表是蜂巢共享的
/// 全局约定，任何 Agent 都能改 = 一个 Agent 的错误用词会污染所有人的图谱；
/// 与「语义判断走 Agent 提案 + 人审」的设计决策一致。
///
/// ⚠️ 读侧也不可怕漏：属性/词表写入本身有兜底（本体未定义的 term 按值自身类型
/// 照写不丢），所以读侧失效不会报错，只会**静默退化成自由造词**。
#[test]
fn ontology_read_tools_are_neural_reachable() {
    let registry = crate::pkg::tool_registry::get_registry();
    for id in [
        "list_ontology_classes",
        "list_ontology_relation_types",
        "list_ontology_properties",
        "list_ontology_synonyms",
        "list_ontology_lexicon",
        "get_ontology_drift_dashboard",
        "list_ontology_drift_class_details",
        "list_ontology_drift_relation_details",
    ] {
        let factory = registry
            .get_builtin_factory(id)
            .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
        let tags = factory.create_po().get_tags();
        assert!(
            tags.iter().any(|t| t == "neural"),
            "{id} 是本体读侧工具，必须带 neural tag（否则不在任何 Agent 的工具面里，\
             本体论技能里「写前先查词表」的引导会变成静默造词）。实际 tags={tags:?}"
        );
        // 保留 ontology_management：装包的管理员 Agent 仍按领域归类
        assert!(
            tags.iter().any(|t| t == "ontology_management"),
            "{id} 应同时保留 ontology_management tag（领域归类 + 装包可见性），\
             实际 tags={tags:?}"
        );
    }
}

/// 护栏：本体**写侧**工具必须**不**带 `neural`（防后人顺手全开）。
///
/// 词表是蜂巢共享的全局约定：写侧一旦对所有 Agent 开放，单个 Agent 的错误用词
/// 会污染整个组织的图谱，且没有回收路径（retire 只挡新写、历史节点照旧）。
/// 写侧应保留在 `ontology_management` 包内，由管理员 Agent 显式装包获得。
#[test]
fn ontology_write_tools_are_not_neural() {
    let registry = crate::pkg::tool_registry::get_registry();
    for id in [
        "create_ontology_class",
        "update_ontology_class",
        "retire_ontology_class",
        "create_ontology_relation_type",
        "update_ontology_relation_type",
        "retire_ontology_relation_type",
        "create_ontology_property",
        "update_ontology_property",
        "retire_ontology_property",
        "create_ontology_synonym",
        "delete_ontology_synonym",
    ] {
        let factory = registry
            .get_builtin_factory(id)
            .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
        let tags = factory.create_po().get_tags();
        assert!(
            !tags.iter().any(|t| t == "neural"),
            "{id} 是本体写侧工具，不得带 neural tag（词表是蜂巢共享全局约定，\
             应对所有 Agent 开放会让单个 Agent 的错误用词污染整个图谱）。\
             实际 tags={tags:?}"
        );
        assert!(
            tags.iter().any(|t| t == "ontology_management"),
            "{id} 必须挂 ontology_management tag（否则管理员装包后也拿不到），\
             实际 tags={tags:?}"
        );
        // 写侧不得并入 ontology 消费包：否则 install_tool_pack("ontology")
        // 会一把梭给出改词表的权限，治理侧就不再是可控的独立授权面
        assert!(
            !tags.iter().any(|t| t == "ontology"),
            "{id} 是本体治理侧工具，不得带 ontology tag（会让 install_tool_pack(\"ontology\") \
             一把梭给出改词表的权限，治理侧必须保持独立授权面）。实际 tags={tags:?}"
        );
    }
}

#[test]
fn message_tools_are_neural_reachable() {
    let registry = crate::pkg::tool_registry::get_registry();
    for id in [
        "send_message",
        "send_message_to_agent",
        "send_task_assignment_message",
        "list_messages",
        "search_messages",
        "recall_message",
        "cancel_thinking",
    ] {
        let factory = registry
            .get_builtin_factory(id)
            .unwrap_or_else(|| panic!("builtin tool {} not registered", id));
        let tags = factory.create_po().get_tags();
        assert!(
            tags.iter().any(|t| t == "neural"),
            "{id} 必须带 neural tag（否则不在任何 Agent 的工具面里，\
             提示词 / 技能的推荐会变成静默丢消息）。实际 tags={tags:?}"
        );
    }
}

/// 护栏：所有内置工具的参数 schema 必须是「LLM 友好」形状（无 `$ref`/`anyOf`/`oneOf`）。
///
/// 背景：模型在属性层看不到 `type`/`enum` 时，会把枚举值写成**未加引号的裸标识符**
/// （`{"status":Pending}`），整段 arguments 变成非法 JSON → 降级为 null →
/// 下游只看到 `empty tool arguments`。线上实测 25 次/天、25/25 命中 `anyOf+$ref` 形状。
///
/// 收敛由 `common::llm_schema::schema_for_llm` 在 `#[tool(..)]` 宏里完成；
/// 这条测试防止新工具绕过它（例如手写 `create_po` 或新增工厂）。
///
/// 判定复用 `common::llm_schema::unflattened_positions`（与折叠规则同源）——
/// 判别联合（`#[serde(tag="type")]`，如 `CredentialBinding`）属设计内保留，不算违规。
#[test]
fn builtin_tool_schemas_are_llm_flattened() {
    let registry = crate::pkg::tool_registry::get_registry();
    let ids = registry.list_builtin_ids();
    // 防空跑守卫：注册表因故为空时会「零工具全绿」，必须挡住
    assert!(ids.len() > 20, "内置工具数异常偏少: {}", ids.len());

    let mut offenders = Vec::new();
    for id in ids {
        let Some(factory) = registry.get_builtin_factory(&id) else {
            continue;
        };
        let Some(schema) = factory.create_po().parameters_schema else {
            continue;
        };
        for pos in common::llm_schema::unflattened_positions(&schema) {
            offenders.push(format!("  {id}: {pos}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "以下内置工具的 schema 未收敛（模型会写坏枚举参数）:\n{}",
        offenders.join("\n")
    );
}

/// 护栏：所有内置工具的 `function.name`（wire name）必须落在跨 provider LCD 内
/// （`^[a-zA-Z0-9_-]{1,128}$`，判定复用 `common::llm_schema::is_wire_tool_name`）。
///
/// 背景（2026-10-21 线上故障）：`ToolDescriptor::from` 原先直发 DB 的 `name`
/// **显示名列**（"List Tool Tags" 带空格），MCP 工具名形如 `mcp.<uuid>.<tool>`
/// （带点号）。GLM 端点宽松不报错，火山方舟严校验：tools 数组里任一名字违规即
/// 整个请求 400 `InvalidParameter: tools.N.function.name: expected 1-128 ASCII
/// letters, digits, underscores or hyphens`，且报错指不出是哪个工具 ⇒
/// 该 Agent 每一轮模型调用全挂（当日 125 次）。
///
/// 出站收敛见 `ToolDescriptor::from` → `to_wire_tool_name(&po.id)`；
/// 本测试守住「新工具 / 手写 create_po 绕过收敛」这条路，并额外断言 wire name
/// 在注册表内唯一（sanitize 非单射，两个工具撞名会让模型永远调不到其中一个）。
#[test]
fn builtin_tool_wire_names_are_lcd_safe_and_unique() {
    let registry = crate::pkg::tool_registry::get_registry();
    let ids = registry.list_builtin_ids();
    // 防空跑守卫 + 已知靶点在册。
    // 锚点只选 #[register_handler_tool] 宏注册的工具（ctor 在库加载时注册）；
    // GENERIC_BUILTIN_TOOLS 里那批 legacy 工具靠 register_all 注册，测试进程
    // 不跑 main，故不在册——与 builtin_tool_schemas_are_llm_flattened 同范围。
    assert!(ids.len() > 20, "内置工具数异常偏少: {}", ids.len());
    for anchor in ["settle_memory", "send_message", "list_messages"] {
        assert!(
            registry.get_builtin_factory(anchor).is_some(),
            "已知靶点 {anchor} 不在册，测试可能跑空"
        );
    }

    let mut offenders = Vec::new();
    let mut seen: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for id in &ids {
        let Some(factory) = registry.get_builtin_factory(id) else {
            continue;
        };
        let po = factory.create_po();
        let wire = common::llm_schema::to_wire_tool_name(&po.id);
        if !common::llm_schema::is_wire_tool_name(&wire) {
            offenders.push(format!("  {id}: wire name {wire:?} 超出 LCD"));
        }
        if let Some(prev) = seen.insert(wire.clone(), id.clone()) {
            offenders.push(format!(
                "  {id} 与 {prev} 的 wire name 撞名: {wire:?}（模型将永远调不到其中一个）"
            ));
        }
    }
    assert!(
        offenders.is_empty(),
        "以下内置工具的 function.name 不合规:\n{}",
        offenders.join("\n")
    );
}

// ==================== 生命周期双方法的默认实现契约 ====================
//
// 默认实现必须对**存量工具零破坏**：非 CLI 型天然 Ready、不自愈；
// 11 个内置工厂都不覆盖也不影响编译与行为（这是「新增方法不强制改造」的保证）。

/// 从 `GENERIC_BUILTIN_TOOLS` 直接取工厂（不依赖全局 registry 是否已注册）
fn factory_of(id: &str) -> Box<dyn BuiltinToolFactory> {
    super::GENERIC_BUILTIN_TOOLS
        .iter()
        .find(|(tool_id, _)| tool_id == id)
        .map(|(_, factory)| factory.clone())
        .unwrap_or_else(|| panic!("{} factory must exist in GENERIC_BUILTIN_TOOLS", id))
}

#[test]
fn default_probe_readiness_is_ready_for_non_cli_tools() {
    use common::api::RuntimeReady;

    // http_fetch / fs_read / shell_exec 等非 CLI 型：默认就绪
    for id in [
        "http_fetch",
        "fs_read",
        "fs_write",
        "shell_exec",
        "mark_artifact",
    ] {
        let factory = factory_of(id);
        let po = factory.create_po();
        assert_eq!(
            factory.probe_readiness(&po),
            RuntimeReady::Ready,
            "{} 是非 CLI 型，默认必须 Ready",
            id
        );
    }
}

#[test]
fn default_propose_self_heal_is_none_for_non_cli_tools() {
    // 非 CLI 型没有「命令路径」可修 → 默认 None（不得凭空生成配置）
    for id in ["http_fetch", "fs_read", "shell_exec", "tavily_search"] {
        let factory = factory_of(id);
        let po = factory.create_po();
        assert!(
            factory.propose_self_heal(&po).is_none(),
            "{} 不应产生自愈提议",
            id
        );
    }
}

#[test]
fn every_cli_factory_implements_lifecycle_methods() {
    use common::api::RuntimeReady;

    // 四个 CLI 型工厂都必须覆盖 probe_readiness：判定要么 Ready、
    // 要么是 cli_not_installed（绝不能落回「默认 Ready」掩盖问题）
    for id in ["lark_cli", "gh_cli", "skillhub_cli", "browser"] {
        let factory = factory_of(id);
        let mut po = factory.create_po();
        // 刻意写一个必然不可解析的命令
        po.config = serde_json::json!({ "command": "/no/such/dir/no-such-cli-xyz" });
        match factory.probe_readiness(&po) {
            RuntimeReady::Ready => {
                // 允许：环境里确实能通过默认布局/PATH 发现该工具的 bin 名
            }
            RuntimeReady::NotReady { reason, .. } => {
                assert_eq!(
                    reason, "cli_not_installed",
                    "{} 的未就绪原因必须是 cli_not_installed 才可自愈",
                    id
                );
            }
            other => panic!("{} unexpected readiness: {:?}", id, other),
        }
    }
}

#[test]
fn self_heal_proposal_is_always_single_command_field_or_none() {
    // 红线守卫：任何工厂的自愈提议都只能含 command 一个字段
    for id in ["lark_cli", "gh_cli", "skillhub_cli", "browser"] {
        let factory = factory_of(id);
        let mut po = factory.create_po();
        po.config = serde_json::json!({ "command": "/no/such/dir/no-such-cli-xyz" });
        if let Some(proposal) = factory.propose_self_heal(&po) {
            let obj = proposal.as_object().expect("proposal must be object");
            assert_eq!(obj.len(), 1, "{} proposal must be single-field", id);
            assert!(
                obj.contains_key("command"),
                "{} must only propose command",
                id
            );
            assert!(
                obj["command"].as_str().is_some_and(|s| !s.is_empty()),
                "{} proposed command must be non-empty string",
                id
            );
        }
    }
}
