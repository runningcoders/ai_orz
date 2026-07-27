//! #[derive(StatsEvent)] 过程宏实现
//!
//! 扫描结构体中的 `#[timestamp]`、`#[tag]`、`#[metric]` 标注的字段，
//! 自动生成 `StatEvent` trait 实现。
//!
//! 结构体级注解：
//! - `#[event_type = "xxx"]` 自定义事件类型名称，默认使用 type_name
//!
//! 字段级注解：
//! - `#[timestamp]` - 时间戳字段（必须，i64）
//! - `#[tag]` - 标签字段（String / `，可多选）
//! - `#[metric]` - 指标字段（数值/字符串，可选多选）
//!
//! 类型处理：
//! - `Option<String>` tag - 为 None 时跳过
//! - `String` tag - 直接插入
//! - 数值 metric - 转成 JSON Number

use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, Ident, Lit, Meta, parse_macro_input};

/// 字段标记类型
#[derive(Debug, Clone, Copy)]
enum FieldKind {
    Timestamp,
    Tag,
    Metric,
    None,
}

/// 判断字段类型是否是 `Option<String>`
fn is_option_string(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty
        && let Some(seg) = type_path.path.segments.last()
    {
        return seg.ident == "Option";
    }
    false
}

/// 判断字段类型是否是 `String`
fn is_string(ty: &syn::Type) -> bool {
    if let syn::Type::Path(type_path) = ty
        && let Some(seg) = type_path.path.segments.last()
    {
        return seg.ident == "String";
    }
    false
}

/// 实际实现逻辑
pub fn derive_stats_event(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = input.ident;

    // 解析结构体级别的 event_type 注解
    let mut event_type: Option<String> = None;
    for attr in &input.attrs {
        if attr.path().is_ident("event_type")
            && let Meta::NameValue(meta) = &attr.meta
            && let syn::Expr::Lit(expr_lit) = &meta.value
            && let Lit::Str(lit_str) = &expr_lit.lit
        {
            event_type = Some(lit_str.value());
        }
    }

    // 遍历所有字段，找出标记
    let mut timestamp_field: Option<Ident> = None;
    let mut tag_fields: Vec<(Ident, syn::Type)> = Vec::new();
    let mut metric_fields: Vec<(Ident, syn::Type)> = Vec::new();

    if let syn::Data::Struct(s) = input.data {
        for field in &s.fields {
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
                None => continue,
            };
            match kind {
                FieldKind::Timestamp => {
                    if timestamp_field.is_some() {
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
                    tag_fields.push((ident, field.ty.clone()));
                }
                FieldKind::Metric => {
                    metric_fields.push((ident, field.ty.clone()));
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

    // event_type 方法
    let event_type_fn = if let Some(ev_type) = event_type {
        quote! {
            fn event_type(&self) -> &str {
                #ev_type
            }
        }
    } else {
        quote! {}
    };

    // 生成 tags_json 方法
    let tags_json = if !tag_fields.is_empty() {
        let entries = tag_fields.iter().map(|(ident, ty)| {
            let field_name = ident.to_string();
            if is_option_string(ty) {
                // Option<String>：Some 时插入，None 时跳过
                quote! {
                    if let Some(v) = &self.#ident {
                        map.insert(#field_name.into(), serde_json::Value::String(v.clone()));
                    }
                }
            } else if is_string(ty) {
                // String：直接插入
                quote! {
                    map.insert(#field_name.into(), serde_json::Value::String(self.#ident.clone()));
                }
            } else {
                // 其他类型：用 into() 转换
                quote! {
                    map.insert(#field_name.into(), self.#ident.clone().into());
                }
            }
        });
        quote! {
            fn tags_json(&self) -> Option<serde_json::Value> {
                let mut map = serde_json::Map::new();
                #(#entries)*
                if !map.is_empty() {
                    Some(serde_json::Value::Object(map))
                } else {
                    None
                }
            }
        }
    } else {
        quote! {}
    };

    // 生成 metrics_json 方法
    let metrics_json = if !metric_fields.is_empty() {
        let entries = metric_fields.iter().map(|(ident, ty)| {
            let field_name = ident.to_string();
            if is_string(ty) {
                // String 类型的 metric
                quote! {
                    map.insert(#field_name.into(), serde_json::Value::String(self.#ident.clone()));
                }
            } else {
                // 数值等类型：直接转
                quote! {
                    map.insert(#field_name.into(), self.#ident.into());
                }
            }
        });
        quote! {
            fn metrics_json(&self) -> Option<serde_json::Value> {
                let mut map = serde_json::Map::new();
                #(#entries)*
                Some(serde_json::Value::Object(map))
            }
        }
    } else {
        quote! {}
    };

    // 生成 impl
    let expanded = quote! {
        impl crate::pkg::stats::StatEvent for #name {
            fn timestamp(&self) -> i64 {
                self.#timestamp_ident
            }

            #event_type_fn
            #tags_json
            #metrics_json
        }
    };

    expanded.into()
}
