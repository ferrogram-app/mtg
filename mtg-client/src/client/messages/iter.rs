// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Iterators over the messages of a chat, a search in a chat and a search everywhere.

use jiff::Timestamp;
use mtg_mtsender::InvocationError;
use mtg_session::types::PeerRef;
use mtg_tl_types::{self as tl, enums::InputPeer};

use crate::client::{Client, IterBuffer};
use crate::message::Message;

const MAX_LIMIT: usize = 100;

impl<R: tl::RemoteCall<Return = tl::enums::messages::Messages>> IterBuffer<R, Message> {
    /// Fetches the total unless cached.
    ///
    /// The `request.limit` should be set to the right value before calling this method.
    async fn get_total(&mut self) -> Result<usize, InvocationError> {
        if let Some(total) = self.total {
            return Ok(total);
        }

        use tl::enums::messages::Messages;

        let total = match self.client.invoke(&self.request).await? {
            Messages::Messages(messages) => messages.messages.len(),
            Messages::Slice(messages) => messages.count as usize,
            Messages::ChannelMessages(messages) => messages.count as usize,
            Messages::NotModified(messages) => messages.count as usize,
        };
        self.total = Some(total);
        Ok(total)
    }

    /// Performs the network call, fills the buffer, and returns the `offset_rate` if any.
    ///
    /// The `request.limit` should be set to the right value before calling this method.
    async fn fill_buffer(
        &mut self,
        limit: i32,
        reverse: bool,
        peer: Option<PeerRef>,
    ) -> Result<Option<i32>, InvocationError> {
        use tl::enums::messages::Messages;

        let (mut messages, users, chats, rate) = match self.client.invoke(&self.request).await? {
            Messages::Messages(m) => {
                self.last_chunk = true;
                self.total = Some(m.messages.len());
                (m.messages, m.users, m.chats, None)
            }
            Messages::Slice(m) => {
                // Can't rely on `count(messages) < limit` as the stop condition.
                // See https://t.me/tdlibchat/57257 for discussion. As an example:
                // offset_id 132002, limit 4 => we get msg 131999 & 131998
                // offset_id 132002, limit 3 => we get msg 131999
                // offset_id 132002, limit 2 => we get msg 132000
                // offset_id 132002, limit 1 => we get msg 132001
                //
                // When iterating newest-to-oldest, the highest fetched message ID is
                // lower than or equal to the limit, there can't be more messages after
                // (highest ID - limit), because the absolute lowest message ID is 1.
                self.last_chunk =
                    m.messages.is_empty() || (!reverse && m.messages[0].id() <= limit);

                self.total = Some(m.count as usize);

                (m.messages, m.users, m.chats, m.next_rate)
            }
            Messages::ChannelMessages(m) => {
                self.last_chunk =
                    m.messages.is_empty() || (!reverse && m.messages[0].id() <= limit);
                self.total = Some(m.count as usize);
                (m.messages, m.users, m.chats, None)
            }
            Messages::NotModified(_) => {
                return Err(InvocationError::Dropped("unexpected Messages::NotModified"));
            }
        };

        if reverse {
            messages.reverse();
        }

        let peers = self.client.build_peer_map(users, chats).await;

        let client = self.client.clone();
        self.buffer.extend(
            messages
                .into_iter()
                .map(|message| Message::from_raw(&client, message, peer, peers.handle())),
        );

        Ok(rate)
    }
}

/// Iterator returned by [`Client::iter_messages`].
pub struct MessageIter {
    inner: IterBuffer<tl::functions::messages::GetHistory, Message>,
    reverse: bool,
}

impl MessageIter {
    pub(super) fn new(client: &Client, peer: PeerRef) -> Self {
        let inner = IterBuffer::from_request(
            client,
            MAX_LIMIT,
            tl::functions::messages::GetHistory {
                peer: peer.into(),
                offset_id: 0,
                offset_date: 0,
                add_offset: 0,
                limit: 0,
                max_id: 0,
                min_id: 0,
                hash: 0,
            },
        );

        Self {
            inner,
            reverse: false,
        }
    }

    /// Changes the starting message ID (exclusive).
    pub fn offset_id(mut self, offset: i32) -> Self {
        self.inner.request.offset_id = offset;
        self
    }

