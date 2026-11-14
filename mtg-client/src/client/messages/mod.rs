// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Methods related to sending messages.

use std::collections::HashMap;

use mtg_mtsender::InvocationError;
use mtg_session::types::{PeerAuth, PeerId, PeerKind, PeerRef};
use mtg_tl_types as tl;
use tracing::{Level, enabled, warn};

use super::Client;
use crate::media::{InputMedia, Media};
use crate::message::{InputMessage, InputReactions, Message};
use crate::utils::{generate_random_id, generate_random_ids};

mod iter;
mod pins;
mod send;

pub use iter::{GlobalSearchIter, MessageIter, SearchIter};

async fn map_random_ids_to_messages(
    client: &Client,
    fetched_in: PeerRef,
    random_ids: &[i64],
    updates: tl::enums::Updates,
) -> Result<Vec<Option<Message>>, Box<dyn std::error::Error + Send + Sync>> {
    match updates {
        tl::enums::Updates::Updates(tl::types::Updates {
            updates,
            users,
            chats,
            date: _,
            seq: _,
        }) => {
            let peers = client.build_peer_map(users, chats).await;

            let rnd_to_id = updates
                .iter()
                .filter_map(|update| match update {
                    tl::enums::Update::MessageId(u) => Some((u.random_id, u.id)),
                    _ => None,
                })
                .collect::<HashMap<_, _>>();

            // TODO ideally this would use the same UpdateIter mechanism to make sure we don't
            //      accidentally miss variants
            let mut id_to_msg = updates
                .into_iter()
                .filter_map(|update| match update {
                    tl::enums::Update::NewMessage(tl::types::UpdateNewMessage {
                        message, ..
                    }) => Some(message),
                    tl::enums::Update::NewChannelMessage(tl::types::UpdateNewChannelMessage {
                        message,
                        ..
                    }) => Some(message),
                    tl::enums::Update::NewScheduledMessage(
                        tl::types::UpdateNewScheduledMessage { message, .. },
                    ) => Some(message),
                    _ => None,
                })
                .map(|message| Message::from_raw(client, message, Some(fetched_in), peers.handle()))
                .map(|message| (message.id(), message))
                .collect::<HashMap<_, _>>();

            Ok(random_ids
                .iter()
                .map(|rnd| {
                    rnd_to_id
                        .get(rnd)
                        .and_then(|id| id_to_msg.remove(id))
                        .or_else(|| {
                            if id_to_msg.len() == 1 {
                                // If there's no random_id to map from, in the common case a single message
                                // should've been produced regardless, so try to recover by returning that.
                                id_to_msg.drain().next().map(|(_, m)| m)
                            } else {
                                None
                            }
                        })
                })
                .collect())
        }
        _ => panic!("API returned something other than Updates so messages can't be mapped"),
    }
}

pub(crate) async fn parse_mention_entities(
    client: &Client,
    mut entities: Vec<tl::enums::MessageEntity>,
) -> Option<Vec<tl::enums::MessageEntity>> {
    if entities.is_empty() {
        return None;
    }

    if entities
        .iter()
        .any(|e| matches!(e, tl::enums::MessageEntity::MentionName(_)))
    {
        for entity in entities.iter_mut() {
            if let tl::enums::MessageEntity::MentionName(mention_name) = entity {
                *entity = tl::types::InputMessageEntityMentionName {
                    offset: mention_name.offset,
                    length: mention_name.length,
                    user_id: tl::enums::InputUser::User(tl::types::InputUser {
                        user_id: mention_name.user_id,
                        access_hash: client
                            .0
                            .session
                            .peer(PeerId::user_unchecked(mention_name.user_id))
                            .await
                            .unwrap_or_default()
                            .and_then(|peer| peer.auth())
                            .unwrap_or_default()
                            .hash(),
                    }),
                }
                .into()
            }
        }
    }

    Some(entities)
}

/// Build a reply-to a plain message by its ID, leaving all other fields unset.
fn input_reply_to(reply_to_msg_id: i32) -> tl::enums::InputReplyTo {
    tl::types::InputReplyToMessage {
        reply_to_msg_id,
        top_msg_id: None,
        reply_to_peer_id: None,
        quote_text: None,
        quote_entities: None,
        quote_offset: None,
        monoforum_peer_id: None,
        todo_item_id: None,
        poll_option: None,
        quote_rich: None,
    }
    .into()
}

