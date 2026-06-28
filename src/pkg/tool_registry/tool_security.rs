//! Common security utilities for tool registry shared by HTTP, HTTP fetch, and other external tools

use std::net::{IpAddr, SocketAddr};
use tokio::net::lookup_host;
use common::error::Result;
use anyhow::{anyhow};

/// Default maximum response size in bytes: 1MB
pub const DEFAULT_RESPONSE_MAX_BYTES: usize = 1024 * 1024;
/// Hard maximum response size in bytes: 10MB
pub const HARD_RESPONSE_MAX_BYTES: usize = 10 * 1024 * 1024;
/// Default request timeout in milliseconds: 30s
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;
/// Hard maximum timeout in milliseconds: 10 minutes
pub const HARD_TIMEOUT_MS: u64 = 600_000;

/// Check if a host is a local network address (SSRF protection)
pub fn is_local_network_host(host: &str) -> bool {
    let normalized = normalize_domain(host);
    if normalized == "localhost" {
        return true;
    }

    normalized.parse::<IpAddr>().is_ok_and(is_local_network_ip)
}

/// Check if an IP address is a local network address (SSRF protection)
pub fn is_local_network_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            ip.is_loopback()
                || ip.is_private()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_unspecified()
                || octets[0] == 0
                || (octets[0] == 100 && (octets[1] & 0b1100_0000) == 64)
                || (octets[0] == 169 && octets[1] == 254)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
                || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                || (octets[0] == 192 && octets[1] == 168)
                || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
                || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
                || octets[0] >= 224
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return is_local_network_ip(IpAddr::V4(mapped));
            }

            let segments = ip.segments();
            let is_nat64_well_known = segments[0] == 0x0064 && segments[1] == 0xff9b;
            let is_teredo = segments[0] == 0x2001 && segments[1] == 0x0000;
            let is_6to4 = segments[0] == 0x2002;

            ip.is_loopback()
                || ip.is_unspecified()
                || (segments[0] & 0xff00) == 0xff00
                || (segments[0] & 0xfe00) == 0xfc00
                || (segments[0] & 0xffc0) == 0xfe80
                || (segments[0] & 0xffc0) == 0xfec0
                || (segments[0] == 0x2001 && segments[1] == 0x0db8)
                || is_nat64_well_known
                || is_teredo
                || is_6to4
        }
    }
}

/// Normalize a domain name for matching
pub fn normalize_domain(domain: &str) -> String {
    domain
        .trim()
        .trim_matches(['[', ']'])
        .trim_end_matches('.')
        .to_ascii_lowercase()
}

/// Check if a domain matches a configured pattern (allowed/blocked)
pub fn domain_matches(host: &str, configured_domain: &str) -> bool {
    let host = normalize_domain(host);
    let configured_domain = normalize_domain(configured_domain);
    if configured_domain.is_empty() {
        return false;
    }

    host == configured_domain || host.ends_with(&format!(".{}", configured_domain))
}

/// Validate a target URL and resolve addresses with SSRF protection
/// Returns the list of validated socket addresses for DNS pinning
pub async fn validate_target_url(
    allow_local_network: Option<bool>,
    allowed_domains: Option<&Vec<String>>,
    blocked_domains: Option<&Vec<String>>,
    url: &reqwest::Url,
) -> Result<Vec<SocketAddr>> {
    let scheme = url.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("unsupported url scheme: {}", scheme).into());
    }

    let host = normalize_domain(
        url.host_str()
            .ok_or_else(|| anyhow!("url host is required"))?
    );

    if host.is_empty() {
        return Err(anyhow!("url host is required").into());
    }

    if let Some(blocked_domains) = blocked_domains {
        if blocked_domains
            .iter()
            .any(|domain| domain_matches(&host, domain))
        {
            return Err(anyhow!("blocked http domain").into());
        }
    }

    if is_local_network_host(&host) && allow_local_network != Some(true) {
        return Err(anyhow!(
            "local network http target requires allow_local_network=true"
        ).into());
    }

    if let Some(allowed_domains) = allowed_domains {
        if !allowed_domains
            .iter()
            .any(|domain| domain_matches(&host, domain))
        {
            return Err(anyhow!("http domain is not allowed").into());
        }
    }

    let addresses = validate_resolved_addresses(
        &host,
        url.port_or_known_default(),
        allow_local_network,
    ).await?;

    Ok(addresses)
}

