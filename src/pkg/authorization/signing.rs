//! 命令规范化 / 签名 / 匹配纯函数（pkg/authorization 基建）

/// 命令规范化：trim + 连续空白折叠为单空格（签名生成的唯一规范化步骤；
/// 不做大小写折叠等有语义风险的变换）
pub fn normalize_command(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 命令签名 = 规范化命令文本（同签名重试放行与授权查表键）
pub fn command_signature(raw: &str) -> String {
    normalize_command(raw)
}

/// 签名匹配：精确（默认最窄授权）或前缀（token 边界）。
/// 前缀匹配要求 grant 签名后紧跟空格边界——"docker push" 不会误伤 "docker pushx"。
pub fn signature_matches(
    grant_signature: &str,
    prefix_match: bool,
    command_signature: &str,
) -> bool {
    if grant_signature == command_signature {
        return true;
    }
    if !prefix_match {
        return false;
    }
    match command_signature.strip_prefix(grant_signature) {
        Some(rest) => rest.is_empty() || rest.starts_with(' '),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_collapses_whitespace_and_trims() {
        assert_eq!(
            normalize_command("  docker   push\nregistry.local/app:1.0  "),
            "docker push registry.local/app:1.0"
        );
        assert_eq!(normalize_command(""), "");
    }

    #[test]
    fn signature_is_normalized_command() {
        assert_eq!(
            command_signature(" docker push  registry.local/app:1.0 "),
            "docker push registry.local/app:1.0"
        );
    }

    #[test]
    fn exact_match_by_default() {
        assert!(signature_matches(
            "docker push registry.local/app:1.0",
            false,
            "docker push registry.local/app:1.0"
        ));
        assert!(!signature_matches(
            "docker push",
            false,
            "docker push registry.local/app:1.0"
        ));
    }

    #[test]
    fn prefix_match_respects_token_boundary() {
        assert!(signature_matches(
            "docker push",
            true,
            "docker push registry.local/app:1.0"
        ));
        assert!(signature_matches("docker push", true, "docker push"));
        assert!(!signature_matches(
            "docker push",
            true,
            "docker pushx registry.local/app:1.0"
        ));
        assert!(!signature_matches(
            "docker push",
            false,
            "docker push registry.local/app:1.0"
        ));
    }

    #[test]
    fn empty_grant_signature_never_prefix_matches() {
        assert!(!signature_matches("", true, "docker push"));
    }
}