/// Flatten a `messages.Messages` response into its messages, users and chats.
///
/// Panics on `NotModified`, which is only returned when a non-zero `hash` was sent.
fn unpack_messages(
    res: tl::enums::messages::Messages,
) -> (
    Vec<tl::enums::Message>,
    Vec<tl::enums::User>,
    Vec<tl::enums::Chat>,
) {
    use tl::enums::messages::Messages;

    match res {
        Messages::Messages(m) => (m.messages, m.users, m.chats),
        Messages::Slice(m) => (m.messages, m.users, m.chats),
        Messages::ChannelMessages(m) => (m.messages, m.users, m.chats),
        Messages::NotModified(_) => {
            panic!("API returned Messages::NotModified even though GetMessages was used")
        }
    }
}

/// Method implementations related to sending, modifying or getting messages.
impl Client {
    /// Gets the [`Message`] to which the input message is replying to.
    ///
    /// See also: [`Message::get_reply`].
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(message: mtg_client::message::Message, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(reply) = client.get_reply_to_message(&message).await? {
    ///     println!("The reply said: {}", reply.text());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_reply_to_message(
        &self,
        message: &Message,
    ) -> Result<Option<Message>, InvocationError> {
        /// Helper method to fetch a single message by its input message.
        async fn get_message(
            client: &Client,
            peer: PeerRef,
            id: tl::enums::InputMessage,
        ) -> Result<(tl::enums::messages::Messages, bool), InvocationError> {
            if peer.id.kind() == PeerKind::Channel {
                client
                    .invoke(&tl::functions::channels::GetMessages {
                        id: vec![id],
                        channel: peer.into(),
                    })
                    .await
                    .map(|res| (res, false))
            } else {
                client
                    .invoke(&tl::functions::messages::GetMessages { id: vec![id] })
                    .await
                    .map(|res| (res, true))
            }
        }

        // TODO shouldn't this method take in a message id anyway?
        let peer_id = message.peer_id();
        let peer = match peer_id.kind() {
            PeerKind::User | PeerKind::Chat => PeerRef {
                id: peer_id,
                auth: PeerAuth::default(), // unused, so no need to bother fetching it
            },
            PeerKind::Channel => message.peer_ref().await?.ok_or(InvocationError::Dropped(
                "could not obtain channel peer ref from message",
            ))?,
        };
        let reply_to_message_id = match message.reply_to_message_id() {
            Some(id) => id,
            None => return Ok(None),
        };

        let input_id =
            tl::enums::InputMessage::ReplyTo(tl::types::InputMessageReplyTo { id: message.id() });

        let (res, filter_req) = match get_message(self, peer, input_id).await {
            Ok(tup) => tup,
            Err(_) => {
                let input_id = tl::enums::InputMessage::Id(tl::types::InputMessageId {
                    id: reply_to_message_id,
                });
                get_message(self, peer, input_id).await?
            }
        };

        let (messages, users, chats) = unpack_messages(res);

        let peers = self.build_peer_map(users, chats).await;
        Ok(messages
            .into_iter()
            .map(|m| Message::from_raw(self, m, Some(peer), peers.handle()))
            .next()
            .filter(|m| !filter_req || m.peer_id() == message.peer_id()))
    }

    /// Iterate over the message history of a peer, from most recent to oldest.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Note we're setting a reasonable limit, or we'd print out ALL the messages in peer!
    /// let mut messages = client.iter_messages(peer).limit(100);
    ///
    /// while let Some(message) = messages.next().await? {
    ///     println!("{}", message.text());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn iter_messages<C: Into<PeerRef>>(&self, peer: C) -> MessageIter {
        MessageIter::new(self, peer.into())
    }

    /// Iterate over the messages that match certain search criteria.
    ///
    /// This allows you to search by text within a peer or filter by media among other things.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Let's print all the people who think grammers is cool.
    /// let mut messages = client.search_messages(peer).query("grammers is cool");
    ///
    /// while let Some(message) = messages.next().await? {
    ///     let sender = message.sender().unwrap();
    ///     println!("{}", sender.name().unwrap_or(&sender.id().to_string()));
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn search_messages<C: Into<PeerRef>>(&self, peer: C) -> SearchIter {
        SearchIter::new(self, peer.into())
    }

