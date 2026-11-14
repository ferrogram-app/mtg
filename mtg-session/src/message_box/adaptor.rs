// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_tl_types as tl;
use tracing::info;

use super::defs::{Gap, Key, NO_PTS, NO_SEQ, PtsInfo, UpdatesLike};

// > The `updateShortMessage`, `updateShortSentMessage` and `updateShortChatMessage` constructors
// > [...] should be transformed to `updateShort` upon receiving.
//
// We don't use `updateShort` because it's fairly limitting. `updatesCombined` is big enough to
// contain all updates inside, and sticking to a single type (and not enum) simplifies things.
//
// This module's job is just converting the various updates types into `updatesCombined`.
// It also converts updates into their corresponding `PtsInfo`.

fn updates(updates: tl::types::Updates) -> tl::types::UpdatesCombined {
    tl::types::UpdatesCombined {
        updates: updates.updates,
        users: updates.users,
        chats: updates.chats,
        date: updates.date,
        seq_start: updates.seq,
        seq: updates.seq,
    }
}

fn update_short(short: tl::types::UpdateShort) -> tl::types::UpdatesCombined {
    tl::types::UpdatesCombined {
        updates: vec![short.update],
        users: Vec::new(),
        chats: Vec::new(),
        date: short.date,
        seq_start: NO_SEQ,
        seq: NO_SEQ,
    }
}

fn adapt_updates(updates: tl::enums::Updates) -> Result<tl::types::UpdatesCombined, Gap> {
    Ok(match updates {
        // > `updatesTooLong` indicates that there are too many events pending to be pushed
        // > to the client, so one needs to fetch them manually.
        tl::enums::Updates::TooLong => {
            info!("received updatesTooLong, treating as gap");
            return Err(Gap);
        }
        // > `updateShortMessage`, `updateShortSentMessage` and `updateShortChatMessage` [...]
        // > should be transformed to `updateShort` upon receiving.
        tl::enums::Updates::UpdateShortMessage(short) => {
            // > Incomplete update: the client is missing data about a chat/user from one of
            // > the shortened constructors, such as `updateShortChatMessage`, etc.
            //
            // The check for missing hashes is done elsewhere to avoid doing the same work twice.
            // This "only" needs to be done for "short messages", to get the private chat (user)
            // where the message occured. Anywhere else, Telegram should send information
            // about the chat so that [min constructors][0] can be used.
            //
            // [0]: https://core.telegram.org/api/min
            update_short_message(short)
        }
        tl::enums::Updates::UpdateShortChatMessage(short) => {
            // No need to check for chats here. Small group chats do not require an access
            // hash, and min constructors can be used to access the user.
            update_short_chat_message(short)
        }
        // > `updateShort` […] have lower priority and are broadcast to a large number of users.
        //
        // There *shouldn't* be updates mentioning peers we're unaware of here.
        //
        // If later it turns out these can happen, the code will need to be updated to
        // consider chats missing here a gap as well.
        tl::enums::Updates::UpdateShort(short) => update_short(short),
        // > [the] `seq` attribute, which indicates the remote `Updates` state after the
        // > generation of the `Updates`, and `seq_start` indicates the remote `Updates` state
        // > after the first of the `Updates` in the packet is generated
        tl::enums::Updates::Combined(combined) => combined,
        // > [the] `seq_start` attribute is omitted, because it is assumed that it is always
        // > equal to `seq`.
        tl::enums::Updates::Updates(updates) => self::updates(updates),
        // Even though we lack fields like the message text, it still needs to be handled, so
        // that the `pts` can be kept consistent.
        tl::enums::Updates::UpdateShortSentMessage(short) => update_short_sent_message(short),
    })
}

