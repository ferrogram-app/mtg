// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_tl_types::{self as tl, Deserializable, Identifiable};
use tracing::info;

use super::*;

mod rpc;
mod service;
mod session;

impl Encrypted {
    pub(super) fn process_message(
        &mut self,
        message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        // Server ids are odd (lower bits 01 for answers, 11 for the rest).
        if message.msg_id & 1 == 0 {
            return Err(DeserializeError::BadMessageId {
                got: message.msg_id,
            });
        }
        // Telegram: drop messages older than 300 s or more than 30 s ahead of our corrected clock.
        // A bad_msg_notification (and its salt variant) is exempt, because it is how a wrong
        // clock gets corrected in the first place.
        let is_bad_msg = matches!(
            message.constructor_id(),
            Ok(tl::types::BadMsgNotification::CONSTRUCTOR_ID
                | tl::types::BadServerSalt::CONSTRUCTOR_ID)
        );
        if !is_bad_msg && !self.msg_id_in_window(message.msg_id) {
            return Err(DeserializeError::BadMessageId {
                got: message.msg_id,
            });
        }
        if message.requires_ack() {
            self.pending_ack.push(message.msg_id);
        }
        // Acknowledged above, so the server stops resending, but processed only once.
        if self.seen_msg_ids.contains(&message.msg_id) {
            return Ok(());
        }
        if self.seen_msg_ids.len() >= SEEN_MSG_IDS {
            self.seen_msg_ids.pop_front();
        }
        self.seen_msg_ids.push_back(message.msg_id);

        self.dispatch(message)
    }

    /// Routes an already validated (and deduplicated) message to its handler.
    fn dispatch(&mut self, message: manual_tl::Message) -> Result<(), DeserializeError> {
        // Handle all the possible Service Messages:
        // * https://core.telegram.org/mtproto/service_messages
        // * https://core.telegram.org/mtproto/service_messages_about_messages
        //
        // The order of the `match` here is the same as the order in which the
        // items appear in the documentation (to make it easier to review).
        // TODO verify what needs ack and what doesn't
        match message.constructor_id()? {
            // Response to an RPC query
            manual_tl::RpcResult::CONSTRUCTOR_ID => self.handle_rpc_result(message),

            // Service Messages about Messages
            // Acknowledgment of Receipt
            tl::types::MsgsAck::CONSTRUCTOR_ID => self.handle_ack(message),
            // Notice of Ignored Error Message
            tl::types::BadMsgNotification::CONSTRUCTOR_ID
            | tl::types::BadServerSalt::CONSTRUCTOR_ID => self.handle_bad_notification(message),
            // Request for Message Status Information
            tl::types::MsgsStateReq::CONSTRUCTOR_ID => self.handle_state_req(message),
            // Informational Message regarding Status of Messages
            tl::types::MsgsStateInfo::CONSTRUCTOR_ID => self.handle_state_info(message),
            // Voluntary Communication of Status of Messages
            tl::types::MsgsAllInfo::CONSTRUCTOR_ID => self.handle_msg_all(message),
            // Extended Voluntary Communication of Status of One Message
            tl::types::MsgDetailedInfo::CONSTRUCTOR_ID
            | tl::types::MsgNewDetailedInfo::CONSTRUCTOR_ID => self.handle_detailed_info(message),
            // Explicit Request to Re-Send Messages & Explicit Request to Re-Send Answers
            tl::types::MsgResendReq::CONSTRUCTOR_ID
            | tl::types::MsgResendAnsReq::CONSTRUCTOR_ID => self.handle_msg_resend(message),

            // Request for several future salts
            tl::types::FutureSalt::CONSTRUCTOR_ID => self.handle_future_salt(message),
            tl::types::FutureSalts::CONSTRUCTOR_ID => self.handle_future_salts(message),
            // Ping Messages (PING/PONG)
            tl::types::Pong::CONSTRUCTOR_ID => self.handle_pong(message),
            // Request to Destroy Session
            tl::types::DestroySessionOk::CONSTRUCTOR_ID
            | tl::types::DestroySessionNone::CONSTRUCTOR_ID => self.handle_destroy_session(message),
            // New Session Creation Notification
            tl::types::NewSessionCreated::CONSTRUCTOR_ID => {
                self.handle_new_session_created(message)
            }
            // Containers (Simple Container)
            manual_tl::MessageContainer::CONSTRUCTOR_ID => self.handle_container(message),
            // Message Copies
            manual_tl::MessageCopy::CONSTRUCTOR_ID => self.handle_msg_copy(message),
            // Packed Object
            manual_tl::GzipPacked::CONSTRUCTOR_ID => self.handle_gzip_packed(message),
            // HTTP Wait/Long Poll
            tl::types::HttpWait::CONSTRUCTOR_ID => self.handle_http_wait(message),
            _ => self.handle_update(message),
        }
    }

