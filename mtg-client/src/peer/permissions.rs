// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use jiff::Timestamp;
use mtg_tl_types as tl;

use crate::utils;

/// Permissions granted to participants with admin roles.
#[derive(Clone, Debug, PartialEq)]
pub struct Permissions {
    pub raw: tl::types::ChatAdminRights,
}

/// Restrictions in effect for participants with banned role.
#[derive(Clone, Debug, PartialEq)]
pub struct Restrictions {
    pub raw: tl::types::ChatBannedRights,
}

impl Permissions {
    pub(crate) fn new_full() -> Self {
        Self {
            raw: tl::types::ChatAdminRights {
                change_info: true,
                post_messages: true,
                edit_messages: true,
                delete_messages: true,
                ban_users: true,
                invite_users: true,
                pin_messages: true,
                add_admins: true,
                anonymous: true,
                manage_call: true,
                other: true,
                manage_topics: true,
                post_stories: true,
                edit_stories: true,
                delete_stories: true,
                manage_direct_messages: true,
                manage_ranks: true,
                manage_linked_peers: true,
                manage_welcome_messages: true,
            },
        }
    }

    pub(crate) fn from_raw(rights: tl::types::ChatAdminRights) -> Self {
        Self { raw: rights }
    }

    pub fn change_info(&self) -> bool {
        self.raw.change_info
    }

    pub fn post_messages(&self) -> bool {
        self.raw.post_messages
    }

    pub fn edit_messages(&self) -> bool {
        self.raw.edit_messages
    }

    pub fn delete_messages(&self) -> bool {
        self.raw.delete_messages
    }

    pub fn ban_users(&self) -> bool {
        self.raw.ban_users
    }

    pub fn invite_users(&self) -> bool {
        self.raw.invite_users
    }

    pub fn pin_messages(&self) -> bool {
        self.raw.pin_messages
    }

    pub fn add_admins(&self) -> bool {
        self.raw.add_admins
    }

    pub fn anonymous(&self) -> bool {
        self.raw.anonymous
    }

    pub fn manage_call(&self) -> bool {
        self.raw.manage_call
    }
}

impl Restrictions {
    pub(crate) fn from_raw(rights: tl::types::ChatBannedRights) -> Self {
        Self { raw: rights }
    }

    pub fn view_messages(&self) -> bool {
        self.raw.view_messages
    }

    pub fn send_messages(&self) -> bool {
        self.raw.send_messages
    }

    pub fn send_media(&self) -> bool {
        self.raw.send_media
    }

    pub fn send_stickers(&self) -> bool {
        self.raw.send_stickers
    }

    pub fn send_gifs(&self) -> bool {
        self.raw.send_gifs
    }

    pub fn send_games(&self) -> bool {
        self.raw.send_games
    }

    pub fn send_inline(&self) -> bool {
        self.raw.send_inline
    }

    pub fn embed_links(&self) -> bool {
        self.raw.embed_links
    }

    pub fn send_polls(&self) -> bool {
        self.raw.send_polls
    }

    pub fn change_info(&self) -> bool {
        self.raw.change_info
    }

    pub fn invite_users(&self) -> bool {
        self.raw.invite_users
    }

    pub fn pin_messages(&self) -> bool {
        self.raw.pin_messages
    }

    pub fn due(&self) -> Timestamp {
        utils::date(self.raw.until_date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_tl_types::Deserializable;

    fn banned(flags: u32, until: i32) -> Restrictions {
        let mut b = flags.to_le_bytes().to_vec();
        b.extend(until.to_le_bytes());
        Restrictions::from_raw(tl::types::ChatBannedRights::from_bytes(&b).unwrap())
    }

    #[test]
    fn full_permissions_grant_everything() {
        let p = Permissions::new_full();
        assert!(p.change_info() && p.post_messages() && p.edit_messages());
        assert!(p.delete_messages() && p.ban_users() && p.invite_users());
        assert!(p.pin_messages() && p.add_admins() && p.anonymous() && p.manage_call());
    }

    #[test]
    fn permissions_follow_the_raw_rights() {
        let mut raw = Permissions::new_full().raw;
        raw.ban_users = false;
        let p = Permissions::from_raw(raw);
        assert!(!p.ban_users());
        assert!(p.change_info());
    }

    #[test]
    fn restrictions_read_every_flag_and_the_deadline() {
        let all = banned(u32::MAX, 1_700_000_000);
        assert!(all.view_messages() && all.send_messages() && all.send_media());
        assert!(all.send_stickers() && all.send_gifs() && all.send_games());
        assert!(all.send_inline() && all.embed_links() && all.send_polls());
        assert!(all.change_info() && all.invite_users() && all.pin_messages());
        assert_eq!(all.due().as_second(), 1_700_000_000);

        let none = banned(0, 0);
        assert!(!none.view_messages() && !none.send_messages() && !none.send_media());
        assert!(!none.send_stickers() && !none.send_gifs() && !none.send_games());
        assert!(!none.send_inline() && !none.embed_links() && !none.send_polls());
        assert!(!none.change_info() && !none.invite_users() && !none.pin_messages());
    }
}
