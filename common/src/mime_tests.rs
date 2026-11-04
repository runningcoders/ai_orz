use super::{OCTET_STREAM, infer_mime_type, is_generic, normalize, normalize_or_infer};

#[test]
fn infer_common_types() {
    assert_eq!(infer_mime_type("cat.png"), "image/png");
    assert_eq!(infer_mime_type("cat.JPG"), "image/jpeg");
    assert_eq!(infer_mime_type("设计稿.webp"), "image/webp");
    assert_eq!(infer_mime_type("report.pdf"), "application/pdf");
    assert_eq!(infer_mime_type("notes.md"), "text/markdown");
    assert_eq!(infer_mime_type("data.csv"), "text/csv");
    assert_eq!(infer_mime_type("clip.mp4"), "video/mp4");
    assert_eq!(infer_mime_type("voice.m4a"), "audio/mp4");
}

#[test]
fn invalid_names_fall_back_to_octet_stream() {
    assert_eq!(infer_mime_type("noext"), OCTET_STREAM);
    assert_eq!(infer_mime_type("unknown.xyz"), OCTET_STREAM);
    // 点开头的点文件没有后缀；结尾是点也不算
    assert_eq!(infer_mime_type(".bashrc"), OCTET_STREAM);
    assert_eq!(infer_mime_type("trailing."), OCTET_STREAM);
    assert_eq!(infer_mime_type(""), OCTET_STREAM);
}

#[test]
fn only_last_segment_counts() {
    // 带目录的路径只看最后一段：目录名里的点不能骗过推断
    assert_eq!(infer_mime_type("a.b/c"), OCTET_STREAM);
    assert_eq!(infer_mime_type("dir.png/cat"), OCTET_STREAM);
    assert_eq!(infer_mime_type("/tmp/x/cat.png"), "image/png");
    assert_eq!(infer_mime_type("C:\\tmp\\cat.png"), "image/png");
}

#[test]
fn generic_detection_ignores_parameters_and_case() {
    assert!(is_generic(""));
    assert!(is_generic("   "));
    assert!(is_generic("application/octet-stream"));
    assert!(is_generic("Application/Octet-Stream; charset=binary"));
    assert!(!is_generic("image/png"));
    assert!(!is_generic("image/png; charset=binary"));
}

#[test]
fn normalize_strips_parameters() {
    assert_eq!(normalize("image/png; charset=binary"), "image/png");
    assert_eq!(normalize("  text/plain  "), "text/plain");
}

#[test]
fn generic_mime_is_upgraded_from_extension() {
    // 本次 bug 的核心场景：前端 / 渠道把类型写成泛型，必须靠后缀救回来
    assert_eq!(
        normalize_or_infer("application/octet-stream", "cat.png"),
        "image/png"
    );
    assert_eq!(normalize_or_infer("", "cat.png"), "image/png");
    assert_eq!(
        normalize_or_infer("application/octet-stream", "noext"),
        OCTET_STREAM
    );
}

#[test]
fn real_mime_is_respected() {
    // 调用方给了具体类型就照用：后缀可被伪造，客户端判定可能更准
    assert_eq!(normalize_or_infer("image/png", "cat.bin"), "image/png");
    assert_eq!(
        normalize_or_infer("application/x-custom", "cat.png"),
        "application/x-custom"
    );
}
