//! Artifact metadata editing modal

use dioxus::prelude::*;
use common::api::ArtifactDetail;

#[derive(Props, Clone, PartialEq)]
pub struct ArtifactMetaModalProps {
    pub artifact: ArtifactDetail,
    pub show: bool,
    pub on_save: EventHandler<(Option<String>, Option<String>, Option<Vec<String>>)>,
    pub on_close: EventHandler<()>,
}

#[component]
pub fn ArtifactMetaModal(props: ArtifactMetaModalProps) -> Element {
    let mut name = use_signal(|| props.artifact.name.clone());
    let mut description = use_signal(|| props.artifact.description.clone());
    let mut tags_text = use_signal(|| props.artifact.tags.join(", "));

    // Clone artifact into a local to avoid partial moves of `props.artifact.*`
    // into use_effect (which would make them unavailable for the onclick closure).
    let artifact = props.artifact.clone();
    use_effect(move || {
        name.set(artifact.name.clone());
        description.set(artifact.description.clone());
        tags_text.set(artifact.tags.join(", "));
    });

    if !props.show {
        return rsx! {};
    }

    let saved = props.artifact.clone();

    rsx! {
        div {
            class: "modal modal-open",
            onclick: move |_| props.on_close.call(()),
            div {
                class: "modal-box",
                onclick: move |e| e.stop_propagation(),
                h3 { class: "font-bold text-lg mb-4", "编辑产物信息" }
                div { class: "form-control mb-3",
                    label { class: "label", span { class: "label-text", "名称" } }
                    input {
                        class: "input input-bordered w-full",
                        value: name(),
                        oninput: move |e| name.set(e.value()),
                    }
                }
                div { class: "form-control mb-3",
                    label { class: "label", span { class: "label-text", "描述" } }
                    textarea {
                        class: "textarea textarea-bordered w-full",
                        rows: 3,
                        value: description(),
                        oninput: move |e| description.set(e.value()),
                    }
                }
                div { class: "form-control mb-4",
                    label { class: "label", span { class: "label-text", "标签（逗号分隔）" } }
                    input {
                        class: "input input-bordered w-full",
                        value: tags_text(),
                        oninput: move |e| tags_text.set(e.value()),
                    }
                }
                div { class: "modal-action",
                    button {
                        class: "btn btn-ghost",
                        onclick: move |_| props.on_close.call(()),
                        "取消"
                    }
                    button {
                        class: "btn btn-primary",
                        onclick: move |_| {
                            let n = if name() != saved.name { Some(name()) } else { None };
                            let d = if description() != saved.description { Some(description()) } else { None };
                            let tags: Vec<String> = tags_text()
                                .split(',')
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                            let t = if tags != saved.tags { Some(tags) } else { None };
                            props.on_save.call((n, d, t));
                        },
                        "保存"
                    }
                }
            }
        }
    }
}
