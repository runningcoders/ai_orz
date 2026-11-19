//! 出站附件的媒体分类与载荷（渠道 DTO，跨层共用）
//!
//! 使用链：消息域从正文 `attachment:` 引用解析出载荷 → 消息渠道 DAL 分派 →
//! 各家 DAO（飞书 / 微信）映射成自己的原生消息类型。
//!
//! ## 为什么在 models
//!
//! DAL 定义它、DAO 消费它，放 DAL 里会让 `dao → dal` 成立（下层反向依赖上层）。
//! 它是纯数据 + 纯 MIME 判定，没有任何 IO，属于「共享实体定义」，位置只能是
//! 依赖链最底端的 models。
//!
//! ## 判定口径
//!
//! [`OutboundMediaKind::from_mime`] 是**全链路唯一判定源**：消息域用它决定大小
//! 上限，两家 DAO 用它把附件映射到各自渠道的原生消息类型。新增媒体类型时只改
//! 这里 + 各家 DAO 的分派映射，不要散落修改判定逻辑。

/// 出站附件的媒体分类（由 MIME 归一化判定）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutboundMediaKind {
    /// 图片（渠道原生图片消息）：JPEG / PNG / GIF / WebP / BMP
    Image,
    /// 视频（渠道原生视频 / 媒体消息）：`video/*`
    Video,
    /// 音频：协议要求转码（微信 silk / 飞书 opus），当前按文件发送保底
    Audio,
    /// 其余（文档 / 压缩包 / 未知二进制）：渠道原生文件消息
    File,
}

impl OutboundMediaKind {
    /// 由 MIME 判定媒体类型（大小写不敏感，忽略 `; charset=` 等参数）。
    ///
    /// ⚠️ 只判 MIME，不看文件名：真正决定渠道载荷的入口是
    /// [`Self::from_mime_and_name`]，本函数保留给「只有 MIME」的上限计算等场景。
    pub fn from_mime(mime: &str) -> Self {
        let base = mime
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        match base.as_str() {
            "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/bmp" => Self::Image,
            m if m.starts_with("video/") => Self::Video,
            m if m.starts_with("audio/") => Self::Audio,
            _ => Self::File,
        }
    }

    /// 出站分派的**唯一判定入口**：MIME + 文件名一起看。
    ///
    /// 除图片白名单外还有一条「无转码能力就别装原生消息」的约束：
    /// - 视频只有 `video/mp4` 或 `.mp4` 文件名才走渠道原生视频消息；
    ///   飞书 `im/v1/files` 只存字节并打 `file_type=mp4` 标记，不做转码，
    ///   mov / mkv / webm / avi 发成原生视频会让对方点开无法播放 → 落 `File`
    ///   走可下载附件保底（与音频「无转码能力按文件发送」口径一致）。
    /// - SVG / HEIC / TIFF 等平台多半拒收的图片同理落 `File`。
    pub fn from_mime_and_name(mime: &str, name: &str) -> Self {
        let kind = Self::from_mime(mime);
        if kind == Self::Video {
            let mime_is_mp4 = mime
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case("video/mp4");
            // 有扩展名时以它为准（更贴近真实容器）；无扩展名才退回 MIME
            let playable = match name.rsplit_once('.') {
                Some((_, ext)) => ext.eq_ignore_ascii_case("mp4"),
                None => mime_is_mp4,
            };
            if !playable {
                return Self::File;
            }
        }
        kind
    }

    /// 该类型的出站单文件上限（字节，取两家渠道的保守下限）
    pub fn max_bytes(self) -> usize {
        match self {
            // 飞书 `image_type=message` 上限 10MB
            Self::Image => 10 * 1024 * 1024,
            // 视频经飞书 im/v1/files 上传（file_type=mp4），与文档同为 30MB 上限
            Self::Video => 30 * 1024 * 1024,
            // 飞书 `im/v1/files` 文档 30MB；audio 按文件发送同口径
            Self::Audio | Self::File => 30 * 1024 * 1024,
        }
    }
}

/// 出站附件载荷（消息域从正文附件引用解析，DAO 层负责渠道发送）
///
/// 文本降级占位仍留在正文里：附件发送失败时信息不丢。
///
/// 渠道按 [`OutboundMediaKind`] 分派：图片 / 视频走渠道原生消息，其余走文件消息。
#[derive(Clone, Debug)]
pub struct OutboundAttachment {
    /// 附件原名（渠道文件名用）
    pub name: String,
    /// 归一化 MIME（`image/png` / `application/pdf` …）
    pub mime_type: String,
    /// 文件明文字节
    pub bytes: Vec<u8>,
}

