use std::{net::{SocketAddr, IpAddr, Ipv4Addr}, sync::Arc};
use quinn::{Endpoint, TransportConfig, ClientConfig};
use rand::Rng;
use rustls::Certificate;
use tokio::net::ToSocketAddrs;
use anyhow::Context;
use std::time::Duration;

use crate::root_certs::load_root_certs;

#[derive(Debug, Clone)]
pub struct ResolvedAddr {
    pub host_name: String,
    pub addr: SocketAddr,
}

impl ResolvedAddr {
    pub async fn lookup_host<T: ToSocketAddrs + ToString + Clone>(host: T) -> anyhow::Result<Self> {
        let addr = tokio::net::lookup_host(host.clone())
            .await?
            .find(SocketAddr::is_ipv4)
            .ok_or_else(|| anyhow::anyhow!("No IPv4 addresses found for: {}", host.to_string()))?;
        let host = host.to_string();
        let host_name = if host.contains(':') {
            host.split(':').next().unwrap().to_string()
        } else {
            host
        };
        Ok(Self { host_name, addr })
    }

    pub fn localhost_with_port(port: u16) -> Self {
        Self {
            host_name: "localhost".into(),
            addr: ([127, 0, 0, 1], port).into(),
        }
    }
}

pub fn create_client_endpoint_random_port(cert: Option<Certificate>) -> anyhow::Result<Endpoint> {
    let mut roots = load_root_certs();

    if let Some(cert) = cert {
        roots
            .add(&cert)
            .context("Failed to add custom certificate")?;
    }

    for _ in 0..10 {
        let client_port = {
            let mut rng = rand::thread_rng();
            rng.gen_range(15000..25000)
        };

        let client_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), client_port);

        if let Ok(mut endpoint) = Endpoint::client(client_addr) {
            let mut tls_config = rustls::ClientConfig::builder()
                .with_safe_default_cipher_suites()
                .with_safe_default_kx_groups()
                .with_protocol_versions(&[&rustls::version::TLS13])
                .unwrap()
                .with_root_certificates(roots)
                .with_no_client_auth();

            // tls_config.enable_early_data = true;
            tls_config.alpn_protocols = vec!["ambient-02".into()];

            let mut transport = TransportConfig::default();
            transport.keep_alive_interval(Some(Duration::from_secs_f32(1.)));

            if std::env::var("AMBIENT_DISABLE_TIMEOUT").is_ok() {
                transport.max_idle_timeout(None);
            } else {
                transport.max_idle_timeout(Some(Duration::from_secs_f32(60.).try_into().unwrap()));
            }
            let mut client_config = ClientConfig::new(Arc::new(tls_config));
            client_config.transport_config(Arc::new(transport));

            endpoint.set_default_client_config(client_config);
            return Ok(endpoint);
        }
    }

    Err(anyhow::anyhow!(
        "Failed to find appropriate port for client endpoint"
    ))
}
