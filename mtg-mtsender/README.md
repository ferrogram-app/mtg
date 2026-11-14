# mtg-mtsender

Network side of MTProto: connects to Telegram, sends requests, receives responses and updates,
reconnects, and keeps a pool of connections to the datacenters.

Telegram documentation this crate implements:

- [MTProto transports](https://core.telegram.org/mtproto/mtproto-transports) over TCP (optionally through a SOCKS5 proxy)
- [Service messages](https://core.telegram.org/mtproto/service_messages) and
  [messages about messages](https://core.telegram.org/mtproto/service_messages_about_messages)
- [Datacenters](https://core.telegram.org/api/datacenter): `dcOption`, migration between DCs; several addresses per DC are tried in turn, the last working one first
- Fallback config for blocked datacenters: encrypted `help.configSimple` from DNS TXT records (`apply_dns_config`; with the `doh` feature and `ConnectionParams::dns_fallback_domain` the records are fetched over DNS-over-HTTPS), as the official client does
- [Error handling](https://core.telegram.org/api/errors): RPC errors, flood waits

The protocol itself is in `mtg-mtproto`; this crate only moves its bytes over sockets.

## MTProxy

Set `ConnectionParams::mtproxy` to `MtProxy::from_link("tg://proxy?server=…&port=…&secret=…")` (hex or
base64url secret; plain or `dd` padded) to reach Telegram through an MTProxy: the proxy's addresses
replace the datacenter's, and the obfuscation head asks it for the right datacenter. Fake-TLS (`ee`)
secrets are rejected with `MtProxyError::FakeTlsUnsupported` for now.
