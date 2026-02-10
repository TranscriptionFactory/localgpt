// src/server/auth.rs
//
// Bearer token authentication for the HTTP API.
// Generates a random token on first run, stored at ~/.localgpt/.api_token (0600).

use anyhow::Result;
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use base64::Engine;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Generate or load the API token from the state directory.
/// Creates a new random 32-byte token if none exists.
pub fn ensure_api_token(state_dir: &Path) -> Result<String> {
    let token_path = api_token_path(state_dir);

    if token_path.exists() {
        let token = std::fs::read_to_string(&token_path)?.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    // Generate new token
    let mut bytes = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);

    // Write with restrictive permissions
    std::fs::write(&token_path, &token)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token_path, std::fs::Permissions::from_mode(0o600))?;
    }

    info!("API token generated at: {}", token_path.display());
    Ok(token)
}

/// Get the path to the API token file.
pub fn api_token_path(state_dir: &Path) -> PathBuf {
    state_dir.join(".api_token")
}

/// Axum middleware that validates Bearer token on /api/* routes.
/// Skips /health and non-API routes.
pub async fn auth_middleware(
    State(state): State<Arc<super::http::AuthState>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let path = request.uri().path();

    // Skip auth for health check and non-API routes
    if path == "/health" || !path.starts_with("/api/") {
        return Ok(next.run(request).await);
    }

    // Skip auth if disabled
    if !state.require_auth {
        return Ok(next.run(request).await);
    }

    // Extract Bearer token
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    match auth_header {
        Some(header) if header.starts_with("Bearer ") => {
            let token = &header[7..];
            if token == state.api_token {
                Ok(next.run(request).await)
            } else {
                Err(StatusCode::UNAUTHORIZED)
            }
        }
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