    /// **[Request for several future salts]**
    ///
    /// The client may at any time request from the server several (between
    /// 1 and 64) future server salts together with their validity periods.
    /// Having stored them in persistent memory, the client may use them to
    /// send messages in the future even if he changes sessions (a server
    /// salt is attached to the Authorization Key rather than being
    /// session-specific).
    ///
    /// ```tl
    /// get_future_salts#b921bd04 num:int = FutureSalts;
    /// future_salt#0949d9dc valid_since:int valid_until:int salt:long = FutureSalt;
    /// future_salts#ae500895 req_msg_id:long now:int salts:vector future_salt = FutureSalts;
    /// ```
    ///
    /// The client must check to see that the response's `req_msg_id` in fact
    /// coincides with `msg_id` of the query for `get_future_salts`. The
    /// server returns a maximum of `num` future server salts (may return
    /// fewer). The response serves as the acknowledgment of the query and
    /// does not require an acknowledgment itself.
    ///
    /// [Request for several future salts]: https://core.telegram.org/mtproto/service_messages#request-for-several-future-salts
    pub(super) fn handle_future_salts(
        &mut self,
        message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        let tl::enums::FutureSalts::Salts(salts) =
            tl::enums::FutureSalts::from_bytes(&message.body)?;

        if self
            .salt_request_msg_id
            .is_some_and(|msg_id| msg_id == MsgId(salts.req_msg_id))
        {
            // Response to internal request, do not propagate.
            self.salt_request_msg_id = None;
        } else {
            // https://core.telegram.org/mtproto/service_messages: "the client MUST check that
            // req_msg_id matches the query". Not ours: hand it to the caller, keep our salts.
            self.deserialization
                .push(Deserialization::RpcResult(RpcResult {
                    msg_id: MsgId(salts.req_msg_id),
                    body: message.body,
                }));
            return Ok(());
        }

        self.start_salt_time = Some((salts.now, Instant::now()));
        self.salts = salts.salts.0;
        self.salts.sort_by_key(|salt| -salt.valid_since);
        info!("got {} future salts", self.salts.len());

        Ok(())
    }

    /// **[Containers]**
    ///
    /// *Containers* are messages containing several other messages. Used
    /// for the ability to transmit several RPC queries and/or service
    /// messages at the same time, using HTTP or even TCP or UDP protocol.
    /// A container may only be accepted or rejected by the other party as
    /// a whole.
    ///
    /// **[Simple Container]**
    ///
    /// A simple container carries several messages as follows:
    ///
    /// ```tl
    /// msg_container#73f1f8dc messages:vector message = MessageContainer;
    /// ```
    ///
    /// Here message refers to any message together with its length and
    /// `msg_id`:
    ///
    /// ```tl
    /// message msg_id:long seqno:int bytes:int body:Object = Message;
    /// ```
    ///
    /// `bytes` is the number of bytes in the body serialization.
    ///
    /// All messages in a container must have `msg_id` lower than that of the
    /// container itself. A container does not require an acknowledgment and
    /// may not carry other simple containers. When messages are re-sent,
    /// they may be combined into a container in a different manner or sent
    /// individually.
    ///
    /// Empty containers are also allowed. They are used by the server,
    /// for example, to respond to an HTTP request when the timeout specified
    /// in `http_wait` expires, and there are no messages to transmit.
    ///
    /// [Containers]: https://core.telegram.org/mtproto/service_messages#containers
    /// [Simple Container]: https://core.telegram.org/mtproto/service_messages#simple-container
    pub(super) fn handle_container(
        &mut self,
        message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        let container = manual_tl::MessageContainer::from_bytes(&message.body)?;
        self.nested(manual_tl::MessageContainer::CONSTRUCTOR_ID, |this| {
            for inner_message in container.messages {
                // "a container may not contain another simple container"
                if inner_message.constructor_id()? == manual_tl::MessageContainer::CONSTRUCTOR_ID {
                    return Err(DeserializeError::UnexpectedConstructor {
                        id: manual_tl::MessageContainer::CONSTRUCTOR_ID,
                    });
                }
                this.process_message(inner_message)?;
            }
            Ok(())
        })
    }

