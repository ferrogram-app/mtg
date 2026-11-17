//! One-to-one calls: runs the sans-IO state machine of `mtg-calls` over a [`Client`].

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex};

use mtg_calls::{Calls, Event, Request};
use mtg_mtsender::InvocationError;
use mtg_session::types::PeerRef;
use mtg_tl_types as tl;

use super::Client;

/// What went wrong with a call.
#[derive(Debug)]
pub enum CallError {
    /// The signalling refused (no such call, wrong state, bad key exchange values).
    Call(mtg_secret::Error),
    /// The request to Telegram failed.
    Invoke(InvocationError),
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Call(e) => write!(f, "call: {e:?}"),
            Self::Invoke(e) => write!(f, "call request: {e}"),
        }
    }
}

impl std::error::Error for CallError {}

impl From<mtg_secret::Error> for CallError {
    fn from(e: mtg_secret::Error) -> Self {
        Self::Call(e)
    }
}

impl From<InvocationError> for CallError {
    fn from(e: InvocationError) -> Self {
        Self::Invoke(e)
    }
}

/// The calls of one account, created by [`Client::calls`]. Feed every `updatePhoneCall` to
/// [`CallsClient::handle_update`]; clones share the same calls.
#[derive(Clone)]
pub struct CallsClient {
    client: Client,
    calls: Arc<Mutex<Calls>>,
}

impl Client {
    /// Calls over this client; `protocol` comes from [`mtg_calls::call::protocol`].
    pub fn calls(&self, protocol: tl::types::PhoneCallProtocol) -> CallsClient {
        CallsClient {
            client: self.clone(),
            calls: Arc::new(Mutex::new(Calls::new(protocol))),
        }
    }
}

impl CallsClient {
    fn lock(&self) -> std::sync::MutexGuard<'_, Calls> {
        // a panic while holding the lock leaves the state machine as it was: keep going
        self.calls.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// `messages.getDhConfig`: also gives the fresh randomness every exchange needs.
    async fn refresh_dh(&self) -> Result<(), CallError> {
        let request = self.lock().dh_config_request();
        let config = self.client.invoke(&request).await?;
        self.lock().set_dh_config(&config)?;
        Ok(())
    }

    /// Calls `user`; returns the call id and the events the already arrived updates produced.
    pub async fn call<P: Into<PeerRef>>(
        &self,
        user: P,
        video: bool,
    ) -> Result<(i64, Vec<Event>), CallError> {
        self.refresh_dh().await?;
        let request = self
            .lock()
            .request(tl::enums::InputUser::from(&user.into()), video)?;
        let result = self.client.invoke(&request).await?;
        let (id, out) = self.lock().on_request_result(request.random_id, &result)?;
        Ok((id, self.run(out).await?))
    }

    /// Accepts a call announced by [`Event::Incoming`]; tells Telegram the device is ringing.
    pub async fn accept(&self, call_id: i64) -> Result<(), CallError> {
        self.refresh_dh().await?;
        let accept = self.lock().accept(call_id)?;
        self.client.invoke(&accept).await?;
        Ok(())
    }

    /// `phone.receivedCall`: tells the caller this device is ringing.
    pub async fn received(&self, call_id: i64) -> Result<(), CallError> {
        let request = self.lock().received(call_id)?;
        self.client.invoke(&request).await?;
        Ok(())
    }

    /// Hangs up or declines; the call is forgotten.
    pub async fn discard(
        &self,
        call_id: i64,
        duration: i32,
        reason: tl::enums::PhoneCallDiscardReason,
        connection_id: i64,
    ) -> Result<(), CallError> {
        let request = self
            .lock()
            .discard(call_id, duration, reason, connection_id)?;
        self.client.invoke(&request).await?;
        Ok(())
    }

    /// `phone.sendSignalingData`: bytes from the local media engine for the peer's engine.
    pub async fn signal(&self, call_id: i64, data: Vec<u8>) -> Result<(), CallError> {
        let request = self.lock().signal(call_id, data)?;
        self.client.invoke(&request).await?;
        Ok(())
    }

    /// `phone.setCallRating` for an ended call.
    pub async fn rate(
        &self,
        call_id: i64,
        access_hash: i64,
        rating: i32,
        comment: &str,
    ) -> Result<(), CallError> {
        let request = Calls::rate(call_id, access_hash, rating, comment);
        self.client.invoke(&request).await?;
        Ok(())
    }

    /// Handles one update (others are ignored) and sends what the state machine asks for.
    pub async fn handle_update(&self, update: &tl::enums::Update) -> Result<Vec<Event>, CallError> {
        // an incoming call is ignored without the DH config, and it may be the first thing we see
        if matches!(update, tl::enums::Update::PhoneCall(_)) && !self.lock().has_dh_config() {
            self.refresh_dh().await?;
        }
        let out = self.lock().handle_update(update);
        self.run(out).await
    }

