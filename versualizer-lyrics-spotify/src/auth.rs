//! Authentication types and secret key fetching for Spotify TOTP authentication.

use serde::Deserialize;
use std::collections::HashMap;
use std::time::Instant;
use thiserror::Error;

/// Authentication errors for Spotify TOTP flow
#[derive(Debug, Error)]
pub enum SpotifyAuthError {
    /// Failed to fetch server time from Spotify
    #[error("Failed to fetch server time: {0}")]
    ServerTimeFailed(String),

    /// Failed to fetch secret key from remote URL
    #[error("Failed to fetch secret key: {0}")]
    SecretKeyFailed(String),

    /// Failed to decode the secret key
    #[error("Failed to decode secret key: no valid versions found")]
    SecretDecodeError,

    /// Failed to get access token from Spotify
    #[error("Failed to get access token: {0}")]
    TokenFetchFailed(String),

    /// `sp_dc` cookie is invalid or expired
    #[error("sp_dc cookie is invalid or expired")]
    SpDcInvalid,

    /// Network error during authentication
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
}

/// Cached access token with expiration tracking
#[derive(Debug, Clone)]
pub struct CachedAccessToken {
    /// The Bearer token for API requests
    pub access_token: String,
    /// When this token expires (milliseconds since Unix epoch)
    pub expires_at_ms: u64,
    /// Local timestamp when the token was fetched (for relative expiration checking)
    pub fetched_at: Instant,
    /// System time when fetched (milliseconds since Unix epoch)
    pub fetched_at_system_ms: u64,
}

impl CachedAccessToken {
    /// Check if token is expired or will expire within the buffer time.
    ///
    /// Uses relative timing from when the token was fetched to avoid system clock issues.
    #[must_use]
    pub fn is_expired(&self, buffer_secs: u64) -> bool {
        let elapsed_ms = u64::try_from(self.fetched_at.elapsed().as_millis()).unwrap_or(u64::MAX);
        let current_time_ms = self.fetched_at_system_ms.saturating_add(elapsed_ms);
        let buffer_ms = buffer_secs.saturating_mul(1000);

        // Token is expired if current time + buffer exceeds expiration
        current_time_ms.saturating_add(buffer_ms) >= self.expires_at_ms
    }
}

/// Cached secret key for TOTP generation
#[derive(Debug, Clone)]
pub struct CachedSecret {
    /// Decoded secret bytes
    pub secret: Vec<u8>,
    /// Version string (e.g., "61")
    pub version: String,
    /// When this was fetched
    pub fetched_at: Instant,
}

impl CachedSecret {
    /// Check if secret cache should be refreshed.
    #[must_use]
    pub fn should_refresh(&self, max_age: std::time::Duration) -> bool {
        self.fetched_at.elapsed() > max_age
    }
}

/// Response from Spotify server time endpoint
#[derive(Debug, Deserialize)]
pub struct ServerTimeResponse {
    /// Server time in seconds since Unix epoch
    #[serde(rename = "serverTime")]
    pub server_time: u64,
}

/// Response from Spotify token endpoint
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    /// The access token for API requests
    #[serde(rename = "accessToken")]
    pub access_token: String,

    /// Token expiration timestamp in milliseconds since Unix epoch
    #[serde(rename = "accessTokenExpirationTimestampMs")]
    pub access_token_expiration_timestamp_ms: u64,

    /// Whether this is an anonymous token (indicates invalid `sp_dc`)
    #[serde(rename = "isAnonymous", default)]
    pub is_anonymous: bool,
}

/// Fetch and decode the latest secret key from the configured URL.
///
/// The secret dictionary is JSON with version keys mapping to byte arrays.
/// We take the latest version (highest numeric key) and decode it using XOR transformation:
/// `decoded[i] = original[i] ^ ((i % 33) + 9)`
///
/// # Errors
///
/// Returns [`SpotifyAuthError::SecretKeyFailed`] if the network request fails or JSON is invalid.
/// Returns [`SpotifyAuthError::SecretDecodeError`] if no valid versions are found.
pub async fn fetch_secret_key(
    client: &reqwest::Client,
    secret_key_url: &str,
) -> Result<CachedSecret, SpotifyAuthError> {
    // Fetch the secret dictionary
    let response = client
        .get(secret_key_url)
        .send()
        .await?
        .error_for_status()
        .map_err(|e| SpotifyAuthError::SecretKeyFailed(e.to_string()))?;

    let secrets: HashMap<String, Vec<u8>> = response
        .json()
        .await
        .map_err(|e| SpotifyAuthError::SecretKeyFailed(e.to_string()))?;

    // Get the latest version (highest numeric key)
    let (version, original_secret) = secrets
        .into_iter()
        .filter_map(|(k, v)| k.parse::<u64>().ok().map(|n| (n, k, v)))
        .max_by_key(|(n, _, _)| *n)
        .map(|(_, k, v)| (k, v))
        .ok_or(SpotifyAuthError::SecretDecodeError)?;

    // Decode: secret[i] ^ ((i % 33) + 9)
    // Then convert to string like PHP's implode('', $transformed)
    // PHP's implode on integers gives their decimal string representation
    // e.g., [65, 66, 67] -> "656667" (not "ABC")
    let decoded_string: String = original_secret
        .into_iter()
        .enumerate()
        .map(|(i, byte)| {
            let xor_key = u8::try_from((i % 33) + 9).unwrap_or(0);
            let decoded = byte ^ xor_key;
            decoded.to_string()
        })
        .collect();

    Ok(CachedSecret {
        secret: decoded_string.into_bytes(),
        version,
        fetched_at: Instant::now(),
    })
}
