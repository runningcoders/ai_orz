//! 头像兜底文字：把任意展示名压缩为「最多 2 个汉字 / 8 个字母」的缩写
//!
//! 用于对话气泡、导航栏用户菜单等没有头像图片时的占位文字。
//! 规则与产品约定一致：含 CJK 时取前 2 个非空白字符，否则取前 8 个非空白字符。

/// 判断是否为 CJK / 假名 / 谚文等表意文字字符
fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{30FF}'   // 平假名 / 片假名
            | '\u{3400}'..='\u{4DBF}' // 扩展 A
            | '\u{4E00}'..='\u{9FFF}' // 基本汉字
            | '\u{AC00}'..='\u{D7AF}' // 谚文音节
            | '\u{F900}'..='\u{FAFF}' // 兼容汉字
    )
}

/// 由展示名生成头像兜底缩写
///
/// - 含 CJK 时取前 2 个非空白字符（如「张三」→「张三」、「奥特曼」→「奥特」）；
/// - 否则取前 8 个非空白字符（如「Claude」→「Claude」、「gpt-4o」→「gpt-4o」）；
/// - 空名兜底为「?」。
pub fn avatar_initials(name: &str) -> String {
    let chars: Vec<char> = name.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.is_empty() {
        return "?".to_string();
    }
    let has_cjk = chars.iter().take(2).any(|c| is_cjk(*c));
    let n = if has_cjk { 2 } else { 8 };
    chars.iter().take(n).collect()
}
