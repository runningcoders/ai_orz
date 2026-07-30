//! MIME type inference utility for artifact file registration.

use common::enums::FileType;

/// Infer MIME type from file extension.
///
/// Returns "application/octet-stream" for unknown extensions.
pub fn infer_mime_type(file_name: &str) -> String {
    let ext = file_name
        .rsplit('.')
        .next()
        .filter(|ext| ext.len() < file_name.len()) // Avoid treating "file" as extension
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "txt" => "text/plain".to_string(),
        "md" => "text/markdown".to_string(),
        "html" | "htm" => "text/html".to_string(),
        "css" => "text/css".to_string(),
        "js" => "application/javascript".to_string(),
        "json" => "application/json".to_string(),
        "xml" => "application/xml".to_string(),
        "csv" => "text/csv".to_string(),
        "tsv" => "text/tab-separated-values".to_string(),
        "yaml" | "yml" => "application/x-yaml".to_string(),
        "toml" => "application/toml".to_string(),
        "pdf" => "application/pdf".to_string(),
        "zip" => "application/zip".to_string(),
        "gz" | "gzip" => "application/gzip".to_string(),
        "tar" => "application/x-tar".to_string(),
        "png" => "image/png".to_string(),
        "jpg" | "jpeg" => "image/jpeg".to_string(),
        "gif" => "image/gif".to_string(),
        "svg" => "image/svg+xml".to_string(),
        "webp" => "image/webp".to_string(),
        "mp3" => "audio/mpeg".to_string(),
        "mp4" => "video/mp4".to_string(),
        "wav" => "audio/wav".to_string(),
        "py" => "text/x-python".to_string(),
        "rs" => "text/x-rust".to_string(),
        "go" => "text/x-go".to_string(),
        "java" => "text/x-java".to_string(),
        "c" | "h" => "text/x-c".to_string(),
        "cpp" | "hpp" | "cc" => "text/x-c++".to_string(),
        "sh" | "bash" => "application/x-sh".to_string(),
        "sql" => "application/sql".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

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
    fn test_infer_mime_type() {
        assert_eq!(infer_mime_type("report.md"), "text/markdown");
        assert_eq!(infer_mime_type("data.csv"), "text/csv");
        assert_eq!(infer_mime_type("image.png"), "image/png");
        assert_eq!(infer_mime_type("unknown.xyz"), "application/octet-stream");
        assert_eq!(infer_mime_type("noext"), "application/octet-stream");
    }

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
