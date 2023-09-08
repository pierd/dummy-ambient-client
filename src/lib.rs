use futures::{SinkExt, StreamExt};
use rustls::Certificate;

use crate::proto::*;

pub mod logging;
pub mod net;
mod proto;
mod root_certs;
pub mod serialization;
pub mod size_histogram;
pub mod stream;

pub const LOCALHOST_CERT: &[u8] = include_bytes!("../localhost.crt");

pub async fn run(mut host: String) -> anyhow::Result<()> {
    let user_id = format!("dummy-user-{}", uuid::Uuid::new_v4());

    if host.starts_with("http://") || host.starts_with("https://") {
        tracing::info!("Resolving host...");
        host = reqwest::get(host).await?.text().await?;

        if host.is_empty() {
            anyhow::bail!("Failed to resolve host");
        }
    }
    if !host.contains(':') {
        host = format!("{host}:9000");
    }
    let server_addr = net::ResolvedAddr::lookup_host(&host).await?;

    tracing::info!(?server_addr, "Connecting...");
    let endpoint =
        net::create_client_endpoint_random_port(Some(Certificate(LOCALHOST_CERT.to_vec())))?;
    let conn = endpoint
        .connect(server_addr.addr, &server_addr.host_name)?
        .await?;

    let mut request_send = stream::FramedSendStream::new(conn.open_uni().await?);
    request_send
        .send(ClientRequest::Connect(user_id.clone()))
        .await?;
    let mut push_recv = stream::FramedRecvStream::new(conn.accept_uni().await?);
    if let Some(request) = push_recv.next().await {
        match request? {
            ServerPush::ServerInfo(info) => {
                tracing::info!("Received info: {:?}", info);
            }
            ServerPush::Disconnect => {
                tracing::info!("Received disconnect");
                return Ok(());
            }
        }
    }

    // keep handling server push messages
    let server_push_handle = tokio::spawn(async move {
        while let Some(request) = push_recv.next().await {
            match request {
                Ok(ServerPush::ServerInfo(info)) => {
                    tracing::warn!("Received server info (again): {:?}", info);
                }
                Ok(ServerPush::Disconnect) => {
                    tracing::info!("Received disconnect");
                    break;
                }
                Err(e) => {
                    tracing::error!("Error from server push stream: {:?}", e);
                    break;
                }
            }
        }
    });

    // open and handle diff stream
    let mut diff_stream = stream::RawFramedRecvStream::new(conn.accept_uni().await?);
    let diff_stream_handle = tokio::spawn(async move {
        let mut size_stats = size_histogram::SizeHistogram::default();
        while let Some(data) = diff_stream.next().await {
            match data {
                Ok(data) => {
                    size_stats.incr(data.len());
                    if size_stats.len() % 300 == 0 {
                        tracing::info!("Received {} diffs", size_stats.len());
                        tracing::debug!("Size stats:\n{}", size_stats);
                    }
                }
                Err(e) => {
                    tracing::warn!("Error: {:?}", e);
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = server_push_handle => {}
        _ = diff_stream_handle => {}
    }

    tracing::info!("Disconnecting...");
    request_send.send(ClientRequest::Disconnect).await?;
    <stream::FramedSendStream<ClientRequest, _> as SinkExt<ClientRequest>>::flush(
        &mut request_send,
    )
    .await?;
    <stream::FramedSendStream<ClientRequest, _> as SinkExt<ClientRequest>>::close(
        &mut request_send,
    )
    .await?;

    Ok(())
}
