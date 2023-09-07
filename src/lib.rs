use futures::{SinkExt, StreamExt};
use rustls::Certificate;

use crate::proto::*;

mod net;
mod proto;
mod root_certs;
mod serialization;
mod size_histogram;
mod stream;

pub const LOCALHOST_CERT: &[u8] = include_bytes!("../localhost.crt");

pub async fn run(mut host: String) -> anyhow::Result<()> {
    let user_id = format!("dummy-user-{}", uuid::Uuid::new_v4());

    if host.starts_with("http://") || host.starts_with("https://") {
        println!("Resolving host...");
        host = reqwest::get(host).await?.text().await?;

        if host.is_empty() {
            anyhow::bail!("Failed to resolve host");
        }
    }
    if !host.contains(':') {
        host = format!("{host}:9000");
    }
    let server_addr = net::ResolvedAddr::lookup_host(&host).await?;

    println!("Connecting to {}...", server_addr.addr);
    let endpoint = net::create_client_endpoint_random_port(Some(Certificate(LOCALHOST_CERT.to_vec())))?;
    let conn = endpoint
        .connect(server_addr.addr, &server_addr.host_name)?
        .await?;

    let mut request_send = stream::FramedSendStream::new(conn.open_uni().await?);
    request_send
        .send(ClientRequest::Connect(user_id.clone()))
        .await?;
    let mut push_recv = stream::FramedRecvStream::new(conn.accept_uni().await?);
    while let Some(request) = push_recv.next().await {
        match request? {
            ServerPush::ServerInfo(info) => {
                println!("Received info: {:?}", info);
                break;
            }
            ServerPush::Disconnect => {
                println!("Received disconnect");
                return Ok(());
            }
        }
    }

    // keep handling server push messages
    let server_push_handle = tokio::spawn(async move {
        while let Some(request) = push_recv.next().await {
            match request {
                Ok(ServerPush::ServerInfo(info)) => {
                    println!("Received server info (again): {:?}", info);
                }
                Ok(ServerPush::Disconnect) => {
                    println!("Received disconnect");
                    break;
                }
                Err(e) => {
                    println!("Error from server push stream: {:?}", e);
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
                        println!("Received {} diffs", size_stats.len());
                        println!("Size stats:\n{}", size_stats);
                    }
                }
                Err(e) => {
                    println!("Error: {:?}", e);
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

    println!("Disconnecting...");
    request_send
        .send(ClientRequest::Disconnect)
        .await?;
    <stream::FramedSendStream<ClientRequest, _> as SinkExt<ClientRequest>>::flush(&mut request_send).await?;
    <stream::FramedSendStream<ClientRequest, _> as SinkExt<ClientRequest>>::close(&mut request_send).await?;

    Ok(())
}
