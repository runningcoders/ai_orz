//! MIME type inference utility for artifact file registration.
//!
//! 「后缀 → MIME」的推断表已下沉到 [`common::mime`]（前端上传 Blob 也要用同一份，
//! 两处各写一份必然漂移）。本模块只保留 artifact 侧独有的 `FileType` 归类与路径工具。

use common::enums::FileType;

/// Infer FileType from MIME type.
///
/// Returns `FileType::Document` for unknown MIME types.
pub fn infer_file_type(mime_type: &str) -> FileType {
    if mime_type.starts_with("image/") {
        FileType::Image
    } else if mime_type.starts_with("video/") {
        FileType::Video
    } else if mime_type.starts_with("audio/") {
        FileType::Audio
    } else if mime_type == "application/zip"
        || mime_type == "application/gzip"
        || mime_type == "application/x-tar"
    {
        FileType::Binary
    } else {
        FileType::Document
    }
}

/// Extract the basename (file name without directory) from a path.
pub fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_file_type() {
        assert_eq!(infer_file_type("image/png"), FileType::Image);
        assert_eq!(infer_file_type("video/mp4"), FileType::Video);
        assert_eq!(infer_file_type("audio/mpeg"), FileType::Audio);
        assert_eq!(infer_file_type("application/zip"), FileType::Binary);
        assert_eq!(infer_file_type("text/plain"), FileType::Document);
        assert_eq!(
            infer_file_type("application/octet-stream"),
            FileType::Document
        );
    }

    #[test]
    fn test_basename() {
        assert_eq!(basename("output/data.csv"), "data.csv");
        assert_eq!(basename("data.csv"), "data.csv");
        assert_eq!(basename("a/b/c/report.md"), "report.md");
        assert_eq!(basename("dir\\file.txt"), "file.txt");
    }
}
