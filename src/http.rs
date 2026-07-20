//! HTTP envelope exchanged with the host's `http_request` function.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

#[derive(Debug, Serialize)]
pub(crate) struct HttpRequest {
    pub method: String,
    pub url: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub headers: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: String,
}

/// Cap responses so a hostile server can't force megabyte-scale parsing.
/// Real Real-Debrid JSON payloads weigh a few kilobytes.
pub(crate) const MAX_BODY_BYTES: usize = 1024 * 1024;

impl HttpResponse {
    /// Unlike AllDebrid, Real-Debrid signals failure with a real 4xx *and*
    /// puts the machine-readable reason in the body, so the caller needs both
    /// halves rather than just the status.
    pub fn into_parts(self) -> Result<(u16, String), PluginError> {
        if self.body.len() > MAX_BODY_BYTES {
            return Err(PluginError::HttpStatus {
                status: self.status,
                message: format!("body exceeds {MAX_BODY_BYTES} bytes"),
            });
        }
        Ok((self.status, self.body))
    }
}

pub(crate) fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut cut = max;
        while !s.is_char_boundary(cut) && cut > 0 {
            cut -= 1;
        }
        format!("{}…", &s[..cut])
    }
}

pub fn parse_http_response(raw: &str) -> Result<HttpResponse, PluginError> {
    serde_json::from_str(raw).map_err(|e| PluginError::HostResponse(e.to_string()))
}

/// Percent-encode a value for a query string or form body.
///
/// Only the unreserved set survives, so a hoster URL round-trips intact.
pub(crate) fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_parts_keeps_the_status_and_body_together() {
        let response = HttpResponse {
            status: 403,
            headers: HashMap::new(),
            body: "{\"error_code\":8}".into(),
        };

        let (status, body) = response.into_parts().expect("parts");

        assert_eq!(status, 403);
        assert_eq!(body, "{\"error_code\":8}");
    }

    #[test]
    fn into_parts_rejects_an_oversized_body() {
        let response = HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: "x".repeat(MAX_BODY_BYTES + 1),
        };

        assert!(response.into_parts().is_err());
    }

    #[test]
    fn percent_encode_escapes_every_reserved_character_of_a_url() {
        assert_eq!(
            percent_encode("https://a.io/f?x=1&y=2"),
            "https%3A%2F%2Fa.io%2Ff%3Fx%3D1%26y%3D2"
        );
    }

    #[test]
    fn percent_encode_leaves_the_unreserved_set_alone() {
        assert_eq!(percent_encode("aZ0-._~"), "aZ0-._~");
    }
}
