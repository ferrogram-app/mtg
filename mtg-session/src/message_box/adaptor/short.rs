use super::*;

pub(super) fn update_short_message(
    short: tl::types::UpdateShortMessage,
) -> tl::types::UpdatesCombined {
    update_short(tl::types::UpdateShort {
        update: tl::types::UpdateNewMessage {
            message: tl::types::Message {
                out: short.out,
                mentioned: short.mentioned,
                media_unread: short.media_unread,
                silent: short.silent,
                post: false,
                from_scheduled: false,
                legacy: false,
                edit_hide: false,
                pinned: false,
                noforwards: false,
                invert_media: false,
                video_processing_pending: false,
                paid_suggested_post_stars: false,
                reactions: None,
                id: short.id,
                from_id: None,
                from_rank: None,
                from_boosts_applied: None,
                peer_id: tl::types::PeerUser {
                    user_id: short.user_id,
                }
                .into(),
                saved_peer_id: None,
                fwd_from: short.fwd_from,
                via_bot_id: short.via_bot_id,
                reply_to: short.reply_to,
                date: short.date,
                message: short.message,
                media: None,
                reply_markup: None,
                entities: short.entities,
                views: None,
                forwards: None,
                replies: None,
                edit_date: None,
                post_author: None,
                grouped_id: None,
                restriction_reason: None,
                ttl_period: short.ttl_period,
                quick_reply_shortcut_id: None,
                offline: false,
                via_business_bot_id: None,
                guestchat_via_from: None,
                effect: None,
                factcheck: None,
                report_delivery_until_date: None,
                paid_message_stars: None,
                paid_suggested_post_ton: false,
                suggested_post: None,
                schedule_repeat_period: None,
                summary_from_language: None,
                rich_message: None,
            }
            .into(),
            pts: short.pts,
            pts_count: short.pts_count,
        }
        .into(),
        date: short.date,
    })
}

pub(super) fn update_short_chat_message(
    short: tl::types::UpdateShortChatMessage,
) -> tl::types::UpdatesCombined {
    update_short(tl::types::UpdateShort {
        update: tl::types::UpdateNewMessage {
            message: tl::types::Message {
                out: short.out,
                mentioned: short.mentioned,
                media_unread: short.media_unread,
                silent: short.silent,
                post: false,
                from_scheduled: false,
                legacy: false,
                edit_hide: false,
                pinned: false,
                noforwards: false,
                invert_media: false,
                video_processing_pending: false,
                paid_suggested_post_stars: false,
                reactions: None,
                id: short.id,
                from_id: Some(
                    tl::types::PeerUser {
                        user_id: short.from_id,
                    }
                    .into(),
                ),
                from_boosts_applied: None,
                from_rank: None,
                peer_id: tl::types::PeerChat {
                    chat_id: short.chat_id,
                }
                .into(),
                saved_peer_id: None,
                fwd_from: short.fwd_from,
                via_bot_id: short.via_bot_id,
                reply_to: short.reply_to,
                date: short.date,
                message: short.message,
                media: None,
                reply_markup: None,
                entities: short.entities,
                views: None,
                forwards: None,
                replies: None,
                edit_date: None,
                post_author: None,
                grouped_id: None,
                restriction_reason: None,
                ttl_period: short.ttl_period,
                quick_reply_shortcut_id: None,
                offline: false,
                via_business_bot_id: None,
                guestchat_via_from: None,
                effect: None,
                factcheck: None,
                report_delivery_until_date: None,
                paid_message_stars: None,
                paid_suggested_post_ton: false,
                suggested_post: None,
                schedule_repeat_period: None,
                summary_from_language: None,
                rich_message: None,
            }
            .into(),
            pts: short.pts,
            pts_count: short.pts_count,
        }
        .into(),
        date: short.date,
    })
}

pub(super) fn update_short_sent_message(
    short: tl::types::UpdateShortSentMessage,
) -> tl::types::UpdatesCombined {
    update_short(tl::types::UpdateShort {
        update: tl::types::UpdateNewMessage {
            message: tl::types::MessageEmpty {
                id: short.id,
                peer_id: None,
            }
            .into(),
            pts: short.pts,
            pts_count: short.pts_count,
        }
        .into(),
        date: short.date,
    })
}
