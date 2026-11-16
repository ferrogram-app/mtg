// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;
#[cfg(feature = "fs")]
use std::path::Path;

use jiff::Timestamp;
use mtg_mtsender::InvocationError;
use mtg_session::types::{PeerId, PeerKind, PeerRef};
use mtg_tl_types as tl;

use super::{InputMessage, InputReactions};
use crate::Client;
use crate::media::{InputMedia, Media, Photo};
#[cfg(any(feature = "markdown", feature = "html"))]
use crate::parsers;
use crate::peer::{Peer, PeerMap};
use crate::utils;

mod actions;

static NO_PEER_REF_REASON: &str = "cannot invoke request without a peer ref";

/// Represents a Telegram message, which includes text messages, messages with media, and service
/// messages.
///
/// This message should be treated as a snapshot in time, that is, if the message is edited while
/// using this object, those changes won't alter this structure.
#[derive(Clone)]
pub struct Message {
    pub raw: tl::enums::Message,
    pub(crate) fetched_in: Option<PeerRef>,
    pub(crate) client: Client,
    // When fetching messages or receiving updates, a set of peers will be present. A single
    // server response contains a lot of peers, and some might be related to deep layers of
    // a message action for instance. Keeping the entire set like this allows for cheaper clones
    // and moves, and saves us from worrying about picking out all the peers we care about.
    pub(crate) peers: PeerMap,
}

impl Message {
    pub fn from_raw(
        client: &Client,
        message: tl::enums::Message,
        fetched_in: Option<PeerRef>,
        peers: PeerMap,
    ) -> Self {
        Self {
            raw: message,
            fetched_in,
            client: client.clone(),
            peers,
        }
    }

    pub fn from_raw_short_updates(
        client: &Client,
        updates: tl::types::UpdateShortSentMessage,
        input: InputMessage,
        peer: PeerRef,
    ) -> Self {
        Self {
            raw: tl::enums::Message::Message(tl::types::Message {
                out: updates.out,
                mentioned: false,
                media_unread: false,
                silent: input.silent,
                post: false, // TODO true if sent to broadcast channel
                from_scheduled: false,
                legacy: false,
                edit_hide: false,
                pinned: false,
                noforwards: false, // TODO true if channel has noforwads?
                video_processing_pending: false,
                paid_suggested_post_stars: false,
                invert_media: input.invert_media,
                id: updates.id,
                from_id: None, // TODO self
                from_boosts_applied: None,
                from_rank: None,
                peer_id: peer.id.into(),
                saved_peer_id: None,
                fwd_from: None,
                via_bot_id: None,
                reply_to: input.reply_to.map(|reply_to_msg_id| {
                    tl::types::MessageReplyHeader {
                        reply_to_scheduled: false,
                        forum_topic: false,
                        quote: false,
                        reply_to_ephemeral: false,
                        reply_to_msg_id: Some(reply_to_msg_id),
                        reply_to_peer_id: None,
                        reply_from: None,
                        reply_media: None,
                        reply_to_top_id: None,
                        quote_text: None,
                        quote_entities: None,
                        quote_offset: None,
                        todo_item_id: None,
                        poll_option: None,
                        quote_rich: None,
                    }
                    .into()
                }),
                date: updates.date,
                message: input.text,
                media: updates.media,
                reply_markup: input.reply_markup,
                entities: updates.entities,
                views: None,
                forwards: None,
                replies: None,
                edit_date: None,
                post_author: None,
                grouped_id: None,
                restriction_reason: None,
                ttl_period: updates.ttl_period,
                reactions: None,
                quick_reply_shortcut_id: None,
                via_business_bot_id: None,
                guestchat_via_from: None,
                offline: false,
                effect: None,
                factcheck: None,
                report_delivery_until_date: None,
                paid_message_stars: None,
                paid_suggested_post_ton: false,
                suggested_post: None,
                schedule_repeat_period: None,
                summary_from_language: None,
                rich_message: None,
            }),
            fetched_in: Some(peer),
            client: client.clone(),
            peers: client.empty_peer_map(),
        }
    }

