// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
pub use tokio::net::tcp::{ReadHalf, WriteHalf};

use super::tls::{TlsRead, TlsStream, TlsWrite};
use tracing::info;

use super::ServerAddr;

pub enum NetStream {
    Tcp(TcpStream),
    Tls(TlsStream),
    #[cfg(feature = "proxy")]
    ProxySocks5(tokio_socks::tcp::Socks5Stream<TcpStream>),
}

impl NetStream {
    pub(crate) fn split(&mut self) -> (NetReader<'_>, NetWriter<'_>) {
        match self {
            Self::Tcp(stream) => {
                let (r, w) = stream.split();
                (NetReader::Plain(r), NetWriter::Plain(w))
            }
            #[cfg(feature = "proxy")]
            Self::ProxySocks5(stream) => {
                let (r, w) = stream.split();
                (NetReader::Plain(r), NetWriter::Plain(w))
            }
            Self::Tls(stream) => {
                let (r, w) = stream.split();
                (NetReader::Tls(r), NetWriter::Tls(w))
            }
        }
    }

    pub(crate) async fn connect(addr: &ServerAddr) -> Result<Self, std::io::Error> {
        info!("connecting...");
        match addr {
            ServerAddr::Tcp { address } => Ok(NetStream::Tcp(TcpStream::connect(address).await?)),
            ServerAddr::FakeTls {
                address,
                secret,
                domain,
            } => {
                let now = mtg_crypto::since_epoch().as_secs() as i32;
                Ok(NetStream::Tls(
                    TlsStream::connect(*address, secret, domain, now).await?,
                ))
            }
            #[cfg(feature = "proxy")]
            ServerAddr::Proxied { address, proxy } => {
                Self::connect_proxy_stream(address, proxy).await
            }
        }
    }

    #[cfg(feature = "proxy")]
    async fn connect_proxy_stream(
        addr: &std::net::SocketAddr,
        proxy_url: &str,
    ) -> Result<NetStream, std::io::Error> {
        use std::{
            io::{self, ErrorKind},
            net::{IpAddr, SocketAddr},
        };

        use hickory_resolver::Resolver;
        use url::Host;

        let proxy = url::Url::parse(proxy_url)
            .map_err(|err| io::Error::new(ErrorKind::InvalidData, err))?;
        let scheme = proxy.scheme();
        let host = proxy.host().ok_or(io::Error::new(
            ErrorKind::NotFound,
            format!("proxy host is missing from url: {}", proxy_url),
        ))?;
        let port = proxy.port().ok_or(io::Error::new(
            ErrorKind::NotFound,
            format!("proxy port is missing from url: {}", proxy_url),
        ))?;
        let username = proxy.username();
        let password = proxy.password().unwrap_or("");
        let socks_addr = match host {
            Host::Domain(domain) => {
                let resolver = Resolver::builder_tokio()
                    .map_err(io::Error::other)?
                    .build()
                    .map_err(io::Error::other)?;
                let response = resolver.lookup_ip(domain).await.map_err(io::Error::other)?;
                let socks_ip_addr = response.iter().next().ok_or(io::Error::new(
                    ErrorKind::NotFound,
                    format!("proxy host did not return any ip address: {}", domain),
                ))?;
                SocketAddr::new(socks_ip_addr, port)
            }
            Host::Ipv4(v4) => SocketAddr::new(IpAddr::from(v4), port),
            Host::Ipv6(v6) => SocketAddr::new(IpAddr::from(v6), port),
        };

        match scheme {
            "socks5" => {
                if username.is_empty() {
                    Ok(NetStream::ProxySocks5(
                        tokio_socks::tcp::Socks5Stream::connect(socks_addr, addr)
                            .await
                            .map_err(|err| io::Error::new(ErrorKind::ConnectionAborted, err))?,
                    ))
                } else {
                    Ok(NetStream::ProxySocks5(
                        tokio_socks::tcp::Socks5Stream::connect_with_password(
                            socks_addr, addr, username, password,
                        )
                        .await
                        .map_err(|err| io::Error::new(ErrorKind::ConnectionAborted, err))?,
                    ))
                }
            }
            scheme => Err(io::Error::new(
                ErrorKind::ConnectionAborted,
                format!("proxy scheme not supported: {}", scheme),
            )),
        }
    }

    pub(crate) async fn disconnect(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(stream) => stream.shutdown().await,
            Self::Tls(stream) => stream.shutdown().await,
            #[cfg(feature = "proxy")]
            Self::ProxySocks5(stream) => stream.shutdown().await,
        }
    }
}

pub enum NetReader<'a> {
    Plain(ReadHalf<'a>),
    Tls(TlsRead<'a>),
}

pub enum NetWriter<'a> {
    Plain(WriteHalf<'a>),
    Tls(TlsWrite<'a>),
}

impl AsyncRead for NetReader<'_> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(r) => Pin::new(r).poll_read(cx, buf),
            Self::Tls(r) => Pin::new(r).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for NetWriter<'_> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(w) => Pin::new(w).poll_write(cx, buf),
            Self::Tls(w) => Pin::new(w).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(w) => Pin::new(w).poll_flush(cx),
            Self::Tls(w) => Pin::new(w).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            Self::Plain(w) => Pin::new(w).poll_shutdown(cx),
            Self::Tls(w) => Pin::new(w).poll_shutdown(cx),
        }
    }
}
