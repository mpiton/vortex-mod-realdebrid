//! Typed views over the two Real-Debrid payloads this plugin consumes.

use serde::Deserialize;

use crate::error::PluginError;
use crate::expiry::parse_iso8601_utc;

/// What `validate_account` reports back to the Accounts view (R-01).
///
/// Real-Debrid premium carries no scalar byte quota — the per-hoster limits
/// live behind `/traffic` as a map — so only the expiry is surfaced.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountStatus {
    /// Unix timestamp, in seconds, when premium lapses.
    pub valid_until: Option<u64>,
}

/// A hoster link Real-Debrid has turned into a direct CDN URL (R-02).
#[derive(Debug, PartialEq, Eq)]
pub struct UnlockedLink {
    pub direct_url: String,
    pub filename: Option<String>,
    pub size_bytes: Option<u64>,
    pub resumable: bool,
}

#[derive(Debug, Deserialize)]
struct User {
    #[serde(default, rename = "type")]
    account_type: String,
    #[serde(default)]
    expiration: String,
}

#[derive(Debug, Deserialize)]
struct Unrestricted {
    #[serde(default)]
    download: String,
    #[serde(default)]
    filename: Option<String>,
    #[serde(default)]
    filesize: Option<u64>,
    #[serde(default)]
    chunks: u32,
}

pub fn parse_user_response(body: &str) -> Result<AccountStatus, PluginError> {
    let user: User = deserialize(body)?;
    if !user.account_type.eq_ignore_ascii_case("premium") {
        return Err(PluginError::AccountExpired(format!(
            "Real-Debrid reports account type '{}'",
            user.account_type
        )));
    }
    Ok(AccountStatus {
        valid_until: parse_iso8601_utc(&user.expiration),
    })
}

pub fn parse_unrestrict_response(body: &str) -> Result<UnlockedLink, PluginError> {
    // A multi-file hoster (YouTube-style) answers with an array; the host
    // only ever downloads one file per link, so the first entry wins.
    let first = match serde_json::from_str::<Vec<serde_json::Value>>(body) {
        Ok(entries) => match entries.into_iter().next() {
            Some(entry) => entry.to_string(),
            None => {
                return Err(PluginError::HosterUnavailable(
                    "unrestrict returned no files".into(),
                ))
            }
        },
        Err(_) => body.to_string(),
    };

    let unrestricted: Unrestricted = deserialize(&first)?;
    // Returning an entry with no direct URL would be the faux succès R-04
    // forbids, so an empty `download` is a hard failure.
    if unrestricted.download.trim().is_empty() {
        return Err(PluginError::InvalidApiResponse(
            "unrestrict succeeded without a download link".into(),
        ));
    }
    Ok(UnlockedLink {
        direct_url: unrestricted.download,
        filename: unrestricted.filename.filter(|name| !name.trim().is_empty()),
        size_bytes: unrestricted.filesize.filter(|size| *size > 0),
        // Real-Debrid advertises how many parallel chunks a link tolerates;
        // more than one means it honours range requests.
        resumable: unrestricted.chunks > 1,
    })
}

fn deserialize<T: serde::de::DeserializeOwned>(body: &str) -> Result<T, PluginError> {
    serde_json::from_str(body)
        .map_err(|e| PluginError::InvalidApiResponse(format!("unexpected payload shape: {e}")))
}