    /// Invokes the requests of `out`; the answer of a confirm is the final `phoneCall`, which
    /// goes back through the state machine (it may ask for more).
    async fn run(&self, out: mtg_calls::Output) -> Result<Vec<Event>, CallError> {
        let mut events = out.events;
        let mut send: VecDeque<Request> = out.send.into();
        while let Some(request) = send.pop_front() {
            match request {
                Request::Discard(d) => {
                    self.client.invoke(&d).await?;
                }
                Request::Confirm(c) => {
                    let tl::enums::phone::PhoneCall::Call(answer) = self.client.invoke(&c).await?;
                    let next = self.lock().handle_update(&tl::enums::Update::PhoneCall(
                        tl::types::UpdatePhoneCall {
                            phone_call: answer.phone_call,
                        },
                    ));
                    events.extend(next.events);
                    send.extend(next.send);
                }
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mtg_calls::call::protocol;
    use mtg_testkit::FakeServer;
    use tl::enums::PhoneCall as P;

    use super::*;
    use crate::test_util::{client, user_ref};

    const P14: &str = "ffffffffffffffffc90fdaa22168c234c4c6628b80dc1cd129024e088a67cc74020bbea63b139b22514a08798e3404ddef9519b3cd3a431b302b0a6df25f14374fe1356d6d51c245e485b576625e7ec6f44c42e9a637ed6b0bff5cb6f406b7edee386bfb5a899fa5ae9f24117c4b1fe649286651ece45b3dc2007cb8a163bf0598da48361c55d39a69163fa8fd24cf5f83655d23dca3ad961c62f356208552bb9ed529077096966d670c354e4abc9804f1746c08ca18217c32905e462e36ce3be39e772c180e86039b2783a2ec07a28fb5c55df06f4c52c9de2bcbf6955817183995497cea956ae515d2261898fa051015728e5a8aacaa68ffffffffffffffff";

    fn proto() -> tl::types::PhoneCallProtocol {
        protocol(92, vec!["2.7.7".into()])
    }

    fn dh_config() -> tl::enums::messages::DhConfig {
        tl::types::messages::DhConfig {
            g: 2,
            p: (0..P14.len() / 2)
                .map(|i| u8::from_str_radix(&P14[2 * i..2 * i + 2], 16).unwrap())
                .collect(),
            version: 1,
            random: vec![5; 256],
        }
        .into()
    }

    fn waiting(id: i64) -> P {
        P::Waiting(tl::types::PhoneCallWaiting {
            video: false,
            id,
            access_hash: 9,
            date: 1,
            admin_id: 1,
            participant_id: 2,
            protocol: tl::enums::PhoneCallProtocol::Protocol(proto()),
            receive_date: None,
        })
    }

    fn answer(call: P) -> tl::enums::phone::PhoneCall {
        tl::types::phone::PhoneCall {
            phone_call: call,
            users: vec![],
        }
        .into()
    }

    fn requested(id: i64, g_a_hash: Vec<u8>) -> P {
        P::Requested(tl::types::PhoneCallRequested {
            video: false,
            id,
            access_hash: 9,
            date: 1,
            admin_id: 1,
            participant_id: 2,
            g_a_hash,
            protocol: tl::enums::PhoneCallProtocol::Protocol(proto()),
        })
    }

    fn accepted(id: i64, g_b: Vec<u8>) -> P {
        P::Accepted(tl::types::PhoneCallAccepted {
            video: false,
            id,
            access_hash: 9,
            date: 1,
            admin_id: 1,
            participant_id: 2,
            g_b,
            protocol: tl::enums::PhoneCallProtocol::Protocol(proto()),
        })
    }

    fn full(id: i64, g: Vec<u8>, fingerprint: i64) -> P {
        P::Call(tl::types::PhoneCall {
            p2p_allowed: false,
            video: false,
            conference_supported: false,
            id,
            access_hash: 9,
            date: 1,
            admin_id: 1,
            participant_id: 2,
            g_a_or_b: g,
            key_fingerprint: fingerprint,
            protocol: tl::enums::PhoneCallProtocol::Protocol(proto()),
            connections: vec![],
            start_date: 100,
            custom_parameters: None,
        })
    }

    fn update(call: P) -> tl::enums::Update {
        tl::enums::Update::PhoneCall(tl::types::UpdatePhoneCall { phone_call: call })
    }

    fn peer() -> Calls {
        let mut c = Calls::new(proto());
        c.set_dh_config(&dh_config()).unwrap();
        c
    }

    fn connected(events: &[Event]) -> mtg_calls::Connected {
        match events {
            [Event::Connected(c)] => c.clone(),
            e => panic!("{e:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_outgoing_call_runs_to_the_key() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::GetDhConfig| Ok(dh_config()));
        let sent = Arc::new(Mutex::new(None));
        let seen = sent.clone();
        server.on(move |r: tl::functions::phone::RequestCall| {
            *seen.lock().unwrap() = Some(r);
            Ok(answer(waiting(7)))
        });
        let g_b = Arc::new(Mutex::new(Vec::new()));
        let g_b_seen = g_b.clone();
        // the final phoneCall for the caller carries the callee's g_b
        server.on(move |c: tl::functions::phone::ConfirmCall| {
            Ok(answer(full(
                7,
                g_b_seen.lock().unwrap().clone(),
                c.key_fingerprint,
            )))
        });
        let calls = client(&server).calls(proto());
        let (id, events) = calls.call(user_ref(5), false).await.unwrap();
        assert_eq!((id, events.len()), (7, 0));

        // the callee, played by a second state machine
        let request = sent.lock().unwrap().take().unwrap();
        let mut callee = peer();
        callee.handle_update(&update(requested(7, request.g_a_hash)));
        let accept = callee.accept(7).unwrap();
        *g_b.lock().unwrap() = accept.g_b.clone();

        // the callee accepted: our confirm is sent, its answer ends the handshake
        let events = calls
            .handle_update(&update(accepted(7, accept.g_b)))
            .await
            .unwrap();
        let ours = connected(&events);
        assert!(ours.outgoing);
        assert_eq!(ours.emoji.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_incoming_call_is_accepted_and_connected() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::GetDhConfig| Ok(dh_config()));
        let accepted_with = Arc::new(Mutex::new(None));
        let seen = accepted_with.clone();
        server.on(move |a: tl::functions::phone::AcceptCall| {
            *seen.lock().unwrap() = Some(a.g_b);
            Ok(answer(waiting(7)))
        });
        server.on(|_: tl::functions::phone::ReceivedCall| Ok(true));
        let calls = client(&server).calls(proto());

        // the caller, played by a second state machine
        let mut caller = peer();
        let request = caller
            .request(tl::enums::InputUser::UserSelf, false)
            .unwrap();
        caller
            .on_request_result(request.random_id, &answer(waiting(7)))
            .unwrap();

        let events = calls
            .handle_update(&update(requested(7, request.g_a_hash)))
            .await
            .unwrap();
        assert!(matches!(events[..], [Event::Incoming { call_id: 7, .. }]));
        calls.received(7).await.unwrap();
        calls.accept(7).await.unwrap();
        let g_b = accepted_with.lock().unwrap().take().unwrap();

        let out = caller.handle_update(&update(accepted(7, g_b)));
        let [Request::Confirm(confirm)] = &out.send[..] else {
            panic!("{out:?}")
        };
        let events = calls
            .handle_update(&update(full(
                7,
                confirm.g_a.clone(),
                confirm.key_fingerprint,
            )))
            .await
            .unwrap();
        assert!(!connected(&events).outgoing);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn discard_rate_and_errors() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::messages::GetDhConfig| Ok(dh_config()));
        server.on(|_: tl::functions::phone::RequestCall| Ok(answer(waiting(7))));
        server.on(|_: tl::functions::phone::DiscardCall| Ok(tl::enums::Updates::TooLong));
        server.on(|_: tl::functions::phone::SetCallRating| Ok(tl::enums::Updates::TooLong));
        server.on(|_: tl::functions::phone::SendSignalingData| Ok(true));
        let calls = client(&server).calls(proto());
        calls.call(user_ref(5), false).await.unwrap();
        calls.signal(7, vec![1]).await.unwrap();
        let reason = || tl::enums::PhoneCallDiscardReason::Hangup;
        calls.discard(7, 3, reason(), 0).await.unwrap();
        calls.rate(7, 9, 5, "fine").await.unwrap();

        // the call is gone: the state machine refuses, the network is not touched
        let e = calls.discard(7, 3, reason(), 0).await.unwrap_err();
        assert!(matches!(e, CallError::Call(_)));
        assert!(e.to_string().starts_with("call:"));
        assert!(matches!(calls.received(7).await, Err(CallError::Call(_))));
        assert!(matches!(
            calls.signal(7, vec![]).await,
            Err(CallError::Call(_))
        ));
        assert!(matches!(calls.accept(7).await, Err(CallError::Call(_))));

        // a failing server is an invoke error
        let broken = FakeServer::start().await;
        broken.on(|_: tl::functions::messages::GetDhConfig| Err((500, "INTERNAL".into())));
        let e = client(&broken)
            .calls(proto())
            .call(user_ref(5), false)
            .await
            .unwrap_err();
        assert!(matches!(e, CallError::Invoke(_)));
        assert!(e.to_string().starts_with("call request:"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_broken_exchange_sends_the_queued_discard() {
        let server = FakeServer::start().await;
        let discards = Arc::new(Mutex::new(0));
        let seen = discards.clone();
        server.on(move |_: tl::functions::phone::DiscardCall| {
            *seen.lock().unwrap() += 1;
            Ok(tl::enums::Updates::TooLong)
        });
        server.on(|_: tl::functions::messages::GetDhConfig| Ok(dh_config()));
        server.on(|_: tl::functions::phone::AcceptCall| Ok(answer(waiting(7))));
        let calls = client(&server).calls(proto());
        // g_a_hash that matches nothing
        calls
            .handle_update(&update(requested(7, vec![0; 32])))
            .await
            .unwrap();
        calls.accept(7).await.unwrap();
        let events = calls
            .handle_update(&update(full(7, vec![1; 256], 5)))
            .await
            .unwrap();
        assert!(matches!(events[..], [Event::Failed { call_id: 7, .. }]));
        assert_eq!(*discards.lock().unwrap(), 1);
    }
}
