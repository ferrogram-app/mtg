// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Iterators over the participants of a chat and the profile photos of a peer.

use std::collections::VecDeque;

use mtg_mtsender::InvocationError;
use mtg_session::types::{PeerKind, PeerRef};
use mtg_tl_types as tl;

use crate::client::{Client, IterBuffer};
use crate::media::Photo;
use crate::message::Message;
use crate::peer::Participant;

const MAX_PARTICIPANT_LIMIT: usize = 200;
const MAX_PHOTO_LIMIT: usize = 100;

enum ParticipantIterInner {
    Empty,
    Chat {
        client: Client,
        chat_id: i64,
        buffer: VecDeque<Participant>,
        total: Option<usize>,
    },
    Channel(IterBuffer<tl::functions::channels::GetParticipants, Participant>),
}

/// Iterator returned by [`Client::iter_participants`].
pub struct ParticipantIter(ParticipantIterInner);

impl ParticipantIter {
    pub(super) fn new(client: &Client, peer: PeerRef) -> Self {
        Self(if peer.id.kind() == PeerKind::Channel {
            ParticipantIterInner::Channel(IterBuffer::from_request(
                client,
                MAX_PARTICIPANT_LIMIT,
                tl::functions::channels::GetParticipants {
                    channel: peer.into(),
                    filter: tl::enums::ChannelParticipantsFilter::ChannelParticipantsRecent,
                    offset: 0,
                    limit: 0,
                    hash: 0,
                },
            ))
        } else if peer.id.kind() == PeerKind::Chat {
            ParticipantIterInner::Chat {
                client: client.clone(),
                chat_id: peer.into(),
                buffer: VecDeque::new(),
                total: None,
            }
        } else {
            ParticipantIterInner::Empty
        })
    }

    /// Determines how many participants there are in total.
    ///
    /// This only performs a network call if `next` has not been called before.
    pub async fn total(&mut self) -> Result<usize, InvocationError> {
        match &self.0 {
            ParticipantIterInner::Empty => Ok(0),
            ParticipantIterInner::Chat { total, .. } => {
                if let Some(total) = total {
                    Ok(*total)
                } else {
                    self.fill_buffer().await
                }
            }
            ParticipantIterInner::Channel(iter) => {
                if let Some(total) = iter.total {
                    Ok(total)
                } else {
                    self.fill_buffer().await
                }
            }
        }
    }

    /// Fills the buffer, and returns the total count.
    async fn fill_buffer(&mut self) -> Result<usize, InvocationError> {
        match &mut self.0 {
            ParticipantIterInner::Empty => Ok(0),
            ParticipantIterInner::Chat {
                client,
                chat_id,
                buffer,
                total,
            } => {
                assert!(buffer.is_empty());
                let tl::enums::messages::ChatFull::Full(full) = client
                    .invoke(&tl::functions::messages::GetFullChat { chat_id: *chat_id })
                    .await?;

                let chat = match full.full_chat {
                    tl::enums::ChatFull::Full(chat) => chat,
                    tl::enums::ChatFull::ChannelFull(_) => panic!(
                        "API returned ChannelFull even though messages::GetFullChat was used"
                    ),
                    tl::enums::ChatFull::CommunityFull(_) => panic!(
                        "API returned CommunityFull even though messages::GetFullChat was used"
                    ),
                };

                // Don't actually care for the chats, just the users.
                let mut peers = client.build_peer_map(full.users, Vec::new()).await;

                let participants = match chat.participants {
                    tl::enums::ChatParticipants::Forbidden(c) => {
                        if let Some(p) = c.self_participant {
                            buffer.push_back(Participant::from_raw_chat(&mut peers, p));
                        }
                        return Ok(buffer.len());
                    }
                    tl::enums::ChatParticipants::Participants(c) => c.participants,
                };

                buffer.extend(
                    participants
                        .into_iter()
                        .map(|p| Participant::from_raw_chat(&mut peers, p)),
                );

                *total = Some(buffer.len());
                Ok(buffer.len())
            }
            ParticipantIterInner::Channel(iter) => {
                assert!(iter.buffer.is_empty());
                use tl::enums::channels::ChannelParticipants::*;

                iter.request.limit = iter.determine_limit(MAX_PARTICIPANT_LIMIT);
                let (count, participants, _, users) =
                    match iter.client.invoke(&iter.request).await? {
                        Participants(p) => (p.count, p.participants, p.chats, p.users),
                        NotModified => {
                            panic!("API returned Dialogs::NotModified even though hash = 0")
                        }
                    };

                // Telegram can return less participants than asked for but the count being higher
                // (for example, count=4825, participants=199, users=200). The missing participant
                // was an admin bot account, not sure why it's not included.
                //
                // In any case we pick whichever size is highest to avoid weird cases like this.
                iter.last_chunk =
                    usize::max(participants.len(), users.len()) < iter.request.limit as usize;
                iter.request.offset += participants.len() as i32;

                // Don't actually care for the chats, just the users.
                let mut peers = iter.client.build_peer_map(users, Vec::new()).await;

                iter.buffer.extend(
                    participants
                        .into_iter()
                        .map(|p| Participant::from_raw_channel(&mut peers, p)),
                );

                iter.total = Some(count as usize);
                Ok(count as usize)
            }
        }
    }

