// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_tl_types as tl;

#[derive(Debug, Clone)]
pub(super) enum ParticipantPermissionsInner {
    Channel(tl::enums::ChannelParticipant),
    Chat(tl::enums::ChatParticipant),
}

/// Permissions returned by [`crate::Client::get_permissions`].
#[derive(Debug, Clone)]
pub struct ParticipantPermissions(pub(super) ParticipantPermissionsInner);

impl ParticipantPermissions {
    /// Whether the user is the creator of the chat or not.
    pub fn is_creator(&self) -> bool {
        matches!(
            self.0,
            ParticipantPermissionsInner::Channel(tl::enums::ChannelParticipant::Creator(_))
                | ParticipantPermissionsInner::Chat(tl::enums::ChatParticipant::Creator(_))
        )
    }

    /// Whether the user is an administrator of the chat or not. The creator also counts as begin an administrator, since they have all permissions.
    pub fn is_admin(&self) -> bool {
        self.is_creator()
            || matches!(
                self.0,
                ParticipantPermissionsInner::Channel(tl::enums::ChannelParticipant::Admin(_))
                    | ParticipantPermissionsInner::Chat(tl::enums::ChatParticipant::Admin(_))
            )
    }

    /// Whether the user is banned in the chat.
    pub fn is_banned(&self) -> bool {
        matches!(
            self.0,
            ParticipantPermissionsInner::Channel(tl::enums::ChannelParticipant::Banned(_))
        )
    }

    /// Whether the user left the chat.
    pub fn has_left(&self) -> bool {
        matches!(
            self.0,
            ParticipantPermissionsInner::Channel(tl::enums::ChannelParticipant::Left(_))
        )
    }

    /// Whether the user is a normal user of the chat (not administrator, but not banned either, and has no restrictions applied).
    pub fn has_default_permissions(&self) -> bool {
        matches!(
            self.0,
            ParticipantPermissionsInner::Channel(tl::enums::ChannelParticipant::Participant(_))
                | ParticipantPermissionsInner::Channel(
                    tl::enums::ChannelParticipant::ParticipantSelf(_)
                )
                | ParticipantPermissionsInner::Chat(tl::enums::ChatParticipant::Participant(_))
        )
    }

    /// Whether the administrator can add new administrators with the same or less permissions than them.
    pub fn can_add_admins(&self) -> bool {
        if !self.is_admin() {
            return false;
        }
        match &self.0 {
            ParticipantPermissionsInner::Channel(tl::enums::ChannelParticipant::Admin(
                participant,
            )) => {
                let tl::enums::ChatAdminRights::Rights(rights) = &participant.admin_rights;
                rights.add_admins
            }
            ParticipantPermissionsInner::Channel(tl::enums::ChannelParticipant::Creator(_)) => true,
            ParticipantPermissionsInner::Chat(_) => self.is_creator(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use mtg_tl_types as tl;

    use super::*;

    fn channel(p: impl Into<tl::enums::ChannelParticipant>) -> ParticipantPermissions {
        ParticipantPermissions(ParticipantPermissionsInner::Channel(p.into()))
    }

    fn chat(p: impl Into<tl::enums::ChatParticipant>) -> ParticipantPermissions {
        ParticipantPermissions(ParticipantPermissionsInner::Chat(p.into()))
    }

    fn rights(add_admins: bool) -> tl::enums::ChatAdminRights {
        tl::types::ChatAdminRights {
            change_info: false,
            post_messages: false,
            edit_messages: false,
            delete_messages: false,
            ban_users: false,
            invite_users: false,
            pin_messages: false,
            add_admins,
            anonymous: false,
            manage_call: false,
            other: false,
            manage_topics: false,
            post_stories: false,
            edit_stories: false,
            delete_stories: false,
            manage_direct_messages: false,
            manage_ranks: false,
            manage_linked_peers: false,
            manage_welcome_messages: false,
        }
        .into()
    }

    fn admin(add_admins: bool) -> tl::types::ChannelParticipantAdmin {
        tl::types::ChannelParticipantAdmin {
            can_edit: true,
            is_self: false,
            user_id: 1,
            inviter_id: None,
            promoted_by: 2,
            date: 3,
            admin_rights: rights(add_admins),
            rank: None,
        }
    }

    fn flags(p: &ParticipantPermissions) -> [bool; 6] {
        [
            p.is_creator(),
            p.is_admin(),
            p.is_banned(),
            p.has_left(),
            p.has_default_permissions(),
            p.can_add_admins(),
        ]
    }

    #[test]
    fn channel_roles_map_to_flags() {
        let peer = || tl::enums::Peer::User(tl::types::PeerUser { user_id: 1 });
        let creator = tl::types::ChannelParticipantCreator {
            user_id: 1,
            admin_rights: rights(false),
            rank: None,
        };
        let banned = tl::types::ChannelParticipantBanned {
            left: false,
            peer: peer(),
            kicked_by: 2,
            date: 3,
            banned_rights: tl::types::ChatBannedRights {
                view_messages: true,
                send_messages: true,
                send_media: true,
                send_stickers: true,
                send_gifs: true,
                send_games: true,
                send_inline: true,
                embed_links: true,
                send_polls: true,
                change_info: true,
                invite_users: true,
                pin_messages: true,
                manage_topics: true,
                send_photos: true,
                send_videos: true,
                send_roundvideos: true,
                send_audios: true,
                send_voices: true,
                send_docs: true,
                send_plain: true,
                edit_rank: true,
                send_reactions: true,
                manage_linked_peers: true,
                until_date: 0,
            }
            .into(),
            rank: None,
        };
        let plain = tl::types::ChannelParticipant {
            user_id: 1,
            date: 3,
            subscription_until_date: None,
            rank: None,
        };
        let me = tl::types::ChannelParticipantSelf {
            via_request: false,
            user_id: 1,
            inviter_id: 2,
            date: 3,
            subscription_until_date: None,
            rank: None,
        };
        let left = tl::types::ChannelParticipantLeft { peer: peer() };
        // creator, admin, banned, left, default
        assert_eq!(
            flags(&channel(creator)),
            [true, true, false, false, false, true]
        );
        assert_eq!(
            flags(&channel(admin(true))),
            [false, true, false, false, false, true]
        );
        assert_eq!(
            flags(&channel(admin(false))),
            [false, true, false, false, false, false]
        );
        assert_eq!(
            flags(&channel(banned)),
            [false, false, true, false, false, false]
        );
        assert_eq!(
            flags(&channel(left)),
            [false, false, false, true, false, false]
        );
        assert_eq!(
            flags(&channel(plain)),
            [false, false, false, false, true, false]
        );
        assert_eq!(
            flags(&channel(me)),
            [false, false, false, false, true, false]
        );
    }

    #[test]
    fn basic_group_roles_map_to_flags() {
        let creator = tl::types::ChatParticipantCreator {
            user_id: 1,
            rank: None,
        };
        let admin = tl::types::ChatParticipantAdmin {
            user_id: 1,
            inviter_id: 2,
            date: 3,
            rank: None,
        };
        let plain = tl::types::ChatParticipant {
            user_id: 1,
            inviter_id: 2,
            date: 3,
            rank: None,
        };
        assert_eq!(
            flags(&chat(creator)),
            [true, true, false, false, false, true]
        );
        // admins of a basic group may not add other admins
        assert_eq!(
            flags(&chat(admin)),
            [false, true, false, false, false, false]
        );
        assert_eq!(
            flags(&chat(plain)),
            [false, false, false, false, true, false]
        );
    }
}
