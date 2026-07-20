//! Plugin error type.

use thiserror::Error;

/// Errors raised by the Real-Debrid plugin.
#[derive(Debug, Error)]
pub enum PluginError {
    #[error("JSON error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    #[error("Real-Debrid HTTP returned status {status}: {message}")]
    HttpStatus { status: u16, message: String },

    #[error("host function response invalid: {0}")]
    HostResponse(String),

    #[error("URL is not on a hoster Real-Debrid unrestricts: {0}")]
    UnsupportedUrl(String),

    #[error("Real-Debrid rejected the configured API token as invalid")]
    InvalidCredentials,

    #[error("the Real-Debrid account is not premium: {0}")]
    AccountExpired(String),

    #[error("Real-Debrid asked us to slow down: {0}")]
    RateLimited(String),

    #[error("the Real-Debrid account has exhausted its quota: {0}")]
    QuotaExceeded(String),

    /// The debrid declines the link — dead, removed, or on a hoster it does
    /// not cover. The host reads this as `HosterNoFile` and moves the
    /// resolution cascade down to the next tier (R-04).
    #[error("Real-Debrid cannot serve this link: {0}")]
    HosterUnavailable(String),

    #[error("Real-Debrid returned an unexpected payload: {0}")]
    InvalidApiResponse(String),
}

impl PluginError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidCredentials => "ACCOUNT_INVALID_CREDENTIALS",
            Self::AccountExpired(_) => "ACCOUNT_EXPIRED",
            Self::RateLimited(_) => "ACCOUNT_COOLDOWN",
            Self::QuotaExceeded(_) => "ACCOUNT_QUOTA_EXCEEDED",
            Self::HosterUnavailable(_) => "HOSTER_NO_FILE",
            _ => "PLUGIN_ERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_errors_have_stable_machine_codes() {
        assert_eq!(
            PluginError::InvalidCredentials.code(),
            "ACCOUNT_INVALID_CREDENTIALS"
        );
        assert_eq!(
            PluginError::AccountExpired("free".into()).code(),
            "ACCOUNT_EXPIRED"
        );
        assert_eq!(
            PluginError::RateLimited("slow down".into()).code(),
            "ACCOUNT_COOLDOWN"
        );
        assert_eq!(
            PluginError::QuotaExceeded("traffic".into()).code(),
            "ACCOUNT_QUOTA_EXCEEDED"
        );
    }

    #[test]
    fn an_uncovered_hoster_reports_the_cascade_fall_through_code() {
        assert_eq!(
            PluginError::HosterUnavailable("unsupported hoster".into()).code(),
            "HOSTER_NO_FILE"
        );
    }
}
