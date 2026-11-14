use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};

use mtg_mtproto::{mtp, transport};
use mtg_session::types::DcOption;
use mtg_tl_types::enums;
use tracing::{error, warn};

use std::net::SocketAddr;
use std::time::Duration;

use super::*;

/// How long one address gets to produce a working connection before the next one is tried.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

impl SenderPoolRunner {
    pub(super) async fn process_request(&mut self, request: Request) -> ControlFlow<()> {
        match request {
            Request::Invoke { dc_id, body, tx } => {
                let connection = match self
                    .connections
                    .iter()
                    .find(|connection| connection.dc_id == dc_id)
                {
                    Some(connection) => connection,
                    None => match self.create_connection(dc_id).await {
                        Ok(x) => x,
                        Err(e) => {
                            let _ = tx.send(Err(e));
                            return ControlFlow::Continue(());
                        }
                    },
                };
                let _ = connection.rpc_tx.send(Rpc { body, tx });
                ControlFlow::Continue(())
            }
            Request::CheckRetry {
                dc_id,
                error,
                fail_count,
                slept_so_far,
                tx,
            } => {
                if is_auth_key_duplicated(&error) {
                    self.forget_auth_key(dc_id).await;
                }
                let retry_context = RetryContext {
                    fail_count,
                    slept_so_far,
                    error,
                };
                let flow = match self
                    .connection_params
                    .retry_policy
                    .should_retry(&retry_context)
                {
                    ControlFlow::Continue(delay) => ControlFlow::Continue(delay),
                    ControlFlow::Break(()) => ControlFlow::Break(retry_context.error),
                };
                let _ = tx.send(flow);
                ControlFlow::Continue(())
            }
            Request::Disconnect { dc_id } => {
                self.connections.retain(|connection| {
                    if connection.dc_id == dc_id {
                        connection.abort_handle.abort();
                        false
                    } else {
                        true
                    }
                });
                ControlFlow::Continue(())
            }
            Request::Quit => ControlFlow::Break(()),
        }
    }

    /// The server invalidated our key (`AUTH_KEY_DUPLICATED`, the key was used from two
    /// connections): forget it and drop the connection, so the next request makes a new key.
    pub(super) async fn forget_auth_key(&mut self, dc_id: i32) {
        warn!(dc_id, "auth key was invalidated by the server; dropping it");
        self.connections.retain(|connection| {
            if connection.dc_id == dc_id {
                connection.abort_handle.abort();
            }
            connection.dc_id != dc_id
        });
        if let Ok(Some(mut dc_option)) = self.session.dc_option(dc_id) {
            dc_option.auth_key = None;
            if let Err(err) = self.session.set_dc_option(&dc_option).await {
                error!(%err, "could not forget the invalidated auth key");
            }
        }
    }

    pub(super) async fn create_connection(
        &mut self,
        dc_id: i32,
    ) -> Result<&ConnectionInfo, InvocationError> {
        let mut dc_option = match self.session.dc_option(dc_id)? {
            Some(x) => x,
            None => return Err(InvocationError::InvalidDc),
        };

        let (sender, address) = self.connect_sender(&dc_option).await?;
        // Remember the address that worked, so the next start tries it first.
        match address {
            SocketAddr::V4(a) => dc_option.ipv4 = a,
            SocketAddr::V6(a) => dc_option.ipv6 = a,
        }

        dc_option.auth_key = Some(sender.auth_key());
        self.session.set_dc_option(&dc_option).await?;

        let (rpc_tx, rpc_rx) = mpsc::unbounded_channel();
        let abort_handle = self.connection_pool.spawn(run_sender(
            sender,
            rpc_rx,
            self.updates_tx.clone(),
            dc_option.id == self.session.home_dc_id()?,
        ));
        self.connections.push(ConnectionInfo {
            dc_id,
            rpc_tx,
            abort_handle,
        });
        // just pushed above
        Ok(self.connections.last().unwrap())
    }