pub(super) fn adapt(updates: UpdatesLike) -> Result<tl::types::UpdatesCombined, Gap> {
    match updates {
        UpdatesLike::Updates(updates) => adapt_updates(updates),
        UpdatesLike::ShortSentMessage { request, update } => {
            Ok(update_short(tl::types::UpdateShort {
                update: tl::types::UpdateNewMessage {
                    message: tl::types::Message {
                        out: update.out,
                        mentioned: false,
                        media_unread: false,
                        silent: request.silent,
                        post: false,
                        from_scheduled: false,
                        legacy: false,
                        edit_hide: false,
                        pinned: false,
                        noforwards: request.noforwards,
                        invert_media: request.invert_media,
                        offline: false,
                        video_processing_pending: false,
                        paid_suggested_post_stars: false,
                        paid_suggested_post_ton: false,
                        id: update.id,
                        from_id: request.send_as.as_ref().map(peer_from_input_peer),
                        from_boosts_applied: None,
                        from_rank: None,
                        peer_id: peer_from_input_peer(&request.peer),
                        saved_peer_id: None,
                        fwd_from: None,
                        via_bot_id: None,
                        via_business_bot_id: None,
                        guestchat_via_from: None,
                        reply_to: request.reply_to.and_then(|r| match r {
                            tl::enums::InputReplyTo::Message(i) => {
                                Some(tl::enums::MessageReplyHeader::Header(
                                    tl::types::MessageReplyHeader {
                                        reply_to_scheduled: false,
                                        forum_topic: false,
                                        quote: i.quote_offset.is_some(),
                                        reply_to_ephemeral: false,
                                        reply_to_msg_id: Some(i.reply_to_msg_id),
                                        reply_to_peer_id: i
                                            .reply_to_peer_id
                                            .as_ref()
                                            .map(peer_from_input_peer),
                                        reply_from: None,
                                        reply_media: None,
                                        reply_to_top_id: i.top_msg_id,
                                        quote_text: i.quote_text,
                                        quote_entities: i.quote_entities,
                                        quote_offset: i.quote_offset,
                                        todo_item_id: None,
                                        poll_option: None,
                                        quote_rich: None,
                                    },
                                ))
                            }
                            tl::enums::InputReplyTo::Story(i) => {
                                Some(tl::enums::MessageReplyHeader::MessageReplyStoryHeader(
                                    tl::types::MessageReplyStoryHeader {
                                        peer: peer_from_input_peer(&i.peer),
                                        story_id: i.story_id,
                                    },
                                ))
                            }
                            tl::enums::InputReplyTo::MonoForum(_) => None,
                            tl::enums::InputReplyTo::EphemeralMessage(_) => None,
                        }),
                        date: update.date,
                        message: request.message,
                        media: update.media,
                        reply_markup: request.reply_markup,
                        entities: update.entities.or(request.entities),
                        views: None,
                        forwards: None,
                        replies: None,
                        edit_date: None,
                        post_author: None,
                        grouped_id: None,
                        reactions: None,
                        restriction_reason: None,
                        ttl_period: update.ttl_period,
                        quick_reply_shortcut_id: request.quick_reply_shortcut.and_then(
                            |q| match q {
                                tl::enums::InputQuickReplyShortcut::Shortcut(_) => None,
                                tl::enums::InputQuickReplyShortcut::Id(i) => Some(i.shortcut_id),
                            },
                        ),
                        effect: request.effect,
                        factcheck: None,
                        report_delivery_until_date: None,
                        paid_message_stars: None,
                        suggested_post: None,
                        schedule_repeat_period: None,
                        summary_from_language: None,
                        rich_message: None,
                    }
                    .into(),
                    pts: update.pts,
                    pts_count: update.pts_count,
                }
                .into(),
                date: update.date,
            }))
        }
        // Reuse a closely-related update type to keep pts consistent.
        UpdatesLike::AffectedMessages(affected) => Ok(update_short(tl::types::UpdateShort {
            update: tl::types::UpdateDeleteMessages {
                messages: Vec::new(),
                pts: affected.pts,
                pts_count: affected.pts_count,
            }
            .into(),
            date: 0,
        })),
        // Channel-specific variant: use UpdateDeleteChannelMessages so the pts is applied
        // to Key::Channel(channel_id) instead of Key::Common.
        UpdatesLike::AffectedChannelMessages {
            affected,
            channel_id,
            message_ids,
        } => Ok(update_short(tl::types::UpdateShort {
            update: tl::types::UpdateDeleteChannelMessages {
                channel_id,
                messages: message_ids,
                pts: affected.pts,
                pts_count: affected.pts_count,
            }
            .into(),
            date: 0,
        })),
        UpdatesLike::InvitedUsers(invited) => adapt_updates(invited.updates),
        UpdatesLike::ChatInviteJoinResult(result) => adapt_updates(result.updates),
        UpdatesLike::ConnectionClosed | UpdatesLike::MalformedUpdates => Err(Gap),
    }
}

