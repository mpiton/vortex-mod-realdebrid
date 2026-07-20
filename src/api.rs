//! Real-Debrid REST API v1.0 — requests and error classification.
//!
//! Real-Debrid answers a failure with a genuine 4xx *and* a body carrying a
//! numeric `error_code`. The status alone is too coarse to drive the
//! resolution cascade, so the body is what decides the typed error.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::PluginError;
use crate::http::{percent_encode, truncate, HttpRequest, HttpResponse};

const API_BASE: &str = "https://api.real-debrid.com/rest/1.0";

#[derive(Debug, Deserialize)]
struct ApiError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_code: i64,
}

pub fn build_user_request(token: &str) -> Result<String, PluginError> {
    serialize(HttpRequest {
        method: "GET".into(),
        url: format!("{API_BASE}/user"),
        headers: auth_headers(token, None),
        body: None,
    })
}

pub fn build_unrestrict_request(url: &str, token: &str) -> Result<String, PluginError> {
    serialize(HttpRequest {
        method: "POST".into(),
        url: format!("{API_BASE}/unrestrict/link"),
        headers: auth_headers(token, Some("application/x-www-form-urlencoded")),
        body: Some(format!("link={}", percent_encode(url))),
    })
}

fn auth_headers(token: &str, content_type: Option<&str>) -> HashMap<String, String> {
    let mut headers = HashMap::from([
        ("Authorization".to_string(), format!("Bearer {token}")),
        ("Accept".to_string(), "application/json".to_string()),
    ]);
    if let Some(content_type) = content_type {
        headers.insert("Content-Type".to_string(), content_type.to_string());
    }
    headers
}

fn serialize(request: HttpRequest) -> Result<String, PluginError> {
    serde_json::to_string(&request).map_err(PluginError::SerdeJson)
}

/// Yield the success body, or the typed error the failure body describes.
pub fn into_api_body(response: HttpResponse) -> Result<String, PluginError> {
    let (status, body) = response.into_parts()?;
    if (200..300).contains(&status) {
        return Ok(body);
    }
    match serde_json::from_str::<ApiError>(&body) {
        Ok(api_error) if api_error.error_code != 0 => {
            Err(classify_error(api_error.error_code, &api_error.error))
        }
        // No usable body: fall back to what the status alone can tell us.
        _ => Err(match status {
            401 | 403 => PluginError::InvalidCredentials,
            429 => PluginError::RateLimited(truncate(&body, 128)),
            404 | 410 => PluginError::HosterUnavailable(format!("status {status}")),
            _ => PluginError::HttpStatus {
                status,
                message: truncate(&body, 256),
            },
        }),
    }
}

/// Map a Real-Debrid `error_code` onto the host's machine codes.
///
/// Code 16 (unsupported hoster) matters most: it is how the cascade learns
/// this debrid does not cover the hoster and falls through to the next tier
/// instead of reporting a fake success (R-04).
fn classify_error(error_code: i64, message: &str) -> PluginError {
    let detail = format!("error_code {error_code}: {}", truncate(message, 128));
    match error_code {
        8 | 9 | 12 | 13 | 14 | 15 | 22 => PluginError::InvalidCredentials,
        20 => PluginError::AccountExpired(detail),
        5 | 21 | 34 => PluginError::RateLimited(detail),
        18 | 23 | 36 => PluginError::QuotaExceeded(detail),
        6 | 7 | 16 | 17 | 19 | 24 | 35 => PluginError::HosterUnavailable(detail),
        _ => PluginError::InvalidApiResponse(detail),
    }
}

/// The host hands credentials over as `{"password": "<api token>"}`.
pub fn parse_credential_response(raw: &str) -> Result<String, PluginError> {
    #[derive(Deserialize)]
    struct CredentialResponse {
        #[serde(default)]
        password: String,
    }
    // The serde error is deliberately dropped: on a type mismatch it quotes
    // the offending value, and here that value is the token (R-05).
    let credential: CredentialResponse = serde_json::from_str(raw).map_err(|_| {
        PluginError::HostResponse("credential payload is not the expected shape".into())
    })?;
    let token = credential.password.trim();
    if token.is_empty() {
        return Err(PluginError::InvalidCredentials);
    }
    Ok(token.to_string())
}
