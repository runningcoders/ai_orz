//! URL 工具：从完整 URL 提取 path（含 query）
//!
//! 联邦出站签名绑定请求的 path（规范串第二项），服务端用收到的
//! `uri.path_and_query()` 比对——两端必须对同一请求产出同一串。

/// 从 URL 提取 path + query（无 query 时仅 path；无法解析 host 返回 None）
///
/// 规则：跳过 `scheme://host[:port]`，取第一个 `/` 起的剩余部分；
/// host 后无路径（`http://h`）按根路径 `/` 处理（与 HTTP 语义一致）。
/// `http://h/a2a` → `/a2a`；`http://h` → `/`。
pub fn path_and_query(url: &str) -> Option<&str> {
    let rest = url.split_once("://")?.1;
    let Some(path_start) = rest.find('/') else {
        return Some("/");
    };
    let path = &rest[path_start..];
    let path = path.split(['#']).next().unwrap_or(path);
    Some(if path.is_empty() { "/" } else { path })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_path_and_query() {
        assert_eq!(path_and_query("http://h/a2a"), Some("/a2a"));
        assert_eq!(
            path_and_query("https://g.example.com/api/v1/x?q=1"),
            Some("/api/v1/x?q=1")
        );
        assert_eq!(path_and_query("http://h"), Some("/"));
        assert_eq!(path_and_query("http://h:8080"), Some("/"));
        assert_eq!(path_and_query("http://h:8080/a/b#frag"), Some("/a/b"));
        assert_eq!(path_and_query("notaurl"), None);
    }
}
