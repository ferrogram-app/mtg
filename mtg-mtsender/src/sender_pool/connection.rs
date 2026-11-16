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
            Request::Invoke {
                dc_id,
                media,
                body,
                tx,
            } => {
                let media = media && self.has_media_route(dc_id);
                let index = match self.find_or_connect(dc_id, media).await {
                    Ok(index) => index,
                    Err(e) if media => {
                        // the media addresses do not work: forget them until the next config
                        // and use the ordinary connection
                        warn!(dc_id, error = %e, "media connection failed; using the ordinary one");
                        self.media_addresses.remove(&dc_id);
                        match self.find_or_connect(dc_id, false).await {
                            Ok(index) => index,
                            Err(e) => {
                                let _ = tx.send(Err(e));
                                return ControlFlow::Continue(());
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return ControlFlow::Continue(());
                    }
                };
                let body = if std::mem::take(&mut self.connections[index].needs_init) {
                    self.with_init_connection(body)
                } else {
                    body
                };
                let _ = self.connections[index].rpc_tx.send(Rpc { body, tx });
                ControlFlow::Continue(())
            }
            Request::CdnKeys(keys) => {
                self.cdn_keys = keys;
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
                if is_unknown_auth_key(&error) {
                    let streak = self.unknown_key.entry(dc_id).or_default();
                    *streak += 1;
                    if *streak >= UNKNOWN_KEY_LIMIT {
                        self.unknown_key.remove(&dc_id);
                        self.forget_auth_key(dc_id).await;
                    }
                } else {
                    self.unknown_key.remove(&dc_id);
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

    /// The DC has media-only addresses and a key to use them with.
    fn has_media_route(&self, dc_id: i32) -> bool {
        self.media_addresses
            .get(&dc_id)
            .is_some_and(|list| !list.is_empty())
            && matches!(self.session.dc_option(dc_id), Ok(Some(o)) if o.auth_key.is_some())
    }

    async fn find_or_connect(&mut self, dc_id: i32, media: bool) -> Result<usize, InvocationError> {
        if let Some(i) = self
            .connections
            .iter()
            .position(|c| c.dc_id == dc_id && c.media == media)
        {
            return Ok(i);
        }
        self.create_connection(dc_id, media).await?;
        Ok(self.connections.len() - 1)
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
        media: bool,
    ) -> Result<&ConnectionInfo, InvocationError> {
        let cdn = self.cdn_addresses.contains_key(&dc_id);
        let mut dc_option = match self.session.dc_option(dc_id)? {
            Some(x) => x,
            // a CDN datacenter is not in the session until its first connection
            None if cdn => DcOption {
                id: dc_id,
                ipv4: SocketAddrV4::new(Ipv4Addr::from_bits(0), 0),
                ipv6: SocketAddrV6::new(Ipv6Addr::from_bits(0), 0, 0, 0),
                auth_key: None,
            },
            None => return Err(InvocationError::InvalidDc),
        };

        let (sender, address) = if media {
            self.connect_any(&dc_option, true).await?
        } else {
            self.connect_sender(&dc_option).await?
        };
        // A media connection uses the stored key and must not replace the primary address.
        if !media {
            // Remember the address that worked, so the next start tries it first.
            match address {
                SocketAddr::V4(a) => dc_option.ipv4 = a,
                SocketAddr::V6(a) => dc_option.ipv6 = a,
            }

            dc_option.auth_key = Some(sender.auth_key());
            self.session.set_dc_option(&dc_option).await?;
        }

        let (rpc_tx, rpc_rx) = mpsc::unbounded_channel();
        let abort_handle = self.connection_pool.spawn(run_sender(
            sender,
            rpc_rx,
            self.updates_tx.clone(),
            !media && dc_option.id == self.session.home_dc_id()?,
            Arc::clone(&self.time_offset),
        ));
        self.connections.push(ConnectionInfo {
            dc_id,
            media,
            needs_init: cdn,
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
        let result = self.connect_any(dc_option, false).await;
        // Every known address failed: ask DNS for the fallback config once, then try again.
        // shortcut: once per runner; add a cool-down if blocks come and go while running.
        #[cfg(feature = "doh")]
        if let (true, Some(domain), false) = (
            result.is_err(),
            self.connection_params.dns_fallback_domain.clone(),
            self.dns_fallback_tried,
        ) {
            self.dns_fallback_tried = true;
            let now = mtg_crypto::since_epoch().as_secs() as i32;
            match crate::fallback::fetch_and_apply(&*self.session, &domain, "", now).await {
                Ok(0) => warn!("the DNS fallback config gave no addresses"),
                Ok(n) => {
                    info!(
                        datacenters = n,
                        "got addresses from the DNS fallback config"
                    );
                    return self.connect_any(dc_option, false).await;
                }
                Err(err) => error!(%err, "could not store the DNS fallback addresses"),
            }
        }
        result
    }

    async fn connect_any(
        &mut self,
        dc_option: &DcOption,
        media: bool,
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
        let cdn = self.cdn_addresses.contains_key(&dc_option.id);
        let stored = if media {
            self.media_addresses
                .get(&dc_option.id)
                .cloned()
                .unwrap_or_default()
        } else if cdn {
            self.cdn_addresses[&dc_option.id].clone()
        } else {
            self.session.dc_addresses(dc_option.id)?
        };
        let mut last_error = InvocationError::Dropped("no address to connect to");
        let list = match &self.connection_params.mtproxy {
            // the proxy is the only way in: its own addresses replace the datacenter's
            Some(proxy) => tokio::net::lookup_host((proxy.host.as_str(), proxy.port))
                .await
                .map_err(InvocationError::Io)?
                .collect(),
            None if media || cdn => stored
                .iter()
                .copied()
                .filter(|a| a.is_ipv6() == use_ipv6)
                .collect(),
            None => candidates(primary, &stored, use_ipv6),
        };
        for address in list {
            let attempt = tokio::time::timeout(
                CONNECT_TIMEOUT,
                self.connect_one(address, dc_option.id, media, dc_option.auth_key),
            );
            match attempt.await {
                Ok(Ok((sender, config))) => {
                    if let Some(config) = config {
                        self.update_config(config).await?;
                    }
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
        dc_id: i32,
        media: bool,
        auth_key: Option<[u8; 256]>,
    ) -> Result<
        (
            Sender<Box<dyn transport::Transport + Send>, mtp::Encrypted>,
            // none for a CDN datacenter
            Option<tl::types::Config>,
        ),
        InvocationError,
    > {
        let params = &self.connection_params;
        let transport = || -> Box<dyn transport::Transport + Send> {
            match &params.mtproxy {
                // the dc id goes in the obfuscation head; negative asks for media
                Some(p) if p.padded => Box::new(transport::Obfuscated::with_proxy(
                    transport::PaddedIntermediate::new(),
                    &p.secret,
                    (if media { -dc_id } else { dc_id }) as i16,
                )),
                Some(p) => Box::new(transport::Obfuscated::with_proxy(
                    transport::Intermediate::new(),
                    &p.secret,
                    (if media { -dc_id } else { dc_id }) as i16,
                )),
                None => (params.transport)(),
            }
        };

        #[cfg(feature = "proxy")]
        let addr = || {
            if let Some((secret, domain)) = fake_tls(&self.connection_params) {
                ServerAddr::FakeTls {
                    address,
                    secret,
                    domain,
                }
            } else if let Some(proxy) = self.connection_params.proxy_url.clone() {
                ServerAddr::Proxied { address, proxy }
            } else {
                ServerAddr::Tcp { address }
            }
        };
        #[cfg(not(feature = "proxy"))]
        let addr = || {
            if let Some((secret, domain)) = fake_tls(&self.connection_params) {
                ServerAddr::FakeTls {
                    address,
                    secret,
                    domain,
                }
            } else {
                ServerAddr::Tcp { address }
            }
        };

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

        if self.cdn_addresses.contains_key(&dc_id) {
            // A CDN DC only answers `getCdnFile`: no `getConfig` here (`initConnection` goes
            // with the first request), and its key must come from `help.getCdnConfig`.
            let sender = if let Some(auth_key) = auth_key {
                let offset = self.time_offset.load(Ordering::Relaxed);
                let mtp = mtp::Encrypted::build().time_offset(offset).finish(auth_key);
                Sender::connect(transport(), mtp, addr())
                    .await
                    .map_err(InvocationError::Io)?
            } else {
                let keys: Vec<_> = self
                    .cdn_keys
                    .iter()
                    .filter(|(id, _)| *id == dc_id)
                    .map(|(_, key)| key.clone())
                    .collect();
                if keys.is_empty() {
                    return Err(InvocationError::Dropped(
                        "no CDN public key for this datacenter",
                    ));
                }
                let plain = Sender::connect(transport(), mtp::Plain::new(), addr())
                    .await
                    .map_err(InvocationError::Io)?;
                crate::generate_cdn_auth_key(plain, &keys).await?
            };
            return Ok((sender, None));
        }

        let mut sender = if let Some(auth_key) = auth_key {
            let offset = self.time_offset.load(Ordering::Relaxed);
            let mtp = mtp::Encrypted::build().time_offset(offset).finish(auth_key);
            Sender::connect(transport(), mtp, addr())
                .await
                .map_err(InvocationError::Io)?
        } else {
            connect(transport(), addr()).await?
        };

        let enums::Config::Config(remote_config) = match sender.invoke(&init_connection).await {
            Ok(config) => config,
            // a media connection must keep the stored key: the caller falls back instead
            Err(InvocationError::Transport(transport::Error::BadStatus { status: 404 }))
                if !media =>
            {
                sender = connect(transport(), addr()).await?;
                sender.invoke(&init_connection).await?
            }
            Err(e) => return Err(e),
        };
        Ok((sender, Some(remote_config)))
    }

    /// `invokeWithLayer(initConnection(body))`, the way the first request of a CDN connection
    /// is sent. Nothing private goes in it: only the app and device names of the configuration.
    fn with_init_connection(&self, body: Vec<u8>) -> Vec<u8> {
        struct Raw(Vec<u8>);
        impl tl::Serializable for Raw {
            fn serialize(&self, sink: &mut impl FnMut(&[u8])) {
                sink(&self.0)
            }
        }
        impl tl::RemoteCall for Raw {
            type Return = Vec<u8>;
        }
        let params = &self.connection_params;
        tl::Serializable::to_bytes(&tl::functions::InvokeWithLayer {
            layer: tl::LAYER,
            query: tl::functions::InitConnection {
                api_id: self.api_id,
                device_model: params.device_model.clone(),
                system_version: params.system_version.clone(),
                app_version: params.app_version.clone(),
                system_lang_code: params.system_lang_code.clone(),
                lang_pack: "".into(),
                lang_code: params.lang_code.clone(),
                proxy: None,
                params: None,
                query: Raw(body),
            },
        })
    }

    pub(super) async fn update_config(
        &mut self,
        config: tl::types::Config,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut lists: std::collections::BTreeMap<i32, Vec<SocketAddr>> = Default::default();
        let mut media: HashMap<i32, Vec<SocketAddr>> = HashMap::new();
        let mut cdn: HashMap<i32, Vec<SocketAddr>> = HashMap::new();
        for option in config
            .dc_options
            .iter()
            .map(|tl::enums::DcOption::Option(option)| option)
            .filter(|option| option.cdn && !option.tcpo_only)
        {
            if let Some(address) = option_address(option) {
                let list = cdn.entry(option.id).or_default();
                if !list.contains(&address) {
                    list.push(address);
                }
            }
        }
        self.cdn_addresses = cdn;
        for option in config
            .dc_options
            .iter()
            .map(|tl::enums::DcOption::Option(option)| option)
            .filter(|option| option.media_only && !option.cdn && !option.tcpo_only)
        {
            if let Some(address) = option_address(option) {
                let list = media.entry(option.id).or_default();
                if !list.contains(&address) {
                    list.push(address);
                }
            }
        }
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
        self.media_addresses = media;
        for (dc_id, list) in lists {
            if self.session.dc_addresses(dc_id)? != list {
                self.session.set_dc_addresses(dc_id, list).await?;
            }
        }
        Ok(())
    }
}

/// The secret and domain when the configured MTProxy wants fake-TLS.
fn fake_tls(params: &crate::ConnectionParams) -> Option<([u8; 16], String)> {
    let proxy = params.mtproxy.as_ref()?;
    Some((proxy.secret, proxy.tls_domain.clone()?))
}
