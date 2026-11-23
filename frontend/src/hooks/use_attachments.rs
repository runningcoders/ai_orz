//! 附件上传状态机（对话页与工作台 dock 共用）
//!
//! 三路入口（回形针选择 / 粘贴 / 拖放）统一收敛到 [`AttachmentComposer::on_files`]，
//! 上传完成后把 `(id, name)` 放入 `pending`；发送时调用方用
//! [`compose_outbound_content`] 把附件按协议写进正文，并把 id 列表随请求带上。
//!
//! 队列化：连续粘贴 / 拖放时，上传中的后来文件先入队，由单个 worker 串行消费，
//! 不因「正在上传」而静默丢弃。

use dioxus::prelude::*;

use crate::api::finance::upload_attachment;
use crate::store::toast::ToastState;

/// 待发送的附件（已上传到服务器，仅保留展示所需字段）
#[derive(Debug, Clone, PartialEq)]
pub struct PendingAttachment {
    /// 附件 ID
    pub id: String,
    /// 文件名（仅展示）
    pub name: String,
}

/// 把已上传附件按协议写进请求正文（与渠道入站共用 common 拼装口）
pub fn compose_outbound_content(base: &str, attachments: &[PendingAttachment]) -> String {
    let refs: Vec<String> = attachments
        .iter()
        .map(|a| {
            let name = a.name.trim();
            let name = if name.is_empty() { "附件" } else { name };
            common::markdown_protocol::format_ref(
                &common::markdown_protocol::ProtocolRef::attachment(&a.id),
                name,
            )
        })
        .collect();
    common::markdown_protocol::append_refs(base, &refs)
}

/// 附件输入三件套 + 上传入口
#[derive(Clone, Copy)]
pub struct AttachmentComposer {
    /// 已上传、待随消息发送的附件
    pub pending: Signal<Vec<PendingAttachment>>,
    /// 是否有文件正在上传（回形针显示 Loading）
    pub uploading: Signal<bool>,
    /// 拖放悬停高亮
    pub drag_hover: Signal<bool>,
    /// 统一的文件入口（回形针 / 粘贴 / 拖放都调它）
    pub on_files: Callback<Vec<dioxus::html::FileData>>,
}

/// 初始化附件上传状态机
pub fn use_attachment_composer(toast: ToastState) -> AttachmentComposer {
    let mut pending = use_signal(Vec::<PendingAttachment>::new);
    let mut uploading = use_signal(|| false);
    let drag_hover = use_signal(|| false);
    // 上传中的后来文件先入队，由单个 worker 串行消费
    let mut upload_queue = use_signal(Vec::<dioxus::html::FileData>::new);

    let on_files = Callback::new(move |files: Vec<dioxus::html::FileData>| {
        if files.is_empty() {
            return;
        }
        // 先入队，无论当前是否正在上传 —— 这是「不丢文件」的关键
        upload_queue.write().extend(files);
        if uploading() {
            return;
        }
        uploading.set(true);
        spawn(async move {
            loop {
                let batch: Vec<dioxus::html::FileData> = upload_queue.write().drain(..).collect();
                if batch.is_empty() {
                    break;
                }
                for fd in batch {
                    let file_name = fd.name();
                    let bytes = match fd.read_bytes().await {
                        Ok(b) => b,
                        Err(e) => {
                            // 剪贴板里的占位 File 常读取即失败，引导换入口，不静默
                            toast.error(format!(
                                "读取文件「{}」失败（{}）：请改用拖拽文件到输入框，或点回形针选择文件",
                                file_name, e
                            ));
                            continue;
                        }
                    };
                    if bytes.is_empty() {
                        toast.error(format!(
                            "文件「{}」内容为空：请改用拖拽文件到输入框，或点回形针选择文件",
                            file_name
                        ));
                        continue;
                    }

                    // Blob 的 type 会成为 multipart part 的 Content-Type，
                    // 必须按后缀给真实类型，否则图片无法内联展示
                    let blob_parts: js_sys::Array = js_sys::Array::new();
                    let uint8_array = js_sys::Uint8Array::new_with_length(bytes.len() as u32);
                    uint8_array.copy_from(&bytes);
                    blob_parts.push(&uint8_array);
                    let blob_bag = web_sys::BlobPropertyBag::new();
                    blob_bag.set_type(common::mime::infer_mime_type(&file_name));
                    let blob =
                        web_sys::Blob::new_with_str_sequence_and_options(&blob_parts, &blob_bag)
                            .ok();

                    let form = match web_sys::FormData::new() {
                        Ok(f) => f,
                        Err(_) => {
                            toast.error(format!("无法初始化上传表单: {}", file_name));
                            continue;
                        }
                    };
                    let _ = form.append_with_str("purpose", "message");
                    if let Some(blob) = blob {
                        let _ = form.append_with_blob_and_filename("file", &blob, &file_name);
                    }

                    match upload_attachment(form).await {
                        Ok(detail) => {
                            pending.write().push(PendingAttachment {
                                id: detail.id,
                                name: detail.original_name,
                            });
                        }
                        Err(e) => {
                            toast.error(format!("上传文件 {} 失败: {}", file_name, e));
                        }
                    }
                }
            }
            uploading.set(false);
        });
    });

    AttachmentComposer {
        pending,
        uploading,
        drag_hover,
        on_files,
    }
}

/// 从粘贴事件里取文件（拿不到原生事件 / 无文件时返回空）
pub fn clipboard_files_from_paste(
    e: &dioxus::events::ClipboardEvent,
) -> Vec<dioxus::html::FileData> {
    use dioxus::web::WebEventExt;
    use wasm_bindgen::JsCast;

    let Some(raw) = e.try_as_web_event() else {
        return Vec::new();
    };
    let Ok(clipboard) = raw.dyn_into::<web_sys::ClipboardEvent>() else {
        return Vec::new();
    };
    let Some(data) = clipboard.clipboard_data() else {
        return Vec::new();
    };
    let Some(list) = data.files() else {
        return Vec::new();
    };

    let mut files = Vec::new();
    for i in 0..list.length() {
        if let Some(file) = list.item(i)
            && let Ok(reader) = web_sys::FileReader::new()
        {
            files.push(dioxus::html::FileData::new(dioxus::web::WebFileData::new(
                file, reader,
            )));
        }
    }
    files
}
