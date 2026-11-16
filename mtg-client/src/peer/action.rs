// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::future::Future;
use std::time::Duration;

use mtg_mtsender::InvocationError;
use mtg_session::types::PeerRef;
use mtg_tl_types::{self as tl, enums::SendMessageAction};
use tokio::time::sleep;

use crate::Client;

const DEFAULT_REPEAT_DELAY: Duration = Duration::from_secs(4);

/// Type to send chat actions (e.g. "Typing…") on repeat.
pub struct ActionSender {
    client: Client,
    peer: PeerRef,
    topic_id: Option<i32>,
    repeat_delay: Duration,
}

impl ActionSender {
    pub fn new<C: Into<PeerRef>>(client: &Client, peer: C) -> Self {
        Self {
            client: client.clone(),
            peer: peer.into(),
            topic_id: None,
            repeat_delay: DEFAULT_REPEAT_DELAY,
        }
    }

    /// Set custom repeat delay
    pub fn repeat_delay(mut self, repeat_delay: Duration) -> Self {
        self.repeat_delay = repeat_delay;
        self
    }

    /// Set a topic id
    pub fn topic_id(mut self, topic_id: i32) -> Self {
        self.topic_id = Some(topic_id);
        self
    }

    /// Cancel any actions
    pub async fn cancel(&self) -> Result<(), InvocationError> {
        self.oneshot(SendMessageAction::SendMessageCancelAction)
            .await?;

        Ok(())
    }

    /// Do a one-shot set action request
    pub async fn oneshot<A: Into<SendMessageAction>>(
        &self,
        action: A,
    ) -> Result<(), InvocationError> {
        self.client
            .invoke(&tl::functions::messages::SetTyping {
                peer: self.peer.into(),
                top_msg_id: self.topic_id,
                action: action.into(),
            })
            .await?;

        Ok(())
    }

    /// Repeat set action request until the future is done
    ///
    /// # Example
    ///
    /// ```
    /// # use std::time::Duration;
    ///
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_tl_types as tl;
    ///
    /// let heavy_task = async {
    ///     tokio::time::sleep(Duration::from_secs(10)).await;
    ///
    ///     42
    /// };
    ///
    /// tokio::pin!(heavy_task);
    ///
    /// let (task_result, _) = client
    ///     .action(peer)
    ///     .repeat(
    ///         // most clients doesn't actually show progress of an action
    ///         || tl::types::SendMessageUploadDocumentAction { progress: 0 },
    ///         heavy_task
    ///     )
    ///     .await;
    ///
    /// // Note: repeat function does not cancel actions automatically, they will just fade away
    ///
    /// assert_eq!(task_result, 42);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn repeat<A: Into<SendMessageAction>, T>(
        &self,
        action: impl Fn() -> A,
        mut future: impl Future<Output = T> + Unpin,
    ) -> (T, Result<(), InvocationError>) {
        let mut request_result = Ok(());

        let future_output = loop {
            if request_result.is_err() {
                // Don't try to make a request again
                return (future.await, request_result);
            }

            let action = async {
                request_result = self.oneshot(action().into()).await;
                sleep(self.repeat_delay).await;
            };

            tokio::select! {
                _ = action => continue,
                output = &mut future => break output,
            };
        };

        (future_output, request_result)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, user_ref};

    type Seen = Arc<std::sync::Mutex<Vec<(Option<i32>, bool)>>>;

    fn record(server: &FakeServer) -> Seen {
        let seen: Seen = Default::default();
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::SetTyping| {
            let cancel = matches!(req.action, SendMessageAction::SendMessageCancelAction);
            log.lock().unwrap().push((req.top_msg_id, cancel));
            Ok(true)
        });
        seen
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn oneshot_and_cancel_send_set_typing() {
        let server = FakeServer::start().await;
        let seen = record(&server);
        let c = client(&server);
        let sender = ActionSender::new(&c, user_ref(5)).topic_id(11);
        sender
            .oneshot(tl::types::SendMessageTypingAction {})
            .await
            .unwrap();
        sender.cancel().await.unwrap();
        assert_eq!(*seen.lock().unwrap(), [(Some(11), false), (Some(11), true)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn repeat_resends_until_the_future_is_done() {
        let server = FakeServer::start().await;
        let seen = record(&server);
        let c = client(&server);
        let sender = ActionSender::new(&c, user_ref(5)).repeat_delay(Duration::from_millis(20));
        let built = Arc::new(AtomicUsize::new(0));
        let counter = built.clone();
        let work = Box::pin(async {
            sleep(Duration::from_millis(150)).await;
            42
        });
        let (out, res) = sender
            .repeat(
                move || {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tl::types::SendMessageTypingAction {}
                },
                work,
            )
            .await;
        assert_eq!(out, 42);
        res.unwrap();
        assert!(built.load(Ordering::SeqCst) >= 2);
        assert!(seen.lock().unwrap().len() >= 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn repeat_stops_requesting_after_an_error() {
        let server = FakeServer::start().await;
        let calls = Arc::new(AtomicUsize::new(0));
        let n = calls.clone();
        server.on(move |_: tl::functions::messages::SetTyping| {
            n.fetch_add(1, Ordering::SeqCst);
            Err((400, "PEER_ID_INVALID".to_string()))
        });
        let c = client(&server);
        let sender = ActionSender::new(&c, user_ref(5)).repeat_delay(Duration::from_millis(5));
        let work = Box::pin(async {
            sleep(Duration::from_millis(100)).await;
            1
        });
        let (out, res) = sender
            .repeat(|| tl::types::SendMessageTypingAction {}, work)
            .await;
        assert_eq!(out, 1);
        assert!(matches!(res, Err(InvocationError::Rpc(e)) if e.name == "PEER_ID_INVALID"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
