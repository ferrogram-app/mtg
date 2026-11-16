// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt;

use mtg_session::types::{PeerAuth, PeerId, PeerInfo, PeerRef};
use mtg_tl_types as tl;

use crate::Client;

/// Platform Identifier referenced only by [`RestrictionReason`].
#[non_exhaustive]
pub enum Platform {
    All,
    Android,
    Ios,
    WindowsPhone,
    Other(String),
}

/// Reason why a user is globally restricted.
pub struct RestrictionReason {
    pub platforms: Vec<Platform>,
    pub reason: String,
    pub text: String,
}

impl RestrictionReason {
    pub fn from_raw(reason: &tl::enums::RestrictionReason) -> Self {
        let tl::enums::RestrictionReason::Reason(reason) = reason;
        Self {
            platforms: reason
                .platform
                .split('-')
                .map(|p| match p {
                    // Taken from https://core.telegram.org/constructor/restrictionReason
                    "all" => Platform::All,
                    "android" => Platform::Android,
                    "ios" => Platform::Ios,
                    "wp" => Platform::WindowsPhone,
                    o => Platform::Other(o.to_string()),
                })
                .collect(),
            reason: reason.reason.to_string(),
            text: reason.text.to_string(),
        }
    }
}

/// A user.
///
/// Users include your contacts, members of a group, bot accounts created by [@BotFather], or
/// anyone with a Telegram account.
///
/// A "normal" (non-bot) user may also behave like a "bot" without actually being one, for
/// example, when controlled with a program as opposed to being controlled by a human through
/// a Telegram application. These are commonly known as "userbots", and some people use them
/// to enhance their Telegram experience (for example, creating "commands" so that the program
/// automatically reacts to them, like translating messages).
///
/// [@BotFather]: https://t.me/BotFather
#[derive(Clone)]
pub struct User {
    pub raw: tl::enums::User,
    pub(crate) client: Client,
}

impl fmt::Debug for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.raw.fmt(f)
    }
}

// TODO: photo
impl User {
    pub fn from_raw(client: &Client, user: tl::enums::User) -> Self {
        Self {
            raw: user,
            client: client.clone(),
        }
    }

    pub(crate) fn user(&self) -> Option<&tl::types::User> {
        match &self.raw {
            tl::enums::User::User(u) => Some(u),
            tl::enums::User::Empty(_) => None,
        }
    }

    /// Return the user presence status (also known as "last seen").
    pub fn status(&self) -> &mtg_tl_types::enums::UserStatus {
        self.user()
            .and_then(|u| u.status.as_ref())
            .unwrap_or(&mtg_tl_types::enums::UserStatus::Empty)
    }

    /// Return the unique identifier for this user.
    pub fn id(&self) -> PeerId {
        PeerId::user_unchecked(self.raw.id())
    }

    /// Non-min auth stored in the user, if any.
    pub(crate) fn auth(&self) -> Option<PeerAuth> {
        let user = self.user()?;
        user.access_hash
            .filter(|_| !user.min)
            .map(PeerAuth::from_hash)
    }

    /// Convert the user to its reference.
    ///
    /// This is only possible if the peer would be usable on all methods or if it is in the session cache.
    pub async fn to_ref(
        &self,
    ) -> Result<Option<PeerRef>, Box<dyn std::error::Error + Send + Sync>> {
        super::to_ref(&self.client, self.id(), self.auth()).await
    }

    /// Return the first name of this user.
    ///
    /// The name will be `None` if the account was deleted. It may also be `None` if you received
    /// it previously.
    pub fn first_name(&self) -> Option<&str> {
        self.user().and_then(|u| u.first_name.as_deref())
    }

    /// Return the last name of this user, if any.
    pub fn last_name(&self) -> Option<&str> {
        self.user()
            .and_then(|u| u.last_name.as_deref().filter(|&name| !name.is_empty()))
    }