    /// Whether the message is outgoing (i.e. you sent this message to some other peer) or
    /// incoming (i.e. someone else sent it to you or the peer).
    pub fn outgoing(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.out,
            tl::enums::Message::Service(message) => message.out,
        }
    }

    /// Whether you were mentioned in this message or not.
    ///
    /// This includes @username mentions, text mentions, and messages replying to one of your
    /// previous messages (even if it contains no mention in the message text).
    pub fn mentioned(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.mentioned,
            tl::enums::Message::Service(message) => message.mentioned,
        }
    }

    /// Whether you have read the media in this message or not.
    ///
    /// Most commonly, these are voice notes that you have not played yet.
    pub fn media_unread(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.media_unread,
            tl::enums::Message::Service(message) => message.media_unread,
        }
    }

    /// Whether the message should notify people with sound or not.
    pub fn silent(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.silent,
            tl::enums::Message::Service(message) => message.silent,
        }
    }

    /// Whether this message is a post in a broadcast channel or not.
    pub fn post(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.post,
            tl::enums::Message::Service(message) => message.post,
        }
    }

    /// Whether this message was originated from a previously-scheduled message or not.
    pub fn from_scheduled(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.from_scheduled,
            tl::enums::Message::Service(_) => false,
        }
    }

    // `legacy` is not exposed, though it can be if it proves to be useful

    /// Whether the edited mark of this message is edited should be hidden (e.g. in GUI clients)
    /// or shown.
    pub fn edit_hide(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.edit_hide,
            tl::enums::Message::Service(_) => false,
        }
    }

    /// Whether this message is currently pinned or not.
    pub fn pinned(&self) -> bool {
        match &self.raw {
            tl::enums::Message::Empty(_) => false,
            tl::enums::Message::Message(message) => message.pinned,
            tl::enums::Message::Service(_) => false,
        }
    }

    /// The ID of this message.
    ///
    /// Message identifiers are counters that start at 1 and grow by 1 for each message produced.
    ///
    /// Every channel has its own unique message counter. This counter is the same for all users,
    /// but unique to each channel.
    ///
    /// Every account has another unique message counter which is used for private conversations
    /// and small group chats. This means different accounts will likely have different message
    /// identifiers for the same message in a private conversation or small group chat. This also
    /// implies that the message identifier alone is enough to uniquely identify the message,
    /// without the need to know the chat ID.
    ///
    /// **You cannot use the message ID of User A when running as User B**, unless this message
    /// belongs to a megagroup or broadcast channel. Beware of this when using methods like
    /// [`Client::delete_messages`], which **cannot** validate the peer where the message
    /// should be deleted for those cases.
    pub fn id(&self) -> i32 {
        self.raw.id()
    }

    /// The [`Self::peer`]'s identifier.
    pub fn peer_id(&self) -> PeerId {
        utils::peer_from_message(&self.raw)
            .map(PeerId::from)
            .or_else(|| self.fetched_in.map(|peer| peer.id))
            .expect("empty messages from updates should contain peer_id")
    }

    /// Cached reference to the [`Self::peer`], if it is in cache.
    pub async fn peer_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        match self.fetched_in {
            Some(peer) => Ok(Some(peer)),
            None => self.peers.get_ref(self.peer_id()).await,
        }
    }

    /// The peer where this message was sent to.
    ///
    /// This might be the user you're talking to for private conversations, or the group or
    /// channel where the message was sent.
    pub fn peer(&self) -> Option<&Peer> {
        self.peers.get(self.peer_id())
    }

    /// The [`Self::sender`]'s identifier, if there is a sender.
    pub fn sender_id(&self) -> Option<PeerId> {
        let from_id = match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.from_id.clone().map(PeerId::from),
            tl::enums::Message::Service(message) => message.from_id.clone().map(PeerId::from),
        };
        from_id.or_else(|| {
            // Incoming messages in private conversations don't include `from_id` since
            // layer 119, but the sender can only be the peer we're in.
            let peer_id = self.peer_id();
            if matches!(peer_id.kind(), PeerKind::User) {
                if self.outgoing() {
                    Some(PeerId::self_user())
                } else {
                    Some(peer_id)
                }
            } else {
                None
            }
        })
    }

    /// Cached reference to the [`Self::sender`], if there is a sender and they are in cache.
    pub async fn sender_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        match self.sender_id() {
            None => Ok(None),
            Some(x) => self.peers.get_ref(x).await,
        }
    }

    /// The sender of this message, if there is a sender **and** the sender is in cache.
    pub fn sender(&self) -> Option<&Peer> {
        self.sender_id().and_then(|id| self.peers.get(id))
    }

    /// If this message was forwarded from a previous message, return the header with information
    /// about that forward.
    pub fn forward_header(&self) -> Option<tl::enums::MessageFwdHeader> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.fwd_from.clone(),
            tl::enums::Message::Service(_) => None,
        }
    }

    /// If this message was sent @via some inline bot, return the bot's user identifier.
    pub fn via_bot_id(&self) -> Option<i64> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.via_bot_id,
            tl::enums::Message::Service(_) => None,
        }
    }

    /// The [`Self::guestchat_via_from`]'s identifier, if there is a sender.
    pub fn guestchat_via_from_id(&self) -> Option<PeerId> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => {
                message.guestchat_via_from.clone().map(PeerId::from)
            }
            tl::enums::Message::Service(_) => None,
        }
    }

    /// Cached reference to the [`Self::guestchat_via_from`], if there is a guestchat_via_from and they are in cache.
    pub async fn guestchat_via_from_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        match self.guestchat_via_from_id() {
            None => Ok(None),
            Some(x) => self.peers.get_ref(x).await,
        }
    }

    /// If the message was posted by a guest bot, the peer on whose behalf the bot sent the message.
    pub fn guestchat_via_from(&self) -> Option<&Peer> {
        self.guestchat_via_from_id()
            .and_then(|id| self.peers.get(id))
    }

    /// If this message is replying to a previous message, return the header with information
    /// about that reply.
    pub fn reply_header(&self) -> Option<tl::enums::MessageReplyHeader> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.reply_to.clone(),
            tl::enums::Message::Service(message) => message.reply_to.clone(),
        }
    }

    pub(crate) fn date_timestamp(&self) -> i32 {
        match &self.raw {
            tl::enums::Message::Empty(_) => 0,
            tl::enums::Message::Message(message) => message.date,
            tl::enums::Message::Service(message) => message.date,
        }
    }

    /// The date when this message was produced.
    pub fn date(&self) -> Timestamp {
        utils::date(self.date_timestamp())
    }

    /// The message's text.
    ///
    /// For service, empty or rich messages, this will be the empty strings.
    ///
    /// If the message has media, this text is the caption commonly displayed underneath it.
    pub fn text(&self) -> &str {
        match &self.raw {
            tl::enums::Message::Empty(_) => "",
            tl::enums::Message::Message(message) => &message.message,
            tl::enums::Message::Service(_) => "",
        }
    }

    /// The message's rich message content if any.
    pub fn rich_message(&self) -> Option<&tl::enums::RichMessage> {
        match &self.raw {
            tl::enums::Message::Message(tl::types::Message { rich_message, .. }) => {
                rich_message.as_ref()
            }
            _ => None,
        }
    }

    fn entities(&self) -> Option<&Vec<tl::enums::MessageEntity>> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.entities.as_ref(),
            tl::enums::Message::Service(_) => None,
        }
    }

    /// Like [`text`](Self::text), but with the [`fmt_entities`](Self::fmt_entities)
    /// applied to produce a markdown string instead.
    ///
    /// Some formatting entities automatically added by Telegram, such as bot commands or
    /// clickable emails, are ignored in the generated string, as those do not need to be
    /// sent for Telegram to include them in the message.
    ///
    /// Formatting entities which cannot be represented in CommonMark without resorting to HTML,
    /// such as underline, are also ignored.
    #[cfg(feature = "markdown")]
    pub fn markdown_text(&self) -> String {
        if let Some(entities) = self.entities() {
            parsers::generate_markdown_message(self.text(), entities)
        } else {
            self.text().to_owned()
        }
    }

    /// Like [`text`](Self::text), but with the [`fmt_entities`](Self::fmt_entities)
    /// applied to produce a HTML string instead.
    ///
    /// Some formatting entities automatically added by Telegram, such as bot commands or
    /// clickable emails, are ignored in the generated string, as those do not need to be
    /// sent for Telegram to include them in the message.
    #[cfg(feature = "html")]
    pub fn html_text(&self) -> String {
        if let Some(entities) = self.entities() {
            parsers::generate_html_message(self.text(), entities)
        } else {
            self.text().to_owned()
        }
    }

    /// The media displayed by this message, if any.
    ///
    /// This not only includes photos or videos, but also contacts, polls, documents, locations
    /// and many other types.
    pub fn media(&self) -> Option<Media> {
        let media = match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.media.clone(),
            tl::enums::Message::Service(_) => None,
        };
        media.and_then(Media::from_raw)
    }

    /// If the message has a reply markup (which can happen for messages produced by bots),
    /// returns said markup.
    pub fn reply_markup(&self) -> Option<tl::enums::ReplyMarkup> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.reply_markup.clone(),
            tl::enums::Message::Service(_) => None,
        }
    }

    /// The formatting entities used to format this message, such as bold, italic, with their
    /// offsets and lengths.
    pub fn fmt_entities(&self) -> Option<&Vec<tl::enums::MessageEntity>> {
        // TODO correct the offsets and lengths to match the byte offsets
        self.entities()
    }

    /// How many views does this message have, when applicable.
    ///
    /// The same user account can contribute to increment this counter indefinitedly, however
    /// there is a server-side cooldown limitting how fast it can happen (several hours).
    pub fn view_count(&self) -> Option<i32> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.views,
            tl::enums::Message::Service(_) => None,
        }
    }

    /// How many times has this message been forwarded, when applicable.
    pub fn forward_count(&self) -> Option<i32> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.forwards,
            tl::enums::Message::Service(_) => None,
        }
    }

    /// How many replies does this message have, when applicable.
    pub fn reply_count(&self) -> Option<i32> {
        match &self.raw {
            tl::enums::Message::Message(tl::types::Message {
                replies: Some(tl::enums::MessageReplies::Replies(replies)),
                ..
            }) => Some(replies.replies),
            _ => None,
        }
    }

    /// React to this message.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(message: mtg_client::message::Message, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// message.react("👍").await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Make animation big & Add to recent
    ///
    /// ```
    /// # async fn f(message: mtg_client::message::Message, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::InputReactions;
    ///
    /// let reactions = InputReactions::emoticon("🤯").big().add_to_recent();
    ///
    /// message.react(reactions).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// Remove reactions
    ///
    /// ```
    /// # async fn f(message: mtg_client::message::Message, client: mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::InputReactions;
    ///
    /// message.react(InputReactions::remove()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn react<R: Into<InputReactions>>(
        &self,
        reactions: R,
    ) -> Result<(), InvocationError> {
        self.client
            .send_reactions(
                self.peer_ref()
                    .await?
                    .ok_or(InvocationError::Dropped(NO_PEER_REF_REASON))?,
                self.id(),
                reactions,
            )
            .await?;
        Ok(())
    }

    /// How many reactions does this message have, when applicable.
    pub fn reaction_count(&self) -> Option<i32> {
        match &self.raw {
            tl::enums::Message::Message(tl::types::Message {
                reactions: Some(tl::enums::MessageReactions::Reactions(reactions)),
                ..
            }) => {
                let count = reactions
                    .results
                    .iter()
                    .map(|reaction: &tl::enums::ReactionCount| {
                        let tl::enums::ReactionCount::Count(reaction) = reaction;
                        reaction.count
                    })
                    .sum();
                Some(count)
            }
            _ => None,
        }
    }

    /// The date when this message was last edited.
    pub fn edit_date(&self) -> Option<Timestamp> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.edit_date.map(utils::date),
            tl::enums::Message::Service(_) => None,
        }
    }

    /// If this message was sent to a channel, return the name used by the author to post it.
    pub fn post_author(&self) -> Option<&str> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.post_author.as_deref(),
            tl::enums::Message::Service(_) => None,
        }
    }

    /// If this message belongs to a group of messages, return the unique identifier for that
    /// group.
    ///
    /// This applies to albums of media, such as multiple photos grouped together.
    ///
    /// Note that there may be messages sent in between the messages forming a group.
    pub fn grouped_id(&self) -> Option<i64> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.grouped_id,
            tl::enums::Message::Service(_) => None,
        }
    }

    /// A list of reasons on why this message is restricted.
    ///
    /// The message is not restricted if the return value is `None`.
    pub fn restriction_reason(&self) -> Option<&Vec<tl::enums::RestrictionReason>> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(message) => message.restriction_reason.as_ref(),
            tl::enums::Message::Service(_) => None,
        }
    }

    /// If this message is a service message, return the service action that occured.
    pub fn action(&self) -> Option<&tl::enums::MessageAction> {
        match &self.raw {
            tl::enums::Message::Empty(_) => None,
            tl::enums::Message::Message(_) => None,
            tl::enums::Message::Service(message) => Some(&message.action),
        }
    }

    /// If this message is replying to another message, return the replied message ID.
    pub fn reply_to_message_id(&self) -> Option<i32> {
        match &self.raw {
            tl::enums::Message::Message(tl::types::Message {
                reply_to: Some(tl::enums::MessageReplyHeader::Header(header)),
                ..
            }) => header.reply_to_msg_id,
            _ => None,
        }
    }

    /// Get photo attached to the message if any.
    pub fn photo(&self) -> Option<Photo> {
        if let Media::Photo(photo) = self.media()? {
            return Some(photo);
        }

        None
    }
}

impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("id", &self.id())
            .field("outgoing", &self.outgoing())
            .field("date", &self.date())
            .field("text", &self.text())
            .field("rich_message", &self.rich_message())
            .field("peer", &self.peer())
            .field("sender", &self.sender())
            .field("reply_to_message_id", &self.reply_to_message_id())
            .field("via_bot_id", &self.via_bot_id())
            .field("media", &self.media())
            .field("mentioned", &self.mentioned())
            .field("media_unread", &self.media_unread())
            .field("silent", &self.silent())
            .field("post", &self.post())
            .field("from_scheduled", &self.from_scheduled())
            .field("edit_hide", &self.edit_hide())
            .field("pinned", &self.pinned())
            .field("forward_header", &self.forward_header())
            .field("reply_header", &self.reply_header())
            .field("reply_markup", &self.reply_markup())
            .field("fmt_entities", &self.fmt_entities())
            .field("view_count", &self.view_count())
            .field("forward_count", &self.forward_count())
            .field("reply_count", &self.reply_count())
            .field("edit_date", &self.edit_date())
            .field("post_author", &self.post_author())
            .field("grouped_id", &self.grouped_id())
            .field("restriction_reason", &self.restriction_reason())
            .field("action", &self.action())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;
    use mtg_tl_types::Serializable;

    use super::*;
    use crate::test_util::{channel_ref, client, user_ref};

    fn plain(c: &Client, peer: PeerRef) -> Message {
        let sent = tl::types::UpdateShortSentMessage {
            out: true,
            id: 11,
            pts: 1,
            pts_count: 1,
            date: 50,
            media: None,
            entities: None,
            ttl_period: None,
        };
        Message::from_raw_short_updates(
            c,
            sent,
            InputMessage::new().text("hi").reply_to(Some(3)),
            peer,
        )
    }

    fn inner(m: &mut Message) -> &mut tl::types::Message {
        let tl::enums::Message::Message(inner) = &mut m.raw else {
            panic!("not a plain message")
        };
        inner
    }

    fn service(c: &Client, peer: PeerRef) -> Message {
        let mut b = 0u32.to_le_bytes().to_vec();
        b.extend(8i32.to_le_bytes());
        b.extend(tl::enums::Peer::from(peer.id).to_bytes());
        b.extend(70i32.to_le_bytes());
        b.extend(tl::enums::MessageAction::Empty.to_bytes());
        let raw = tl::enums::Message::Service(
            <tl::types::MessageService as mtg_tl_types::Deserializable>::from_bytes(&b).unwrap(),
        );
        Message::from_raw(c, raw, Some(peer), c.empty_peer_map())
    }

    fn empty(c: &Client, peer: PeerRef) -> Message {
        let raw = tl::enums::Message::Empty(tl::types::MessageEmpty {
            id: 2,
            peer_id: None,
        });
        Message::from_raw(c, raw, Some(peer), c.empty_peer_map())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_message_built_from_a_short_answer_has_the_basics() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let m = plain(&c, user_ref(5));
        assert_eq!((m.id(), m.text(), m.outgoing()), (11, "hi", true));
        assert_eq!(m.date_timestamp(), 50);
        assert_eq!(m.date().as_second(), 50);
        assert_eq!(m.reply_to_message_id(), Some(3));
        assert!(m.reply_header().is_some());
        assert_eq!(m.peer_id(), user_ref(5).id);
        assert!(m.peer().is_none());
        assert!(m.media().is_none() && m.photo().is_none());
        assert!(m.reply_markup().is_none() && m.rich_message().is_none());
        assert!(m.fmt_entities().is_none() && m.action().is_none());
        assert_eq!(m.peer_ref().await.unwrap(), Some(user_ref(5)));
        for flag in [
            m.mentioned(),
            m.media_unread(),
            m.silent(),
            m.post(),
            m.from_scheduled(),
            m.edit_hide(),
            m.pinned(),
        ] {
            assert!(!flag);
        }
        assert_eq!(m.html_text(), "hi");
        assert_eq!(m.markdown_text(), "hi");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn flags_and_optional_fields_are_read_from_a_full_message() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let mut m = plain(&c, user_ref(5));
        let raw = inner(&mut m);
        raw.mentioned = true;
        raw.media_unread = true;
        raw.silent = true;
        raw.post = true;
        raw.from_scheduled = true;
        raw.edit_hide = true;
        raw.pinned = true;
        raw.via_bot_id = Some(77);
        raw.views = Some(10);
        raw.forwards = Some(2);
        raw.edit_date = Some(60);
        raw.post_author = Some("me".into());
        raw.grouped_id = Some(99);
        raw.restriction_reason = Some(vec![tl::enums::RestrictionReason::Reason(
            tl::types::RestrictionReason {
                platform: "all".into(),
                reason: "r".into(),
                text: "t".into(),
            },
        )]);
        raw.guestchat_via_from = Some(tl::enums::Peer::User(tl::types::PeerUser { user_id: 8 }));
        raw.from_id = Some(tl::enums::Peer::User(tl::types::PeerUser { user_id: 9 }));
        raw.replies = Some(tl::enums::MessageReplies::Replies(
            tl::types::MessageReplies {
                comments: true,
                replies: 4,
                replies_pts: 1,
                recent_repliers: None,
                channel_id: None,
                max_id: None,
                read_max_id: None,
            },
        ));
        raw.reactions = Some(tl::enums::MessageReactions::Reactions(
            tl::types::MessageReactions {
                min: false,
                can_see_list: false,
                reactions_as_tags: false,
                results: vec![
                    tl::enums::ReactionCount::Count(tl::types::ReactionCount {
                        chosen_order: None,
                        reaction: tl::enums::Reaction::Empty,
                        count: 3,
                    }),
                    tl::enums::ReactionCount::Count(tl::types::ReactionCount {
                        chosen_order: None,
                        reaction: tl::enums::Reaction::Empty,
                        count: 4,
                    }),
                ],
                recent_reactions: None,
                top_reactors: None,
            },
        ));
        raw.entities = Some(vec![
            tl::types::MessageEntityBold {
                offset: 0,
                length: 2,
            }
            .into(),
        ]);

        assert!(m.mentioned() && m.media_unread() && m.silent() && m.post());
        assert!(m.from_scheduled() && m.edit_hide() && m.pinned());
        assert_eq!(m.via_bot_id(), Some(77));
        assert_eq!(
            (m.view_count(), m.forward_count(), m.reply_count()),
            (Some(10), Some(2), Some(4))
        );
        assert_eq!(m.reaction_count(), Some(7));
        assert_eq!(m.edit_date().unwrap().as_second(), 60);
        assert_eq!((m.post_author(), m.grouped_id()), (Some("me"), Some(99)));
        assert_eq!(m.restriction_reason().unwrap().len(), 1);
        assert_eq!(m.sender_id(), Some(PeerId::user_unchecked(9)));
        assert_eq!(m.guestchat_via_from_id(), Some(PeerId::user_unchecked(8)));
        assert!(m.guestchat_via_from().is_none() && m.sender().is_none());
        assert!(m.sender_ref().await.unwrap().is_none());
        assert!(m.guestchat_via_from_ref().await.unwrap().is_none());
        assert_eq!(m.fmt_entities().unwrap().len(), 1);
        assert_eq!(m.html_text(), "<b>hi</b>");
        assert_eq!(m.markdown_text(), "**hi**");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_sender_of_a_private_message_follows_who_sent_it() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let mut m = plain(&c, user_ref(5));
        assert_eq!(m.sender_id(), Some(PeerId::self_user()));
        inner(&mut m).out = false;
        assert_eq!(m.sender_id(), Some(user_ref(5).id));
        let group = plain(&c, channel_ref(9));
        assert_eq!(group.sender_id(), None);
        assert!(group.guestchat_via_from_id().is_none());
        assert!(group.sender_ref().await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn service_and_empty_messages_answer_with_defaults() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let peer = user_ref(5);
        let s = service(&c, peer);
        assert_eq!((s.id(), s.date_timestamp(), s.text()), (8, 70, ""));
        assert!(s.action().is_some());
        assert!(!s.outgoing() && !s.mentioned() && !s.silent() && !s.post() && !s.media_unread());
        assert!(!s.from_scheduled() && !s.edit_hide() && !s.pinned());
        assert!(s.reply_header().is_none() && s.media().is_none() && s.reply_markup().is_none());
        assert!(s.forward_header().is_none() && s.via_bot_id().is_none());
        assert!(s.view_count().is_none() && s.edit_date().is_none() && s.post_author().is_none());
        assert!(s.guestchat_via_from_id().is_none() && s.fmt_entities().is_none());
        assert_eq!(s.sender_id(), Some(peer.id));
        assert!(s.html_text().is_empty());

        let e = empty(&c, peer);
        assert_eq!((e.id(), e.date_timestamp(), e.text()), (2, 0, ""));
        assert!(e.action().is_none() && e.forward_header().is_none() && e.rich_message().is_none());
        assert!(!e.outgoing() && !e.mentioned() && !e.silent() && !e.post() && !e.media_unread());
        assert!(!e.from_scheduled() && !e.edit_hide() && !e.pinned());
        assert!(e.reply_header().is_none() && e.reply_markup().is_none() && e.media().is_none());
        assert!(e.view_count().is_none() && e.forward_count().is_none() && e.edit_date().is_none());
        assert!(
            e.post_author().is_none()
                && e.grouped_id().is_none()
                && e.restriction_reason().is_none()
        );
        assert!(e.via_bot_id().is_none() && e.guestchat_via_from_id().is_none());
        assert!(e.reply_count().is_none() && e.reaction_count().is_none());
        assert!(e.reply_to_message_id().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn debug_output_names_the_message() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let peer = user_ref(5);
        for m in [plain(&c, peer), service(&c, peer), empty(&c, peer)] {
            assert!(format!("{m:?}").starts_with("Message {"));
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reacting_sends_the_reaction_or_needs_a_chat() {
        let server = FakeServer::start().await;
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        server.on(move |req: tl::functions::messages::SendReaction| {
            log.lock().unwrap().push(req.msg_id);
            Ok(tl::enums::Updates::Updates(tl::types::Updates {
                updates: Vec::new(),
                users: Vec::new(),
                chats: Vec::new(),
                date: 0,
                seq: 0,
            }))
        });
        let c = client(&server);
        plain(&c, user_ref(5))
            .react(InputReactions::emoticon("👍"))
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), [11]);
        let mut lost = plain(&c, user_ref(5));
        lost.fetched_in = None;
        assert!(matches!(
            lost.react(InputReactions::remove()).await,
            Err(InvocationError::Dropped(_))
        ));
    }
}
