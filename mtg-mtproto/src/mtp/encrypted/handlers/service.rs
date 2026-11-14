use super::*;

impl Encrypted {
    pub(super) fn handle_state_req(
        &self,
        _message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        // TODO implement
        Ok(())
    }

    /// **[Informational Message regarding Status of Messages]**
    ///
    /// ```tl
    /// msgs_state_info#04deb57d req_msg_id:long info:string = MsgsStateInfo;
    /// ```
    ///
    /// Here, `info` is a string that contains exactly one byte of message
    /// status for each message from the incoming `msg_ids` list:
    ///
    /// * 1 = nothing is known about the message (`msg_id` too low, the other
    ///   party may have forgotten it)
    /// * 2 = message not received (`msg_id` falls within the range of stored
    ///   identifiers; however, the other party has certainly not received a
    ///   message like that)
    /// * 3 = message not received (`msg_id` too high; however, the other
    ///   party has certainly not received it yet)
    /// * 4 = message received (note that this response is also at the same
    ///   time a receipt acknowledgment)
    /// * +8 = message already acknowledged
    /// * +16 = message not requiring acknowledgment
    /// * +32 = RPC query contained in message being processed or processing
    ///   already complete
    /// * +64 = content-related response to message already generated
    /// * +128 = other party knows for a fact that message is already received
    ///
    /// This response does not require an acknowledgment. It is an
    /// acknowledgment of the relevant `msgs_state_req`, in and of itself.
    ///
    /// Note that if it turns out suddenly that the other party does not have
    /// a message that looks like it has been sent to it, the message can
    /// simply be re-sent. Even if the other party should receive two copies
    /// of the message at the same time, the duplicate will be ignored.
    /// (If too much time has passed, and the original `msg_id` is not longer
    /// valid, the message is to be wrapped in `msg_copy`).
    ///
    /// [Informational Message regarding Status of Messages]: https://core.telegram.org/mtproto/service_messages_about_messages#informational-message-regarding-status-of-messages
    pub(super) fn handle_state_info(
        &mut self,
        _message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        // TODO implement
        Ok(())
    }

    /// **[Voluntary Communication of Status of Messages]**
    ///
    /// Either party may voluntarily inform the other party of the status of
    /// the messages transmitted by the other party.
    ///
    /// ```tl
    /// msgs_all_info#8cc0d131 msg_ids:Vector long info:string = MsgsAllInfo;
    /// ```
    ///
    /// All message codes known to this party are enumerated, with the
    /// exception of those for which the +128 and the +16 flags are set.
    /// However, if the +32 flag is set but not +64, then the message status
    /// will still be communicated.
    ///
    /// This message does not require an acknowledgment.
    ///
    /// [Voluntary Communication of Status of Messages]: https://core.telegram.org/mtproto/service_messages_about_messages#voluntary-communication-of-status-of-messages
    pub(super) fn handle_msg_all(
        &mut self,
        _message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        // TODO implement
        Ok(())
    }

    /// **[Extended Voluntary Communication of Status of One Message]**
    ///
    /// Normally used by the server to respond to the receipt of a duplicate
    /// `msg_id`, especially if a response to the message has already been
    /// generated and the response is large. If the response is small, the
    /// server may re-send the answer itself instead. This message can also
    /// be used as a notification instead of resending a large message.
    ///
    /// ```tl
    /// msg_detailed_info#276d3ec6 msg_id:long answer_msg_id:long bytes:int status:int = MsgDetailedInfo;
    /// msg_new_detailed_info#809db6df answer_msg_id:long bytes:int status:int = MsgDetailedInfo;
    /// ```
    ///
    /// The second version is used to notify of messages that were created on
    /// the server not in response to an RPC query (such as notifications of
    /// new messages) and were transmitted to the client some time ago, but
    /// not acknowledged.
    ///
    /// Currently, `status` is always zero. This may change in future.
    ///
    /// This message does not require an acknowledgment.
    ///
    /// [Extended Voluntary Communication of Status of One Message]: https://core.telegram.org/mtproto/service_messages_about_messages#extended-voluntary-communication-of-status-of-one-message
    pub(super) fn handle_detailed_info(
        &mut self,
        message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        let msg_detailed = tl::enums::MsgDetailedInfo::from_bytes(&message.body)?;
        let answer_msg_id = match msg_detailed {
            tl::enums::MsgDetailedInfo::Info(x) => x.answer_msg_id,
            tl::enums::MsgDetailedInfo::MsgNewDetailedInfo(x) => x.answer_msg_id,
        };
        self.pending_ack.push(answer_msg_id);
        self.ask_for_answer(answer_msg_id);
        Ok(())
    }

    /// **[Explicit Request to Re-Send Messages]**
    ///
    /// ```tl
    /// msg_resend_req#7d861a08 msg_ids:Vector long = MsgResendReq;
    /// ```
    ///
    /// The remote party immediately responds by re-sending the requested
    /// messages, normally using the same connection that was used to transmit
    /// the query. If at least one message with requested msg_id does not
    /// exist or has already been forgotten, or has been sent by the
    /// requesting party (known from parity), `MsgsStateInfo` is returned for
    /// all messages requested as if the `MsgResendReq` query had been a
    /// `MsgsStateReq` query as well.
    ///
    /// **[Explicit Request to Re-Send Answers]**
    ///
    /// ```tl
    /// msg_resend_ans_req#8610baeb msg_ids:Vector long = MsgResendReq;
    /// ```
    ///
    /// The remote party immediately responds by re-sending answers to the
    /// requested messages, normally using the same connection that was used
    /// to transmit the query. `MsgsStateInfo` is returned for all messages
    /// requested as if the `MsgResendReq` query had been a MsgsStateReq query
    /// as well.
    ///
    /// [Explicit Request to Re-Send Answers]: https://core.telegram.org/mtproto/service_messages_about_messages#explicit-request-to-re-send-answers
    /// [Explicit Request to Re-Send Messages]: https://core.telegram.org/mtproto/service_messages_about_messages#explicit-request-to-re-send-messages
    pub(super) fn handle_msg_resend(
        &self,
        _message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        // TODO implement
        // `msg_resend_ans_req` seems to never occur (it was even missing from `mtproto.tl`)
        Ok(())
    }

    pub(super) fn handle_msg_copy(
        &self,
        _message: manual_tl::Message,
    ) -> Result<(), DeserializeError> {
        Err(DeserializeError::UnexpectedConstructor {
            id: manual_tl::MessageCopy::CONSTRUCTOR_ID,
        })
    }
}
