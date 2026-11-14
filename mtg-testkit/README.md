# mtg-testkit

A fake MTProto server for tests of the `mtg-*` crates (not published).

`FakeServer::start()` listens on a free local port and speaks MTProto 2.0 over the "full" transport with a
pre-shared auth key (`KEY`), so clients skip the key exchange. `server.session()` is a `MemorySession` whose
DC 2 points at it. `server.on(|req: SomeRequest| Ok(result))` answers one request type (`Err((code, "TEXT"))`
becomes an `rpc_error`); `server.requests()` lists what arrived, by name. Unmocked requests get `NOT_MOCKED_<id>`.

Moved from `ferrogram-android/core/tests/common`; core switches to this crate after the next `rev` bump.
