//! Common security utilities for tool registry
//!
//! HTTP 安全函数统一在 `pkg::http::ssrf`，此处再导出保持向后兼容。
//! URL 模板校验和文件系统安全仍保留在此模块。

// 再导出：HTTP 安全函数（SSRF / 大小限制 / 敏感头）从 pkg::http 共享
pub use crate::pkg::http::{
    DEFAULT_RESPONSE_MAX_BYTES, DEFAULT_TIMEOUT_MS, HARD_RESPONSE_MAX_BYTES, MAX_TIMEOUT_MS,
    domain_matches, is_local_network_host, is_local_network_ip, is_sensitive_header,
    normalize_domain, read_limited_response_body, sanitize_response_headers, validate_target_url,
};

use anyhow::anyhow;
use common::error::Result;

/// Validate URL template boundary (scheme and authority must be fixed)
pub fn validate_url_template_boundary(url_template: &str) -> Result<()> {
    let trimmed = url_template.trim();
    let Some(scheme_end) = trimmed.find("://") else {
        return Err(anyhow!("http tool url must include http/https scheme").into());
    };

    let scheme = &trimmed[..scheme_end];
    if scheme.contains("{{") {
        return Err(anyhow!(
            "http url scheme must be fixed and must not contain template placeholders"
        )
        .into());
    }
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("unsupported http url scheme: {}", scheme).into());
    }

    let after_scheme = &trimmed[scheme_end + 3..];
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    if authority.is_empty() {
        return Err(anyhow!("http url host is required").into());
    }
    if authority.contains("{{") {
        return Err(anyhow!(
            "http url host must be fixed and must not contain template placeholders"
        )
        .into());
    }
    if authority.contains('@') {
        return Err(anyhow!("http url must not contain userinfo credentials").into());
    }

    Ok(())
}

/// Replace placeholders with sentinels for URL validation
pub fn url_template_with_placeholder_sentinels(url_template: &str) -> Result<String> {
    let mut output = String::with_capacity(url_template.len());
    let mut rest = url_template;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err(anyhow!("unresolved or unsupported http template placeholder").into());
        };
        output.push_str("placeholder");
        rest = &after_start[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

/// Validate that all placeholders are of the supported form `{{args.key}}`
pub fn validate_supported_placeholders(template: &str) -> Result<()> {
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err(anyhow!("unresolved or unsupported http template placeholder").into());
        };
        let placeholder = &after_start[..end];
        if placeholder.trim() != placeholder
            || !placeholder.starts_with("args.")
            || placeholder.len() <= "args.".len()
        {
            return Err(anyhow!("unresolved or unsupported http template placeholder").into());
        }
        rest = &after_start[end + 2..];
    }

    if rest.contains("}}") {
        return Err(anyhow!("unresolved or unsupported http template placeholder").into());
    }

    Ok(())
}

/// File system tool security utilities shared by fs_read and fs_write
pub mod fs {
    use anyhow::anyhow;
    use common::error::Result;
    use std::path::{Path, PathBuf};

    /// Validation result for file path checks
    #[derive(Debug)]
    pub enum ValidationResult {
        /// Path is valid and can be accessed
        Valid(PathBuf),
        /// Path outside default scope - needs explicit user confirmation
        NeedConfirmation(String),
    }

