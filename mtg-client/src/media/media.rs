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

#[cfg(test)]
mod tests {
    use mtg_tl_types::Deserializable;

    use super::*;

    fn photo() -> Photo {
        Photo::from_raw(
            tl::types::Photo {
                has_stickers: false,
                id: 5,
                access_hash: 6,
                file_reference: vec![],
                date: 0,
                sizes: vec![
                    tl::types::PhotoSize {
                        r#type: "m".into(),
                        w: 1,
                        h: 1,
                        size: 10,
                    }
                    .into(),
                ],
                video_sizes: None,
                dc_id: 4,
            }
            .into(),
        )
    }

    fn document(sticker: bool) -> tl::types::MessageMediaDocument {
        let attributes = if sticker {
            let mut b = 0u32.to_le_bytes().to_vec(); // no flags
            b.extend([0, 0, 0, 0]); // alt = ""
            b.extend(4290128789u32.to_le_bytes()); // inputStickerSetEmpty
            vec![
                tl::enums::DocumentAttribute::Sticker(
                    tl::types::DocumentAttributeSticker::from_bytes(&b).unwrap(),
                ),
                tl::enums::DocumentAttribute::Animated,
            ]
        } else {
            vec![]
        };
        tl::types::MessageMediaDocument {
            nopremium: false,
            spoiler: false,
            video: false,
            round: false,
            voice: false,
            document: Some(
                tl::types::Document {
                    id: 8,
                    access_hash: 9,
                    file_reference: vec![],
                    date: 0,
                    mime_type: "x/y".into(),
                    size: 55,
                    thumbs: None,
                    video_thumbs: None,
                    dc_id: 3,
                    attributes,
                }
                .into(),
            ),
            alt_documents: None,
            video_cover: None,
            video_timestamp: None,
            ttl_seconds: None,
        }
    }

    fn webpage() -> tl::types::MessageMediaWebPage {
        tl::types::MessageMediaWebPage {
            force_large_media: false,
            force_small_media: false,
            manual: false,
            safe: false,
            webpage: tl::types::WebPageEmpty { id: 1, url: None }.into(),
        }
    }

    fn point() -> tl::enums::GeoPoint {
        tl::types::GeoPoint {
            long: 1.0,
            lat: 2.0,
            access_hash: 3,
            accuracy_radius: None,
        }
        .into()
    }

    #[test]
    fn from_raw_maps_every_supported_media() {
        use tl::enums::MessageMedia as M;

        let photo_media = M::Photo(photo().raw);
        assert!(matches!(
            Media::from_raw(photo_media),
            Some(Media::Photo(_))
        ));
        assert!(matches!(
            Media::from_raw(M::Document(document(false))),
            Some(Media::Document(_))
        ));
        assert!(matches!(
            Media::from_raw(M::Document(document(true))),
            Some(Media::Sticker(s)) if s.animated
        ));
        assert!(matches!(
            Media::from_raw(M::WebPage(webpage())),
            Some(Media::WebPage(_))
        ));
        assert!(matches!(
            Media::from_raw(M::Geo(tl::types::MessageMediaGeo { geo: point() })),
            Some(Media::Geo(_))
        ));
        assert!(
            Media::from_raw(M::Geo(tl::types::MessageMediaGeo {
                geo: tl::types::GeoPointEmpty {}.into()
            }))
            .is_none()
        );
        assert!(matches!(
            Media::from_raw(M::Dice(tl::types::MessageMediaDice {
                value: 1,
                emoticon: "🎲".into(),
                game_outcome: None
            })),
            Some(Media::Dice(_))
        ));
        assert!(Media::from_raw(M::Empty).is_none());
        assert!(Media::from_raw(M::Unsupported).is_none());
    }

    #[test]
    fn size_input_media_location_and_dc_follow_the_kind() {
        let p = Media::from(photo());
        assert_eq!(Media::size(&p), Some(10));
        assert_eq!(Downloadable::size(&p), Some(10));
        assert!(p.to_raw_input_media().is_some());
        assert!(p.to_raw_input_location().is_some());
        assert_eq!(p.dc_id(), Some(4));

        let d = Media::Document(Document::from_raw_media(document(false)));
        assert_eq!((d.size(), d.dc_id()), (Some(55), Some(3)));
        assert!(d.to_raw_input_media().is_some() && d.to_raw_input_location().is_some());

        let doc = Document::from_raw_media(document(true));
        let sticker = Media::Sticker(Sticker::from_document(&doc).unwrap());
        assert_eq!((sticker.size(), sticker.dc_id()), (Some(55), Some(3)));
        assert!(
            sticker.to_raw_input_media().is_some() && sticker.to_raw_input_location().is_some()
        );

        let web = Media::WebPage(WebPage::from_raw_media(webpage()));
        assert!(web.to_raw_input_media().is_none());
        assert!(
            web.size().is_none() && web.dc_id().is_none() && web.to_raw_input_location().is_none()
        );

        let others = [
            Media::Geo(Geo::from_raw_media(tl::types::MessageMediaGeo { geo: point() }).unwrap()),
            Media::Dice(Dice::from_raw_media(tl::types::MessageMediaDice {
                value: 1,
                emoticon: "🎲".into(),
                game_outcome: None,
            })),
        ];
        for m in others {
            assert!(
                m.size().is_none() && m.dc_id().is_none() && m.to_raw_input_location().is_none()
            );
            assert!(m.to_raw_input_media().is_some());
        }
    }

    #[test]
    fn chat_photo_downloads_its_location() {
        let loc = tl::types::InputPeerPhotoFileLocation {
            big: true,
            peer: tl::types::InputPeerSelf {}.into(),
            photo_id: 1,
        };
        let p = ChatPhoto { raw: loc.into() };
        assert_eq!(p.to_raw_input_location(), Some(p.raw.clone()));
    }
}
