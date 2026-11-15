# mtg-secret

Secret chats (end-to-end encrypted one-to-one chats), sans-IO: the crate builds the TL requests and
reads the updates; sending them (`mtg-client`), storing chats and messages, and drawing the key
picture stay in the application.

- `dh`: group checks (safe prime, `g_a`/`g_b` range), key exchange, key fingerprint.
- `cipher`: MTProto 2.0 encryption of one message (`msg_key`, AES-256-IGE, padding checks).
- `schema`: `decryptedMessageLayer`, messages, service actions, media, attributes; layers 46 to 151.
- `chat`: `SecretChat`: seq numbers, duplicates, gaps and `resend`, layer negotiation, PFS rekeying
  (`needs_rekey`, `request_key`, collisions, abort), state snapshot (`state` / `restore`).
- `chats`: `SecretChats`: all chats of an account driven by `updateEncryption`,
  `updateNewEncryptedMessage`, typing and read updates; `getDhConfig`, `requestEncryption`,
  `acceptEncryption`, `discardEncryption`, `receivedQueue`.
- `file`: attachment encryption (key, iv, fingerprint).
- `key::visualization`: the 36 bytes the identicon / emoji picture is drawn from.

Not done: layers below 46 and MTProto 1.0 (layer < 73) decryption, `decryptedMessageMediaExternalDocument`,
old `documentAttributeVideo#5910cccb`, automatic rekeying (the application calls `needs_rekey` /
`request_key`), a resend log beyond 256 messages, persisting a rekey in progress. Tests use two
in-process peers; no live Telegram was tried.

Telegram documentation implemented:

- [End-to-End Encryption, Secret Chats](https://core.telegram.org/api/end-to-end)
- [Sequence numbers](https://core.telegram.org/api/end-to-end/seq_no)
- [Perfect Forward Secrecy in Secret Chats](https://core.telegram.org/api/end-to-end/pfs)
- [Security guidelines](https://core.telegram.org/mtproto/security_guidelines)