    /// Resolve and validate a user-provided path against base data directory and additional allowed paths.
    /// Follows sandbox security rules:
    /// 1. Normalize lexically (kills `.`/`..` so the scope check can't be fooled)
    /// 2. Canonicalize to resolve symlinks
    /// 3. Check that final path is within allowed scope (base or additional)，
    ///    并取得「相对作用域根」的路径段
    /// 4. Reject sensitive filenames on the scope-relative portion（按 [`FsAccessMode`] 分档）
    /// 5. Reject symbolic links
    ///
    /// 目标路径**尚不存在**时（写新文件 / 建目录）不报错：向上找最近的已存在祖先
    /// canonicalize 后拼回缺失尾部，作用域校验仍在完整路径上进行。
    ///
    /// 敏感名判定只看**作用域根以内的相对部分**：基址（`.ai_orz`）与白名单根
    /// （如 `/home/u/.config/app`）自身的隐藏成分不属于 Agent 可操控的路径，
    /// 不参与判定——否则白名单外部路径因根里带隐藏成分被整体误拦。
    /// `mode` 决定检查档位（见 [`FsAccessMode`]）：读宽松 / 写严格。
    pub fn resolve_and_validate_path(
        base_path: &Path,
        user_path: &str,
        additional_allowed_paths: &[String],
        mode: FsAccessMode,
    ) -> Result<ValidationResult> {
        // 1. Build absolute path (lexically normalized — see normalize_lexically)
        let user_path_input = user_path;
        let user_path = Path::new(user_path_input);
        let raw_path = if user_path.is_absolute() {
            user_path.to_path_buf()
        } else {
            base_path.join(user_path)
        };
        let absolute_path = normalize_lexically(&raw_path);

        // 2. Canonicalize to resolve .. and symlinks.
        //    Path exists: canonicalize directly.
        //    Path missing: canonicalize the nearest existing ancestor and re-join
        //    the missing tail (see resolve_missing_path).
        let canonical = if absolute_path.exists() {
            absolute_path
                .canonicalize()
                .map_err(|_| anyhow!("Failed to resolve path: file not found or permission denied"))
        } else {
            resolve_missing_path(&absolute_path)
        }
        .map_err(common::error::Error::from)?;

        // 3. Check that canonical path is in allowed scope:
        //    - either under base_path, OR under one of the additional allowed paths
        //    命中的根同时给出「相对段」，供第 4 步敏感名判定使用
        let base_canonical = base_path
            .canonicalize()
            .map_err(|e| anyhow!("Invalid base data path: {}", e))
            .map_err(common::error::Error::from)?;

        let mut scope_rel = canonical
            .strip_prefix(&base_canonical)
            .ok()
            .map(|p| p.to_path_buf());

        // Check additional allowed paths from configuration
        if scope_rel.is_none() {
            for additional in additional_allowed_paths {
                let additional_path = Path::new(additional);
                let additional_canon = if additional_path.is_absolute() {
                    additional_path.to_path_buf()
                } else {
                    base_path.join(additional_path)
                }
                .canonicalize();
                if let Ok(additional_canon) = additional_canon
                    && let Ok(rel) = canonical.strip_prefix(&additional_canon)
                {
                    scope_rel = Some(rel.to_path_buf());
                    break;
                }
                // Ignore invalid/unresolvable additional paths
            }
        }

        let Some(scope_rel) = scope_rel else {
            // Not strictly denied, but needs user confirmation
            return Ok(ValidationResult::NeedConfirmation(format!(
                "Path '{}' is outside the default working directory. \
            You MUST STOP and ask the user for explicit confirmation before accessing this file.",
                user_path_input
            )));
        };

        // 4. Check sensitive filename patterns on the scope-relative portion（按 mode 分档）
        if is_sensitive_path(&scope_rel.to_string_lossy(), mode) {
            return Err(anyhow!("Access denied: cannot access sensitive file").into());
        }

        // 5. Reject symlinks
        if let Ok(metadata) = canonical.symlink_metadata()
            && metadata.file_type().is_symlink()
        {
            return Err(anyhow!("Access denied: symbolic links are not allowed").into());
        }

        Ok(ValidationResult::Valid(canonical))
    }

    /// 词法规范化：消除路径中的 `.` / `..` 成分（不触碰文件系统）
    ///
    /// 作用域校验用 `starts_with`（纯词法逐成分比较）：若 `a/../../etc/x` 这类残留
    /// `..` 的成分参与比较，前缀匹配会被绕过 ⇒ 必须在拼接后、校验前规范化。
    /// 与「先 canonicalize 再校验」相比，词法优先还顺带消掉「符号链接 + `..`」
    /// 组合逃逸（`..` 不再穿透链接目标解析）。
    fn normalize_lexically(path: &Path) -> PathBuf {
        use std::path::Component;
        let mut out = PathBuf::new();
        for comp in path.components() {
            match comp {
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                other => out.push(other.as_os_str()),
            }
        }
        out
    }

    /// 为「尚不存在的路径」构造校验用路径：自下向上找最近的已存在祖先，
    /// canonicalize 该祖先后拼回缺失尾部（写新文件 / 建目录时路径本就不存在）。
    ///
    /// 祖先经 canonicalize ⇒ 链条上的符号链接被物理展开，逃逸链接会在
    /// 作用域校验时现形；向上直到文件系统根都找不到已存在祖先才报错。
    fn resolve_missing_path(absolute_path: &Path) -> anyhow::Result<PathBuf> {
        let mut tail: Vec<std::ffi::OsString> = Vec::new();
        let mut current = absolute_path.to_path_buf();
        loop {
            match current.parent() {
                // 父路径非空：记录尾部成分后继续向上
                Some(parent) if !parent.as_os_str().is_empty() => {
                    if let Some(name) = current.file_name() {
                        tail.push(name.to_os_string());
                    }
                    current = parent.to_path_buf();
                    if current.exists() {
                        let anchor = current
                            .canonicalize()
                            .map_err(|_| anyhow!("Failed to resolve path: permission denied"))?;
                        tail.reverse();
                        return Ok(anchor.join(tail.iter().collect::<PathBuf>()));
                    }
                }
                // 已到文件系统根仍无已存在祖先
                _ => {
                    return Err(anyhow!(
                        "Parent directory does not exist or permission denied"
                    ));
                }
            }
        }
    }

