use proc_macro::TokenStream;
use quote::quote;
use syn::parse_macro_input;
use syn::{Ident, ItemFn, Lit, LitStr, Meta, MetaNameValue, Type};

mod stats_event;
use stats_event::derive_stats_event as stats_event_derive;

mod log_fields;
use log_fields::derive_log_fields as log_fields_derive;

/// Derive macro to mark HTTP params and their source locations.
/// This is used together with #[generate_http_handler] to automatically
/// collect which fields come from path/query/body.
#[proc_macro_derive(Params, attributes(param))]
pub fn derive_params(_input: TokenStream) -> TokenStream {
    // This derive doesn't generate any code, it just holds the #[param] attributes
    // for #[generate_http_handler] to read.
    TokenStream::new()
}

/// Derive macro to automatically implement StatEvent trait.
///
/// # Usage
/// ```rust,ignore
/// use ai_orz_macros::StatsEvent;
///
/// #[derive(Debug, Clone, StatsEvent)]
/// pub struct ModelCallEvent {
///     #[timestamp]
///     pub timestamp: i64,
///     #[tag]
///     pub model_provider_id: String,
///     #[tag]
///     pub agent_id: Option<String>,
///     #[metric]
///     pub tokens_input: i64,
///     #[metric]
///     pub tokens_output: i64,
/// }
/// ```
#[proc_macro_derive(StatsEvent, attributes(timestamp, tag, metric, event_type))]
pub fn derive_stats_event(input: TokenStream) -> TokenStream {
    stats_event_derive(input)
}

