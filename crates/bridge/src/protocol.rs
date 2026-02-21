use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
pub enum BridgeError {
    #[error("Bridge not registered")]
    NotRegistered,
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

#[tarpc::service]
pub trait BridgeService {
    async fn get_credentials(bridge_id: String) -> Result<Vec<u8>, BridgeError>;
}