    /// Return the full name of this user.
    ///
    /// This is equal to the user's first name concatenated with the user's last name, if this
    /// is not empty. Otherwise, it equals the user's first name.
    pub fn full_name(&self) -> String {
        let first_name = self.first_name().unwrap_or_default();
        if let Some(last_name) = self.last_name() {
            let mut name = String::with_capacity(first_name.len() + 1 + last_name.len());
            name.push_str(first_name);
            name.push(' ');
            name.push_str(last_name);
            name
        } else {
            first_name.to_string()
        }
    }

    /// Return the public @username of this user, if any.
    ///
    /// The returned username does not contain the "@" prefix.
    ///
    /// Outside of the application, people may link to this user with one of Telegram's URLs, such
    /// as https://t.me/username.
    pub fn username(&self) -> Option<&str> {
        self.user().and_then(|u| u.username.as_deref())
    }

    /// Return collectible usernames of this user, if any.
    ///
    /// The returned usernames do not contain the "@" prefix.
    ///
    /// Outside of the application, people may link to this user with one of its username, such
    /// as https://t.me/username.
    pub fn usernames(&self) -> Vec<&str> {
        self.user()
            .and_then(|u| u.usernames.as_deref())
            .map_or(Vec::new(), |usernames| {
                usernames
                    .iter()
                    .map(|username| match username {
                        tl::enums::Username::Username(username) => username.username.as_ref(),
                    })
                    .collect()
            })
    }

    /// Return the phone number of this user, if they are not a bot and their privacy settings
    /// allow you to see it.
    pub fn phone(&self) -> Option<&str> {
        self.user().and_then(|u| u.phone.as_deref())
    }

    /// Return the photo of this user, if any.
    pub fn photo(&self) -> Option<&tl::types::UserProfilePhoto> {
        match self.user().and_then(|u| u.photo.as_ref()) {
            Some(maybe_photo) => match maybe_photo {
                tl::enums::UserProfilePhoto::Empty => None,
                tl::enums::UserProfilePhoto::Photo(photo) => Some(photo),
            },
            None => None,
        }
    }

    /// Does this user represent the account that's currently logged in?
    pub fn is_self(&self) -> bool {
        // TODO if is_self is false, check in peer cache if id == ourself
        self.user().map(|u| u.is_self).unwrap_or(false)
    }

    /// Is this user in your account's contact list?
    pub fn contact(&self) -> bool {
        self.user().map(|u| u.contact).unwrap_or(false)
    }

    /// Is this user a mutual contact?
    ///
    /// Contacts are mutual if both the user of the current account and this user have eachother
    /// in their respective contact list.
    pub fn mutual_contact(&self) -> bool {
        self.user().map(|u| u.mutual_contact).unwrap_or(false)
    }

    /// Has the account of this user been deleted?
    pub fn deleted(&self) -> bool {
        self.user().map(|u| u.deleted).unwrap_or(false)
    }

    /// Is the current account a bot?
    ///
    /// Bot accounts are those created by [@BotFather](https://t.me/BotFather).
    pub fn is_bot(&self) -> bool {
        self.user().map(|u| u.bot).unwrap_or(false)
    }

    /// If the current user is a bot, does it have [privacy mode] enabled?
    ///
    /// * Bots with privacy enabled won't see messages in groups unless they are replied or the
    ///   command includes their name (`/command@bot`).
    /// * Bots with privacy disabled will be able to see all messages in a group.
    ///
    /// [privacy mode]: https://core.telegram.org/bots#privacy-mode
    pub fn bot_privacy(&self) -> bool {
        self.user().map(|u| !u.bot_chat_history).unwrap_or(false)
    }

    /// If the current user is a bot, can it be added to groups?
    pub fn bot_supports_chats(self) -> bool {
        self.user().map(|u| u.bot_nochats).unwrap_or(false)
    }

    /// Has the account of this user been verified?
    ///
    /// Verified accounts, such as [@BotFather](https://t.me/BotFather), have a special icon next
    /// to their names in official applications (commonly a blue starred checkmark).
    pub fn verified(&self) -> bool {
        self.user().map(|u| u.verified).unwrap_or(false)
    }

