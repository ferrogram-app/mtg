// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

mod tcp;
mod tls;

pub use tcp::NetStream;

/// Represents a socket address which may be proxied.
#[derive(Debug, Clone)]
pub enum ServerAddr {
    /// Socket address whose connection should be proxied.
    #[cfg(feature = "proxy")]
    Proxied {
        /// The server to reach through the proxy.
        address: std::net::SocketAddr,
        /// The proxy URL (`socks5://[user:password@]host:port`).
        proxy: String,
    },
    /// An MTProxy behind fake-TLS: the handshake is done on connect.
    FakeTls {
        /// The proxy.
        address: std::net::SocketAddr,
        /// The 16 secret bytes.
        secret: [u8; 16],
        /// The domain the hello pretends to visit.
        domain: String,
    },
    /// Proxy address for direct connection.
    Tcp {
        /// The server to connect to.
        address: std::net::SocketAddr,
    },
}
