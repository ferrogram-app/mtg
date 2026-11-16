// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use mtg_session::updates::State;
use mtg_tl_types as tl;

/// Update that all receive whenever a message is deleted.
#[derive(Debug, Clone)]
pub struct MessageDeletion {
    pub raw: tl::enums::Update,
    pub state: State,
}

impl MessageDeletion {
    /// Returns the channel ID if the message was deleted from a channel.
    pub fn channel_id(&self) -> Option<i64> {
        match &self.raw {
            tl::enums::Update::DeleteMessages(_) => None,
            tl::enums::Update::DeleteChannelMessages(update) => Some(update.channel_id),
            _ => unreachable!(),
        }
    }

    /// Returns the slice of message IDs that was deleted.
    pub fn messages(&self) -> &[i32] {
        match &self.raw {
            tl::enums::Update::DeleteMessages(update) => update.messages.as_slice(),
            tl::enums::Update::DeleteChannelMessages(update) => update.messages.as_slice(),
            _ => unreachable!(),
        }
    }

    /// Gain ownership of underlying Vec of message IDs that was deleted.
    pub fn into_messages(self) -> Vec<i32> {
        match self.raw {
            tl::enums::Update::DeleteMessages(update) => update.messages,
            tl::enums::Update::DeleteChannelMessages(update) => update.messages,
            _ => unreachable!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State {
            date: 0,
            seq: 0,
            message_box: None,
        }
    }

    #[test]
    fn private_and_channel_deletions() {
        let private = MessageDeletion {
            raw: tl::types::UpdateDeleteMessages {
                messages: vec![1, 2],
                pts: 1,
                pts_count: 2,
            }
            .into(),
            state: state(),
        };
        assert_eq!(
            (private.channel_id(), private.messages()),
            (None, &[1, 2][..])
        );
        assert_eq!(private.into_messages(), [1, 2]);

        let channel = MessageDeletion {
            raw: tl::types::UpdateDeleteChannelMessages {
                channel_id: 9,
                messages: vec![5],
                pts: 1,
                pts_count: 1,
            }
            .into(),
            state: state(),
        };
        assert_eq!(
            (channel.channel_id(), channel.messages()),
            (Some(9), &[5][..])
        );
        assert_eq!(channel.into_messages(), [5]);
    }
}