    /// 敏感关键词：按「词元」匹配（词元 = 成分按非字母数字切分后的段），
    /// 词元等于或以关键词结尾才算命中——避免子串误杀（`author.md` / `tokenizer.rs`）
    const SENSITIVE_KEYWORDS: [&str; 5] = ["password", "secret", "token", "credential", "auth"];

    /// 敏感路径检查档位：读宽松 / 写严格
    ///
    /// - [`FsAccessMode::Read`]：读取与目录列举。仅拦「承载机密」的路径成分
    ///   （凭据 / 密钥命名、机密隐藏名），普通隐藏文件（`.gitignore`、
    ///   `docs/.hidden/x.txt`）可读可列——读档不为写侧的注入面买单。
    /// - [`FsAccessMode::Write`]：写入与建目录。任何隐藏成分一律拦——写入
    ///   `.git/hooks/` 会在工作区惰性 git init 后被 git 钩子执行（注入面），
    ///   必须从严；隐藏配置文件也不该被 Agent 覆写。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FsAccessMode {
        /// 读取 / 目录列举（宽松档）
        Read,
        /// 写入 / 建目录（严格档）
        Write,
    }

    /// 承载机密的隐藏名（读档特判名单）：`.env*` 之外的凭据载体。
    /// 按「名字.」派生族匹配：`.yarnrc.yml` 命中 `.yarnrc`，而 `.gitignore`
    /// 不命中 `.git`（派生族要求 `.` 分隔后缀）。
    const SECRET_HIDDEN_NAMES: [&str; 9] = [
        ".git", // config 可含带令牌的远端 URL（工作区会惰性 git init，非理论风险）
        ".ssh", ".aws", ".gnupg", ".npmrc", ".netrc", ".pypirc", ".docker", ".yarnrc",
    ];

    /// 隐藏成分是否承载机密（读档判定；写档一律拦隐藏成分，不走本函数）
    fn is_secret_hidden_component(comp: &str) -> bool {
        // .env / .env.local / .envrc —— 环境变量文件普遍含凭据，前缀匹配
        if comp.starts_with(".env") {
            return true;
        }
        SECRET_HIDDEN_NAMES.iter().any(|name| {
            comp == *name
                || comp
                    .strip_prefix(name)
                    .is_some_and(|rest| rest.starts_with('.'))
        })
    }

    /// Check if a path (or bare filename) matches sensitive patterns that should be blocked.
    ///
    /// 按**路径成分**匹配，任意层级的成分（中间目录成分与文件名成分）一视同仁，
    /// 按 [`FsAccessMode`] 分档：
    /// 1. 隐藏成分：写档一律拦（`.env`、`.git`、`.ssh`、`docs/.hidden/x.txt` 的
    ///    `.hidden` 等）；读档仅拦 [`is_secret_hidden_component`]（机密载体），
    ///    普通隐藏文件放行；
    /// 2. 成分等于敏感名或其「名字.」派生族：`id_rsa` / `id_rsa.pub` / `id_dsa.pub` …；
    /// 3. 成分以敏感后缀结尾：`.pem` / `.key` / `.p12` / `.pfx` / `.env`（`config.env`
    ///    这类「名字.env」形态不靠隐藏规则也能拦）；
    /// 4. 成分的词元命中 [`SENSITIVE_KEYWORDS`]：`password.txt`、`secret_key`、
    ///    `api_token` 拦；`author.md`、`tokenizer.rs` 这类子串误杀放行。
    pub fn is_sensitive_path(path: &str, mode: FsAccessMode) -> bool {
        let lower = path.to_lowercase();
        for comp in lower.split('/') {
            // 跳过空段与词法归一化成分，避免把 `..` 误判为隐藏文件
            if comp.is_empty() || comp == "." || comp == ".." {
                continue;
            }
            // 1. 隐藏成分：写档一律拦；读档仅拦机密载体
            if comp.starts_with('.') {
                match mode {
                    FsAccessMode::Write => return true,
                    FsAccessMode::Read => {
                        if is_secret_hidden_component(comp) {
                            return true;
                        }
                    }
                }
            }
            // 2. 敏感名精确匹配或「名字.」派生族（如 id_rsa.pub）
            if matches!(comp, "id_rsa" | "id_dsa" | "id_ecdsa")
                || ["id_rsa", "id_dsa", "id_ecdsa"].iter().any(|name| {
                    comp.strip_prefix(name)
                        .is_some_and(|rest| rest.starts_with('.'))
                })
            {
                return true;
            }
            // 3. 敏感后缀（`.env` 同时以隐藏成分本名与「名字.env」形态出现，
            //    如 config.env / prod.env——不能只靠隐藏成分规则）
            if [".pem", ".key", ".p12", ".pfx", ".env"]
                .iter()
                .any(|ext| comp.ends_with(ext))
            {
                return true;
            }
            // 4. 敏感关键词词元：等于或以关键词结尾才算（tokenizer ≠ token）
            if comp
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|token| SENSITIVE_KEYWORDS.iter().any(|kw| token.ends_with(kw)))
            {
                return true;
            }
        }
        false
    }

    /// 校验「已 canonicalize 的绝对路径」是否落在作用域内（base 或附加允许路径）
    ///
    /// 与 [`resolve_and_validate_path`] 第 4 步同一套 `starts_with` 语义，
    /// 供写入侧 `create_dir_all` 之后的 TOCTOU 复核复用。
    /// 不做敏感名检查：canonical 形态含 `.ai_orz` 等隐藏基础目录成分，
    /// 名字规则只适用于用户输入路径。
    pub fn canonical_path_in_scope(
        base_path: &Path,
        canonical: &Path,
        additional_allowed_paths: &[String],
    ) -> bool {
        let Ok(base_canonical) = base_path.canonicalize() else {
            return false;
        };
        if canonical.starts_with(&base_canonical) {
            return true;
        }
        for additional in additional_allowed_paths {
            let additional_path = Path::new(additional);
            let additional_canon = if additional_path.is_absolute() {
                additional_path.to_path_buf()
            } else {
                base_path.join(additional_path)
            }
            .canonicalize();
            if let Ok(additional_canon) = additional_canon
                && canonical.starts_with(&additional_canon)
            {
                return true;
            }
            // Ignore invalid/unresolvable additional paths
        }
        false
    }

    /// 兼容包装：等价于 [`is_sensitive_path`] 的写档（严格档）语义。
    /// 新代码请显式传 [`FsAccessMode`] 选择档位。
    pub fn is_sensitive_filename(path: &str) -> bool {
        is_sensitive_path(path, FsAccessMode::Write)
    }

    /// Sanitize IO error to remove absolute paths for security
    pub fn sanitize_error<E: std::fmt::Display>(e: E) -> String {
        let s = e.to_string();
        // Remove absolute path prefixes, keep only the error message
        // This is a simple sanitization, enough for our purposes
        s.split('/')
            .next_back()
            .map(|last| last.to_string())
            .unwrap_or(s)
    }

    /// 判定目标路径是否越过「用户树」身份边界（`{base}/users/{other}`）
    ///
    /// # 规则
    ///
    /// 目标位于 `{base}/users/{X}` 下且 `X` 与当前 `user_id` 不一致时视为越界；
    /// 无用户身份（None）时任何 `users/{X}` 均视为越界。
    /// 仅覆盖用户树；顶层系统目录（agents/、skills/ 等）不在本检查范围。
    ///
    /// 返回 true 表示需要用户确认（NeedConfirmation 语义），不是硬拒绝。
    pub fn crosses_user_boundary(base_root: &Path, target: &Path, user_id: Option<&str>) -> bool {
        let Some(rel) = relative_to_root(base_root, target) else {
            return false;
        };
        let mut comps = rel.components();
        if comps.next().and_then(|c| c.as_os_str().to_str()) != Some("users") {
            return false;
        }
        let Some(owner) = comps.next().and_then(|c| c.as_os_str().to_str()) else {
            return false;
        };
        user_id != Some(owner)
    }

    /// 判定目标路径是否越过「Agent 工作区」边界（写保护场景）
    ///
    /// # 规则
    ///
    /// 仅当 `agent_id` 存在（Agent 调用）时检查，覆盖两类路径：
    /// - `{base}/agents/{other}`：其他 Agent 的顶层树（记忆/技能/自身工作区）
    /// - `{base}/users/{any}/agents/{other}`：用户树内其他 Agent 的工作区
    ///
    /// 用户/系统直接调用（agent_id 为 None）不受限。
    pub fn crosses_agent_workspace(
        base_root: &Path,
        target: &Path,
        agent_id: Option<&str>,
    ) -> bool {
        let Some(current) = agent_id else {
            return false;
        };
        let Some(rel) = relative_to_root(base_root, target) else {
            return false;
        };
        let comps: Vec<&str> = rel
            .components()
            .filter_map(|c| c.as_os_str().to_str())
            .collect();
        match comps.first() {
            // 顶层 Agent 树：agents/{other}/...
            Some(&"agents") => comps.get(1).is_some_and(|a| *a != current),
            // 用户树内 Agent 工作区：users/{any}/agents/{other}/...
            Some(&"users") => {
                comps.get(1).is_some()
                    && comps.get(2) == Some(&"agents")
                    && comps.get(3).is_some_and(|a| *a != current)
            }
            _ => false,
        }
    }

    /// 将 target 规约为相对 base_root 的路径
    ///
    /// macOS 上临时目录含符号链接（/var → /private/var），且 target 可能尚不存在
    /// 无法 canonicalize，因此对 root/target 各取「原样 + canonicalize」两种形态
    /// 交叉匹配，任一命中即认为在根内。
    fn relative_to_root(base_root: &Path, target: &Path) -> Option<PathBuf> {
        let root_variants = [
            base_root.to_path_buf(),
            base_root
                .canonicalize()
                .unwrap_or_else(|_| base_root.to_path_buf()),
        ];
        let target_variants = [
            target.to_path_buf(),
            target
                .canonicalize()
                .unwrap_or_else(|_| target.to_path_buf()),
        ];
        for root in &root_variants {
            for tgt in &target_variants {
                if let Ok(rel) = tgt.strip_prefix(root) {
                    return Some(rel.to_path_buf());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod fs_boundary_tests {
    use super::fs::{crosses_agent_workspace, crosses_user_boundary};
    use std::path::Path;

    const BASE: &str = "/data/.ai_orz";

    #[test]
    fn user_boundary_allows_own_and_non_user_tree() {
        let base = Path::new(BASE);
        // 自己的用户树：不越界
        assert!(!crosses_user_boundary(
            base,
            Path::new("/data/.ai_orz/users/u1/shared/p/readme.md"),
            Some("u1")
        ));
        // 用户树之外：不越界
        assert!(!crosses_user_boundary(
            base,
            Path::new("/data/.ai_orz/skills/s1/skill.md"),
            Some("u1")
        ));
    }

    #[test]
    fn user_boundary_blocks_other_user_tree() {
        assert!(crosses_user_boundary(
            Path::new(BASE),
            Path::new("/data/.ai_orz/users/u2/agents/a1/work/x.md"),
            Some("u1")
        ));
        // 无用户身份：任何用户树都视为越界
        assert!(crosses_user_boundary(
            Path::new(BASE),
            Path::new("/data/.ai_orz/users/u1/shared/x"),
            None
        ));
    }

    #[test]
    fn agent_workspace_boundary_rules() {
        let base = Path::new(BASE);
        // 顶层：自己的树不越界，其他 Agent 越界
        assert!(!crosses_agent_workspace(
            base,
            Path::new("/data/.ai_orz/agents/a1/work/x"),
            Some("a1")
        ));
        assert!(crosses_agent_workspace(
            base,
            Path::new("/data/.ai_orz/agents/a2/work/x"),
            Some("a1")
        ));
        // 用户树内：其他 Agent 工作区越界（无论属主用户是谁）
        assert!(crosses_agent_workspace(
            base,
            Path::new("/data/.ai_orz/users/u1/agents/a2/work/x"),
            Some("a1")
        ));
        // 用户树内：自己的工作区不越界；shared 区不越界
        assert!(!crosses_agent_workspace(
            base,
            Path::new("/data/.ai_orz/users/u1/agents/a1/work/x"),
            Some("a1")
        ));
        assert!(!crosses_agent_workspace(
            base,
            Path::new("/data/.ai_orz/users/u1/shared/projects/p"),
            Some("a1")
        ));
        // 无 Agent 身份：不受限
        assert!(!crosses_agent_workspace(
            base,
            Path::new("/data/.ai_orz/agents/a2/work/x"),
            None
        ));
    }
}
