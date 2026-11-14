use super::*;

impl PtsInfo {
    pub(in crate::message_box) fn from_update(update: &tl::enums::Update) -> Option<Self> {
        use tl::enums::Update::*;
        match update {
            NewMessage(u) => {
                // channels are sequenced by their own `pts`; a channel here is bogus input, so the update
                // is not sequenced instead of trusting it (and panicking)
                if matches!(message_peer(&u.message), Some(tl::enums::Peer::Channel(_))) {
                    return None;
                }
                Some(Self {
                    key: Key::Common,
                    pts: u.pts,
                    count: u.pts_count,
                })
            }
            MessageId(_) => None,
            DeleteMessages(u) => Some(Self {
                key: Key::Common,
                pts: u.pts,
                count: u.pts_count,
            }),
            UserTyping(_) => None,
            ChatUserTyping(_) => None,
            ChatParticipants(_) => None,
            UserStatus(_) => None,
            UserName(_) => None,
            NewAuthorization(_) => None,
            NewEncryptedMessage(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            EncryptedChatTyping(_) => None,
            Encryption(_) => None,
            EncryptedMessagesRead(_) => None,
            ChatParticipantAdd(_) => None,
            ChatParticipantDelete(_) => None,
            DcOptions(_) => None,
            NotifySettings(_) => None,
            ServiceNotification(_) => None,
            Privacy(_) => None,
            UserPhone(_) => None,
            ReadHistoryInbox(u) => {
                // channels are sequenced by their own `pts`; a channel here is bogus input, so the update
                // is not sequenced instead of trusting it (and panicking)
                if matches!(u.peer, tl::enums::Peer::Channel(_)) {
                    return None;
                }
                Some(Self {
                    key: Key::Common,
                    pts: u.pts,
                    count: u.pts_count,
                })
            }
            ReadHistoryOutbox(u) => {
                // channels are sequenced by their own `pts`; a channel here is bogus input, so the update
                // is not sequenced instead of trusting it (and panicking)
                if matches!(u.peer, tl::enums::Peer::Channel(_)) {
                    return None;
                }
                Some(Self {
                    key: Key::Common,
                    pts: u.pts,
                    count: u.pts_count,
                })
            }
            WebPage(u) => Some(Self {
                key: Key::Common,
                pts: u.pts,
                count: u.pts_count,
            }),
            ReadMessagesContents(u) => Some(Self {
                key: Key::Common,
                pts: u.pts,
                count: u.pts_count,
            }),
            ChannelTooLong(u) => u.pts.map(|pts| Self {
                key: Key::Channel(u.channel_id),
                pts,
                count: 0,
            }),
            Channel(_) => None,
            // Telegram actually sends `updateNewChannelMessage(messageEmpty(…))`, and because
            // there's no way to tell which channel ID this `pts` belongs to, the best we can
            // do is ignore it.
            //
            // Future messages should trigger a gap that we need to recover from.
            NewChannelMessage(u) => message_channel_id(&u.message).map(|channel_id| Self {
                key: Key::Channel(channel_id),
                pts: u.pts,
                count: u.pts_count,
            }),
            ReadChannelInbox(u) => Some(Self {
                key: Key::Channel(u.channel_id),
                pts: u.pts,
                count: 0,
            }),
            DeleteChannelMessages(u) => Some(Self {
                key: Key::Channel(u.channel_id),
                pts: u.pts,
                count: u.pts_count,
            }),
            ChannelMessageViews(_) => None,
            ChatParticipantAdmin(_) => None,
            NewStickerSet(_) => None,
            StickerSetsOrder(_) => None,
            StickerSets(_) => None,
            SavedGifs => None,
            BotInlineQuery(_) => None,
            BotInlineSend(_) => None,
            EditChannelMessage(u) => message_channel_id(&u.message).map(|channel_id| Self {
                key: Key::Channel(channel_id),
                pts: u.pts,
                count: u.pts_count,
            }),
            BotCallbackQuery(_) => None,
            EditMessage(u) => {
                // channels are sequenced by their own `pts`; a channel here is bogus input, so the update
                // is not sequenced instead of trusting it (and panicking)
                if matches!(message_peer(&u.message), Some(tl::enums::Peer::Channel(_))) {
                    return None;
                }
                Some(Self {
                    key: Key::Common,
                    pts: u.pts,
                    count: u.pts_count,
                })
            }
            InlineBotCallbackQuery(_) => None,
            ReadChannelOutbox(_) => None,
            DraftMessage(_) => None,
            ReadFeaturedStickers => None,
            RecentStickers => None,
            Config => None,
            PtsChanged => None,
            ChannelWebPage(u) => Some(Self {
                key: Key::Channel(u.channel_id),
                pts: u.pts,
                count: u.pts_count,
            }),
            DialogPinned(_) => None,
            PinnedDialogs(_) => None,
            BotWebhookJson(_) => None,
            BotWebhookJsonquery(_) => None,
            BotShippingQuery(_) => None,
            BotPrecheckoutQuery(_) => None,
            PhoneCall(_) => None,
            LangPackTooLong(_) => None,
            LangPack(_) => None,
            FavedStickers => None,
            ChannelReadMessagesContents(_) => None,
            ContactsReset => None,
            ChannelAvailableMessages(_) => None,
            DialogUnreadMark(_) => None,
            MessagePoll(_) => None,
            ChatDefaultBannedRights(_) => None,
            FolderPeers(u) => Some(Self {
                key: Key::Common,
                pts: u.pts,
                count: u.pts_count,
            }),
            PeerSettings(_) => None,
            PeerLocated(_) => None,
            NewScheduledMessage(_) => None,
            DeleteScheduledMessages(_) => None,
            Theme(_) => None,
            GeoLiveViewed(_) => None,
            LoginToken => None,
            MessagePollVote(_) => None,
            DialogFilter(_) => None,
            DialogFilterOrder(_) => None,
            DialogFilters => None,
            PhoneCallSignalingData(_) => None,
            ChannelMessageForwards(_) => None,
            ReadChannelDiscussionInbox(_) => None,
            ReadChannelDiscussionOutbox(_) => None,
            PeerBlocked(_) => None,
            ChannelUserTyping(_) => None,
            PinnedMessages(u) => {
                // channels are sequenced by their own `pts`; a channel here is bogus input, so the update
                // is not sequenced instead of trusting it (and panicking)
                if matches!(u.peer, tl::enums::Peer::Channel(_)) {
                    return None;
                }
                Some(Self {
                    key: Key::Common,
                    pts: u.pts,
                    count: u.pts_count,
                })
            }
            PinnedChannelMessages(u) => Some(Self {
                key: Key::Channel(u.channel_id),
                pts: u.pts,
                count: u.pts_count,
            }),
            Chat(_) => None,
            GroupCallParticipants(_) => None,
            GroupCall(_) => None,
            PeerHistoryTtl(_) => None,
            ChatParticipant(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            ChannelParticipant(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            BotStopped(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            GroupCallConnection(_) => None,
            BotCommands(_) => None,
            PendingJoinRequests(_) => None,
            BotChatInviteRequester(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            MessageReactions(_) => None,
            AttachMenuBots => None,
            WebViewResultSent(_) => None,
            BotMenuButton(_) => None,
            SavedRingtones => None,
            TranscribedAudio(_) => None,
            ReadFeaturedEmojiStickers => None,
            UserEmojiStatus(_) => None,
            RecentEmojiStatuses => None,
            RecentReactions => None,
            MoveStickerSetToTop(_) => None,
            MessageExtendedMedia(_) => None,
            User(_) => None,
            AutoSaveSettings => None,
            Story(_) => None,
            NewStoryReaction(_) => None,
            ReadStories(_) => None,
            StoryId(_) => None,
            StoriesStealthMode(_) => None,
            SentStoryReaction(_) => None,
            BotChatBoost(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            ChannelViewForumAsMessages(_) => None,
            PeerWallpaper(_) => None,
            BotMessageReaction(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            BotMessageReactions(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            SavedDialogPinned(_) => None,
            PinnedSavedDialogs(_) => None,
            SavedReactionTags => None,
            SmsJob(_) => None,
            QuickReplies(_) => None,
            NewQuickReply(_) => None,
            DeleteQuickReply(_) => None,
            QuickReplyMessage(_) => None,
            DeleteQuickReplyMessages(_) => None,
            BotBusinessConnect(_) => None,
            BotNewBusinessMessage(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            BotEditBusinessMessage(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1,
            }),
            BotDeleteBusinessMessage(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: u.messages.len() as i32,
            }),
            StarsBalance(_) => None,
            BusinessBotCallbackQuery(_) => None,
            StarsRevenueStatus(_) => None,
            BotPurchasedPaidMedia(u) => Some(Self {
                key: Key::Secondary,
                pts: u.qts,
                count: 1, // TODO unsure if 1
            }),
            PaidReactionPrivacy(_) => None,
            _ => None,
        }
        .filter(|info| info.pts != NO_PTS)
    }
}