    /// Return the next `Participant` from the internal buffer, filling the buffer previously if
    /// it's empty.
    ///
    /// Returns `None` if the `limit` is reached or there are no participants left.
    pub async fn next(&mut self) -> Result<Option<Participant>, InvocationError> {
        // Need to split the `match` because `fill_buffer()` borrows mutably.
        match &mut self.0 {
            ParticipantIterInner::Empty => {}
            ParticipantIterInner::Chat { buffer, .. } => {
                if buffer.is_empty() {
                    self.fill_buffer().await?;
                }
            }
            ParticipantIterInner::Channel(iter) => {
                if let Some(result) = iter.next_raw() {
                    return result;
                }
                self.fill_buffer().await?;
            }
        }

        match &mut self.0 {
            ParticipantIterInner::Empty => Ok(None),
            ParticipantIterInner::Chat { buffer, .. } => {
                let result = buffer.pop_front();
                if buffer.is_empty() {
                    self.0 = ParticipantIterInner::Empty;
                }
                Ok(result)
            }
            ParticipantIterInner::Channel(iter) => Ok(iter.pop_item()),
        }
    }

    /// apply a filter on fetched participants, note that this filter will apply only on large `Channel` and not small groups
    pub fn filter(mut self, filter: tl::enums::ChannelParticipantsFilter) -> Self {
        match self.0 {
            ParticipantIterInner::Channel(ref mut c) => {
                c.request.filter = filter;
                self
            }
            _ => self,
        }
    }
}

enum ProfilePhotoIterInner {
    User(IterBuffer<tl::functions::photos::GetUserPhotos, Photo>),
    Chat(IterBuffer<tl::functions::messages::Search, Message>),
}

/// Iterator returned by [`Client::iter_profile_photos`].
pub struct ProfilePhotoIter(ProfilePhotoIterInner);

impl ProfilePhotoIter {
    pub(super) fn new(client: &Client, peer: PeerRef) -> Self {
        Self(if matches!(peer.id.kind(), PeerKind::User) {
            ProfilePhotoIterInner::User(IterBuffer::from_request(
                client,
                MAX_PHOTO_LIMIT,
                tl::functions::photos::GetUserPhotos {
                    user_id: peer.into(),
                    offset: 0,
                    max_id: 0,
                    limit: 0,
                },
            ))
        } else {
            ProfilePhotoIterInner::Chat(
                client
                    .search_messages(peer)
                    .filter(tl::enums::MessagesFilter::InputMessagesFilterChatPhotos),
            )
        })
    }

    /// Determines how many profile photos there are in total.
    ///
    /// This only performs a network call if `next` has not been called before.
    pub async fn total(&mut self) -> Result<usize, InvocationError> {
        match &mut self.0 {
            ProfilePhotoIterInner::User(iter) => {
                if let Some(total) = iter.total {
                    Ok(total)
                } else {
                    self.fill_buffer().await
                }
            }
            ProfilePhotoIterInner::Chat(iter) => iter.total().await,
        }
    }

    /// Fills the buffer, and returns the total count.
    async fn fill_buffer(&mut self) -> Result<usize, InvocationError> {
        match &mut self.0 {
            ProfilePhotoIterInner::User(iter) => {
                use tl::enums::photos::Photos;

                iter.request.limit = iter.determine_limit(MAX_PHOTO_LIMIT);
                let (total, photos) = match iter.client.invoke(&iter.request).await? {
                    Photos::Photos(p) => {
                        iter.last_chunk = true;
                        iter.total = Some(p.photos.len());
                        (p.photos.len(), p.photos)
                    }
                    Photos::Slice(p) => {
                        iter.last_chunk = p.photos.len() < iter.request.limit as usize;
                        iter.total = Some(p.count as usize);
                        (p.count as usize, p.photos)
                    }
                };

                // Don't bother updating offsets if this is the last time stuff has to be fetched.
                if !iter.last_chunk && !photos.is_empty() {
                    iter.request.offset += photos.len() as i32;
                }

                iter.buffer.extend(photos.into_iter().map(Photo::from_raw));

                Ok(total)
            }
            ProfilePhotoIterInner::Chat(_) => {
                panic!("fill_buffer should not be called for Chat variant")
            }
        }
    }

    /// Return the next photo from the internal buffer, filling the buffer previously if it's
    /// empty.
    ///
    /// Returns `None` if the `limit` is reached or there are no photos left.
    pub async fn next(&mut self) -> Result<Option<Photo>, InvocationError> {
        // Need to split the `match` because `fill_buffer()` borrows mutably.
        match &mut self.0 {
            ProfilePhotoIterInner::User(iter) => {
                if let Some(result) = iter.next_raw() {
                    return result;
                }
                self.fill_buffer().await?;
            }
            ProfilePhotoIterInner::Chat(iter) => {
                while let Some(message) = iter.next().await? {
                    if let Some(tl::enums::MessageAction::ChatEditPhoto(
                        tl::types::MessageActionChatEditPhoto { photo },
                    )) = message.action()
                    {
                        return Ok(Some(Photo::from_raw(photo.clone())));
                    } else {
                        continue;
                    }
                }
            }
        }

        match &mut self.0 {
            ProfilePhotoIterInner::User(iter) => Ok(iter.pop_item()),
            ProfilePhotoIterInner::Chat(_) => Ok(None),
        }
    }
}
