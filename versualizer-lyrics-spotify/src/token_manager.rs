//! Token lifecycle management for Spotify TOTP authentication.
//!
//! This module handles the complete authentication flow:
//! 1. Fetch server time from Spotify
//! 2. Fetch and decode secret key (with caching)
//! 3. Generate TOTP code
//! 4. Exchange `sp_dc` + TOTP for access token
//! 5. Cache and refresh access tokens

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::auth::{
    fetch_secret_key, CachedAccessToken, CachedSecret, ServerTimeResponse, SpotifyAuthError,
    TokenResponse,
};
use crate::totp::generate_totp;

/// URL for fetching server time from Spotify
const SERVER_TIME_URL: &str = "https://open.spotify.com/api/server-time";

/// URL for fetching access token from Spotify
const TOKEN_URL: &str = "https://open.spotify.com/api/token";

/// Maximum age for cached secret key (24 hours)
const SECRET_CACHE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Buffer time before token expiration to trigger refresh (60 seconds)
const TOKEN_REFRESH_BUFFER_SECS: u64 = 60;

/// User agent for requests
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Manages Spotify access token lifecycle with TOTP authentication.
///
/// This manager handles:
/// - Caching and refreshing access tokens
/// - Caching secret keys (refreshed every 24 hours)
/// - Generating TOTP codes for authentication
pub struct SpotifyTokenManager {
    sp_dc: String,
    secret_key_url: String,
    client: reqwest::Client,
    cached_token: Arc<RwLock<Option<CachedAccessToken>>>,
    cached_secret: Arc<RwLock<Option<CachedSecret>>>,
}

impl SpotifyTokenManager {
    /// Create a new token manager.
    ///
    /// # Arguments
    ///
    /// * `sp_dc` - The Spotify `sp_dc` cookie value
    /// * `secret_key_url` - URL to fetch the secret key dictionary from
    /// * `client` - HTTP client for making requests
    #[must_use]
    pub fn new(sp_dc: impl Into<String>, secret_key_url: impl Into<String>, client: reqwest::Client) -> Self {
        Self {
            sp_dc: sp_dc.into(),
            secret_key_url: secret_key_url.into(),
            client,
            cached_token: Arc::new(RwLock::new(None)),
            cached_secret: Arc::new(RwLock::new(None)),
        }
    }

    /// Get a valid access token, refreshing if necessary.
    ///
    /// # Errors
    ///
    /// Returns [`SpotifyAuthError`] if authentication fails.
    pub async fn get_access_token(&self) -> Result<String, SpotifyAuthError> {
        // Fast path: check if we have a valid cached token
        {
            let token_guard = self.cached_token.read().await;
            if let Some(ref token) = *token_guard {
                if !token.is_expired(TOKEN_REFRESH_BUFFER_SECS) {
                    debug!("Using cached Spotify access token");
                    return Ok(token.access_token.clone());
                }
                debug!("Cached token is expired or expiring soon");
            }
        }

        // Slow path: need to refresh token
        self.refresh_token().await
    }

    /// Force refresh the access token.
    async fn refresh_token(&self) -> Result<String, SpotifyAuthError> {
        info!("Refreshing Spotify access token via TOTP");

        // Step 1: Ensure we have a valid secret key
        let secret = self.ensure_secret().await?;
        debug!("Using secret key version: {}", secret.version);

        // Step 2: Get server time
        let server_time = self.fetch_server_time().await?;
        debug!("Spotify server time: {server_time}");

        // Step 3: Convert server time to milliseconds for ts parameter
        // Python syrics uses: server_time_ms = server_time_seconds * 1000
        let server_time_ms = server_time.saturating_mul(1000);

        // Step 4: Generate TOTP using server time in seconds
        let totp_code = generate_totp(&secret.secret, server_time)
            .map_err(|e| SpotifyAuthError::TokenFetchFailed(e.to_string()))?;
        debug!(
            "Generated TOTP code: {} (secret len: {}, version: {}, server_time: {})",
            totp_code,
            secret.secret.len(),
            secret.version,
            server_time
        );

        // Step 5: Fetch access token with server time in milliseconds as ts
        let token = self
            .fetch_access_token(&totp_code, &secret.version, server_time_ms)
            .await?;

        // Step 5: Cache the token
        let access_token = token.access_token.clone();
        {
            let mut token_guard = self.cached_token.write().await;
            *token_guard = Some(token);
        }

        info!("Successfully obtained Spotify access token");
        Ok(access_token)
    }

    /// Ensure we have a valid (non-stale) secret key.
    async fn ensure_secret(&self) -> Result<CachedSecret, SpotifyAuthError> {
        // Check if we have a valid cached secret
        {
            let secret_guard = self.cached_secret.read().await;
            if let Some(ref secret) = *secret_guard {
                if !secret.should_refresh(SECRET_CACHE_MAX_AGE) {
                    return Ok(secret.clone());
                }
                debug!("Secret key cache is stale, refreshing");
            }
        }

        // Need to fetch new secret
        info!("Fetching secret key from: {}", self.secret_key_url);
        let secret = fetch_secret_key(&self.client, &self.secret_key_url).await?;
        info!("Fetched secret key version: {}", secret.version);

        // Cache it
        {
            let mut secret_guard = self.cached_secret.write().await;
            *secret_guard = Some(secret.clone());
        }

        Ok(secret)
    }

    /// Fetch server time from Spotify.
    async fn fetch_server_time(&self) -> Result<u64, SpotifyAuthError> {
        let response: ServerTimeResponse = self
            .client
            .get(SERVER_TIME_URL)
            .header("User-Agent", USER_AGENT)
            .send()
            .await?
            .error_for_status()
            .map_err(|e| SpotifyAuthError::ServerTimeFailed(e.to_string()))?
            .json()
            .await
            .map_err(|e| SpotifyAuthError::ServerTimeFailed(e.to_string()))?;

        Ok(response.server_time)
    }

    /// Fetch access token using TOTP.
    async fn fetch_access_token(
        &self,
        totp: &str,
        version: &str,
        timestamp: u64,
    ) -> Result<CachedAccessToken, SpotifyAuthError> {
        let url = format!(
            "{TOKEN_URL}?reason=init&productType=web-player&totp={totp}&totpVer={version}&ts={timestamp}"
        );
        debug!("Token request URL: {}", url);

        let response = self
            .client
            .get(&url)
            .header("Cookie", format!("sp_dc={}", self.sp_dc))
            .header("User-Agent", USER_AGENT)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            warn!("Token request failed: HTTP {} - {}", status, body);
            return Err(SpotifyAuthError::TokenFetchFailed(format!("HTTP {status}")));
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .map_err(|e| SpotifyAuthError::TokenFetchFailed(e.to_string()))?;

        // Check if token is anonymous (indicates invalid sp_dc)
        if token_response.is_anonymous {
            warn!("Received anonymous token - sp_dc cookie is invalid or expired");
            return Err(SpotifyAuthError::SpDcInvalid);
        }

        // Get current system time for relative expiration tracking
        #[allow(clippy::cast_possible_truncation)] // ms since epoch won't exceed u64 for centuries
        let fetched_at_system_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(CachedAccessToken {
            access_token: token_response.access_token,
            expires_at_ms: token_response.access_token_expiration_timestamp_ms,
            fetched_at: Instant::now(),
            fetched_at_system_ms,
        })
    }

    /// Invalidate the cached token, forcing a refresh on next request.
    pub async fn invalidate_token(&self) {
        *self.cached_token.write().await = None;
        debug!("Invalidated cached Spotify access token");
    }
}
