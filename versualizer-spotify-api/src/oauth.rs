use crate::error::SpotifyError;
use axum::{extract::Query, response::Html, routing::get, Router};
use rspotify::{prelude::*, scopes, AuthCodeSpotify, Credentials, OAuth, Token};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tracing::{debug, info, warn};

/// Timeout for interactive OAuth callback (10 minutes)
const OAUTH_CALLBACK_TIMEOUT_SECS: u64 = 600;

/// Refresh token proactively if it expires within this many seconds
const PROACTIVE_REFRESH_THRESHOLD_SECS: i64 = 60;

/// Persisted token data
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedToken {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<i64>, // Unix timestamp
    scopes: Vec<String>,
}

impl From<&Token> for PersistedToken {
    fn from(token: &Token) -> Self {
        Self {
            access_token: token.access_token.clone(),
            refresh_token: token.refresh_token.clone(),
            expires_at: token.expires_at.map(|d| d.timestamp()),
            scopes: token.scopes.iter().cloned().collect(),
        }
    }
}

impl TryFrom<PersistedToken> for Token {
    type Error = SpotifyError;

    fn try_from(persisted: PersistedToken) -> Result<Self, Self::Error> {
        Ok(Self {
            access_token: persisted.access_token,
            refresh_token: persisted.refresh_token,
            expires_at: persisted
                .expires_at
                .and_then(|ts| chrono::DateTime::from_timestamp(ts, 0)),
            expires_in: chrono::TimeDelta::zero(),
            scopes: persisted.scopes.into_iter().collect(),
        })
    }
}

/// Spotify OAuth manager
pub struct SpotifyOAuth {
    client: AuthCodeSpotify,
    token_path: PathBuf,
}

impl SpotifyOAuth {
    /// Create a new Spotify OAuth manager
    ///
    /// # Errors
    ///
    /// This function currently does not return errors but may in future versions.
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Result<Self, SpotifyError> {
        let creds = Credentials::new(&client_id.into(), &client_secret.into());

        let oauth = OAuth {
            redirect_uri: redirect_uri.into(),
            scopes: scopes!("user-read-currently-playing", "user-read-playback-state"),
            ..Default::default()
        };

        let client = AuthCodeSpotify::new(creds, oauth);

        let token_path = Self::token_path();

        Ok(Self { client, token_path })
    }

    /// Get the token file path (~/.`config/versualizer/.spotify_token_cache.json`)
    fn token_path() -> PathBuf {
        crate::paths::spotify_token_cache_path()
    }

