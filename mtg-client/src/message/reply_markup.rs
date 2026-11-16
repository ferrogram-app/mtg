// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Contains functions to build reply markups.
//!
//! These can only be used by bot accounts when sending
//! messages through [`InputMessage::reply_markup`].
//!
//! Each function returns a concrete builder-like type that
//! may be further configured via their inherent methods.
//!
//! The trait is used to group all types as "something that
//! may be used as a reply markup".
//!
//! [`InputMessage::reply_markup`]: crate::message::InputMessage::reply_markup

use mtg_tl_types as tl;

use super::{Button, Key};

const EMPTY_KEYBOARD_MARKUP: tl::types::ReplyKeyboardMarkup = tl::types::ReplyKeyboardMarkup {
    resize: false,
    single_use: false,
    selective: false,
    persistent: false,
    rows: Vec::new(),
    placeholder: None,
    force_reply: false,
};

/// Markup to be used as the intended way to reply to the message it is attached to.
pub struct ReplyMarkup {
    pub raw: tl::enums::ReplyMarkup,
}

impl ReplyMarkup {
    /// Define inline buttons for a message.
    ///
    /// These will display right under the message.
    ///
    /// You cannot add images to the buttons, but you can use emoji (simply copy-paste them into your
    /// code, or use the correct escape sequence, or using any other input methods you like).
    ///
    /// You will need to provide a matrix of [`Button`], that is, a vector that contains the
    /// rows from top to bottom, where the rows consist of a vector of buttons from left to right.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: &mut mtg_client::Client, peer: mtg_session::types::PeerRef) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::{InputMessage, ReplyMarkup, Button};
    ///
    /// let artist = "Krewella";
    /// let markup = ReplyMarkup::from_buttons(&[
    ///     vec![Button::data(format!("Song by {}", artist), b"play")],
    ///     vec![Button::data("Previous", b"prev"), Button::data("Next", b"next")],
    /// ]);
    /// client.send_message(peer, InputMessage::new().text("Select song").reply_markup(markup)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_buttons(buttons: &[Vec<Button>]) -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyInlineMarkup(tl::types::ReplyInlineMarkup {
                rows: buttons
                    .iter()
                    .map(|row| {
                        tl::types::KeyboardInlineButtonRow {
                            buttons: row.iter().map(|button| button.raw.clone()).collect(),
                        }
                        .into()
                    })
                    .collect(),
                force_reply: false,
            }),
        }
    }

    /// Creates a [`ReplyMarkup::from_buttons`] with a single row.
    pub fn from_buttons_row(buttons: &[Button]) -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyInlineMarkup(tl::types::ReplyInlineMarkup {
                rows: vec![tl::enums::KeyboardInlineButtonRow::Row(
                    tl::types::KeyboardInlineButtonRow {
                        buttons: buttons.iter().map(|button| button.raw.clone()).collect(),
                    },
                )],
                force_reply: false,
            }),
        }
    }

    /// Creates a [`ReplyMarkup::from_buttons`] with a single column.
    pub fn from_buttons_col(buttons: &[Button]) -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyInlineMarkup(tl::types::ReplyInlineMarkup {
                rows: buttons
                    .iter()
                    .map(|button| {
                        tl::enums::KeyboardInlineButtonRow::Row(
                            tl::types::KeyboardInlineButtonRow {
                                buttons: vec![button.raw.clone()],
                            },
                        )
                    })
                    .collect(),
                force_reply: false,
            }),
        }
    }

    /// Define a custom keyboard, replacing the user's own virtual keyboard.
    ///
    /// This will be displayed below the input message field for users, and on mobile devices, this
    /// also hides the virtual keyboard (effectively "replacing" it).
    ///
    /// You cannot add images to the buttons, but you can use emoji (simply copy-paste them into your
    /// code, or use the correct escape sequence, or using any other input methods you like).
    ///
    /// You will need to provide a matrix of [`Key`], that is, a vector that contains the
    /// rows from top to bottom, where the rows consist of a vector of buttons from left to right.
    ///
    /// The return type may continue to be configured before being used.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: &mut mtg_client::Client, peer: mtg_session::types::PeerRef) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::{InputMessage, ReplyMarkup, Key};
    ///
    /// let markup = ReplyMarkup::from_keys(&[
    ///     vec![Key::text("Accept")],
    ///     vec![Key::text("Cancel"), Key::text("Try something else")],
    /// ]);
    /// client.send_message(peer, InputMessage::new().text("What do you want to do?").reply_markup(markup)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_keys(keys: &[Vec<Key>]) -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyKeyboardMarkup(tl::types::ReplyKeyboardMarkup {
                rows: keys
                    .iter()
                    .map(|row| {
                        tl::types::KeyboardButtonRow {
                            buttons: row.iter().map(|key| key.raw.clone()).collect(),
                        }
                        .into()
                    })
                    .collect(),
                ..EMPTY_KEYBOARD_MARKUP
            }),
        }
    }

    /// Creates a [`ReplyMarkup::from_keys`] with a single row.
    pub fn from_keys_row(keys: &[Key]) -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyKeyboardMarkup(tl::types::ReplyKeyboardMarkup {
                rows: vec![tl::enums::KeyboardButtonRow::Row(
                    tl::types::KeyboardButtonRow {
                        buttons: keys.iter().map(|key| key.raw.clone()).collect(),
                    },
                )],
                ..EMPTY_KEYBOARD_MARKUP
            }),
        }
    }

    /// Creates a [`ReplyMarkup::from_keys`] with a single column.
    pub fn from_keys_col(keys: &[Key]) -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyKeyboardMarkup(tl::types::ReplyKeyboardMarkup {
                rows: keys
                    .iter()
                    .map(|key| {
                        tl::enums::KeyboardButtonRow::Row(tl::types::KeyboardButtonRow {
                            buttons: vec![key.raw.clone()],
                        })
                    })
                    .collect(),
                ..EMPTY_KEYBOARD_MARKUP
            }),
        }
    }

    /// Hide a previously-sent keyboard.
    ///
    /// See the return type for further configuration options.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: &mut mtg_client::Client, peer: mtg_session::types::PeerRef) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::{InputMessage, ReplyMarkup};
    ///
    /// let markup = ReplyMarkup::hide();
    /// client.send_message(peer, InputMessage::new().text("Bot keyboards removed.").reply_markup(markup)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn hide() -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyKeyboardHide(tl::types::ReplyKeyboardHide {
                selective: false,
            }),
        }
    }

    /// "Forces" the user to send a reply.
    ///
    /// This will cause the user's application to automatically select the message for replying to it,
    /// although the user is still able to dismiss the reply and send a normal message.
    ///
    /// See the return type for further configuration options.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: &mut mtg_client::Client, peer: mtg_session::types::PeerRef) -> Result<(), Box<dyn std::error::Error>> {
    /// use mtg_client::message::{InputMessage, ReplyMarkup};
    ///
    /// let markup = ReplyMarkup::force_reply().single_use();
    /// client.send_message(peer, InputMessage::new().text("Reply me!").reply_markup(markup)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn force_reply() -> Self {
        Self {
            raw: tl::enums::ReplyMarkup::ReplyKeyboardForceReply(
                tl::types::ReplyKeyboardForceReply {
                    single_use: false,
                    selective: false,
                    placeholder: None,
                },
            ),
        }
    }

    /// Requests clients to resize the keyboard vertically for optimal fit (e.g., make the
    /// keyboard smaller if there are just two rows of buttons). Otherwise, the custom keyboard
    /// is always of the same height as the virtual keyboard.
    ///
    /// Only has effect on reply markups that use keys.
    pub fn fit_size(mut self) -> Self {
        use mtg_tl_types::enums::ReplyMarkup as RM;
        match &mut self.raw {
            RM::ReplyKeyboardMarkup(keyboard) => keyboard.resize = true,
            RM::ReplyKeyboardHide(_)
            | RM::ReplyKeyboardForceReply(_)
            | RM::ReplyInlineMarkup(_) => {}
        }
        self
    }

    /// Requests clients to hide the keyboard as soon as it's been used.
    ///
    /// The keyboard will still be available, but clients will automatically display the usual
    /// letter-keyboard in the conversation – the user can press a special button in the input field to
    /// see the custom keyboard again.
    ///
    /// Only has effect on reply markups that use keys or when forcing the user to reply.
    pub fn single_use(mut self) -> Self {
        use mtg_tl_types::enums::ReplyMarkup as RM;
        match &mut self.raw {
            RM::ReplyKeyboardForceReply(keyboard) => keyboard.single_use = true,
            RM::ReplyKeyboardMarkup(keyboard) => keyboard.single_use = true,
            RM::ReplyKeyboardHide(_) | RM::ReplyInlineMarkup(_) => {}
        }
        self
    }

    /// Force the markup to only apply to specific users.
    ///
    /// The selected user will be either the people @-mentioned in the text of the `Message`
    /// object, or if the bot's message is a reply, the sender of the original message.
    ///
    /// Has no effect on markups that consist of inline buttons.
    pub fn selective(mut self) -> Self {
        use mtg_tl_types::enums::ReplyMarkup as RM;
        match &mut self.raw {
            RM::ReplyKeyboardHide(keyboard) => keyboard.selective = true,
            RM::ReplyKeyboardForceReply(keyboard) => keyboard.selective = true,
            RM::ReplyKeyboardMarkup(keyboard) => keyboard.selective = true,
            RM::ReplyInlineMarkup(_) => {}
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tl::enums::ReplyMarkup as RM;

    fn buttons() -> Vec<Button> {
        vec![Button::data("a", "1"), Button::data("b", "2")]
    }

    fn keys() -> Vec<Key> {
        vec![Key::text("a"), Key::text("b")]
    }

    fn inline_rows(m: ReplyMarkup) -> Vec<usize> {
        let RM::ReplyInlineMarkup(m) = m.raw else {
            panic!("not inline")
        };
        m.rows
            .into_iter()
            .map(|tl::enums::KeyboardInlineButtonRow::Row(r)| r.buttons.len())
            .collect()
    }

    fn key_rows(m: &ReplyMarkup) -> Vec<usize> {
        let RM::ReplyKeyboardMarkup(m) = &m.raw else {
            panic!("not a keyboard")
        };
        m.rows
            .iter()
            .map(|tl::enums::KeyboardButtonRow::Row(r)| r.buttons.len())
            .collect()
    }

    #[test]
    fn inline_markups_are_laid_out_as_asked() {
        assert_eq!(
            inline_rows(ReplyMarkup::from_buttons(&[
                buttons(),
                buttons().into_iter().take(1).collect()
            ])),
            [2, 1]
        );
        assert_eq!(inline_rows(ReplyMarkup::from_buttons_row(&buttons())), [2]);
        assert_eq!(
            inline_rows(ReplyMarkup::from_buttons_col(&buttons())),
            [1, 1]
        );
    }

    #[test]
    fn keyboards_are_laid_out_as_asked() {
        assert_eq!(
            key_rows(&ReplyMarkup::from_keys(&[
                keys(),
                keys().into_iter().take(1).collect()
            ])),
            [2, 1]
        );
        assert_eq!(key_rows(&ReplyMarkup::from_keys_row(&keys())), [2]);
        assert_eq!(key_rows(&ReplyMarkup::from_keys_col(&keys())), [1, 1]);
    }

    #[test]
    fn flags_apply_only_where_they_exist() {
        let RM::ReplyKeyboardMarkup(k) = ReplyMarkup::from_keys_row(&keys())
            .fit_size()
            .single_use()
            .selective()
            .raw
        else {
            panic!()
        };
        assert!(k.resize && k.single_use && k.selective);

        let RM::ReplyKeyboardForceReply(f) = ReplyMarkup::force_reply()
            .single_use()
            .selective()
            .fit_size()
            .raw
        else {
            panic!()
        };
        assert!(f.single_use && f.selective);

        let RM::ReplyKeyboardHide(h) = ReplyMarkup::hide().single_use().selective().fit_size().raw
        else {
            panic!()
        };
        assert!(h.selective);

        // an inline markup ignores all three
        let m = ReplyMarkup::from_buttons_row(&buttons())
            .fit_size()
            .single_use()
            .selective();
        assert_eq!(inline_rows(m), [2]);
    }
}
