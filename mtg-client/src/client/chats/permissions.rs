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