/// Register a handler function as a built-in tool
///
/// # Usage
/// ```rust,ignore
/// use ai_orz_macros::register_handler_tool;
/// use serde_json::Value;
///
/// async fn handler(ctx: (), params: ()) -> ::std::result::Result<Value, ()> {
///     Ok(Value::Null)
/// }
///
/// #[register_handler_tool(
///     id = "test_tool",
///     name = "test_tool",
///     description = "Test tool",
///     params = "()",
/// )]
/// async fn handler(ctx: (), params: ()) -> ::std::result::Result<Value, ()> {
///     Ok(Value::Null)
/// }
/// ```
#[proc_macro_attribute]
pub fn register_handler_tool(args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(input as syn::ItemFn);

    // Parse with a parser that allows trailing commas
    use syn::parse::Parser;
    let mut id = None;
    let mut name = None;
    let mut description = None;
    let mut params_type = None;
    let handler_ident: Option<Ident> = None;
    let mut neural = false;
    let mut extra_tags: Vec<String> = Vec::new();

    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("id") {
            id = Some(meta.value()?.parse::<LitStr>()?.value());
            Ok(())
        } else if meta.path.is_ident("name") {
            name = Some(meta.value()?.parse::<LitStr>()?.value());
            Ok(())
        } else if meta.path.is_ident("description") {
            description = Some(meta.value()?.parse::<LitStr>()?.value());
            Ok(())
        } else if meta.path.is_ident("params") {
            let s = meta.value()?.parse::<LitStr>()?;
            let ty: Type = syn::parse_str(&s.value()).unwrap();
            params_type = Some(ty);
            Ok(())
        } else if meta.path.is_ident("neural") {
            neural = true;
            Ok(())
        } else if meta.path.is_ident("tags") {
            let s = meta.value()?.parse::<LitStr>()?.value();
            let tags: Vec<String> = s.split(',').map(|s| s.trim().to_string()).collect();
            extra_tags.extend(tags);
            Ok(())
        } else {
            Err(meta.error("unexpected argument"))
        }
    });

    parser.parse(args).unwrap();

    let id = match id {
        Some(id) => id,
        _ => panic!("Missing `id = \"...\"` attribute"),
    };
    let name = match name {
        Some(name) => name,
        _ => panic!("Missing `name = \"...\"` attribute"),
    };
    let description = match description {
        Some(desc) => desc,
        _ => panic!("Missing `description = \"...\"` attribute"),
    };
    let params_type = match params_type {
        Some(ty) => ty,
        _ => panic!("Missing `params = \"Type\"` attribute"),
    };
    let handler_ident = match handler_ident {
        Some(p) => p.clone(),
        _ => {
            // If not specified, use the function itself
            item_fn.sig.ident.clone()
        }
    };

    // Get the output type from the function signature
    let output = match &item_fn.sig.output {
        syn::ReturnType::Type(_, ty) => ty,
        syn::ReturnType::Default => panic!("Function must return Result<Output, AppError>"),
    };

    // Check if it's Result<Value, AppError> or Result<O, AppError>
    // We need to extract the inner Output type
    let output_ty = extract_output_type(output);
    let is_value_output = is_value_output(output_ty);

    let factory_expanded = if is_value_output {
        quote! {
            Box::new(GenericHandlerFn::new(move |ctx, params| {
                Box::pin(#handler_ident(ctx, params))
            }))
        }
    } else {
        quote! {
            Box::new(GenericTypedHandlerFn::new(move |ctx, params| {
                Box::pin(#handler_ident(ctx, params))
            }))
        }
    };

    // Generate the factory struct and registration
    let factory_ident = syn::Ident::new(
        &format!("{}_FACTORY", id.to_ascii_uppercase()),
        item_fn.sig.ident.span(),
    );

    let expanded = quote! {
        #item_fn

        #[allow(non_camel_case_types)]
        struct #factory_ident;

        use crate::models::tool::{CoreTool, ToolPo};
        use crate::pkg::tool_registry::BuiltinToolFactory;

        impl Clone for #factory_ident {
            fn clone(&self) -> Self {
                #factory_ident
            }
        }

        impl Copy for #factory_ident {}

        unsafe impl Send for #factory_ident {}
        unsafe impl Sync for #factory_ident {}

        impl BuiltinToolFactory for #factory_ident {
            fn create_po(&self) -> ToolPo {
                use common::enums::tool::{ControlMode, ToolProtocol, ToolStatus};
                let schema = schemars::schema_for!(#params_type);
                let schema_json = serde_json::to_value(&schema).unwrap();
                let mut tags_vec = Vec::new();
                if #neural {
                    tags_vec.push("neural".to_string());
                }
                #(tags_vec.push(#extra_tags.to_string());)*
                let mut po = ToolPo::new(
                    #id.to_string(),
                    #name.to_string(),
                    #description.to_string(),
                    ToolProtocol::Builtin,
                    schema_json,
                    None,
                    tags_vec,
                    None,
                );
                po.status = ToolStatus::Enabled;
                po.control_mode = ControlMode::Auto;
                po
            }

            fn create(&self, po: ToolPo) -> Box<dyn CoreTool> {
                use crate::pkg::tool_registry::handler_adapter::*;
                let adapter = HandlerToolAdapter::<#params_type>::new(
                    po,
                    #factory_expanded,
                );
                Box::new(adapter)
            }
        }

        // Force registration at startup
        const _: () = {
            #[ctor::ctor]
            fn register() {
                let factory = Box::new(#factory_ident);
                crate::pkg::tool_registry::get_registry()
                    .register_builtin_factory(factory);
            }
        };
    };

    expanded.into()
}

