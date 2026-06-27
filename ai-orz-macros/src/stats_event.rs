//! #[derive(StatsEvent)] 过程宏实现

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, DeriveInput, Ident};

/// 字段标记类型
#[derive(Debug, Clone, Copy)]
enum FieldKind {
    Timestamp,
    Tag,
    Metric,
    None,
}

/// 实际实现逻辑
pub fn derive_stats_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    // 遍历所有字段，找出标记
    let mut timestamp_field: Option<Ident> = None;
    let mut tag_fields: Vec<Ident> = Vec::new();
    let mut metric_fields: Vec<Ident> = Vec::new();

    if let syn::Data::Struct(s) = input.data {
        for field in s.fields {
            let mut kind = FieldKind::None;
            for attr in &field.attrs {
                if attr.path().is_ident("timestamp") {
                    kind = FieldKind::Timestamp;
                } else if attr.path().is_ident("tag") {
                    kind = FieldKind::Tag;
                } else if attr.path().is_ident("metric") {
                    kind = FieldKind::Metric;
                }
            }
            let ident = match &field.ident {
                Some(ident) => ident.clone(),
                None => continue, // 元组结构体不处理
            };
            match kind {
                FieldKind::Timestamp => {
                    if timestamp_field.is_some() {
                        // 多个 timestamp 标记，编译错误
                        return syn::Error::new(
                            ident.span(),
                            "only one field can be marked #[timestamp]",
                        )
                        .to_compile_error()
                        .into();
                    }
                    timestamp_field = Some(ident);
                }
                FieldKind::Tag => {
                    tag_fields.push(ident);
                }
                FieldKind::Metric => {
                    metric_fields.push(ident);
                }
                FieldKind::None => {}
            }
        }
    }

    // 必须有 timestamp 字段
    let timestamp_ident = match timestamp_field {
        Some(ident) => ident,
        None => {
            return syn::Error::new(
                name.span(),
                "StatsEvent requires exactly one field marked #[timestamp]",
            )
            .to_compile_error()
            .into();
        }
    };

    // 生成 tags_json 方法
    let tags_json = if !tag_fields.is_empty() {
        let entries = tag_fields.iter().map(|f| {
            quote! {
                (#f.to_string(), self.#f.clone().into())
            }
        });
        quote! {
            Some(serde_json::json!({
                #(#entries),*
            }))
        }
    } else {
        quote! { None }
    };

    // 生成 metrics_json 方法
    let metrics_json = if !metric_fields.is_empty() {
        let entries = metric_fields.iter().map(|f| {
            quote! {
                (#f.to_string(), self.#f.clone().into())
            }
        });
        quote! {
            Some(serde_json::json!({
                #(#entries),*
            }))
        }
    } else {
        quote! { None }
    };

    // 生成 impl
    let expanded = quote! {
        impl crate::pkg::stats::StatEvent for #name {
            fn timestamp(&self) -> i64 {
                self.#timestamp_ident
            }

            fn tags_json(&self) -> Option<serde_json::Value> {
                #tags_json
            }

            fn metrics_json(&self) -> Option<serde_json::Value> {
                #metrics_json
            }
        }
    };

    expanded.into()
}
