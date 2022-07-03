// std
use std::io::{Error, ErrorKind};
use std::net::SocketAddr;

// third parties
use tokio::io;

use async_trait::async_trait;

#[async_trait]
pub trait DnsResolver {
    async fn resolve(&mut self, target: &str) -> io::Result<SocketAddr>;
}

#[derive(Clone)]
pub struct SimpleDnsResolver {}

#[async_trait]
impl DnsResolver for SimpleDnsResolver {
    // TODO: generic str param?
    async fn resolve(&mut self, target: &str) -> io::Result<SocketAddr> {
        let resolved: Vec<SocketAddr> = SimpleDnsResolver::resolve(target).await?;
        // Note: not sure if resolved can be an empty vec
        match resolved.get(0) {
            Some(r) => Ok(*r),
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData, "Empty resolve".to_string())),
        }
    }
}

impl SimpleDnsResolver {
    pub fn new() -> Self {
        Self {}
    }

    async fn resolve(target: &str) -> io::Result<Vec<SocketAddr>> {

        let resolved: Vec<_> = tokio::net::lookup_host(target).await?.collect();
        if resolved.is_empty() {
            return Err(Error::from(ErrorKind::AddrNotAvailable));
        }
        Ok(resolved)
    }
}