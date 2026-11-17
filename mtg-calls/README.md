# mtg-calls

Sans-IO signalling for one-to-one Telegram calls: the state machine, the key exchange and the
emoji fingerprint. It does not touch the network or the media: you invoke the requests it returns
and feed it the answers and the `updatePhoneCall` updates.

Docs: <https://core.telegram.org/api/end-to-end/voice-calls>, <https://core.telegram.org/api/end-to-end/video-calls>

- `Calls::request` / `on_request_result`: place a call (`phone.requestCall`, `g_a_hash` first).
- `handle_update`: `Incoming`, `Ringing`, `Connected` (key, emoji, relay endpoints, protocol,
  `custom_parameters`), `Ended`, `Failed` (bad hash, key or fingerprint; a discard is queued).
- `accept`, `received`, `discard`, `rate`, `debug`: `phone.acceptCall`, `receivedCall`,
  `discardCall`, `setCallRating`, `saveCallDebug`.
- `emoji(&Key)`: the four emojis of the key (SHA-256 of the key, as the official client).
- DH group, exchange and `Key` come from `mtg-secret`.

`mtg-client` runs it over a `Client` with feature `calls`: `client.calls(protocol)` gives `CallsClient`
(`call`, `accept`, `received`, `discard`, `rate`, `handle_update`); `has_dh_config` tells whether incoming
calls can be seen yet (the driver fetches the config on the first call update).

Updates that arrive before the `requestCall` answer are held (8 at most) and replayed.

Not done: group calls (`phone.joinGroupCall` needs a WebRTC offer), conference calls, the media
stack (the application binds a call library with `Connected`). No live Telegram run.
