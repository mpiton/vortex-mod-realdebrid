//! Parser tests against captured Real-Debrid payloads.
//!
//! Real-Debrid pairs a 4xx status with a numeric `error_code`, and only the
//! code is precise enough to tell the cascade whether to retry, rotate, or
//! fall through to the next tier (R-04).

use std::collections::HashMap;

use vortex_mod_realdebrid::api::{
    build_unrestrict_request, build_user_request, into_api_body, parse_credential_response,
};
use vortex_mod_realdebrid::http::{parse_http_response, HttpResponse};
use vortex_mod_realdebrid::responses::{parse_unrestrict_response, parse_user_response};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap_or_else(|e| panic!("fixture {name}: {e}"))
}

/// Rebuild the 4xx-plus-body pair the host would hand us.
fn failed_call(status: u16, fixture_name: &str) -> String {
    let response = HttpResponse {
        status,
        headers: HashMap::new(),
        body: fixture(fixture_name),
    };
    into_api_body(response)
        .expect_err("a 4xx is never a success")
        .code()
        .to_string()
}

#[test]
fn test_parse_user_premium_account_returns_the_expiry_timestamp() {
    let status = parse_user_response(&fixture("user_premium.json")).expect("premium is valid");

    assert_eq!(status.valid_until, Some(1_789_732_800));
}

#[test]
fn test_parse_user_free_account_reports_expired() {
    let error = parse_user_response(&fixture("user_free.json")).expect_err("free is unusable");

    assert_eq!(error.code(), "ACCOUNT_EXPIRED");
}

#[test]
fn test_parse_unrestrict_success_returns_the_direct_cdn_url() {
    let link =
        parse_unrestrict_response(&fixture("unrestrict_success.json")).expect("unrestrict works");

    assert_eq!(
        link.direct_url,
        "https://34.download.real-debrid.com/d/XYZABC123/archive.zip"
    );
    assert_eq!(link.filename.as_deref(), Some("archive.zip"));
    assert_eq!(link.size_bytes, Some(104857600));
    assert!(
        link.resumable,
        "16 chunks means range requests are honoured"
    );
}

#[test]
fn test_parse_unrestrict_multi_file_payload_takes_the_first_entry() {
    let link =
        parse_unrestrict_response(&fixture("unrestrict_multi.json")).expect("array is accepted");

    assert_eq!(
        link.direct_url,
        "https://34.download.real-debrid.com/d/AAA/part1.mkv"
    );
}

#[test]
fn test_parse_unrestrict_without_a_download_link_is_never_reported_as_success() {
    let error = parse_unrestrict_response(&fixture("unrestrict_without_download.json"))
        .expect_err("no direct url is a failure, not a file");

    assert_eq!(error.code(), "PLUGIN_ERROR");
}

#[test]
fn test_bad_token_reports_invalid_credentials() {
    assert_eq!(
        failed_call(401, "error_bad_token.json"),
        "ACCOUNT_INVALID_CREDENTIALS"
    );
}

#[test]
fn test_unsupported_hoster_reports_no_file_so_the_cascade_falls_through() {
    assert_eq!(
        failed_call(503, "error_unsupported_hoster.json"),
        "HOSTER_NO_FILE"
    );
}

#[test]
fn test_traffic_exhausted_reports_quota_exceeded() {
    assert_eq!(
        failed_call(509, "error_traffic_exhausted.json"),
        "ACCOUNT_QUOTA_EXCEEDED"
    );
}

#[test]
fn test_too_many_requests_reports_cooldown() {
    assert_eq!(
        failed_call(429, "error_too_many_requests.json"),
        "ACCOUNT_COOLDOWN"
    );
}

#[test]
fn test_a_failure_without_a_readable_body_still_reports_a_typed_error() {
    let response = HttpResponse {
        status: 403,
        headers: HashMap::new(),
        body: "<html>gateway</html>".into(),
    };

    let error = into_api_body(response).expect_err("403 is a failure");

    assert_eq!(error.code(), "ACCOUNT_INVALID_CREDENTIALS");
}

#[test]
fn test_a_2xx_response_yields_its_body_untouched() {
    let response = parse_http_response(
        &serde_json::json!({"status": 200, "headers": {}, "body": "{\"type\":\"premium\"}"})
            .to_string(),
    )
    .expect("envelope");

    assert_eq!(
        into_api_body(response).expect("2xx is success"),
        "{\"type\":\"premium\"}"
    );
}

#[test]
fn test_build_user_request_carries_the_bearer_token() {
    let request: serde_json::Value =
        serde_json::from_str(&build_user_request("t0k3n").expect("build")).expect("valid json");

    assert_eq!(request["method"], "GET");
    assert_eq!(request["url"], "https://api.real-debrid.com/rest/1.0/user");
    assert_eq!(request["headers"]["Authorization"], "Bearer t0k3n");
}

#[test]
fn test_build_unrestrict_request_posts_the_link_as_an_encoded_form_body() {
    let request: serde_json::Value = serde_json::from_str(
        &build_unrestrict_request("https://mediafire.com/file/a?b=1", "t0k3n").expect("build"),
    )
    .expect("valid json");

    assert_eq!(request["method"], "POST");
    assert_eq!(
        request["url"],
        "https://api.real-debrid.com/rest/1.0/unrestrict/link"
    );
    assert_eq!(
        request["headers"]["Content-Type"],
        "application/x-www-form-urlencoded"
    );
    assert_eq!(
        request["body"],
        "link=https%3A%2F%2Fmediafire.com%2Ffile%2Fa%3Fb%3D1"
    );
}

#[test]
fn test_the_api_token_never_appears_in_an_error_message() {
    // R-05: a leaked token in a log line is exactly what the keyring is for.
    let response = HttpResponse {
        status: 401,
        headers: HashMap::new(),
        body: fixture("error_bad_token.json"),
    };

    let error = into_api_body(response).expect_err("bad token");

    assert!(!error.to_string().contains("t0k3n"), "{error}");
}

#[test]
fn test_a_malformed_credential_payload_never_echoes_the_token() {
    // serde quotes the offending value on a type mismatch, and on this path
    // the offending value is the token itself (R-05).
    let error = parse_credential_response(r#"{"password": 90210}"#).expect_err("not a string");

    assert!(!error.to_string().contains("90210"), "{error}");
}