/// Validate resolved IP addresses with SSRF protection
async fn validate_resolved_addresses(
    host: &str,
    port: Option<u16>,
    allow_local_network: Option<bool>,
) -> Result<Vec<SocketAddr>> {
    let Some(port) = port else {
        return Ok(Vec::new());
    };

    let lookup_host_name = host.trim_matches(['[', ']']);
    let addresses: Vec<SocketAddr> = lookup_host((lookup_host_name, port))
        .await
        .map_err(|_| anyhow!("failed to resolve http target host"))
        .map_err(|e| common::error::Error::from(e))?
        .collect();

    if allow_local_network != Some(true) {
        if addresses
            .iter()
            .any(|address| is_local_network_ip(address.ip()))
        {
            return Err(anyhow!(
                "resolved local network http target requires allow_local_network=true"
            ).into());
        }
    }

    Ok(addresses)
}

/// Check if a header name is sensitive and should be redacted
pub fn is_sensitive_header(name: &str) -> bool {
    let normalized = name.to_ascii_lowercase();
    normalized == "authorization"
        || normalized == "cookie"
        || normalized == "set-cookie"
        || normalized.contains("api-key")
        || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("password")
}

/// Sanitize response headers, redacting sensitive values
pub fn sanitize_response_headers(headers: &reqwest::header::HeaderMap) -> serde_json::Value {
    use serde_json::{Map, Value};
    let mut sanitized = Map::new();
    for (name, value) in headers {
        let key = name.as_str().to_string();
        let value = if is_sensitive_header(name.as_str()) {
            Value::String("[REDACTED]".to_string())
        } else {
            Value::String(value.to_str().unwrap_or("<non-utf8>").to_string())
        };
        sanitized.insert(key, value);
    }
    Value::Object(sanitized)
}

/// Read response body with a hard size limit
pub async fn read_limited_response_body(
    response: &mut reqwest::Response,
    max_bytes: usize,
) -> Result<Vec<u8>> {
    if let Some(content_length) = response.content_length() {
        if content_length > max_bytes as u64 {
            return Err(anyhow!(
                "http response too large: {} bytes exceeds limit {}",
                content_length,
                max_bytes
            ).into());
        }
    }

    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow!("http response read failed"))?
    {
        if bytes.len() + chunk.len() > max_bytes {
            return Err(anyhow!(
                "http response too large: exceeds limit {} bytes",
                max_bytes
            ).into());
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}

/// Validate URL template boundary (scheme and authority must be fixed)
pub fn validate_url_template_boundary(url_template: &str) -> Result<()> {
    let trimmed = url_template.trim();
    let Some(scheme_end) = trimmed.find("://") else {
        return Err(anyhow!("http tool url must include http/https scheme").into());
    };

    let scheme = &trimmed[..scheme_end];
    if scheme.contains("{{") {
        return Err(anyhow!(
            "http url scheme must be fixed and must not contain template placeholders"
        ).into());
    }
    if scheme != "http" && scheme != "https" {
        return Err(anyhow!("unsupported http url scheme: {}", scheme).into());
    }

    let after_scheme = &trimmed[scheme_end + 3..];
    let authority_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let authority = &after_scheme[..authority_end];
    if authority.is_empty() {
        return Err(anyhow!("http url host is required").into());
    }
    if authority.contains("{{") {
        return Err(anyhow!(
            "http url host must be fixed and must not contain template placeholders"
        ).into());
    }
    if authority.contains('@') {
        return Err(anyhow!("http url must not contain userinfo credentials").into());
    }

    Ok(())
}

/// Replace placeholders with sentinels for URL validation
pub fn url_template_with_placeholder_sentinels(url_template: &str) -> Result<String> {
    let mut output = String::with_capacity(url_template.len());
    let mut rest = url_template;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err(anyhow!(
                "unresolved or unsupported http template placeholder"
            ).into());
        };
        output.push_str("placeholder");
        rest = &after_start[end + 2..];
    }
    output.push_str(rest);
    Ok(output)
}

/// Validate that all placeholders are of the supported form `{{args.key}}`
pub fn validate_supported_placeholders(template: &str) -> Result<()> {
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err(anyhow!(
                "unresolved or unsupported http template placeholder"
            ).into());
        };
        let placeholder = &after_start[..end];
        if placeholder.trim() != placeholder
            || !placeholder.starts_with("args.")
            || placeholder.len() <= "args.".len()
        {
            return Err(anyhow!(
                "unresolved or unsupported http template placeholder"
            ).into());
        }
        rest = &after_start[end + 2..];
    }

    if rest.contains("}}") {
        return Err(anyhow!(
            "unresolved or unsupported http template placeholder"
        ).into());
    }

    Ok(())
}