    /// **[Message Copies]**
    ///
    /// In some situations, an old message with a `msg_id` that is no longer
    /// valid needs to be re-sent. Then, it is wrapped in a copy container:
    ///
    /// ```tl
    /// msg_copy#e06046b2 orig_message:Message = MessageCopy;
    /// ```
    ///
    /// Once received, the message is processed as if the wrapper were not
    /// there. However, if it is known for certain that the message
    /// `orig_message.msg_id` was received, then the new message is not
    /// processed (while at the same time, it and `orig_message.msg_id`
    /// are acknowledged). The value of `orig_message.msg_id` must be lower
    /// than the container's `msg_id`.
    ///
    /// This is not used at this time, because an old message can be wrapped
    /// in a simple container with the same result.
    ///
    /// [Message Copies]: https://core.telegram.org/mtproto/service_messages#message-copies
    /// Runs `f` one level deeper, refusing chains deeper than [`MAX_NESTING`].
    fn nested(
        &mut self,
        id: u32,
        f: impl FnOnce(&mut Self) -> Result<(), DeserializeError>,
    ) -> Result<(), DeserializeError> {
        if self.nesting >= MAX_NESTING {
            return Err(DeserializeError::UnexpectedConstructor { id });
        }
        self.nesting += 1;
        let result = f(self);
        self.nesting -= 1;
        result
    }

    /// **[Packed Object]**
    ///
    /// Used to replace any other object (or rather, a serialization thereof)
    /// with its archived (gzipped) representation:
    ///
    /// ```tl
    /// gzip_packed#3072cfa1 packed_data:string = Object;
    /// ```
    ///
    /// At the present time, it is supported in the body of an RPC response
    /// (i.e., as result in rpc_result) and generated by the server for a
    /// limited number of high-level queries. In addition, in the future it
    /// may be used to transmit non-service messages (i.e. RPC queries) from
    /// client to server.
    ///
    /// [Packed Object]: https://core.telegram.org/mtproto/service_messages#packed-object
    fn handle_gzip_packed(&mut self, message: manual_tl::Message) -> Result<(), DeserializeError> {
        let container = manual_tl::GzipPacked::from_bytes(&message.body)?;
        let inner = manual_tl::Message {
            body: container.decompress()?,
            ..message
        };
        // The wrapper was already validated and recorded under this msg_id: dispatch the
        // payload directly, or the duplicate check would drop it.
        self.nested(manual_tl::GzipPacked::CONSTRUCTOR_ID, |this| {
            this.dispatch(inner)
        })
    }

    /// Anything else that's not Service Message will be `Updates`.
    ///
    /// Since we handle all the possible service messages, we can
    /// safely treat whatever message body we received as `Updates`.
    fn handle_update(&mut self, message: manual_tl::Message) -> Result<(), DeserializeError> {
        // TODO if this `Updates` cannot be deserialized, `getDifference` should be used
        self.deserialization
            .push(Deserialization::Update(message.body));
        Ok(())
    }
}
