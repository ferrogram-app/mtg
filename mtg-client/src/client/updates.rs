// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Methods to deal with and offer access to updates.

use std::collections::VecDeque;

use mtg_mtsender::{InvocationError, UpdatesConfiguration, UpdatesReceiver};
use mtg_session::updates::{State, UpdatesLike};
use mtg_tl_types as tl;
use tokio::sync::mpsc;

use super::Client;
use crate::peer::PeerMap;
use crate::update::Update;

/// Iterator returned by [`Client::stream_updates`].
pub struct UpdateStream {
    client: Client,
    updates: UpdatesReceiver,
    buffer: VecDeque<(tl::enums::Update, State, PeerMap)>,
}

impl UpdateStream {
    /// Pops an update from the queue, waiting for an update to arrive first if the queue is empty.
    pub async fn next(&mut self) -> Result<Update, InvocationError> {
        let (update, state, peers) = self.next_raw().await?;
        Ok(Update::from_raw(&self.client, update, state, peers))
    }

    pub async fn next_raw(
        &mut self,
    ) -> Result<(tl::enums::Update, State, PeerMap), InvocationError> {
        loop {
            if let Some(update) = self.buffer.pop_front() {
                return Ok(update);
            }

            let (updates, users, chats) = self.updates.next().await?;
            let peer_map = self.client.build_peer_map(users, chats).await;
            self.buffer
                .extend(updates.into_iter().map(|(u, s)| (u, s, peer_map.handle())));
        }
    }

    /// Synchronize the updates state to the session.
    ///
    /// This is **not** automatically done on drop.
    pub async fn sync_update_state(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.updates.sync_update_state().await
    }
}

impl Client {
    /// Returns an asynchronous stream of processed updates.
    ///
    /// The updates are guaranteed to be in order, and any gaps will be resolved.\
    /// **Important** to note that for gaps to be resolved, the peers must have been
    /// persisted in the session cache beforehand (i.e. be retrievable with [`mtg_session::Session::peer`]).
    /// A good way to achieve this is to use [`Self::iter_dialogs`] at least once after login.
    ///
    /// The updates are wrapped in [`crate::update::Update`] to make them more convenient to use,
    /// but their raw type is still accessible to bridge any missing functionality.
    pub async fn stream_updates(
        &self,
        updates: mpsc::Receiver<UpdatesLike>,
        configuration: UpdatesConfiguration,
    ) -> Result<UpdateStream, Box<dyn std::error::Error + Send + Sync>> {
        let updates = UpdatesReceiver::create(
            self.0.handle.clone(),
            self.0.session.clone(),
            updates,
            configuration,
        )
        .await?;

        Ok(UpdateStream {
            client: self.clone(),
            updates,
            buffer: VecDeque::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use core::future::Future;

    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, message, sent, user_ref};

    async fn stream(server: &FakeServer) -> (UpdateStream, mpsc::Sender<UpdatesLike>) {
        let (tx, rx) = mpsc::channel(8);
        let stream = client(server)
            .stream_updates(rx, UpdatesConfiguration { catch_up: false })
            .await
            .unwrap();
        (stream, tx)
    }

    fn new_message(id: i32, pts: i32) -> UpdatesLike {
        UpdatesLike::Updates(tl::enums::Updates::Updates(tl::types::Updates {
            updates: vec![
                tl::types::UpdateNewMessage {
                    message: message(id, user_ref(5), "x", 0),
                    pts,
                    pts_count: 1,
                }
                .into(),
            ],
            users: Vec::new(),
            chats: Vec::new(),
            date: 0,
            seq: 0,
        }))
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn updates_come_out_in_order_raw_and_typed() {
        let server = FakeServer::start().await;
        let (mut stream, tx) = stream(&server).await;
        tx.send(new_message(1, 1)).await.unwrap();
        tx.send(new_message(2, 2)).await.unwrap();

        let (raw, _, _) = stream.next_raw().await.unwrap();
        assert!(matches!(raw, tl::enums::Update::NewMessage(m) if m.pts == 1));
        // `next` wraps the same raw update into the typed one
        assert!(matches!(stream.next().await.unwrap(), Update::NewMessage(m) if m.id() == 2));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_update_state_can_be_synced() {
        let server = FakeServer::start().await;
        let (stream, _tx) = stream(&server).await;
        stream.sync_update_state().await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_closed_feed_ends_the_stream_with_an_error() {
        let server = FakeServer::start().await;
        let (mut stream, tx) = stream(&server).await;
        drop(tx);
        assert!(stream.next_raw().await.is_err());
    }

    fn get_update_stream() -> UpdateStream {
        panic!()
    }

    #[test]
    fn ensure_next_update_future_impls_send() {
        if false {
            // We just want it to type-check, not actually run.
            fn typeck(_: impl Future + Send) {}
            typeck(get_update_stream().next());
        }
    }
}
