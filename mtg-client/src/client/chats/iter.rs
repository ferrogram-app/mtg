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
                    tl::enums::ChatFull::ChannelFull(_) | tl::enums::ChatFull::CommunityFull(_) => {
                        return Err(InvocationError::Dropped(
                            "unexpected non-chat ChatFull from messages.getFullChat",
                        ));
                    }
                };

                // Don't actually care for the chats, just the users.
                let mut peers = client.build_peer_map(full.users, Vec::new()).await;

                let participants = match chat.participants {
                    tl::enums::ChatParticipants::Forbidden(c) => {
                        if let Some(p) = c.self_participant {
                            buffer.extend(Participant::from_raw_chat(&mut peers, p));
                        }
                        return Ok(buffer.len());
                    }
                    tl::enums::ChatParticipants::Participants(c) => c.participants,
                };

                buffer.extend(
                    participants
                        .into_iter()
                        .filter_map(|p| Participant::from_raw_chat(&mut peers, p)),
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
                            return Err(InvocationError::Dropped(
                                "unexpected ChannelParticipants::NotModified",
                            ));
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
                        .filter_map(|p| Participant::from_raw_channel(&mut peers, p)),
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use mtg_testkit::FakeServer;
    use mtg_tl_types::{Deserializable, Serializable};

    use super::*;
    use crate::peer::Role;
    use crate::test_util::{channel_ref, chat_ref, client, message, user, user_ref};

    fn uid(p: &Participant) -> i64 {
        p.user.id().bare_id().unwrap()
    }

    fn full_chat(
        participants: tl::enums::ChatParticipants,
        users: Vec<tl::enums::User>,
    ) -> tl::enums::messages::ChatFull {
        // chatFull has too many fields to write out: flags, id, about, participants, settings
        let mut b = 0u32.to_le_bytes().to_vec();
        b.extend(1i64.to_le_bytes());
        b.extend(String::new().to_bytes());
        b.extend(participants.to_bytes());
        b.extend(0x99622c0cu32.to_le_bytes()); // peerNotifySettings
        b.extend(0u32.to_le_bytes());
        let full = tl::types::ChatFull::from_bytes(&b).unwrap();
        tl::types::messages::ChatFull {
            full_chat: full.into(),
            chats: Vec::new(),
            users,
        }
        .into()
    }

    fn chat_participants(list: Vec<tl::enums::ChatParticipant>) -> tl::enums::ChatParticipants {
        tl::types::ChatParticipants {
            chat_id: 1,
            participants: list,
            version: 1,
        }
        .into()
    }

    fn plain(id: i64) -> tl::enums::ChatParticipant {
        tl::types::ChatParticipant {
            user_id: id,
            inviter_id: 1,
            date: 7,
            rank: None,
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_private_peer_has_no_participants() {
        let server = FakeServer::start().await;
        let mut it = client(&server).iter_participants(user_ref(5));
        assert_eq!(it.total().await.unwrap(), 0);
        assert!(it.next().await.unwrap().is_none());
        // the filter only means something for channels
        let it = client(&server)
            .iter_participants(user_ref(5))
            .filter(tl::enums::ChannelParticipantsFilter::ChannelParticipantsBots);
        assert!(matches!(it.0, ParticipantIterInner::Empty));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_members_of_a_basic_group_come_from_the_full_chat() {
        let server = FakeServer::start().await;
        server.on(|req: tl::functions::messages::GetFullChat| {
            assert_eq!(req.chat_id, 3);
            let creator = tl::types::ChatParticipantCreator {
                user_id: 6,
                rank: None,
            };
            // user 99 is not among the users and is skipped
            Ok(full_chat(
                chat_participants(vec![plain(5), creator.into(), plain(99)]),
                vec![user(5), user(6)],
            ))
        });
        let mut it = client(&server).iter_participants(chat_ref(3));
        assert_eq!(it.total().await.unwrap(), 2);
        assert_eq!(it.total().await.unwrap(), 2); // cached
        let first = it.next().await.unwrap().unwrap();
        assert_eq!(uid(&first), 5);
        assert!(matches!(first.role, Role::User(_)));
        let second = it.next().await.unwrap().unwrap();
        assert!(matches!(second.role, Role::Creator(_)));
        assert!(it.next().await.unwrap().is_none());
        assert!(it.next().await.unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_forbidden_member_list_shows_only_yourself() {
        let server = FakeServer::start().await;
        let with_self = Arc::new(Mutex::new(true));
        let w = with_self.clone();
        server.on(move |_: tl::functions::messages::GetFullChat| {
            let me = w.lock().unwrap().then(|| plain(5));
            Ok(full_chat(
                tl::types::ChatParticipantsForbidden {
                    chat_id: 1,
                    self_participant: me,
                }
                .into(),
                vec![user(5)],
            ))
        });
        let c = client(&server);
        let mut it = c.iter_participants(chat_ref(3));
        assert_eq!(uid(&it.next().await.unwrap().unwrap()), 5);
        assert!(it.next().await.unwrap().is_none());

        *with_self.lock().unwrap() = false;
        assert!(
            c.iter_participants(chat_ref(3))
                .next()
                .await
                .unwrap()
                .is_none()
        );
    }

    fn channel_participants(ids: &[i64]) -> tl::enums::channels::ChannelParticipants {
        tl::types::channels::ChannelParticipants {
            count: 42,
            participants: ids
                .iter()
                .map(|&id| {
                    tl::types::ChannelParticipant {
                        user_id: id,
                        date: 1,
                        subscription_until_date: None,
                        rank: None,
                    }
                    .into()
                })
                .collect(),
            chats: Vec::new(),
            users: ids.iter().map(|&id| user(id)).collect(),
        }
        .into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn channel_members_are_paged_with_the_filter() {
        let server = FakeServer::start().await;
        let offsets = Arc::new(Mutex::new(Vec::new()));
        let o = offsets.clone();
        server.on(move |req: tl::functions::channels::GetParticipants| {
            assert!(matches!(
                req.filter,
                tl::enums::ChannelParticipantsFilter::ChannelParticipantsAdmins
            ));
            o.lock().unwrap().push((req.offset, req.limit));
            Ok(channel_participants(&[5, 6]))
        });
        let mut it = client(&server)
            .iter_participants(channel_ref(9))
            .filter(tl::enums::ChannelParticipantsFilter::ChannelParticipantsAdmins);
        assert_eq!(it.total().await.unwrap(), 42);
        assert_eq!(it.total().await.unwrap(), 42); // cached
        assert_eq!(uid(&it.next().await.unwrap().unwrap()), 5);
        assert_eq!(uid(&it.next().await.unwrap().unwrap()), 6);
        // the page was shorter than the limit: that was the last one
        assert!(it.next().await.unwrap().is_none());
        assert_eq!(*offsets.lock().unwrap(), [(0, 200)]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_full_page_asks_for_the_next_one() {
        let server = FakeServer::start().await;
        let offsets = Arc::new(Mutex::new(Vec::new()));
        let o = offsets.clone();
        server.on(move |req: tl::functions::channels::GetParticipants| {
            o.lock().unwrap().push(req.offset);
            let n = req.limit as i64;
            if req.offset == 0 {
                Ok(channel_participants(&(1..=n).collect::<Vec<_>>()))
            } else {
                Ok(channel_participants(&[1000]))
            }
        });
        let mut it = client(&server).iter_participants(channel_ref(9));
        let mut n = 0;
        let mut last = 0;
        while let Some(p) = it.next().await.unwrap() {
            n += 1;
            last = uid(&p);
        }
        assert_eq!((n, last), (201, 1000));
        assert_eq!(*offsets.lock().unwrap(), [0, 200]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn not_modified_members_are_an_error() {
        let server = FakeServer::start().await;
        server.on(|_: tl::functions::channels::GetParticipants| {
            Ok(tl::enums::channels::ChannelParticipants::NotModified)
        });
        let err = client(&server)
            .iter_participants(channel_ref(9))
            .next()
            .await
            .unwrap_err();
        assert!(matches!(err, InvocationError::Dropped(_)));
    }

    fn photo(id: i64) -> tl::enums::Photo {
        tl::types::PhotoEmpty { id }.into()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn user_photos_come_from_the_photos_method() {
        let server = FakeServer::start().await;
        let slice = Arc::new(Mutex::new(false));
        let s = slice.clone();
        server.on(move |_: tl::functions::photos::GetUserPhotos| {
            Ok(if *s.lock().unwrap() {
                tl::enums::photos::Photos::from(tl::types::photos::PhotosSlice {
                    count: 5,
                    photos: vec![photo(1)],
                    users: Vec::new(),
                })
            } else {
                tl::types::photos::Photos {
                    photos: vec![photo(1), photo(2)],
                    users: Vec::new(),
                }
                .into()
            })
        });
        let c = client(&server);

        let mut it = c.iter_profile_photos(user_ref(5));
        assert_eq!(it.total().await.unwrap(), 2);
        assert_eq!(it.total().await.unwrap(), 2); // cached
        assert_eq!(it.next().await.unwrap().unwrap().id(), 1);
        assert_eq!(it.next().await.unwrap().unwrap().id(), 2);
        assert!(it.next().await.unwrap().is_none());

        *slice.lock().unwrap() = true;
        let mut it = c.iter_profile_photos(user_ref(5));
        assert_eq!(it.total().await.unwrap(), 5);
    }

    fn edit_photo_service(id: i32, peer: PeerRef, photo_id: i64) -> tl::enums::Message {
        let mut b = 0u32.to_le_bytes().to_vec();
        b.extend(id.to_le_bytes());
        b.extend(tl::enums::Peer::from(peer.id).to_bytes());
        b.extend(100i32.to_le_bytes());
        b.extend(
            tl::enums::MessageAction::from(tl::types::MessageActionChatEditPhoto {
                photo: photo(photo_id),
            })
            .to_bytes(),
        );
        tl::enums::Message::Service(tl::types::MessageService::from_bytes(&b).unwrap())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn group_photos_are_found_among_the_service_messages() {
        let server = FakeServer::start().await;
        let peer = chat_ref(3);
        server.on(move |req: tl::functions::messages::Search| {
            assert!(matches!(
                req.filter,
                tl::enums::MessagesFilter::InputMessagesFilterChatPhotos
            ));
            Ok(tl::enums::messages::Messages::from(
                tl::types::messages::Messages {
                    // a message that is not a photo change is skipped
                    messages: vec![message(3, peer, "text", 0), edit_photo_service(2, peer, 77)],
                    topics: Vec::new(),
                    chats: Vec::new(),
                    users: Vec::new(),
                },
            ))
        });
        let mut it = client(&server).iter_profile_photos(peer);
        assert_eq!(it.next().await.unwrap().unwrap().id(), 77);
        assert!(it.next().await.unwrap().is_none());

        let mut it = client(&server).iter_profile_photos(peer);
        assert_eq!(it.total().await.unwrap(), 2);
    }
}