impl OutboundAttachment {
    /// 由 MIME + 文件名判定的媒体类型（全链路统一判定源；视频按可播放性降级）
    pub fn kind(&self) -> OutboundMediaKind {
        OutboundMediaKind::from_mime_and_name(&self.mime_type, &self.name)
    }

    /// 是否走渠道图片消息（等价 `kind() == Image`，保留便于调用点兼容）
    ///
    /// 按 RFC 大小写不敏感；SVG / HEIC / TIFF 等平台多半拒收，归一为文件通道
    /// （仍以可下载附件送达，正文文本占位也在，信息不丢）。
    pub fn is_image(&self) -> bool {
        self.kind() == OutboundMediaKind::Image
    }
}

#[cfg(test)]
mod tests {
    use super::{OutboundAttachment, OutboundMediaKind};

    #[test]
    fn from_mime_classifies_case_insensitively_and_ignores_params() {
        let kind = OutboundMediaKind::from_mime;
        assert_eq!(kind("image/png"), OutboundMediaKind::Image);
        assert_eq!(kind("IMAGE/JPEG"), OutboundMediaKind::Image);
        assert_eq!(kind("image/webp; charset=binary"), OutboundMediaKind::Image);
        assert_eq!(kind("video/mp4"), OutboundMediaKind::Video);
        assert_eq!(kind("VIDEO/QUICKTIME"), OutboundMediaKind::Video);
        assert_eq!(kind("video/x-matroska"), OutboundMediaKind::Video);
        assert_eq!(kind("audio/mpeg"), OutboundMediaKind::Audio);
        assert_eq!(kind("AUDIO/WAV"), OutboundMediaKind::Audio);
        assert_eq!(kind("application/pdf"), OutboundMediaKind::File);
        // 平台多半拒收的图片格式落回文件通道
        assert_eq!(kind("image/svg+xml"), OutboundMediaKind::File);
        assert_eq!(kind("image/heic"), OutboundMediaKind::File);
        assert_eq!(kind("application/octet-stream"), OutboundMediaKind::File);
    }

    #[test]
    fn unmapped_video_falls_back_to_file_without_transcoding() {
        let pick = OutboundMediaKind::from_mime_and_name;
        // 可播放的 mp4 保持原生通道（MIME 或文件名任一明确即可）
        assert_eq!(pick("video/mp4", "clip"), OutboundMediaKind::Video);
        assert_eq!(pick("video/mp4", "clip.mp4"), OutboundMediaKind::Video);
        assert_eq!(
            pick("application/octet-stream", "CLIP.MP4"),
            OutboundMediaKind::File
        );
        // 其余视频格式无转码能力，按文件发送保底
        assert_eq!(pick("video/quicktime", "clip.mov"), OutboundMediaKind::File);
        assert_eq!(
            pick("video/x-matroska", "clip.mkv"),
            OutboundMediaKind::File
        );
        assert_eq!(pick("video/webm", "clip.webm"), OutboundMediaKind::File);
        assert_eq!(pick("video/mp4", "clip.mov"), OutboundMediaKind::File);
        // 非视频不受文件名影响
        assert_eq!(pick("image/png", "clip.mp4"), OutboundMediaKind::Image);
    }

    #[test]
    fn attachment_kind_and_is_image_share_one_source() {
        let make = |mime: &str| OutboundAttachment {
            name: "x".into(),
            mime_type: mime.to_string(),
            bytes: vec![],
        };
        assert!(make("image/png").is_image());
        assert_eq!(make("video/mp4").kind(), OutboundMediaKind::Video);
        assert!(!make("application/pdf").is_image());
    }

    #[test]
    fn max_bytes_takes_conservative_channel_limit() {
        let mb = 1024 * 1024;
        assert_eq!(OutboundMediaKind::Image.max_bytes(), 10 * mb);
        assert_eq!(OutboundMediaKind::Video.max_bytes(), 30 * mb);
        assert_eq!(OutboundMediaKind::Audio.max_bytes(), 30 * mb);
        assert_eq!(OutboundMediaKind::File.max_bytes(), 30 * mb);
    }
}
