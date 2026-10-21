//! wire_name_tests 单元测试（拆分自 llm_schema.rs，拆分前 969 行）
//!
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod wire_name_tests` 层级，
//! 测试里 `use super::...` 仍能看到父模块的私有函数。
//!
//! 本模块为**手工拆分**：llm_schema.rs 当时有两个顶层测试模块
//! （`tests` + 本模块，后者由 wire name 修复引入），
//! `tools/split_inline_tests.py` 一次只处理一个——先拆本模块，再跑工具拆 `mod tests`。

use super::{is_wire_tool_name, to_wire_tool_name};

#[test]
fn 合规标识符_恒等() {
    // 内置工具 id 本就是 snake_case，一根头发都不许动
    assert_eq!(to_wire_tool_name("list_tool_tags"), "list_tool_tags");
    assert_eq!(to_wire_tool_name("settle_memory"), "settle_memory");
    assert_eq!(to_wire_tool_name("fs_read"), "fs_read");
}

#[test]
fn 显示名非法字符_全部替换为下划线() {
    // DB tools.name 是人类显示名：空格 / 斜杠 / 括号 / 撇号全超 LCD
    assert_eq!(to_wire_tool_name("List Tool Tags"), "List_Tool_Tags");
    assert_eq!(to_wire_tool_name("Feishu/Lark CLI"), "Feishu_Lark_CLI");
    assert_eq!(
        to_wire_tool_name("Search Web (Tavily)"),
        "Search_Web__Tavily_"
    );
    assert_eq!(
        to_wire_tool_name("List Agent's Tasks"),
        "List_Agent_s_Tasks"
    );
    assert_eq!(to_wire_tool_name("GitHub CLI"), "GitHub_CLI");
}

#[test]
fn mcp_工具名_点号换下划线() {
    // MCP 入库 id = mcp.<server-uuid>.<remote-tool>，点号不在 LCD 内
    let wire = to_wire_tool_name("mcp.01a139fc-766a-7600-be25-0cc8cd9313c4.add_transaction");
    assert_eq!(
        wire,
        "mcp_01a139fc-766a-7600-be25-0cc8cd9313c4_add_transaction"
    );
    assert!(is_wire_tool_name(&wire));
}

#[test]
fn 中文与纯符号_不出现原字符() {
    assert_eq!(to_wire_tool_name("查询任务"), "____");
    assert_eq!(to_wire_tool_name("!!!"), "___");
    assert!(is_wire_tool_name(to_wire_tool_name("查询任务").as_str()));
}

#[test]
fn 空输入_兜底非空() {
    assert_eq!(to_wire_tool_name(""), "tool");
}

#[test]
fn 超长名_截断到128且同输入可复现() {
    let long = "a".repeat(200);
    let wire = to_wire_tool_name(&long);
    assert_eq!(wire.len(), 128);
    assert_eq!(wire, to_wire_tool_name(&long));
    assert!(is_wire_tool_name(&wire));
}

#[test]
fn 超长名_不同输入_后缀不撞() {
    let a = to_wire_tool_name(&"a".repeat(200));
    let b = to_wire_tool_name(&format!("{}b", "a".repeat(199)));
    assert_ne!(a, b, "截断后缀必须把同前缀不同输入区分开");
}

#[test]
fn 输出恒落在_lcd_内() {
    for raw in [
        "list_tool_tags",
        "List Tool Tags",
        "Feishu/Lark CLI",
        "mcp.01a139fc-766a-7600-be25-0cc8cd9313c4.add_transaction",
        "查询任务",
        "",
        &"x".repeat(500),
        "a b.c/d(e)f'g\"h",
    ] {
        let wire = to_wire_tool_name(raw);
        assert!(
            is_wire_tool_name(&wire),
            "wire name 超出 LCD: {raw:?} -> {wire:?}"
        );
    }
}

#[test]
fn lcd_判定_双向() {
    // 合规的放行
    assert!(is_wire_tool_name("list_tool_tags"));
    assert!(is_wire_tool_name("mcp_abc_def"));
    assert!(is_wire_tool_name("a-b_c"));
    // 违规的挡住
    assert!(!is_wire_tool_name("List Tool Tags"));
    assert!(!is_wire_tool_name("mcp.abc.def"));
    assert!(!is_wire_tool_name(""));
    assert!(!is_wire_tool_name(&"a".repeat(129)));
}
