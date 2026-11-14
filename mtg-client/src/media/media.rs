// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::fmt::Debug;

use jiff::Timestamp;
use mtg_tl_types as tl;

use super::{Downloadable, PhotoSize};

mod document;
mod kinds;
mod photo;

/// Photo media contained within a message.
#[derive(Clone, Debug, PartialEq)]
pub struct Photo {
    pub raw: tl::types::MessageMediaPhoto,
}

/// File documents unlike the alternative of compressed photos.
///
/// This includes uncompressed photos, videos, stickers, and files of any type.
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub raw: tl::types::MessageMediaDocument,
}

/// A sticker [`Document`].
#[derive(Clone, Debug, PartialEq)]
pub struct Sticker {
    pub document: Document,
    pub raw_attrs: tl::types::DocumentAttributeSticker,
    animated: bool,
}

/// Data uploaded by the client to be used as either a [`Photo`] or [`Document`].
#[derive(Clone, Debug, PartialEq)]
pub struct Uploaded {
    pub raw: tl::enums::InputFile,
}

/// Contact media containing a phone number.
#[derive(Clone, Debug, PartialEq)]
pub struct Contact {
    pub raw: tl::types::MessageMediaContact,
}

/// Poll or quiz which may be voted on while open.
#[derive(Clone, Debug, PartialEq)]
pub struct Poll {
    pub raw: tl::types::Poll,
    pub raw_results: Box<tl::types::PollResults>,
}

/// Geo point with latitude and longitude coordinates on the globe.
#[derive(Clone, Debug, PartialEq)]
pub struct Geo {
    pub raw: tl::types::GeoPoint,
}

/// Animated built-in sticker with a set of possible outcomes.
///
/// Originally used to roll dice (🎲), but other emoji such as darts (🎯),
/// basket-balls (🏀) and slot-machines (🎰) can also be used.
#[derive(Clone, Debug, PartialEq)]
pub struct Dice {
    pub raw: tl::types::MessageMediaDice,
}

/// A place with optional coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Venue {
    pub geo: Option<Geo>,
    pub raw_venue: tl::types::MessageMediaVenue,
}

/// Like [`Geo`], but periodcally edits itself with updated information.
#[derive(Clone, Debug, PartialEq)]
pub struct GeoLive {
    pub geo: Option<Geo>,
    pub raw_geolive: tl::types::MessageMediaGeoLive,
}

/// Instant View web-page previews.
#[derive(Clone, Debug, PartialEq)]
pub struct WebPage {
    pub raw: Box<tl::types::MessageMediaWebPage>,
}

/// Profile picture of a group.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatPhoto {
    pub raw: tl::enums::InputFileLocation,
}

/// Message media (e.g. photos, polls, videos).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Media {
    /// Compressed JPEG photo.
    Photo(Photo),
    /// File which may or not be rendered directly by clients.
    Document(Document),
    /// Document that has the attributes of a sticker.
    Sticker(Sticker),
    /// Contact information.
    Contact(Contact),
    /// Poll which can be voted on while open.
    Poll(Poll),
    /// Coordinates of a place.
    Geo(Geo),
    /// Dice-like built-in sticker media.
    Dice(Dice),
    /// Information about a place with optional coordinates.
    Venue(Venue),
    /// Self-updating coordinates of a place or person.
    GeoLive(GeoLive),
    /// Web-page instant view.
    WebPage(WebPage),
}

impl Media {
    pub fn from_raw(media: tl::enums::MessageMedia) -> Option<Self> {
        use tl::enums::MessageMedia as M;

        // TODO implement the rest
        match media {
            M::Empty => None,
            M::Photo(photo) => Some(Self::Photo(Photo::from_raw_media(photo))),
            M::Geo(geo) => Geo::from_raw_media(geo).map(Self::Geo),
            M::Contact(contact) => Some(Self::Contact(Contact::from_raw_media(contact))),
            M::Unsupported => None,
            M::Document(document) => {
                let document = Document::from_raw_media(document);
                Some(if let Some(sticker) = Sticker::from_document(&document) {
                    Self::Sticker(sticker)
                } else {
                    Self::Document(document)
                })
            }
            M::WebPage(webpage) => Some(Self::WebPage(WebPage::from_raw_media(webpage))),
            M::Venue(venue) => Some(Self::Venue(Venue::from_raw_media(venue))),
            M::Game(_) => None,
            M::Invoice(_) => None,
            M::GeoLive(geolive) => Some(Self::GeoLive(GeoLive::from_raw_media(geolive))),
            M::Poll(poll) => Some(Self::Poll(Poll::from_raw_media(*poll))),
            M::Dice(dice) => Some(Self::Dice(Dice::from_raw_media(dice))),
            M::Story(_) => None,
            M::Giveaway(_) => None,
            M::GiveawayResults(_) => None,
            M::PaidMedia(_) => None,
            M::ToDo(_) => None,
            M::VideoStream(_) => None,
            M::Widget(_) => None,
        }
    }

    pub fn size(&self) -> Option<usize> {
        match self {
            Media::Photo(photo) => photo.size(),
            Media::Document(document) => document.size(),
            Media::Sticker(sticker) => sticker.document.size(),
            Media::Contact(_) => None,
            Media::Poll(_) => None,
            Media::Geo(_) => None,
            Media::Dice(_) => None,
            Media::Venue(_) => None,
            Media::GeoLive(_) => None,
            Media::WebPage(_) => None,
        }
    }

    pub fn to_raw_input_media(&self) -> Option<tl::enums::InputMedia> {
        match self {
            Media::Photo(photo) => Some(photo.to_raw_input_media().into()),
            Media::Document(document) => Some(document.to_raw_input_media().into()),
            Media::Sticker(sticker) => Some(sticker.document.to_raw_input_media().into()),
            Media::Contact(contact) => Some(contact.to_raw_input_media().into()),
            Media::Poll(poll) => Some(poll.to_raw_input_media().into()),
            Media::Geo(geo) => Some(geo.to_raw_input_media().into()),
            Media::Dice(dice) => Some(dice.to_raw_input_media().into()),
            Media::Venue(venue) => Some(venue.to_raw_input_media().into()),
            Media::GeoLive(geolive) => Some(geolive.to_raw_input_media().into()),
            Media::WebPage(_) => None,
        }
    }
}

impl Downloadable for Media {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        match self {
            Media::Photo(photo) => photo.to_raw_input_location(),
            Media::Document(document) => document.to_raw_input_location(),
            Media::Sticker(sticker) => sticker.document.to_raw_input_location(),
            Media::Contact(_) => None,
            Media::Poll(_) => None,
            Media::Geo(_) => None,
            Media::Dice(_) => None,
            Media::Venue(_) => None,
            Media::GeoLive(_) => None,
            Media::WebPage(_) => None,
        }
    }

    fn size(&self) -> Option<usize> {
        self.size()
    }

    fn dc_id(&self) -> Option<i32> {
        match self {
            Media::Photo(photo) => photo.dc_id(),
            Media::Document(document) => document.dc_id(),
            Media::Sticker(sticker) => sticker.document.dc_id(),
            Media::Contact(_) => None,
            Media::Poll(_) => None,
            Media::Geo(_) => None,
            Media::Dice(_) => None,
            Media::Venue(_) => None,
            Media::GeoLive(_) => None,
            Media::WebPage(_) => None,
        }
    }
}

impl From<Photo> for Media {
    fn from(photo: Photo) -> Self {
        Self::Photo(photo)
    }
}

impl Downloadable for ChatPhoto {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        Some(self.raw.clone())
    }
}