    /// Iterate over the messages that match certain search criteria, without being restricted to
    /// searching in a specific peer. The downside is that this global search supports less filters.
    ///
    /// This allows you to search by text within a peer or filter by media among other things.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// // Let's print all the peers were people think grammers is cool.
    /// let mut messages = client.search_all_messages().query("grammers is cool");
    ///
    /// while let Some(message) = messages.next().await? {
    ///     println!("{}", message.peer().unwrap().name().unwrap_or(&message.peer().unwrap().id().to_string()));
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn search_all_messages(&self) -> GlobalSearchIter {
        GlobalSearchIter::new(self)
    }

    /// Get up to 100 messages using their ID.
    ///
    /// Returns the new retrieved messages in a list. Those messages that could not be retrieved
    /// or do not belong to the input peer will be `None`. The length of the resulting list is the
    /// same as the length of the input message IDs, and the indices from the list of IDs map to
    /// the indices in the result so you can map them into the new list.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let message_ids = [123, 456, 789];
    ///
    /// let messages = client.get_messages_by_id(peer, &message_ids).await?;
    /// let count = messages.into_iter().filter(Option::is_some).count();
    /// println!("{} out of {} messages were deleted!", message_ids.len() - count, message_ids.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_messages_by_id<C: Into<PeerRef>>(
        &self,
        peer: C,
        message_ids: &[i32],
    ) -> Result<Vec<Option<Message>>, InvocationError> {
        let peer = peer.into();
        let id = message_ids
            .iter()
            .map(|&id| tl::enums::InputMessage::Id(tl::types::InputMessageId { id }))
            .collect();

        let result = if peer.id.kind() == PeerKind::Channel {
            self.invoke(&tl::functions::channels::GetMessages {
                channel: peer.into(),
                id,
            })
            .await
        } else {
            self.invoke(&tl::functions::messages::GetMessages { id })
                .await
        }?;

        let (messages, users, chats) = unpack_messages(result);

        let peers = self.build_peer_map(users, chats).await;
        let mut map = messages
            .into_iter()
            .map(|m| Message::from_raw(self, m, Some(peer), peers.handle()))
            .filter(|m| m.peer_id() == peer.id)
            .map(|m| (m.id(), m))
            .collect::<HashMap<_, _>>();

        Ok(message_ids.iter().map(|id| map.remove(id)).collect())
    }

    /// Send reaction.
    ///
    /// # Examples
    ///
    /// Via emoticon
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// let message_id = 123;
    ///
    /// client.send_reactions(peer, message_id, "👍").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Make animation big & Add to recent
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::InputReactions;
    ///
    /// let message_id = 123;
    /// let reactions = InputReactions::emoticon("🤯").big().add_to_recent();
    ///
    /// client.send_reactions(peer, message_id, reactions).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Remove reactions
    ///
    /// ```
    /// # async fn f(peer: mtg_session::types::PeerRef, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::InputReactions;
    ///
    /// let message_id = 123;
    ///
    /// client.send_reactions(peer, message_id, InputReactions::remove()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_reactions<C: Into<PeerRef>, R: Into<InputReactions>>(
        &self,
        peer: C,
        message_id: i32,
        reactions: R,
    ) -> Result<(), InvocationError> {
        let reactions = reactions.into();

        self.invoke(&tl::functions::messages::SendReaction {
            big: reactions.big,
            add_to_recent: reactions.add_to_recent,
            peer: peer.into().into(),
            msg_id: message_id,
            reaction: Some(reactions.reactions),
        })
        .await?;

        Ok(())
    }

    /// Convert `InputMedia` to [`Media`].
    pub async fn upload_media<P: Into<PeerRef>, M: Into<tl::enums::InputMedia>>(
        &self,
        peer: P,
        input_media: M,
    ) -> Result<Option<Media>, InvocationError> {
        let media = self
            .invoke(&tl::functions::messages::UploadMedia {
                business_connection_id: None,
                peer: peer.into().into(),
                media: input_media.into(),
            })
            .await?;
        Ok(Media::from_raw(media))
    }
}