    /// Acquire lock on token.
    ///
    /// # Errors
    ///
    /// Returns an error if the lock cannot be acquired.
    async fn lock_token(
        &self,
    ) -> Result<futures::lock::MutexGuard<'_, Option<Token>>, SpotifyError> {
        self.client
            .token
            .lock()
            .await
            .map_err(|_| SpotifyError::AuthFailed {
                reason: "Failed to acquire token lock".to_string(),
            })
    }

    /// Try to load cached token
    ///
    /// # Errors
    ///
    /// Returns an error if the token file cannot be read, parsed, or the token cannot be refreshed.
    pub async fn load_cached_token(&self) -> Result<bool, SpotifyError> {
        if !self.token_path.exists() {
            info!("No cached token file found at {:?}", self.token_path);
            return Ok(false);
        }

        let content = fs::read_to_string(&self.token_path)?;
        let persisted: PersistedToken = serde_json::from_str(&content)?;
        let token = Token::try_from(persisted)?;

        // Check if token is expired
        if token.is_expired() {
            if token.refresh_token.is_some() {
                info!("Cached token is expired but has refresh token, attempting refresh...");
                *self.lock_token().await? = Some(token);
                return self.refresh_token().await.map(|()| true);
            }
            info!("Cached token is expired and has no refresh token, re-authentication required");
            return Ok(false);
        }

        *self.lock_token().await? = Some(token);
        info!("Loaded valid cached Spotify token");
        Ok(true)
    }

    /// Save current token to file
    async fn save_token(&self) -> Result<(), SpotifyError> {
        let token_guard = self.lock_token().await?;
        if let Some(ref token) = *token_guard {
            let persisted = PersistedToken::from(token);

            // Ensure directory exists
            if let Some(parent) = self.token_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let content = serde_json::to_string_pretty(&persisted)?;
            fs::write(&self.token_path, content)?;
            debug!("Saved Spotify token to {:?}", self.token_path);
        }
        Ok(())
    }

    /// Refresh the access token
    ///
    /// # Errors
    ///
    /// Returns an error if the token refresh fails or the token cannot be saved.
    pub async fn refresh_token(&self) -> Result<(), SpotifyError> {
        info!("Refreshing Spotify access token");

        self.client
            .refresh_token()
            .await
            .map_err(|e| SpotifyError::AuthFailed {
                reason: format!("Token refresh failed: {e}"),
            })?;

        self.save_token().await?;
        Ok(())
    }

    /// Proactively refresh the token if it will expire soon (within 60 seconds).
    ///
    /// This should be called before making API requests to avoid authentication
    /// errors during the request.
    ///
    /// # Errors
    ///
    /// Returns an error if the token refresh fails.
    pub async fn ensure_token_fresh(&self) -> Result<(), SpotifyError> {
        let needs_refresh = {
            let token_guard = self.lock_token().await?;
            Self::check_needs_refresh(token_guard.as_ref())
        };

        if needs_refresh {
            self.refresh_token().await?;
        }

        Ok(())
    }

    /// Check if token needs refresh (expires within threshold or no token).
    fn check_needs_refresh(token_opt: Option<&Token>) -> bool {
        let Some(token) = token_opt else {
            warn!("No token available for proactive refresh check");
            return false;
        };

        let Some(expires_at) = token.expires_at else {
            // No expiration time, assume it's fine
            return false;
        };

        let now = chrono::Utc::now();
        let seconds_until_expiry = (expires_at - now).num_seconds();

        if seconds_until_expiry <= PROACTIVE_REFRESH_THRESHOLD_SECS {
            debug!(
                "Token expires in {}s (threshold: {}s), refreshing proactively",
                seconds_until_expiry, PROACTIVE_REFRESH_THRESHOLD_SECS
            );
            true
        } else {
            false
        }
    }

    /// Get the authorization URL for the user to visit
    ///
    /// # Errors
    ///
    /// Returns an error if the authorization URL cannot be generated.
    pub fn get_authorize_url(&self) -> Result<String, SpotifyError> {
        self.client
            .get_authorize_url(false)
            .map_err(|e| SpotifyError::AuthFailed {
                reason: format!("Failed to generate auth URL: {e}"),
            })
    }

    /// Handle the OAuth callback code
    ///
    /// # Errors
    ///
    /// Returns an error if the token exchange or save fails.
    pub async fn handle_callback(&self, code: &str) -> Result<(), SpotifyError> {
        self.client
            .request_token(code)
            .await
            .map_err(|e| SpotifyError::AuthFailed {
                reason: format!("Token exchange failed: {e}"),
            })?;

        self.save_token().await?;
        info!("Successfully authenticated with Spotify");
        Ok(())
    }

    /// Start the OAuth flow with a local HTTP server using axum
    ///
    /// # Errors
    ///
    /// Returns an error if the server cannot start, the browser cannot be opened, or authentication fails.
    pub async fn authenticate_interactive(&self) -> Result<(), SpotifyError> {
        let (host, port, callback_path) = self.parse_redirect_uri()?;

        // Create a oneshot channel to receive the auth code
        let (tx, rx) = oneshot::channel::<String>();
        let tx = Arc::new(tokio::sync::Mutex::new(Some(tx)));

        // Build router and start server
        let app = Self::build_callback_router(&callback_path, tx);
        let (listener, addr) = Self::start_callback_server(&host, port, &callback_path).await?;

        // Get auth URL and prompt user
        let auth_url = self.get_authorize_url()?;
        Self::prompt_authorization(&auth_url, addr, &callback_path);

        // Wait for callback
        let code = Self::wait_for_callback(rx, listener, app).await?;

        info!("Received authorization code, exchanging for token...");
        self.handle_callback(&code).await
    }

    /// Ensure we have a valid token, refreshing or re-authenticating if needed
    ///
    /// # Errors
    ///
    /// Returns an error if authentication or token refresh fails.
    pub async fn ensure_authenticated(&self) -> Result<(), SpotifyError> {
        info!("Checking for cached Spotify token...");

        // Try loading cached token
        if self.load_cached_token().await? {
            info!("Valid cached token found, skipping OAuth server");

            // Check if we need to refresh
            let needs_refresh = {
                let token_guard = self.lock_token().await?;
                token_guard.as_ref().is_none_or(rspotify::Token::is_expired)
            };

            if needs_refresh {
                info!("Token needs refresh, refreshing...");
                self.refresh_token().await?;
            }
            return Ok(());
        }

        // No valid cached token, need to authenticate
        info!("No valid cached token, starting OAuth server for interactive authentication...");
        self.authenticate_interactive().await
    }

    /// Clear cached tokens
    pub fn clear_tokens(&self) {
        if self.token_path.exists() {
            let _ = fs::remove_file(&self.token_path);
        }
    }

    /// Get the underlying Spotify client
    #[must_use]
    pub const fn client(&self) -> &AuthCodeSpotify {
        &self.client
    }

    /// Parse redirect URI components for OAuth callback server
    fn parse_redirect_uri(&self) -> Result<(String, u16, String), SpotifyError> {
        let redirect_uri = &self.client.oauth.redirect_uri;
        let parsed_uri = url::Url::parse(redirect_uri).map_err(|e| SpotifyError::AuthFailed {
            reason: format!("Invalid redirect URI: {e}"),
        })?;

        let host = parsed_uri.host_str().unwrap_or("localhost").to_string();
        let port = parsed_uri.port().unwrap_or(8888);
        let callback_path = parsed_uri.path().to_string();

        Ok((host, port, callback_path))
    }

    /// Build the OAuth callback router
    fn build_callback_router(
        callback_path: &str,
        tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<String>>>>,
    ) -> Router {
        Router::new().route(
            callback_path,
            get(move |Query(params): Query<CallbackParams>| {
                let tx = tx.clone();
                async move { Self::handle_callback_request(params, tx).await }
            }),
        )
    }

    /// Handle incoming OAuth callback request
    async fn handle_callback_request(
        params: CallbackParams,
        tx: Arc<tokio::sync::Mutex<Option<oneshot::Sender<String>>>>,
    ) -> Html<String> {
        if let Some(code) = params.code {
            let sender = tx.lock().await.take();
            if let Some(sender) = sender {
                let _ = sender.send(code);
            }
            Html(SUCCESS_HTML.to_string())
        } else if let Some(error) = params.error {
            Html(format!(
                r#"<!DOCTYPE html>
                <html>
                <head><title>Authorization Failed</title></head>
                <body style="font-family: sans-serif; text-align: center; padding: 50px;">
                    <h1>Authorization Failed</h1>
                    <p>Error: {error}</p>
                    <p>Please close this window and try again.</p>
                </body>
                </html>"#
            ))
        } else {
            Html(ERROR_NO_CODE_HTML.to_string())
        }
    }

    /// Start the callback server and bind to address
    async fn start_callback_server(
        host: &str,
        port: u16,
        callback_path: &str,
    ) -> Result<(tokio::net::TcpListener, SocketAddr), SpotifyError> {
        let addr: SocketAddr = format!(
            "{}:{}",
            if host == "localhost" {
                "127.0.0.1"
            } else {
                host
            },
            port
        )
        .parse()
        .map_err(|e| SpotifyError::AuthFailed {
            reason: format!("Invalid address: {e}"),
        })?;

        let listener =
            tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| SpotifyError::AuthFailed {
                    reason: format!("Failed to bind to {addr}: {e}"),
                })?;

        info!(
            "OAuth callback server listening on http://{}{}",
            addr, callback_path
        );
        Ok((listener, addr))
    }

    /// Display authorization prompt and open browser
    fn prompt_authorization(auth_url: &str, addr: SocketAddr, callback_path: &str) {
        info!("");
        info!("╔════════════════════════════════════════════════════════════════╗");
        info!("║                    Spotify Authorization                        ║");
        info!("╠════════════════════════════════════════════════════════════════╣");
        info!("║ Opening browser for authorization...                           ║");
        info!("╚════════════════════════════════════════════════════════════════╝");
        info!("");

        if let Err(e) = open::that(auth_url) {
            warn!("Could not open browser automatically: {}", e);
            info!("Please open this URL manually:\n{auth_url}");
        }

        info!(
            "Waiting for authorization callback on http://{}{callback_path}...",
            addr
        );
    }

    /// Wait for OAuth callback with timeout
    async fn wait_for_callback(
        rx: oneshot::Receiver<String>,
        listener: tokio::net::TcpListener,
        app: Router,
    ) -> Result<String, SpotifyError> {
        let server = axum::serve(listener, app);

        tokio::select! {
            result = rx => {
                result.map_err(|_| SpotifyError::AuthFailed {
                    reason: "Callback channel closed unexpectedly".into(),
                })
            }
            _ = server => {
                Err(SpotifyError::AuthFailed {
                    reason: "Server stopped unexpectedly".into(),
                })
            }
            () = tokio::time::sleep(Duration::from_secs(OAUTH_CALLBACK_TIMEOUT_SECS)) => {
                Err(SpotifyError::AuthFailed {
                    reason: format!(
                        "OAuth callback timed out after {} minutes. Please try again.",
                        OAUTH_CALLBACK_TIMEOUT_SECS / 60
                    ),
                })
            }
        }
    }
}