    /// Tries the datacenter's addresses in turn (see [`candidates`]); returns the sender and the
    /// address that worked.
    pub(super) async fn connect_sender(
        &mut self,
        dc_option: &DcOption,
    ) -> Result<
        (
            Sender<Box<dyn transport::Transport + Send>, mtp::Encrypted>,
            SocketAddr,
        ),
        InvocationError,
    > {
        let use_ipv6 = self.connection_params.use_ipv6;
        let primary: SocketAddr = if use_ipv6 {
            dc_option.ipv6.into()
        } else {
            dc_option.ipv4.into()
        };
        let stored = self.session.dc_addresses(dc_option.id)?;
        let mut last_error = InvocationError::Dropped("no address to connect to");
        for address in candidates(primary, &stored, use_ipv6) {
            let attempt = tokio::time::timeout(
                CONNECT_TIMEOUT,
                self.connect_one(address, dc_option.auth_key),
            );
            match attempt.await {
                Ok(Ok((sender, config))) => {
                    self.update_config(config).await?;
                    return Ok((sender, address));
                }
                Ok(Err(e)) => {
                    warn!(%address, error = %e, "could not connect; trying the next address");
                    last_error = e;
                }
                Err(_) => {
                    warn!(%address, "connection timed out; trying the next address");
                    last_error = InvocationError::Io(std::io::ErrorKind::TimedOut.into());
                }
            }
        }
        Err(last_error)
    }

    async fn connect_one(
        &self,
        address: SocketAddr,
        auth_key: Option<[u8; 256]>,
    ) -> Result<
        (
            Sender<Box<dyn transport::Transport + Send>, mtp::Encrypted>,
            tl::types::Config,
        ),
        InvocationError,
    > {
        let transport = &self.connection_params.transport;

        #[cfg(feature = "proxy")]
        let addr = || {
            if let Some(proxy) = self.connection_params.proxy_url.clone() {
                ServerAddr::Proxied { address, proxy }
            } else {
                ServerAddr::Tcp { address }
            }
        };
        #[cfg(not(feature = "proxy"))]
        let addr = || ServerAddr::Tcp { address };

        let init_connection = tl::functions::InvokeWithLayer {
            layer: tl::LAYER,
            query: tl::functions::InitConnection {
                api_id: self.api_id,
                device_model: self.connection_params.device_model.clone(),
                system_version: self.connection_params.system_version.clone(),
                app_version: self.connection_params.app_version.clone(),
                system_lang_code: self.connection_params.system_lang_code.clone(),
                lang_pack: "".into(),
                lang_code: self.connection_params.lang_code.clone(),
                proxy: None,
                params: None,
                query: tl::functions::help::GetConfig {},
            },
        };

        let mut sender = if let Some(auth_key) = auth_key {
            connect_with_auth(transport(), addr(), auth_key)
                .await
                .map_err(InvocationError::Io)?
        } else {
            connect(transport(), addr()).await?
        };

        let enums::Config::Config(remote_config) = match sender.invoke(&init_connection).await {
            Ok(config) => config,
            Err(InvocationError::Transport(transport::Error::BadStatus { status: 404 })) => {
                sender = connect(transport(), addr()).await?;
                sender.invoke(&init_connection).await?
            }
            Err(e) => return Err(e),
        };
        Ok((sender, remote_config))
    }

    pub(super) async fn update_config(
        &self,
        config: tl::types::Config,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut lists: std::collections::BTreeMap<i32, Vec<SocketAddr>> = Default::default();
        for option in config
            .dc_options
            .iter()
            .map(|tl::enums::DcOption::Option(option)| option)
            .filter(|option| !option.media_only && !option.tcpo_only && option.r#static)
        {
            if let Some(address) = option_address(option) {
                let list = lists.entry(option.id).or_default();
                if !list.contains(&address) {
                    list.push(address);
                }
            }
            let mut dc_option = self
                .session
                .dc_option(option.id)?
                .unwrap_or_else(|| DcOption {
                    id: option.id,
                    ipv4: SocketAddrV4::new(Ipv4Addr::from_bits(0), 0),
                    ipv6: SocketAddrV6::new(Ipv6Addr::from_bits(0), 0, 0, 0),
                    auth_key: None,
                });
            // An unparsable address from the server skips that option, not the whole config.
            if !merge_dc_option(&mut dc_option, option) {
                continue;
            }
            self.session.set_dc_option(&dc_option).await?;
        }
        for (dc_id, list) in lists {
            if self.session.dc_addresses(dc_id)? != list {
                self.session.set_dc_addresses(dc_id, list).await?;
            }
        }
        Ok(())
    }
}