    /// Changes the starting message date.
    pub fn offset_date(mut self, offset: i32) -> Self {
        self.inner.request.offset_date = offset;
        self
    }

    /// Changes the order to oldest-to-newest. (default is newest-to-oldest)
    pub fn reverse(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.inner = self.inner.limit(limit);
        self
    }

    /// Determines the total number of messages in the chat.
    ///
    /// This only performs a network call if `next` has not been called before.
    pub async fn total(&mut self) -> Result<usize, InvocationError> {
        self.inner.request.limit = 1;
        self.inner.get_total().await
    }

    /// Returns the next `Message` from the internal buffer, filling the buffer previously if it's
    /// empty.
    ///
    /// Returns `None` if the `limit` is reached or there are no messages left.
    pub async fn next(&mut self) -> Result<Option<Message>, InvocationError> {
        if let Some(result) = self.inner.next_raw() {
            return result;
        }

        self.inner.request.limit = self.inner.determine_limit(MAX_LIMIT);
        let request = &mut self.inner.request;
        if self.reverse {
            request.add_offset = -request.limit;
            request.min_id = request.offset_id;
            if request.offset_id == 0 && request.offset_date == 0 {
                // With no explicit offset, start from the oldest available page.
                request.offset_id = 1;
                request.add_offset = -request.limit + 1; // +1 to include the first message (ID=1)
            }
        }
        self.inner
            .fill_buffer(
                self.inner.request.limit,
                self.reverse,
                Some(self.inner.request.peer.clone().into()),
            )
            .await?;

        // Don't bother updating offsets if this is the last time stuff has to be fetched.
        if !self.inner.last_chunk && !self.inner.buffer.is_empty() {
            let last = &self.inner.buffer[self.inner.buffer.len() - 1];
            self.inner.request.offset_id = last.id();
            self.inner.request.offset_date = last.date_timestamp();
        }

        Ok(self.inner.pop_item())
    }
}

/// Iterator returned by [`Client::search_messages`].
pub type SearchIter = IterBuffer<tl::functions::messages::Search, Message>;

impl SearchIter {
    pub(super) fn new(client: &Client, peer: PeerRef) -> Self {
        // TODO let users tweak all the options from the request
        Self::from_request(
            client,
            MAX_LIMIT,
            tl::functions::messages::Search {
                peer: peer.into(),
                q: String::new(),
                from_id: None,
                saved_peer_id: None,
                saved_reaction: None,
                top_msg_id: None,
                filter: tl::enums::MessagesFilter::InputMessagesFilterEmpty,
                min_date: 0,
                max_date: 0,
                offset_id: 0,
                add_offset: 0,
                limit: 0,
                max_id: 0,
                min_id: 0,
                hash: 0,
            },
        )
    }

    /// Changes the message identifier upper bound.
    pub fn offset_id(mut self, offset: i32) -> Self {
        self.request.offset_id = offset;
        self
    }

    /// Changes the query of the search. Telegram servers perform a somewhat fuzzy search over
    /// this query (so a word in singular may also return messages with the word in plural, for
    /// example).
    pub fn query(mut self, query: &str) -> Self {
        self.request.q = query.to_string();
        self
    }

    /// Restricts results to messages sent by the logged-in user
    pub fn sent_by_self(mut self) -> Self {
        self.request.from_id = Some(InputPeer::PeerSelf);
        self
    }