/// `Err(Gap)` when `channelDifferenceTooLong` carries no usable `pts` (folder, community or none).
pub(super) fn adapt_channel_difference(
    difference: tl::enums::updates::ChannelDifference,
) -> Result<tl::types::updates::ChannelDifference, Gap> {
    Ok(match difference {
        tl::enums::updates::ChannelDifference::Empty(difference) => {
            tl::types::updates::ChannelDifference {
                r#final: difference.r#final,
                pts: difference.pts,
                timeout: difference.timeout,
                new_messages: Vec::new(),
                other_updates: Vec::new(),
                chats: Vec::new(),
                users: Vec::new(),
            }
        }
        tl::enums::updates::ChannelDifference::TooLong(difference) => {
            tl::types::updates::ChannelDifference {
                r#final: difference.r#final,
                pts: match difference.dialog {
                    tl::enums::Dialog::Dialog(tl::types::Dialog { pts: Some(pts), .. }) => pts,
                    _ => return Err(Gap),
                },
                timeout: difference.timeout,
                new_messages: Vec::new(),
                other_updates: Vec::new(),
                chats: difference.chats,
                users: difference.users,
            }
        }
        tl::enums::updates::ChannelDifference::Difference(difference) => difference,
    })
}

fn peer_from_input_peer(input_peer: &tl::enums::InputPeer) -> tl::enums::Peer {
    match input_peer {
        tl::enums::InputPeer::Empty => tl::types::PeerUser { user_id: 0 }.into(),
        tl::enums::InputPeer::PeerSelf => tl::types::PeerUser { user_id: 0 }.into(), // TODO can get self from client
        tl::enums::InputPeer::Chat(chat) => tl::types::PeerChat {
            chat_id: chat.chat_id,
        }
        .into(),
        tl::enums::InputPeer::User(user) => tl::types::PeerUser {
            user_id: user.user_id,
        }
        .into(),
        tl::enums::InputPeer::Channel(channel) => tl::types::PeerChannel {
            channel_id: channel.channel_id,
        }
        .into(),
        tl::enums::InputPeer::UserFromMessage(user) => tl::types::PeerUser {
            user_id: user.user_id,
        }
        .into(),
        tl::enums::InputPeer::ChannelFromMessage(channel) => tl::types::PeerChannel {
            channel_id: channel.channel_id,
        }
        .into(),
    }
}

fn message_peer(message: &tl::enums::Message) -> Option<tl::enums::Peer> {
    match message {
        tl::enums::Message::Empty(m) => m.peer_id.clone(),
        tl::enums::Message::Message(m) => Some(m.peer_id.clone()),
        tl::enums::Message::Service(m) => Some(m.peer_id.clone()),
    }
}

fn message_channel_id(message: &tl::enums::Message) -> Option<i64> {
    match message {
        tl::enums::Message::Empty(m) => match &m.peer_id {
            Some(tl::enums::Peer::Channel(c)) => Some(c.channel_id),
            _ => None,
        },
        tl::enums::Message::Message(m) => match &m.peer_id {
            tl::enums::Peer::Channel(c) => Some(c.channel_id),
            _ => None,
        },
        tl::enums::Message::Service(m) => match &m.peer_id {
            tl::enums::Peer::Channel(c) => Some(c.channel_id),
            _ => None,
        },
    }
}

mod pts_info;
mod short;

use short::{update_short_chat_message, update_short_message, update_short_sent_message};
