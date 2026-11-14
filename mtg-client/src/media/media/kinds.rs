// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Contact, poll, geo, dice, venue, live location, web page and uploaded media.

use super::*;

impl Contact {
    pub fn from_raw_media(contact: tl::types::MessageMediaContact) -> Self {
        Self { raw: contact }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaContact {
        tl::types::InputMediaContact {
            phone_number: self.raw.phone_number.clone(),
            first_name: self.raw.first_name.clone(),
            last_name: self.raw.last_name.clone(),
            vcard: self.raw.vcard.clone(),
        }
    }

    /// The contact's phone number, in international format. This field will always be a non-empty
    /// string of digits, although there's no guarantee that the number actually exists.
    pub fn phone_number(&self) -> &str {
        self.raw.phone_number.as_str()
    }

    /// The contact's first name. Although official clients will always send a non-empty string,
    /// it is possible for this field to be empty when sent via different means.
    pub fn first_name(&self) -> &str {
        self.raw.first_name.as_str()
    }

    /// The contact's last name. May be empty if it's not set by sender.
    pub fn last_name(&self) -> &str {
        self.raw.last_name.as_str()
    }

    /// Contact information in [vCard format]. Applications such as Telegram Desktop leave this
    /// field empty. The vCard version used in this field could be any. The field may also contain
    /// arbitrary text when sent by non-official clients.
    ///
    /// [vCard format]: https://en.wikipedia.org/wiki/VCard
    pub fn vcard(&self) -> &str {
        self.raw.vcard.as_str()
    }
}

impl Poll {
    pub fn from_raw_media(poll: tl::types::MessageMediaPoll) -> Self {
        Self {
            raw: match poll.poll {
                tl::enums::Poll::Poll(poll) => poll,
            },
            raw_results: match poll.results {
                tl::enums::PollResults::Results(results) => results,
            },
        }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaPoll {
        tl::types::InputMediaPoll {
            poll: mtg_tl_types::enums::Poll::Poll(self.raw.clone()),
            correct_answers: None,
            solution: None,
            solution_entities: None,
            attached_media: None,
            solution_media: None,
        }
    }

    /// Returns the question asked by the poll.
    pub fn question(&self) -> &mtg_tl_types::enums::TextWithEntities {
        &self.raw.question
    }

    /// Return `true` if the poll is a quiz with a set of correct answers.
    pub fn is_quiz(&self) -> bool {
        self.raw.quiz
    }

    /// Returns `true` if the poll is closed.
    pub fn closed(&self) -> bool {
        self.raw.closed
    }

    /// Returns an iterator over the poll answer options.
    pub fn iter_answers(&self) -> impl Iterator<Item = &tl::enums::PollAnswer> {
        self.raw.answers.iter()
    }

    /// Total voters that took part in the vote.
    ///
    /// May be `None` if poll hasn't started.
    pub fn total_voters(&self) -> Option<i32> {
        self.raw_results.total_voters
    }

    /// Returns details of the voters choices,
    /// such as amount of voters per answer and whether the answer
    /// is currently selected by the logged-in account.
    pub fn iter_voters_summary(
        &self,
    ) -> Option<impl Iterator<Item = &tl::types::PollAnswerVoters>> {
        self.raw_results.results.as_ref().map(|results| {
            results.iter().map(|result| match result {
                tl::enums::PollAnswerVoters::Voters(voters) => voters,
            })
        })
    }
}

impl Geo {
    pub fn from_raw_media(geo: tl::types::MessageMediaGeo) -> Option<Self> {
        use tl::enums::GeoPoint as eGeoPoint;

        match &geo.geo {
            eGeoPoint::Empty => None,
            eGeoPoint::Point(point) => Some(Self { raw: point.clone() }),
        }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaGeoPoint {
        use tl::types::InputGeoPoint;

        tl::types::InputMediaGeoPoint {
            geo_point: InputGeoPoint {
                lat: self.raw.lat,
                long: self.raw.long,
                accuracy_radius: self.raw.accuracy_radius,
            }
            .into(),
        }
    }

    pub fn to_raw_input_geo_point(&self) -> tl::enums::InputGeoPoint {
        use tl::{enums::InputGeoPoint as eInputGeoPoint, types::InputGeoPoint};

        eInputGeoPoint::Point(InputGeoPoint {
            lat: self.raw.lat,
            long: self.raw.long,
            accuracy_radius: self.raw.accuracy_radius,
        })
    }

    /// Get the latitude of the location.
    pub fn latitue(&self) -> f64 {
        self.raw.lat
    }

    /// Get the latitude of the location.
    pub fn longitude(&self) -> f64 {
        self.raw.long
    }

    /// Get the accuracy of the geo location, in meters.
    pub fn accuracy_radius(&self) -> Option<i32> {
        self.raw.accuracy_radius
    }
}

impl Dice {
    pub fn from_raw_media(dice: tl::types::MessageMediaDice) -> Self {
        Self { raw: dice }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaDice {
        tl::types::InputMediaDice {
            emoticon: self.raw.emoticon.clone(),
        }
    }

    /// Get the emoji of the dice.
    pub fn emoji(&self) -> &str {
        &self.raw.emoticon
    }

    /// Get the value of the dice.
    pub fn value(&self) -> i32 {
        self.raw.value
    }
}

impl Venue {
    pub fn from_raw_media(venue: tl::types::MessageMediaVenue) -> Self {
        use tl::types::MessageMediaGeo;
        Self {
            geo: Geo::from_raw_media(MessageMediaGeo {
                geo: venue.geo.clone(),
            }),
            raw_venue: venue,
        }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaVenue {
        tl::types::InputMediaVenue {
            geo_point: match self.geo {
                Some(ref geo) => geo.to_raw_input_geo_point(),
                None => tl::enums::InputGeoPoint::Empty,
            },
            title: self.raw_venue.title.clone(),
            address: self.raw_venue.address.clone(),
            provider: self.raw_venue.provider.clone(),
            venue_id: self.raw_venue.venue_id.clone(),
            venue_type: self.raw_venue.venue_type.clone(),
        }
    }

    /// Returns the title of the venue.
    pub fn title(&self) -> &str {
        &self.raw_venue.title
    }

    /// Returns the address of the venue.
    pub fn address(&self) -> &str {
        &self.raw_venue.address
    }

    /// Returns the provider of the venue location.
    pub fn provider(&self) -> &str {
        &self.raw_venue.provider
    }

    /// Returns the id of the venue.
    pub fn venue_id(&self) -> &str {
        &self.raw_venue.venue_id
    }

    /// Returns the type of the venue.
    pub fn venue_type(&self) -> &str {
        &self.raw_venue.venue_type
    }
}

impl GeoLive {
    pub fn from_raw_media(geolive: tl::types::MessageMediaGeoLive) -> Self {
        use tl::types::MessageMediaGeo;
        Self {
            geo: Geo::from_raw_media(MessageMediaGeo {
                geo: geolive.geo.clone(),
            }),
            raw_geolive: geolive,
        }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaGeoLive {
        tl::types::InputMediaGeoLive {
            geo_point: match self.geo {
                Some(ref geo) => geo.to_raw_input_geo_point(),
                None => tl::enums::InputGeoPoint::Empty,
            },
            heading: self.raw_geolive.heading,
            period: Some(self.raw_geolive.period),
            proximity_notification_radius: self.raw_geolive.proximity_notification_radius,
            stopped: false,
        }
    }

    /// Returns the heading of the live location in degress (1-360).
    pub fn heading(&self) -> Option<i32> {
        self.raw_geolive.heading
    }

    /// Returns the validity period of the live location.
    pub fn period(&self) -> i32 {
        self.raw_geolive.period
    }

    /// Returns the radius of the proximity alert.
    pub fn proximity_notification_radius(&self) -> Option<i32> {
        self.raw_geolive.proximity_notification_radius
    }
}

impl WebPage {
    pub fn from_raw_media(webpage: tl::types::MessageMediaWebPage) -> Self {
        Self {
            raw: Box::new(webpage),
        }
    }
}

impl Uploaded {
    pub fn from_raw(input_file: tl::enums::InputFile) -> Self {
        Self { raw: input_file }
    }

    pub(crate) fn name(&self) -> &str {
        match &self.raw {
            tl::enums::InputFile::File(f) => f.name.as_ref(),
            tl::enums::InputFile::Big(f) => f.name.as_ref(),
            tl::enums::InputFile::StoryDocument(_) => "",
        }
    }
}