    /// Returns only messages with date bigger than date_time.
    ///
    /// ```
    /// use jiff::Timestamp;
    ///
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Search messages sent after Jan 1st, 2021
    /// let min_date: Timestamp = "2022-12-25T00:00:00-00:00".parse().unwrap();
    ///
    /// let mut messages = client.search_messages(peer).min_date(min_date);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn min_date(mut self, date_time: Timestamp) -> Self {
        self.request.min_date = date_time.as_second() as i32;
        self
    }

    /// Returns only messages with date smaller than date_time
    ///
    /// ```
    /// use jiff::Timestamp;
    ///
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Search messages sent before Dec, 25th 2022
    /// let max_date: Timestamp = "2022-12-25T00:00:00-00:00".parse().unwrap();
    ///
    /// let mut messages = client.search_messages(peer).max_date(max_date);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn max_date(mut self, date_time: Timestamp) -> Self {
        self.request.max_date = date_time.as_second() as i32;
        self
    }

    /// Changes the media filter. Only messages with this type of media will be fetched.
    pub fn filter(mut self, filter: tl::enums::MessagesFilter) -> Self {
        self.request.filter = filter;
        self
    }

    /// Determines how many messages there are in total.
    ///
    /// This only performs a network call if `next` has not been called before.
    pub async fn total(&mut self) -> Result<usize, InvocationError> {
        // Unlike most requests, a limit of 0 actually returns 0 and not a default amount
        // (as of layer 120).
        self.request.limit = 0;
        self.get_total().await
    }

    /// Return the next `Message` from the internal buffer, filling the buffer previously if it's
    /// empty.
    ///
    /// Returns `None` if the `limit` is reached or there are no messages left.
    pub async fn next(&mut self) -> Result<Option<Message>, InvocationError> {
        if let Some(result) = self.next_raw() {
            return result;
        }

        self.request.limit = self.determine_limit(MAX_LIMIT);
        self.fill_buffer(
            self.request.limit,
            false,
            Some(self.request.peer.clone().into()),
        )
        .await?;

        // Don't bother updating offsets if this is the last time stuff has to be fetched.
        if !self.last_chunk && !self.buffer.is_empty() {
            let last = &self.buffer[self.buffer.len() - 1];
            self.request.offset_id = last.id();
            self.request.max_date = last.date_timestamp();
        }

        Ok(self.pop_item())
    }
}

/// Iterator returned by [`Client::search_all_messages`].
pub type GlobalSearchIter = IterBuffer<tl::functions::messages::SearchGlobal, Message>;

impl GlobalSearchIter {
    pub(super) fn new(client: &Client) -> Self {
        // TODO let users tweak all the options from the request
        Self::from_request(
            client,
            MAX_LIMIT,
            tl::functions::messages::SearchGlobal {
                folder_id: None,
                q: String::new(),
                filter: tl::enums::MessagesFilter::InputMessagesFilterEmpty,
                min_date: 0,
                max_date: 0,
                offset_rate: 0,
                offset_peer: tl::enums::InputPeer::Empty,
                offset_id: 0,
                limit: 0,
                broadcasts_only: false,
                groups_only: false,
                users_only: false,
                community: None,
            },
        )
    }

    /// Changes the message identifier upper bound.
    pub fn offset_id(mut self, offset: i32) -> Self {
        self.request.offset_id = offset;
        self
    }

    /// Changes the query of the search. Telegram servers perform a somewhat fuzzy search over
    /// this query (so a word in singular may also return messages with the word in plural, for
    /// example).
    pub fn query(mut self, query: &str) -> Self {
        self.request.q = query.to_string();
        self
    }

    /// Changes the media filter. Only messages with this type of media will be fetched.
    pub fn filter(mut self, filter: tl::enums::MessagesFilter) -> Self {
        self.request.filter = filter;
        self
    }

    /// Determines how many messages there are in total.
    ///
    /// This only performs a network call if `next` has not been called before.
    pub async fn total(&mut self) -> Result<usize, InvocationError> {
        self.request.limit = 1;
        self.get_total().await
    }

    /// Return the next `Message` from the internal buffer, filling the buffer previously if it's
    /// empty.
    ///
    /// Returns `None` if the `limit` is reached or there are no messages left.
    pub async fn next(&mut self) -> Result<Option<Message>, InvocationError> {
        if let Some(result) = self.next_raw() {
            return result;
        }

        self.request.limit = self.determine_limit(MAX_LIMIT);
        let offset_rate = self.fill_buffer(self.request.limit, false, None).await?;

        // Don't bother updating offsets if this is the last time stuff has to be fetched.
        if !self.last_chunk && !self.buffer.is_empty() {
            let last = &self.buffer[self.buffer.len() - 1];
            self.request.offset_rate = offset_rate.unwrap_or(0);
            self.request.offset_peer = last
                .peer_ref()
                .await?
                .map(|peer| peer.into())
                .unwrap_or(tl::enums::InputPeer::Empty);
            self.request.offset_id = last.id();
        }

        Ok(self.pop_item())
    }
}