/// Query parameters for the OAuth callback
#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: Option<String>,
    error: Option<String>,
}

/// HTML response for authorization error (no code received)
const ERROR_NO_CODE_HTML: &str = r#"<!DOCTYPE html>
<html>
<head><title>Authorization Failed</title></head>
<body style="font-family: sans-serif; text-align: center; padding: 50px;">
    <h1>Authorization Failed</h1>
    <p>No authorization code received.</p>
    <p>Please close this window and try again.</p>
</body>
</html>"#;

/// HTML response shown on successful authorization
const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <title>Authorization Successful</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            text-align: center;
            padding: 50px;
            background: linear-gradient(135deg, #1DB954 0%, #191414 100%);
            color: white;
            min-height: 100vh;
            margin: 0;
            display: flex;
            flex-direction: column;
            justify-content: center;
            align-items: center;
        }
        .container {
            background: rgba(0, 0, 0, 0.3);
            padding: 40px 60px;
            border-radius: 16px;
            backdrop-filter: blur(10px);
        }
        h1 { font-size: 2.5em; margin-bottom: 10px; }
        p { font-size: 1.2em; opacity: 0.9; }
        .checkmark {
            font-size: 4em;
            margin-bottom: 20px;
        }
    </style>
</head>
<body>
    <div class="container">
        <div class="checkmark">✓</div>
        <h1>Authorization Successful!</h1>
        <p>Versualizer is now connected to Spotify.</p>
        <p>You can close this window.</p>
    </div>
</body>
</html>"#;
