//! tests 单元测试（拆分自 file.rs）
//!
//! 文件瘦身：原 570 行 → 373 行，测试体 198 行。
//! 用 `#[path]` 而非 mod.rs 注册：保住 `mod tests` 层级，
//! 测试里 `use super::*` 仍能看到父模块的私有 use 与私有 helper。
//!
//! 拆分命令见 `tools/split_inline_tests.py`。

use super::*;
use axum::body::to_bytes;

fn opts<'a>(mime: &'a str, name: &'a str) -> FileServeOptions<'a> {
    FileServeOptions {
        mime_type: mime,
        file_name: name,
        cache_control: Some("private, max-age=60"),
        force_download: false,
    }
}

#[test]
fn range_variants_are_parsed() {
    assert_eq!(
        parse_range("bytes=0-3", 10),
        RangeOutcome::Satisfiable(ByteRange { start: 0, end: 3 })
    );
    assert_eq!(
        parse_range("bytes=5-", 10),
        RangeOutcome::Satisfiable(ByteRange { start: 5, end: 9 })
    );
    assert_eq!(
        parse_range("bytes=-4", 10),
        RangeOutcome::Satisfiable(ByteRange { start: 6, end: 9 })
    );
    // 末尾超过文件长度 → 截到文件末尾
    assert_eq!(
        parse_range("bytes=8-99", 10),
        RangeOutcome::Satisfiable(ByteRange { start: 8, end: 9 })
    );
    // 后缀长度大于文件 → 整个文件
    assert_eq!(
        parse_range("bytes=-99", 10),
        RangeOutcome::Satisfiable(ByteRange { start: 0, end: 9 })
    );
}

#[test]
fn invalid_or_multi_ranges_are_ignored() {
    for raw in [
        "items=0-3",
        "bytes=abc-def",
        "bytes=3-1",
        "bytes=0-1,4-5",
        "bytes=",
        "",
    ] {
        assert_eq!(parse_range(raw, 10), RangeOutcome::Ignore, "应忽略: {raw}");
    }
}

/// 空文件是 `parse_range` 的直接调用边界：`size - 1` 会下溢，必须提前拦住。
#[test]
fn empty_file_range_is_ignored_without_panicking() {
    for raw in ["bytes=0-", "bytes=0-3", "bytes=-4"] {
        assert_eq!(parse_range(raw, 0), RangeOutcome::Ignore, "应忽略: {raw}");
    }
}

#[test]
fn out_of_bounds_range_is_unsatisfiable() {
    assert_eq!(parse_range("bytes=10-", 10), RangeOutcome::Unsatisfiable);
    assert_eq!(parse_range("bytes=99-100", 10), RangeOutcome::Unsatisfiable);
    assert_eq!(parse_range("bytes=-0", 10), RangeOutcome::Unsatisfiable);
}

#[test]
fn inline_policy_blocks_executable_types() {
    assert!(is_inline_safe("image/png"));
    assert!(is_inline_safe("video/mp4"));
    assert!(is_inline_safe("application/pdf"));
    assert!(is_inline_safe("text/plain; charset=utf-8"));
    // 可执行 / 可内嵌脚本类型必须走下载
    for mime in [
        "text/html",
        "application/xhtml+xml",
        "image/svg+xml",
        "application/octet-stream",
        "application/zip",
        "",
    ] {
        assert!(!is_inline_safe(mime), "不应内联: {mime}");
    }
}

#[test]
fn content_disposition_encodes_non_ascii_name() {
    let value = content_disposition("image/png", "测试 图.png", false);
    let text = value.to_str().unwrap();
    assert!(text.starts_with("inline;"));
    assert!(text.contains("filename*=UTF-8''"));
    assert!(text.contains("%E6%B5%8B"), "中文应百分号编码: {text}");
    // header 值必须是 ASCII，否则构造会失败
    assert!(text.is_ascii());
}

#[test]
fn content_disposition_forces_download_for_html() {
    let value = content_disposition("text/html", "x.html", false);
    assert!(value.to_str().unwrap().starts_with("attachment;"));
    // force_download 对白名单类型同样生效
    let forced = content_disposition("image/png", "x.png", true);
    assert!(forced.to_str().unwrap().starts_with("attachment;"));
}

#[test]
fn mime_normalization_falls_back() {
    assert_eq!(normalize_mime(""), HeaderValue::from_static(DEFAULT_MIME));
    assert_eq!(
        normalize_mime("   "),
        HeaderValue::from_static(DEFAULT_MIME)
    );
    // 换行是非法 header 值字符，必须兜底而非 panic
    assert_eq!(
        normalize_mime("image/png\nx"),
        HeaderValue::from_static(DEFAULT_MIME)
    );
    assert_eq!(
        normalize_mime("image/png"),
        HeaderValue::from_static("image/png")
    );
}

#[tokio::test]
async fn serves_full_body_without_range() {
    let dir = std::env::temp_dir().join(format!("ai_orz_file_serve_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("a.txt");
    std::fs::write(&path, b"hello world").unwrap();

    let resp = serve_file(&path, None, opts("text/plain", "a.txt"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers()[header::CONTENT_LENGTH], "11");
    assert_eq!(resp.headers()[header::ACCEPT_RANGES], "bytes");
    assert_eq!(
        resp.headers()[HeaderName::from_static("x-content-type-options")],
        "nosniff"
    );
    let body = to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"hello world");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn serves_partial_content_for_range() {
    let dir = std::env::temp_dir().join(format!("ai_orz_file_range_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("b.bin");
    std::fs::write(&path, b"0123456789").unwrap();

    let resp = serve_file(
        &path,
        Some("bytes=2-5"),
        opts("application/octet-stream", "b.bin"),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(resp.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
    assert_eq!(resp.headers()[header::CONTENT_LENGTH], "4");
    let body = to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"2345");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn returns_416_for_unsatisfiable_range() {
    let dir = std::env::temp_dir().join(format!("ai_orz_file_416_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("c.bin");
    std::fs::write(&path, b"abc").unwrap();

    let resp = serve_file(
        &path,
        Some("bytes=10-"),
        opts("application/octet-stream", "c.bin"),
    )
    .await
    .unwrap();
    assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(resp.headers()[header::CONTENT_RANGE], "bytes */3");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn missing_file_reports_not_found() {
    let path = std::env::temp_dir().join("ai_orz_definitely_missing_file.bin");
    let err = serve_file(&path, None, opts("text/plain", "x.txt"))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "not_found");
}
