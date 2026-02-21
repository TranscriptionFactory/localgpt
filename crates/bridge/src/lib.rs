pub mod protocol;
pub mod peer_identity;

use interprocess::local_socket::tokio::{LocalSocketListener, LocalSocketStream};
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::peer_identity::{PeerIdentity, get_peer_identity};

// Re-export protocol
pub use protocol::{BridgeService, BridgeServiceClient};

pub struct BridgeServer;

impl BridgeServer {
    pub async fn listen(socket_name: &str) -> std::io::Result<()> {
        let listener = LocalSocketListener::bind(socket_name)?;

        loop {
            let conn = listener.accept().await?;
            
            // Verify peer identity
            #[cfg(unix)]
            let identity = get_peer_identity(&conn)?;
            
            #[cfg(windows)]
            let identity = get_peer_identity(&conn)?;
            
            tracing::info!("Accepted connection from: {:?}", identity);
            
            // TODO: Spawn service
            // This requires the service implementation to be passed in.
            // For now, we just verify identity.
        }
        // unreachable
    }
}

pub async fn connect(socket_name: &str) -> anyhow::Result<BridgeServiceClient> {
    let conn = LocalSocketStream::connect(socket_name).await?;
    
    // interprocess 1.2.1 impls futures::io traits, not tokio::io.
    // Wrap with tokio-util compat.
    use tokio_util::compat::FuturesAsyncReadCompatExt;
    let conn = conn.compat();
    
    use tokio_serde::formats::Json;
    use tarpc::tokio_util::codec::{Framed, LengthDelimitedCodec};

    let transport = tarpc::serde_transport::new(
        Framed::new(conn, LengthDelimitedCodec::new()),
        Json::default(),
    );
    
    let client = BridgeServiceClient::new(tarpc::client::Config::default(), transport).spawn();
    Ok(client)
}
