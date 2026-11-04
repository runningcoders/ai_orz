//! Handler: GET /api/v1/project/artifacts/{id}/file - 读取 Artifact 原始二进制流
//!
//! 与 `/artifacts/{id}/content` 分工不同：
//! - `/content` 返回 `GetArtifactContentResponse`（metadata + UTF-8 文本），供详情页
//!   编辑器加载/保存。**不是**字节流：非文本内容直接 400，`<img>` 用不了。
//! - `/file` 返回**原始字节流** + 真实 `Content-Type`，供 `<img> / <video> / <audio> /
//!   <iframe> / <a>` 直接渲染或下载，覆盖文档、图片、音视频等所有文件类型；支持
//!   `Range`（视频/音频可拖动进度条）。
//!
//! 响应构造（流式读取、Range、`inline`/`attachment` 安全判定、`nosniff`）全部交给
//! [`crate::pkg::http::file::serve_file`]，本 handler 只负责「鉴权 + 定位文件」。
//!
//! 不注册为 `neural` 工具：返回原始字节不适合作为 LLM 工具结果；Agent 读文本请用
//! `get_artifact_content`，读元信息请用 `get_artifact`。

use axum::{
    extract::{Extension, Path},
    http::{HeaderMap, header},
    response::Response,
};
use common::error::{Result, bail_err, err};

use crate::pkg::RequestContext;
use crate::pkg::http::file::{FileServeOptions, serve_file};
use crate::service::domain::project;

/// 读取 Artifact 原始字节流（任意文件类型，支持 Range）。
pub async fn get_artifact_file(
    Extension(ctx): Extension<RequestContext>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<Response> {
    if ctx.uid().is_empty() {
        bail_err!(InvalidRequest, "当前请求缺少用户上下文");
    }

    // 复用领域读侧：内部已按项目归属校验访问权限，无权访问一律 None（不泄露存在性）。
    let Some(artifact) = project::domain().artifact_manage().get(ctx, &id).await? else {
        return Err(err!(NotFound, "Artifact {} not found", id));
    };

    // 按来源类型定位落点：GeneratedContent 在产物目录，Attachment 型指向附件存储区。
    // 具体布局只在模型层维护（见 `ArtifactPo::resolve_file_abs_path`）。
    let abs_path = artifact.resolve_file_abs_path(&crate::config::get().base_data_path())?;

    // 下载文件名优先取存储路径的 basename（更接近原始文件名），拿不到再退显示名。
    // 这里用全限定路径：`axum::extract::Path` 已占用 `Path` 这个名字。
    let stored_name = std::path::Path::new(&artifact.po.file_meta.0.file_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&artifact.po.name);

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());

    serve_file(
        &abs_path,
        range,
        FileServeOptions {
            mime_type: &artifact.po.file_meta.0.mime_type,
            file_name: stored_name,
            // 私有资产：允许浏览器短期缓存以减少重复拉取，但禁止共享缓存留存。
            cache_control: Some("private, max-age=3600"),
            force_download: false,
        },
    )
    .await
}