fn extract_output_type(ty: &Type) -> &Type {
    // We expect: Result<Output, AppError> (old) or Result<Output> (new with common::error::Result type alias)
    match ty {
        Type::Path(type_path) => {
            if let Some(last_segment) = type_path.path.segments.last() {
                match &last_segment.arguments {
                    syn::PathArguments::AngleBracketed(syn::AngleBracketedGenericArguments {
                        args,
                        ..
                    }) => {
                        // If 1 argument: it's Result<Output> -> return the only argument
                        // If 2 arguments: it's Result<Output, AppError> -> return the first argument
                        if let Some(syn::GenericArgument::Type(output)) = args.first() {
                            return output;
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    panic!("Expected return type to be Result<Output> or Result<Output, AppError>");
}

fn is_value_output(ty: &Type) -> bool {
    // Check if it's serde_json::Value
    match ty {
        Type::Path(type_path) => {
            let segments: Vec<String> = type_path
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            if segments.len() == 1 && segments[0] == "Value" {
                return true;
            }
            if segments.len() == 2 && segments[0] == "serde_json" && segments[1] == "Value" {
                return true;
            }
            false
        }
        _ => false,
    }
}

/// Generate axum HTTP handler from a core handler function
///
/// The generated handler will be named `{function_name}_handler`.
/// Path fields are automatically detected from `#[param(source = "path")]` attribute on the
/// parameter struct fields.
#[proc_macro_attribute]
pub fn generate_http_handler(_args: TokenStream, input: TokenStream) -> TokenStream {
    let item_fn = parse_macro_input!(input as ItemFn);

    // Extract information from the function
    let core_ident = &item_fn.sig.ident;
    let handler_ident = syn::Ident::new(&format!("{}_handler", core_ident), core_ident.span());

    // Get the params type from the function signature
    // Expected: (ctx: RequestContext, params: Params)
    let params_ty = if item_fn.sig.inputs.len() == 2 {
        match item_fn.sig.inputs[1] {
            syn::FnArg::Typed(ref pat_ty) => &pat_ty.ty,
            _ => panic!("Expected second parameter to be params: Params"),
        }
    } else {
        panic!(
            "Expected function signature: async fn name(ctx: RequestContext, params: Params) -> ::std::result::Result<Output, AppError>"
        );
    };

    // Get the output type
    let output_ty = match &item_fn.sig.output {
        syn::ReturnType::Type(_, ty) => extract_output_type(ty),
        syn::ReturnType::Default => panic!("Function must return Result<Output, AppError>"),
    };

    // Get the type path to the params struct
    let params_ty_path = match &**params_ty {
        Type::Path(type_path) => type_path.path.clone(),
        _ => panic!("Params must be a named struct type"),
    };

    // Collect #[path_param] and #[query_param] annotations from the struct definition
    let (path_fields, query_fields, flattened_query_fields, total_named_fields) =
        collect_path_and_query_fields_from_type(params_ty_path);
    let path_idents: Vec<Ident> = path_fields.iter().map(|(ident, _)| ident.clone()).collect();
    let path_types: Vec<syn::Type> = path_fields.iter().map(|(_, ty)| ty.clone()).collect();
    let has_path = !path_idents.is_empty();
    // Empty struct (no named fields): no body to extract, generate parameterless handler.
    let is_empty_params = total_named_fields == 0;
    let query_idents: Vec<Ident> = query_fields
        .iter()
        .map(|(ident, _)| ident.clone())
        .collect();
    let flattened_query_idents: Vec<Ident> = flattened_query_fields
        .iter()
        .map(|(ident, _)| ident.clone())
        .collect();
    let has_query = !query_idents.is_empty() || !flattened_query_idents.is_empty();

    // Generate the handler code
    let expanded = match (has_path, has_query) {
        (true, true) => {
            let path_tuple = quote! { ( #( #path_idents, )* ) };
            let path_ty_tuple = quote! { ( #( #path_types, )* ) };
            let assign_paths = quote! { #( params.#path_idents = #path_idents; )* };

            // 为非 flatten query 字段生成提取代码
            let extract_query_fields = {
                let query_idents_str: Vec<String> =
                    query_idents.iter().map(|i| i.to_string()).collect();
                quote! {
                    #(
                        if let Some(__v) = __query_value.get(#query_idents_str) {
                            if let Ok(__parsed) = serde_json::from_value(__v.clone()) {
                                params.#query_idents = __parsed;
                            }
                        }
                    )*
                }
            };

            // 为 flatten query 字段生成提取代码
            let extract_flattened_query_fields = {
                quote! {
                    #(
                        if let Ok(__parsed) = serde_json::from_value(__query_value.clone()) {
                            params.#flattened_query_idents = __parsed;
                        }
                    )*
                }
            };

            // 判断是否所有非 path 字段都是 query（即无 body 字段）
            let total_query_fields = query_fields.len() + flattened_query_fields.len();
            if total_named_fields == path_fields.len() + total_query_fields {
                // path+query only GET：无 Json 提取器
                quote! {
                    #item_fn

                    pub async fn #handler_ident(
                        axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                        axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                        axum::extract::RawQuery(__raw_query): axum::extract::RawQuery,
                    ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                        let mut params = <#params_ty as ::std::default::Default>::default();

                        // 解析 query string 并构建 serde_json::Value（带类型推断）
                        let __query_value: serde_json::Value = if let Some(__qs) = __raw_query.as_deref() {
                            let __query_map: std::collections::HashMap<String, String> =
                                serde_urlencoded::from_str(__qs).unwrap_or_default();
                            let mut __obj = serde_json::Map::new();
                            for (__k, __v) in &__query_map {
                                let __parsed: serde_json::Value = if __v == "true" {
                                    serde_json::Value::Bool(true)
                                } else if __v == "false" {
                                    serde_json::Value::Bool(false)
                                } else if __v == "null" {
                                    serde_json::Value::Null
                                } else if let Ok(__n) = __v.parse::<i64>() {
                                    serde_json::Value::Number(__n.into())
                                } else if let Ok(__n) = __v.parse::<f64>() {
                                    serde_json::Number::from_f64(__n)
                                        .map(serde_json::Value::Number)
                                        .unwrap_or(serde_json::Value::String(__v.clone()))
                                } else {
                                    serde_json::Value::String(__v.clone())
                                };
                                __obj.insert(__k.clone(), __parsed);
                            }
                            serde_json::Value::Object(__obj)
                        } else {
                            serde_json::Value::Object(serde_json::Map::new())
                        };

                        // 提取非 flatten query 字段
                        #extract_query_fields
                        // 提取 flatten query 字段
                        #extract_flattened_query_fields
                        // 提取 path 字段（path 优先级最高）
                        #assign_paths

                        let __result = #core_ident(ctx, params).await?;
                        Ok(axum::Json(common::api::ApiResponse::success(__result)))
                    }
                }
            } else {
                // path + query + body 混合：保留 Json 提取器
                quote! {
                    #item_fn

                    pub async fn #handler_ident(
                        axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                        axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                        axum::extract::RawQuery(__raw_query): axum::extract::RawQuery,
                        axum::Json(mut params): axum::Json<#params_ty>,
                    ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                        // 解析 query string 并构建 serde_json::Value
                        let __query_value: serde_json::Value = if let Some(__qs) = __raw_query.as_deref() {
                            let __query_map: std::collections::HashMap<String, String> =
                                serde_urlencoded::from_str(__qs).unwrap_or_default();
                            let mut __obj = serde_json::Map::new();
                            for (__k, __v) in &__query_map {
                                let __parsed: serde_json::Value = if __v == "true" {
                                    serde_json::Value::Bool(true)
                                } else if __v == "false" {
                                    serde_json::Value::Bool(false)
                                } else if __v == "null" {
                                    serde_json::Value::Null
                                } else if let Ok(__n) = __v.parse::<i64>() {
                                    serde_json::Value::Number(__n.into())
                                } else if let Ok(__n) = __v.parse::<f64>() {
                                    serde_json::Number::from_f64(__n)
                                        .map(serde_json::Value::Number)
                                        .unwrap_or(serde_json::Value::String(__v.clone()))
                                } else {
                                    serde_json::Value::String(__v.clone())
                                };
                                __obj.insert(__k.clone(), __parsed);
                            }
                            serde_json::Value::Object(__obj)
                        } else {
                            serde_json::Value::Object(serde_json::Map::new())
                        };

                        // 优先级：path > query > body
                        // 先用 query 覆盖 body 字段
                        #extract_query_fields
                        #extract_flattened_query_fields
                        // 最后用 path 覆盖
                        #assign_paths

                        let __result = #core_ident(ctx, params).await?;
                        Ok(axum::Json(common::api::ApiResponse::success(__result)))
                    }
                }
            }
        }
        (true, false) => {
            let path_tuple = quote! {
                ( #( #path_idents, )* )
            };
            let path_ty_tuple = quote! {
                ( #( #path_types, )* )
            };
            let assign_paths = quote! {
                #(
                    params.#path_idents = #path_idents;
                )*
            };

            // 当所有命名字段都是 path 参数时，无需 Json body 提取器
            // 典型场景：GET /items/{id}, DELETE /items/{id}
            // 要求 Params: Default（用 Default::default() 构造空实例，再用 path 值覆盖）
            if total_named_fields == path_fields.len() {
                quote! {
                    #item_fn

                    pub async fn #handler_ident(
                        axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                        axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                    ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                        let mut params = <#params_ty as ::std::default::Default>::default();
                        #assign_paths
                        let result = #core_ident(ctx, params).await?;
                        Ok(axum::Json(common::api::ApiResponse::success(result)))
                    }
                }
            } else {
                // path + body 混合：保留 Json 提取器，path 字段后覆盖 body 字段
                // 优先级：path > body（path 字段从 URL 提取，覆盖 body 中的同名字段）
                quote! {
                    #item_fn

                    pub async fn #handler_ident(
                        axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                        axum::extract::Path(#path_tuple): axum::extract::Path<#path_ty_tuple>,
                        axum::Json(mut params): axum::Json<#params_ty>,
                    ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                        #assign_paths
                        let result = #core_ident(ctx, params).await?;
                        Ok(axum::Json(common::api::ApiResponse::success(result)))
                    }
                }
            }
        }
        (false, true) => {
            quote! {
                #item_fn

                pub async fn #handler_ident(
                    axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                    axum::extract::Query(params): axum::extract::Query<#params_ty>,
                ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                    let result = #core_ident(ctx, params).await?;
                    Ok(axum::Json(common::api::ApiResponse::success(result)))
                }
            }
        }
        (false, false) => {
            if is_empty_params {
                // Empty struct: no body to extract (typical for GET endpoints without params).
                // Requires `Params: Default` to construct an empty value.
                quote! {
                    #item_fn

                    pub async fn #handler_ident(
                        axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                    ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                        let params = <#params_ty as ::std::default::Default>::default();
                        let result = #core_ident(ctx, params).await?;
                        Ok(axum::Json(common::api::ApiResponse::success(result)))
                    }
                }
            } else {
                // No path, no query - all in body
                quote! {
                    #item_fn

                    pub async fn #handler_ident(
                        axum::extract::Extension(ctx): axum::extract::Extension<RequestContext>,
                        axum::Json(params): axum::Json<#params_ty>,
                    ) -> ::std::result::Result<axum::Json<common::api::ApiResponse<#output_ty>>, common::error::Error> {
                        let result = #core_ident(ctx, params).await?;
                        Ok(axum::Json(common::api::ApiResponse::success(result)))
                    }
                }
            }
        }
    };

    expanded.into()
}
/// Collect field ids that have #[param(source = "path")] or #[param(source = "query")] attribute by parsing the source file.
///
/// Returns `(path_fields, query_fields, flattened_query_fields, total_named_fields)`. `total_named_fields` counts all
/// named fields of the struct (regardless of #[param] annotation), used to detect empty unit
/// structs for which no `Json` extractor should be generated. `flattened_query_fields` holds
/// query fields annotated with `#[serde(flatten)]`.
fn collect_path_and_query_fields_from_type(
    path: syn::Path,
) -> (
    Vec<(Ident, Type)>,
    Vec<(Ident, Type)>,
    Vec<(Ident, Type)>,
    usize,
) {
    // Get the last segment (the type name)
    let type_name = path.segments.last().unwrap().ident.to_string();

    // Get the project root from current environment
    let workspace_root = std::env::var("CARGO_WORKSPACE_DIR").unwrap_or_else(|_| ".".to_string());

    // Search for the file that contains this type
    // common/src/api/**/*.rs
    let pattern = format!("{}/**/*.rs", workspace_root);

    for entry in glob::glob(&pattern).unwrap() {
        if let Ok(path) = entry {
            let content = std::fs::read_to_string(path).ok();
            if let Some(content) = content {
                let syntax = syn::parse_file(&content).ok();
                if let Some(syntax) = syntax {
                    for item in &syntax.items {
                        if let syn::Item::Struct(item_struct) = item
                            && item_struct.ident.to_string() == type_name
                        {
                                // Found it! Collect fields with #[param(source = "...")] attribute
                                let mut path_fields = Vec::new();
                                let mut query_fields = Vec::new();
                                let mut flattened_query_fields = Vec::new();
                                // Count named fields (Fields::Named has idents; Fields::Unit has none)
                                let total_named_fields = match &item_struct.fields {
                                    syn::Fields::Named(named) => named.named.len(),
                                    syn::Fields::Unnamed(unnamed) => unnamed.unnamed.len(),
                                    syn::Fields::Unit => 0,
                                };
                                for field in &item_struct.fields {
                                    for attr in &field.attrs {
                                        if attr.path().is_ident("param") {
                                            // `#[param(source = "path")]` 整体形式是 Meta::List：
                                            //   attr.path = "param"，tokens = `source = "path"`
                                            // 修复前 bug：用 `Meta::NameValue` 匹配整个 attr.meta，
                                            //   但 attr.meta 实际是 `Meta::List`，导致所有 #[param]
                                            //   标注被静默忽略，path/query 字段全部被当作 body。
                                            //   修复方式：先匹配 Meta::List，再用 parse_args 解析内层。
                                            if let Meta::List(meta_list) = &attr.meta
                                                && let Ok(nv) =
                                                    meta_list.parse_args::<MetaNameValue>()
                                                && nv.path.is_ident("source")
                                                && let syn::Expr::Lit(syn::ExprLit {
                                                    lit: Lit::Str(s),
                                                    ..
                                                }) = &nv.value
                                            {
                                                match s.value().as_str() {
                                                    "path" => {
                                                        if let Some(ident) =
                                                            &field.ident
                                                        {
                                                            path_fields.push((
                                                                ident.clone(),
                                                                field.ty.clone(),
                                                            ));
                                                        }
                                                    }
                                                    "query" => {
                                                        if let Some(ident) =
                                                            &field.ident
                                                        {
                                                            // 检查是否有 #[serde(flatten)] 属性
                                                            let is_flattened = field.attrs.iter().any(|attr| {
                                                                if attr.path().is_ident("serde")
                                                                    && let Meta::List(meta_list) = &attr.meta
                                                                {
                                                                    let tokens_str = meta_list.tokens.to_string();
                                                                    return tokens_str.contains("flatten");
                                                                }
                                                                false
                                                            });

                                                            if is_flattened {
                                                                flattened_query_fields
                                                                    .push((
                                                                        ident.clone(),
                                                                        field
                                                                            .ty
                                                                            .clone(),
                                                                    ));
                                                            } else {
                                                                query_fields.push((
                                                                    ident.clone(),
                                                                    field.ty.clone(),
                                                                ));
                                                            }
                                                        }
                                                    }
                                                    "body" => {
                                                        // body is default, no need to collect
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }
                                return (
                                    path_fields,
                                    query_fields,
                                    flattened_query_fields,
                                    total_named_fields,
                                );
                        }
                    }
                }
            }
        }
    }

    // If not found, panic
    panic!("Could not find struct type {} in source files", type_name);
}

/// Derive macro to automatically implement LogFields trait.
///
/// Marks fields with `#[log_field]` to include them in tracing spans.
///
/// # Usage
/// ```rust,ignore
/// use ai_orz_macros::LogFields;
///
/// #[derive(Debug, Clone, LogFields)]
/// pub struct RequestContext {
///     #[log_field]
///     pub log_id: String,
///     #[log_field]
///     pub user_id: Option<String>,
/// }
/// ```
#[proc_macro_derive(LogFields, attributes(log_field))]
pub fn derive_log_fields(input: TokenStream) -> TokenStream {
    log_fields_derive(input)
}