    /// Does this user have restrictions applied to their account?
    pub fn restricted(&self) -> bool {
        self.user().map(|u| u.restricted).unwrap_or(false)
    }

    /// If the current user is a bot, does it want geolocation information on inline queries?
    pub fn bot_inline_geo(&self) -> bool {
        self.user().map(|u| u.bot_inline_geo).unwrap_or(false)
    }

    /// Is this user an official member of the support team?
    pub fn support(&self) -> bool {
        self.user().map(|u| u.support).unwrap_or(false)
    }

    /// Has this user been flagged for trying to scam other people?
    pub fn scam(&self) -> bool {
        self.user().map(|u| u.scam).unwrap_or(false)
    }

    /// The reason(s) why this user is restricted, could be empty.
    pub fn restriction_reason(&self) -> Vec<RestrictionReason> {
        if let Some(reasons) = self.user().and_then(|u| u.restriction_reason.as_ref()) {
            reasons.iter().map(RestrictionReason::from_raw).collect()
        } else {
            Vec::new()
        }
    }

    /// Return the placeholder for inline queries if the current user is a bot and has said
    /// placeholder configured.
    pub fn bot_inline_placeholder(&self) -> Option<&str> {
        self.user()
            .and_then(|u| u.bot_inline_placeholder.as_deref())
    }

    /// Language code of the user, if any.
    pub fn lang_code(&self) -> Option<&str> {
        self.user().and_then(|u| u.lang_code.as_deref())
    }
}

impl From<User> for PeerInfo {
    #[inline]
    fn from(user: User) -> Self {
        <Self as From<&User>>::from(&user)
    }
}
impl<'a> From<&'a User> for PeerInfo {
    fn from(user: &'a User) -> Self {
        <PeerInfo as From<&'a tl::enums::User>>::from(&user.raw)
    }
}

#[cfg(test)]
mod tests {
    use mtg_testkit::FakeServer;

    use super::*;
    use crate::test_util::{client, user as raw_user, user_ref};

    fn with(c: &Client, edit: impl FnOnce(&mut tl::types::User)) -> User {
        let mut raw = raw_user(5);
        let tl::enums::User::User(u) = &mut raw else {
            unreachable!()
        };
        edit(u);
        User::from_raw(c, raw)
    }