/// File system tool security utilities shared by fs_read and fs_write
pub mod fs {
    use std::path::{Path, PathBuf};
use common::error::Result;
use anyhow::anyhow;

/// Validation result for file path checks
#[derive(Debug)]
pub enum ValidationResult {
    /// Path is valid and can be accessed
    Valid(PathBuf),
    /// Path outside default scope - needs explicit user confirmation
    NeedConfirmation(String),
}

/// Resolve and validate a user-provided path against base data directory and additional allowed paths.
/// Follows sandbox security rules:
/// 1. Reject sensitive filenames (.env, .key, etc.) immediately
/// 2. Canonicalize to resolve .. and symlinks
/// 3. Check that final path is within allowed scope (base or additional)
/// 4. Reject symbolic links
pub fn resolve_and_validate_path(
    base_path: &Path,
    user_path: &str,
    additional_allowed_paths: &[String],
) -> Result<ValidationResult> {
    // 1. Check sensitive filename patterns first
    if is_sensitive_filename(user_path) {
        return Err(anyhow!("Access denied: cannot access sensitive file").into());
    }

    // 2. Build absolute path
    let user_path_input = user_path;
    let user_path = Path::new(user_path_input);
    let absolute_path = if user_path.is_absolute() {
        user_path.to_path_buf()
    } else {
        base_path.join(user_path)
    };

    // 3. Canonicalize to resolve .. and symlinks
    let canonical = if absolute_path.exists() {
        absolute_path.canonicalize()
            .map_err(|_| anyhow!("Failed to resolve path: file not found or permission denied"))
    } else {
        // File doesn't exist yet - canonicalize parent directory
        match absolute_path.parent() {
            Some(parent) => {
                let parent_canon = parent.canonicalize()
                    .map_err(|_| anyhow!("Parent directory does not exist or permission denied"))?;
                Ok(parent_canon.join(absolute_path.file_name().unwrap()))
            }
            None => {
                Err(anyhow!("Invalid path: no parent directory"))
            }
        }
    }
    .map_err(|e| common::error::Error::from(e))?;

    // 4. Check that canonical path is in allowed scope:
    //    - either under base_path, OR under one of the additional allowed paths
    let base_canonical = base_path.canonicalize()
        .map_err(|e| anyhow!("Invalid base data path: {}", e))
        .map_err(|e| common::error::Error::from(e))?;

    let mut allowed = canonical.starts_with(&base_canonical);

    // Check additional allowed paths from configuration
    if !allowed {
        for additional in additional_allowed_paths {
            let additional_path = Path::new(additional);
            let additional_canon = if additional_path.is_absolute() {
                additional_path.to_path_buf()
            } else {
                base_path.join(additional_path)
            }.canonicalize();
            if let Ok(additional_canon) = additional_canon {
                if canonical.starts_with(&additional_canon) {
                    allowed = true;
                    break;
                }
            }
            // Ignore invalid/unresolvable additional paths
        }
    }

    if !allowed {
        // Not strictly denied, but needs user confirmation
        return Ok(ValidationResult::NeedConfirmation(format!(
            "Path '{}' is outside the default working directory. \
            You MUST STOP and ask the user for explicit confirmation before accessing this file.",
            user_path_input
        )));
    }

    // 5. Reject symlinks
    if let Ok(metadata) = canonical.symlink_metadata() {
        if metadata.file_type().is_symlink() {
            return Err(anyhow!("Access denied: symbolic links are not allowed").into());
        }
    }

    Ok(ValidationResult::Valid(canonical))
}

/// Check if filename matches sensitive patterns that should be blocked
pub fn is_sensitive_filename(path: &str) -> bool {
    let lower = path.to_lowercase();
    // Sensitive patterns
    if lower.contains(".env") { return true; }
    if lower.contains(".pem") { return true; }
    if lower.contains(".key") { return true; }
    if lower.contains(".p12") { return true; }
    if lower.contains(".pfx") { return true; }
    if lower.contains("id_rsa") { return true; }
    if lower.contains("id_dsa") { return true; }
    if lower.contains("id_ecdsa") { return true; }
    if lower.contains("password") { return true; }
    if lower.contains("secret") { return true; }
    if lower.contains("token") { return true; }
    if lower.contains("credential") { return true; }
    if lower.contains("auth") { return true; }
    // Hidden files starting with .
    if path.split('/').last()
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
    {
        return true;
    }
    false
}

/// Sanitize IO error to remove absolute paths for security
pub fn sanitize_error<E: std::fmt::Display>(e: E) -> String {
    let s = e.to_string();
    // Remove absolute path prefixes, keep only the error message
    // This is a simple sanitization, enough for our purposes
    s.split('/')
        .last()
        .map(|last| last.to_string())
        .unwrap_or(s)
}
}
