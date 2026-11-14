use super::*;

impl Encrypted {
    /// **[Response to an RPC query]**
    ///
    /// A response to an RPC query is normally wrapped as follows:
    ///
    /// ```tl
    /// rpc_result#f35c6d01 req_msg_id:long result:Object = RpcResult;
    /// ```
    ///
    /// Here `req_msg_id` is the identifier of the message sent by the other
    /// party and containing an RPC query. This way, the recipient knows that
    /// the result is a response to the specific RPC query in question.
    ///
    /// At the same time, this response serves as acknowledgment of the other
    /// party's receipt of the `req_msg_id` message.
    ///
    /// Note that the response to an RPC query must also be acknowledged. Most
    /// frequently, this coincides with the transmission of the next message
    /// (which may have a container attached to it carrying a service message
    /// with the acknowledgment).
    ///
    /// **[RPC Error]**
    ///
    /// The result field returned in response to any RPC query may also
    /// contain an error message in the following format:
    ///
    /// ```tl
    /// rpc_result#f35c6d01 req_msg_id:long result:Object = RpcResult;
    /// ```
    ///
    /// **[Cancellation of an RPC Query]**
    ///
    /// In certain situations, the client does not want to receive a response
    /// to an already transmitted RPC query, for example because the response
    /// turns out to be long and the client has decided to do without it
    /// because of insufficient link capacity. Simply interrupting the TCP
    /// connection will not have any effect because the server would re-send
    /// the missing response at the first opportunity. Therefore, the client
    /// needs a way to cancel receipt of the RPC response message, actually
    /// acknowledging its receipt prior to it being in fact received, which
    /// will settle the server down and prevent it from re-sending the response.
    /// However, the client does not know the RPC response's msg_id prior to
    /// receiving the response; the only thing it knows is the req_msg_id.
    /// i.e. the msg_id of the relevant RPC query. Therefore, a special query
    /// is used:
    ///
    /// ```tl
    /// rpc_drop_answer#58e4a740 req_msg_id:long = RpcDropAnswer;
    /// ```
    ///
    /// The response to this query returns as one of the following messages
    /// wrapped in `rpc_result` and requiring an acknowledgment:
    ///
    /// ```tl
    /// rpc_answer_unknown#5e2ad36e = RpcDropAnswer;
    /// rpc_answer_dropped_running#cd78e586 = RpcDropAnswer;
    /// rpc_answer_dropped#a43ad8b7 msg_id:long seq_no:int bytes:int = RpcDropAnswer;
    /// ```
    ///
    /// The first version of the response is used if the server remembers
    /// nothing of the incoming req_msg_id (if it has already been responded
    /// to, for example). The second version is used if the response was
    /// canceled while the RPC query was being processed (where the RPC query
    /// itself was still fully processed); in this case, the same
    /// `rpc_answer_dropped_running` is also returned in response to the
    /// original query, and both of these responses require an acknowledgment
    /// from the client. The final version means that the RPC response was
    /// removed from the server's outgoing queue, and its `msg_id`, `seq_no`,
    /// and length in `bytes` are transmitted to the client.
    ///
    /// Note that `rpc_answer_dropped_running` and `rpc_answer_dropped` serve
    /// as acknowledgments of the server's receipt of the original query (the
    /// same one, the response to which we wish to forget). In addition, same
    /// as for any RPC queries, any response to `rpc_drop_answer` is an
    /// acknowledgment for `rpc_drop_answer` itself.
    ///
    /// As an alternative to using `rpc_drop_answer`, a new session may be
    /// created after the connection is reset and the old session is removed
    /// through `destroy_session`.
    ///
    /// [Response to an RPC query]: https://core.telegram.org/mtproto/service_messages#response-to-an-rpc-query
    /// [RPC Error]: https://core.telegram.org/mtproto/service_messages#rpc-error
    /// [Cancellation of an RPC Query]: https://core.telegram.org/mtproto/service_messages#cancellation-of-an-rpc-query
    pub(super) fn handle_rpc_result(
        &mut self,
        message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        let rpc_result = manual_tl::RpcResult::from_bytes(&message.body)?;
        let inner_constructor = rpc_result.inner_constructor();
        let manual_tl::RpcResult { req_msg_id, result } = rpc_result;
        let msg_id = MsgId(req_msg_id);

        // Any error during a RPC result will be given to the user,
        // which means this method itself is doing its job `Ok`.
        let inner_constructor = match inner_constructor {
            Ok(x) => x,
            Err(e) => {
                self.deserialization
                    .push(Deserialization::Failure(DeserializationFailure {
                        msg_id,
                        error: e.into(),
                    }));
                return Ok(());
            }
        };

        match inner_constructor {
            // RPC Error
            tl::types::RpcError::CONSTRUCTOR_ID => match tl::enums::RpcError::from_bytes(&result) {
                Ok(tl::enums::RpcError::Error(error)) => self
                    .deserialization
                    .push(Deserialization::RpcError(RpcResultError { msg_id, error })),
                Err(e) => {
                    self.deserialization
                        .push(Deserialization::Failure(DeserializationFailure {
                            msg_id,
                            error: e.into(),
                        }))
                }
            },

            // Cancellation of an RPC Query
            tl::types::RpcAnswerUnknown::CONSTRUCTOR_ID => {
                // The `msg_id` corresponds to the `rpc_drop_answer` request.
            }
            tl::types::RpcAnswerDroppedRunning::CONSTRUCTOR_ID => {
                // We will receive two `rpc_result`, one with the `msg_id` of
                // `rpc_drop_answer` request and other for the original RPC.
            }
            tl::types::RpcAnswerDropped::CONSTRUCTOR_ID => {
                // "the RPC response was removed from the server's outgoing
                // queue, and its msg_id, seq_no, and length in bytes are
                // transmitted to the client."
            }

            // Response to an RPC query
            // Telegram shouldn't send compressed errors (the overhead
            // would probably outweight the benefits) so we don't check
            // that the decompressed payload is an error or answer drop.
            manual_tl::GzipPacked::CONSTRUCTOR_ID => {
                let body = match manual_tl::GzipPacked::from_bytes(&result) {
                    Ok(gzip) => match gzip.decompress() {
                        Ok(x) => {
                            self.store_own_updates(msg_id, &x);
                            Ok(x)
                        }
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(DeserializeError::from(e)),
                };

                match body {
                    Ok(body) => self
                        .deserialization
                        .push(Deserialization::RpcResult(RpcResult { msg_id, body })),
                    Err(e) => self.deserialization.push(Deserialization::Failure(
                        DeserializationFailure { msg_id, error: e },
                    )),
                }
            }
            _ => {
                self.store_own_updates(msg_id, &result);
                self.deserialization
                    .push(Deserialization::RpcResult(RpcResult {
                        msg_id,
                        body: result,
                    }));
            }
        }

        Ok(())
    }

    /// Updates produced by `rpc_result` must be considered as any other updates, since they can
    /// change the `pts`. If this wasn't done, eventually higher levels would find gaps.
    ///
    /// Users may also be interested in handling updates produced by the client as if they were
    /// like any other.
    ///
    /// This MUST be done before storing the [`Deserialization::RpcResult`],
    /// because the stored update might need to reference the request, which
    /// must not have been dropped yet.
    pub(super) fn store_own_updates(&mut self, msg_id: MsgId, body: &[u8]) {
        match u32::from_bytes(body) {
            Ok(body_id) => {
                if UPDATE_IDS.contains(&body_id) {
                    self.deserialization.push(Deserialization::OwnUpdate {
                        msg_id,
                        update: body.to_vec(),
                    });
                }
            }
            Err(_err) => {
                // Failing is fine, it likely means the update was bad and eventually there will
                // be a gap to fill.
            }
        }
    }

    /// **[Acknowledgment of Receipt]**
    ///
    /// Receipt of virtually all messages (with the exception of some purely
    /// service ones as well as the plain-text messages used in the protocol
    /// for creating an Authorization Key) must be acknowledged.
    ///
    /// This requires the use of the following service message (not requiring
    /// an acknowledgment):
    ///
    /// ```tl
    /// msgs_ack#62d6b459 msg_ids:Vector long = MsgsAck;
    /// ```
    ///
    /// A server usually acknowledges the receipt of a message from a client
    /// (normally, an RPC query) using an RPC response. If a response is a
    /// long time coming, a server may first send a receipt acknowledgment,
    /// and somewhat later, the RPC response itself.
    ///
    /// A client normally acknowledges the receipt of a message from a server
    /// (usually, an RPC response) by adding an acknowledgment to the next RPC
    /// query if it is not transmitted too late (if it is generated, say,
    /// 60-120 seconds following the receipt of a message from the server).
    /// However, if for a long period of time there is no reason to send
    /// messages to the server or if there is a large number of
    /// unacknowledged messages from the server (say, over 16), the client
    /// transmits a stand-alone acknowledgment.
    ///
    /// [Acknowledgment of Receipt]: https://core.telegram.org/mtproto/service_messages_about_messages#acknowledgment-of-receipt
    pub(super) fn handle_ack(&self, message: manual_tl::Message) -> Result<(), DeserializeError> {
        // TODO notify about this somehow
        let _ack = tl::enums::MsgsAck::from_bytes(&message.body)?;
        Ok(())
    }

    /// **[Notice of Ignored Error Message]**
    ///
    /// In certain cases, a server may notify a client that its incoming
    /// message was ignored for whatever reason. Note that such a notification
    /// cannot be generated unless a message is correctly decoded by the
    /// server.
    ///
    /// ```tl
    /// bad_msg_notification#a7eff811 bad_msg_id:long bad_msg_seqno:int error_code:int = BadMsgNotification;
    /// bad_server_salt#edab447b bad_msg_id:long bad_msg_seqno:int error_code:int new_server_salt:long = BadMsgNotification;
    /// ```
    ///
    /// Here, `error_code` can also take on the following values:
    ///
    /// * 16: `msg_id` too low (most likely, client time is wrong; it would
    ///   be worthwhile to synchronize it using msg_id notifications and re-
    ///   send the original message with the “correct” msg_id or wrap it in a
    ///   container with a new msg_id if the original message had waited too
    ///   long on the client to be transmitted)
    /// * 17: `msg_id` too high (similar to the previous case, the client time
    ///   has to be synchronized, and the message re-sent with the correct
    ///   `msg_id`)
    /// * 18: incorrect two lower order `msg_id` bits (the server expects
    ///   client message `msg_id` to be divisible by 4)
    /// * 19: container `msg_id` is the same as `msg_id` of a previously
    ///   received message (this must never happen)
    /// * 20: message too old, and it cannot be verified whether the server
    ///   has received a message with this `msg_id` or not
    /// * 32: `msg_seqno` too low (the server has already received a message
    ///   with a lower `msg_id` but with either a higher or an equal and odd
    ///   `seqno`)
    /// * 33: `msg_seqno` too high (similarly, there is a message with a
    ///   higher `msg_id` but with either a lower or an equal and odd `seqno`)
    /// * 34: an even `msg_seqno` expected (irrelevant message), but odd
    ///   received
    /// * 35: odd `msg_seqno` expected (relevant message), but even received
    /// * 48: incorrect server salt (in this case, the `bad_server_salt`
    ///   response is received with the correct salt, and the message is to be
    ///   re-sent with it)
    /// * 64: invalid container.
    ///
    /// The intention is that `error_code` values are grouped
    /// (`error_code >> 4`): for example, the codes `0x40 - 0x4f` correspond
    /// to errors in container decomposition.
    ///
    /// Notifications of an ignored message do not require acknowledgment
    /// (i.e., are irrelevant).
    ///
    /// **Important**: if `server_salt` has changed on the server or if client
    /// time is incorrect, any query will result in a notification in the
    /// above format. The client must check that it has, in fact, recently
    /// sent a message with the specified msg_id, and if that is the case,
    /// update its time correction value (the difference between the client's
    /// and the server's clocks) and the server salt based on msg_id and the
    /// `server_salt` notification, so as to use these to (re)send future
    /// messages. In the meantime, the original message (the one that caused
    /// the error message to be returned) must also be re-sent with a better
    /// `msg_id` and/or `server_salt`.
    ///
    /// In addition, the client can update the `server_salt` value used to
    /// send messages to the server, based on the values of RPC responses or
    /// containers carrying an RPC response, provided that this RPC response
    /// is actually a match for the query sent recently. (If there is doubt,
    /// it is best not to update since there is risk of a replay attack).
    ///
    /// [Notice of Ignored Error Message]: https://core.telegram.org/mtproto/service_messages_about_messages#notice-of-ignored-error-message
    pub(super) fn handle_bad_notification(
        &mut self,
        message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        let bad_msg = tl::enums::BadMsgNotification::from_bytes(&message.body)?;

        if self
            .salt_request_msg_id
            .is_some_and(|msg_id| msg_id == MsgId(bad_msg.bad_msg_id()))
        {
            // Response to internal request, do not propagate.
            self.salt_request_msg_id = None;
        } else {
            self.deserialization
                .push(Deserialization::BadMessage(crate::mtp::BadMessage {
                    msg_id: MsgId(bad_msg.bad_msg_id()),
                    code: bad_msg.error_code(),
                }));
        }

        let bad_msg = match bad_msg {
            tl::enums::BadMsgNotification::Notification(x) => x,
            tl::enums::BadMsgNotification::BadServerSalt(x) => {
                self.salts.clear();
                self.salts.push(tl::types::FutureSalt {
                    valid_since: 0,
                    valid_until: i32::MAX,
                    salt: x.new_server_salt,
                });
                self.salt_request_msg_id = None;
                // The server's id is the best clock we have, and ours must not go below the
                // one it just rejected. The single salt left makes `push` ask for more.
                self.correct_time_offset(message.msg_id);
                self.last_msg_id = self.last_msg_id.max(message.msg_id);

                info!("got bad salt; salts have been reset down to a single one");
                return Ok(());
            }
        };

        match bad_msg.error_code {
            16 => {
                // Sent `msg_id` was too low (our `time_offset` is wrong).
                self.correct_time_offset(message.msg_id);
            }
            17 => {
                // Sent `msg_id` was too high (our `time_offset` is wrong).
                self.correct_time_offset(message.msg_id);
            }
            19 | 32 | 33 | 64 => {
                // Sequence numbers or containers the server does not accept: the official
                // client starts a new session instead of guessing a correction.
                self.recreate_session();
            }
            _ => {
                // Just notify about it.
            }
        }

        Ok(())
    }

    /// **[Request for Message Status Information]**
    ///
    /// If either party has not received information on the status of its
    /// outgoing messages for a while, it may explicitly request it from the
    /// other party:
    ///
    /// ```tl
    /// msgs_state_req#da69fb52 msg_ids:Vector long = MsgsStateReq;
    /// ```
    ///
    /// [Request for Message Status Information]: https://core.telegram.org/mtproto/service_messages_about_messages#request-for-message-status-information
    /// Queues a `msg_resend_req` for an answer the server announced, unless we have it already
    /// or asked for it in the last minute.
    pub(super) fn ask_for_answer(&mut self, answer_msg_id: i64) {
        if self.seen_msg_ids.contains(&answer_msg_id) {
            return;
        }
        self.resend_asked
            .retain(|(_, at)| at.elapsed() < RESEND_ASK_INTERVAL);
        if self.resend_asked.iter().any(|(id, _)| *id == answer_msg_id) {
            return;
        }
        self.resend_asked.push((answer_msg_id, Instant::now()));
        self.pending_resend.push(answer_msg_id);
    }
}
