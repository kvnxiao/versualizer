//! Spotify provider configuration.

use const_format::concatcp;
use serde::{Deserialize, Serialize};
use versualizer_core::{CoreError, ProvidersConfig};

/// Provider name used in config file
pub const PROVIDER_NAME: &str = "spotify";

/// Default URL for fetching Spotify TOTP secret keys
pub const DEFAULT_SECRET_KEY_URL: &str =
    "https://raw.githubusercontent.com/xyloflake/spot-secrets-go/refs/heads/main/secrets/secretDict.json";

/// Spotify-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotifyProviderConfig {
    /// Spotify OAuth client ID
    pub client_id: String,
    /// Spotify OAuth client secret
    pub client_secret: String,
    /// OAuth redirect URI
    #[serde(default = "default_redirect_uri")]
    pub oauth_redirect_uri: String,
    /// Polling interval in milliseconds
    #[serde(default = "default_poll_interval")]
    pub poll_interval_ms: u64,
    /// Optional: For unofficial Spotify lyrics API (use at your own risk)
    pub sp_dc: Option<String>,
    /// Optional: URL for fetching Spotify TOTP secret keys
    #[serde(default)]
    pub secret_key_url: Option<String>,
}

fn default_redirect_uri() -> String {
    "http://127.0.0.1:8888/callback".into()
}

const fn default_poll_interval() -> u64 {
    1000
}

impl SpotifyProviderConfig {
    /// Extract Spotify config from the dynamic providers config.
    ///
    /// # Errors
    ///
    /// Returns an error if the config cannot be parsed.
    pub fn from_providers(providers: &ProvidersConfig) -> Result<Option<Self>, CoreError> {
        providers.get(PROVIDER_NAME)
    }

    /// Validate that required fields are present.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing or empty.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.client_id.is_empty() {
            return Err(CoreError::ConfigMissingField {
                field: "providers.spotify.client_id".into(),
            });
        }
        if self.client_secret.is_empty() {
            return Err(CoreError::ConfigMissingField {
                field: "providers.spotify.client_secret".into(),
            });
        }
        Ok(())
    }
}

/// Config template for Spotify provider.
/// This is appended to the base config template when creating a new config file.
pub const CONFIG_TEMPLATE: &str = concatcp!(
    r#"[providers.spotify]
# Required when music.source = "spotify"
# Get these from https://developer.spotify.com/dashboard
client_id = ""
client_secret = ""
oauth_redirect_uri = "http://127.0.0.1:8888/callback"
poll_interval_ms = 1000
# Optional: For unofficial Spotify lyrics API (use at your own risk - may violate TOS)
# sp_dc = ""
# Optional: URL for fetching TOTP secret keys
# secret_key_url = ""#,
    DEFAULT_SECRET_KEY_URL,
    "\"\n\n"
);