    fn empty(c: &Client) -> User {
        User::from_raw(c, tl::types::UserEmpty { id: 8 }.into())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn names_and_contact_details() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let u = with(&c, |u| {
            u.first_name = Some("Ann".into());
            u.last_name = Some("Lee".into());
            u.username = Some("ann".into());
            u.phone = Some("123".into());
            u.lang_code = Some("en".into());
            u.bot_inline_placeholder = Some("search".into());
            u.usernames = Some(vec![
                tl::types::Username {
                    editable: false,
                    active: true,
                    deletable: false,
                    expired: false,
                    username: "ann2".into(),
                }
                .into(),
            ]);
        });
        assert_eq!(u.id(), PeerId::user_unchecked(5));
        assert_eq!((u.first_name(), u.last_name()), (Some("Ann"), Some("Lee")));
        assert_eq!(u.full_name(), "Ann Lee");
        assert_eq!(
            (u.username(), u.phone(), u.lang_code()),
            (Some("ann"), Some("123"), Some("en"))
        );
        assert_eq!(u.usernames(), ["ann2"]);
        assert_eq!(u.bot_inline_placeholder(), Some("search"));
        assert!(format!("{u:?}").contains("Ann"));

        let bare = with(&c, |_| {});
        assert_eq!(bare.full_name(), "");
        assert!(bare.usernames().is_empty() && bare.username().is_none());
        // an empty last name counts as none
        let blank = with(&c, |u| {
            u.first_name = Some("Bo".into());
            u.last_name = Some(String::new());
        });
        assert_eq!(
            (blank.last_name(), blank.full_name().as_str()),
            (None, "Bo")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn flags_follow_the_raw_user() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let on = with(&c, |u| {
            u.is_self = true;
            u.contact = true;
            u.mutual_contact = true;
            u.deleted = true;
            u.bot = true;
            u.bot_chat_history = false;
            u.bot_nochats = true;
            u.verified = true;
            u.restricted = true;
            u.bot_inline_geo = true;
            u.support = true;
            u.scam = true;
        });
        assert!(on.is_self() && on.contact() && on.mutual_contact() && on.deleted());
        assert!(on.is_bot() && on.bot_privacy() && on.verified() && on.restricted());
        assert!(on.bot_inline_geo() && on.support() && on.scam());
        assert!(on.bot_supports_chats());

        let off = with(&c, |u| u.bot_chat_history = true);
        assert!(!off.is_self() && !off.contact() && !off.is_bot() && !off.bot_privacy());
        assert!(!off.bot_supports_chats());

        let e = empty(&c);
        assert!(!e.is_self() && !e.contact() && !e.mutual_contact() && !e.deleted());
        assert!(!e.is_bot() && !e.bot_privacy() && !e.verified() && !e.restricted());
        assert!(!e.bot_inline_geo() && !e.support() && !e.scam());
        assert!(!e.clone().bot_supports_chats());
        assert!(e.first_name().is_none() && e.last_name().is_none() && e.phone().is_none());
        assert_eq!(e.full_name(), "");
        assert!(e.usernames().is_empty() && e.username().is_none() && e.lang_code().is_none());
        assert!(e.bot_inline_placeholder().is_none() && e.restriction_reason().is_empty());
        assert!(e.photo().is_none());
        assert!(matches!(e.status(), tl::enums::UserStatus::Empty));
        assert_eq!(e.id(), PeerId::user_unchecked(8));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn status_photo_and_restrictions() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let u = with(&c, |u| {
            u.status = Some(tl::types::UserStatusOnline { expires: 9 }.into());
            u.photo = Some(
                tl::types::UserProfilePhoto {
                    has_video: false,
                    personal: false,
                    photo_id: 4,
                    stripped_thumb: None,
                    dc_id: 2,
                }
                .into(),
            );
            u.restriction_reason = Some(vec![
                tl::types::RestrictionReason {
                    platform: "all-ios-wp-web".into(),
                    reason: "porn".into(),
                    text: "no".into(),
                }
                .into(),
            ]);
        });
        assert!(matches!(u.status(), tl::enums::UserStatus::Online(s) if s.expires == 9));
        assert_eq!(u.photo().unwrap().photo_id, 4);
        let reasons = u.restriction_reason();
        assert_eq!(
            (reasons[0].reason.as_str(), reasons[0].text.as_str()),
            ("porn", "no")
        );
        assert!(matches!(
            reasons[0].platforms[..],
            [
                Platform::All,
                Platform::Ios,
                Platform::WindowsPhone,
                Platform::Other(_)
            ]
        ));
        let android = RestrictionReason::from_raw(
            &tl::types::RestrictionReason {
                platform: "android".into(),
                reason: String::new(),
                text: String::new(),
            }
            .into(),
        );
        assert!(matches!(android.platforms[..], [Platform::Android]));

        let no_photo = with(&c, |u| u.photo = Some(tl::enums::UserProfilePhoto::Empty));
        assert!(no_photo.photo().is_none());
        assert!(with(&c, |_| {}).photo().is_none());
        assert!(with(&c, |_| {}).restriction_reason().is_empty());
        assert!(matches!(
            with(&c, |_| {}).status(),
            tl::enums::UserStatus::Empty
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn references_need_a_full_access_hash() {
        let server = FakeServer::start().await;
        let c = client(&server);
        let u = with(&c, |_| {});
        assert_eq!(u.to_ref().await.unwrap(), Some(user_ref(5)));
        assert!(matches!(PeerInfo::from(&u), PeerInfo::User { .. }));
        assert!(matches!(PeerInfo::from(u), PeerInfo::User { .. }));
        // a min user carries a hash that is useless for requests; nothing is cached for it either
        let min = with(&c, |u| u.min = true);
        assert!(min.auth().is_none());
        assert_eq!(min.to_ref().await.unwrap(), None);
        assert!(empty(&c).auth().is_none());
    }
}
