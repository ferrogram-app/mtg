// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Document and sticker.

use super::*;

impl Document {
    pub fn from_raw_media(document: tl::types::MessageMediaDocument) -> Self {
        Self { raw: document }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaDocument {
        tl::types::InputMediaDocument {
            spoiler: false,
            id: self.to_raw_input_document(),
            ttl_seconds: self.raw.ttl_seconds,
            query: None,
            video_cover: None,
            video_timestamp: None,
        }
    }

    pub fn to_raw_input_document(&self) -> tl::enums::InputDocument {
        use tl::{
            enums::{Document, InputDocument as eInputDocument},
            types::InputDocument,
        };

        match self.raw.document {
            Some(Document::Document(ref document)) => InputDocument {
                id: document.id,
                access_hash: document.access_hash,
                file_reference: document.file_reference.clone(),
            }
            .into(),
            _ => eInputDocument::Empty,
        }
    }

    /// The globally unique identifier for this media, as seen by any account.
    pub fn id(&self) -> i64 {
        use tl::enums::Document as D;

        // media without a document (e.g. one whose time-to-live ran out) has no id
        match self.raw.document.as_ref() {
            Some(D::Empty(document)) => document.id,
            Some(D::Document(document)) => document.id,
            None => 0,
        }
    }

    /// Return the file's name.
    ///
    /// If the file was uploaded with no file name, the returned string will be empty.
    ///
    /// Note that the file name is not necessarily valid as seen by the OS,
    /// and using it directly as the download location is subject to causing
    /// path trasversal attacks.
    pub fn name(&self) -> Option<&str> {
        use tl::enums::Document as D;

        match self.raw.document.as_ref() {
            Some(D::Document(document)) => document.attributes.iter().find_map(|attr| match attr {
                tl::enums::DocumentAttribute::Filename(attr) => Some(attr.file_name.as_ref()),
                _ => None,
            }),
            Some(D::Empty(_)) | None => None,
        }
    }

    /// Get the file's MIME type, if any.
    pub fn mime_type(&self) -> Option<&str> {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => Some(d.mime_type.as_str()),
            _ => None,
        }
    }

    /// The date on which the file was created, if any.
    pub fn creation_date(&self) -> Option<Timestamp> {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => {
                Some(Timestamp::from_second(d.date as i64).expect("date out of range"))
            }
            _ => None,
        }
    }

    /// The size of the file.
    pub fn size(&self) -> Option<usize> {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => Some(d.size as usize),
            _ => None,
        }
    }

    /// Get document thumbs.
    /// See <https://core.telegram.org/api/files#image-thumbnail-types>.
    pub fn thumbs(&self) -> Vec<PhotoSize> {
        use tl::enums::Document as D;

        let document = match self.raw.document.as_ref() {
            Some(document) => document,
            None => return vec![],
        };

        match document {
            D::Empty(_) => vec![],
            D::Document(document) => match &document.thumbs {
                Some(thumbs) => thumbs
                    .iter()
                    .map(|x| PhotoSize::make_from_document(x, document))
                    .collect(),
                None => vec![],
            },
        }
    }

    /// Duration of video/audio, in seconds.
    pub fn duration(&self) -> Option<f64> {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => {
                for attr in &d.attributes {
                    match attr {
                        tl::enums::DocumentAttribute::Video(v) => return Some(v.duration),
                        tl::enums::DocumentAttribute::Audio(a) => return Some(a.duration as _),
                        _ => {}
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Width & height of video/image, in pixels.
    pub fn resolution(&self) -> Option<(i32, i32)> {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => {
                for attr in &d.attributes {
                    match attr {
                        tl::enums::DocumentAttribute::Video(v) => return Some((v.w, v.h)),
                        tl::enums::DocumentAttribute::ImageSize(i) => return Some((i.w, i.h)),
                        _ => {}
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Title of audio.
    pub fn audio_title(&self) -> Option<&str> {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => {
                for attr in &d.attributes {
                    #[allow(clippy::single_match)]
                    match attr {
                        tl::enums::DocumentAttribute::Audio(a) => return a.title.as_deref(),
                        _ => {}
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Performer (artist) of audio.
    pub fn performer(&self) -> Option<&str> {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => {
                for attr in &d.attributes {
                    #[allow(clippy::single_match)]
                    match attr {
                        tl::enums::DocumentAttribute::Audio(a) => return a.performer.as_deref(),
                        _ => {}
                    }
                }
                None
            }
            _ => None,
        }
    }

    /// Returns `true` if the document is an animated sticker.
    pub fn is_animated(&self) -> bool {
        match self.raw.document.as_ref() {
            Some(tl::enums::Document::Document(d)) => {
                for attr in &d.attributes {
                    #[allow(clippy::single_match)]
                    match attr {
                        tl::enums::DocumentAttribute::Animated => return true,
                        _ => {}
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// Returns `true` if the document is a spoiler.
    pub fn is_spoiler(&self) -> bool {
        self.raw.spoiler
    }
}

impl Downloadable for Document {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        use tl::enums::Document as D;

        self.raw.document.as_ref().and_then(|p| match p {
            D::Empty(_) => None,
            D::Document(document) => Some(
                tl::types::InputDocumentFileLocation {
                    id: document.id,
                    access_hash: document.access_hash,
                    file_reference: document.file_reference.clone(),
                    thumb_size: String::new(),
                }
                .into(),
            ),
        })
    }

    fn size(&self) -> Option<usize> {
        self.size()
    }

    fn dc_id(&self) -> Option<i32> {
        use tl::enums::Document as D;
        match self.raw.document.as_ref()? {
            D::Empty(_) => None,
            D::Document(document) => Some(document.dc_id),
        }
    }
}

impl Sticker {
    pub fn from_document(document: &Document) -> Option<Self> {
        match document.raw.document {
            Some(tl::enums::Document::Document(ref doc)) => {
                let mut animated = false;
                let mut sticker_attrs: Option<tl::types::DocumentAttributeSticker> = None;
                for attr in &doc.attributes {
                    match attr {
                        tl::enums::DocumentAttribute::Sticker(s) => sticker_attrs = Some(s.clone()),
                        tl::enums::DocumentAttribute::Animated => animated = true,
                        _ => (),
                    }
                }
                Some(Self {
                    document: document.clone(),
                    raw_attrs: sticker_attrs?,
                    animated,
                })
            }
            _ => None,
        }
    }

    /// Get the emoji associated with the sticker.
    pub fn emoji(&self) -> &str {
        self.raw_attrs.alt.as_str()
    }

    /// Is this sticker an animated sticker?
    pub fn is_animated(&self) -> bool {
        self.animated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(attributes: Vec<tl::enums::DocumentAttribute>) -> tl::types::Document {
        tl::types::Document {
            id: 5,
            access_hash: 6,
            file_reference: vec![1],
            date: 1_700_000_000,
            mime_type: "video/mp4".into(),
            size: 1234,
            thumbs: Some(vec![
                tl::types::PhotoSize {
                    r#type: "m".into(),
                    w: 1,
                    h: 2,
                    size: 3,
                }
                .into(),
            ]),
            video_thumbs: None,
            dc_id: 4,
            attributes,
        }
    }

    fn wrap(document: Option<tl::enums::Document>) -> Document {
        Document::from_raw_media(tl::types::MessageMediaDocument {
            nopremium: false,
            spoiler: true,
            video: false,
            round: false,
            voice: false,
            document,
            alt_documents: None,
            video_cover: None,
            video_timestamp: None,
            ttl_seconds: Some(9),
        })
    }

    fn with(attributes: Vec<tl::enums::DocumentAttribute>) -> Document {
        wrap(Some(doc(attributes).into()))
    }

    fn empty() -> Document {
        wrap(Some(tl::types::DocumentEmpty { id: 8 }.into()))
    }

    fn video() -> tl::enums::DocumentAttribute {
        tl::types::DocumentAttributeVideo {
            round_message: false,
            supports_streaming: true,
            nosound: false,
            duration: 2.5,
            w: 640,
            h: 480,
            preload_prefix_size: None,
            video_start_ts: None,
            video_codec: None,
        }
        .into()
    }

    fn audio() -> tl::enums::DocumentAttribute {
        tl::types::DocumentAttributeAudio {
            voice: false,
            duration: 7,
            title: Some("t".into()),
            performer: Some("p".into()),
            waveform: None,
        }
        .into()
    }

    fn sticker_attr() -> tl::enums::DocumentAttribute {
        tl::types::DocumentAttributeSticker {
            mask: false,
            alt: "😀".into(),
            stickerset: tl::enums::InputStickerSet::Empty,
            mask_coords: None,
        }
        .into()
    }

    #[test]
    fn plain_fields() {
        let d = with(vec![
            tl::types::DocumentAttributeFilename {
                file_name: "a.mp4".into(),
            }
            .into(),
        ]);
        assert_eq!(
            (d.id(), d.name(), d.mime_type(), d.size()),
            (5, Some("a.mp4"), Some("video/mp4"), Some(1234))
        );
        assert_eq!(d.creation_date().unwrap().as_second(), 1_700_000_000);
        assert!(d.is_spoiler());
        assert_eq!(d.thumbs().len(), 1);
        assert_eq!(Downloadable::size(&d), Some(1234));
        assert_eq!(d.dc_id(), Some(4));
        assert!(with(vec![]).name().is_none());
    }

    #[test]
    fn an_empty_document_or_none_has_nothing() {
        for d in [empty(), wrap(None)] {
            assert!(d.name().is_none() && d.mime_type().is_none() && d.size().is_none());
            assert!(d.creation_date().is_none() && d.thumbs().is_empty());
            assert!(d.duration().is_none() && d.resolution().is_none());
            assert!(d.audio_title().is_none() && d.performer().is_none() && !d.is_animated());
            assert!(d.to_raw_input_location().is_none() && d.dc_id().is_none());
            assert!(matches!(
                d.to_raw_input_document(),
                tl::enums::InputDocument::Empty
            ));
        }
        assert_eq!(empty().id(), 8);
        // no document at all must not panic
        assert_eq!(wrap(None).id(), 0);
    }

    #[test]
    fn a_document_without_thumbs_has_none() {
        let mut raw = doc(vec![]);
        raw.thumbs = None;
        assert!(wrap(Some(raw.into())).thumbs().is_empty());
    }

    #[test]
    fn duration_and_resolution_come_from_the_first_matching_attribute() {
        assert_eq!(with(vec![video()]).duration(), Some(2.5));
        assert_eq!(with(vec![audio()]).duration(), Some(7.0));
        assert_eq!(with(vec![]).duration(), None);
        assert_eq!(with(vec![video()]).resolution(), Some((640, 480)));
        let size = tl::types::DocumentAttributeImageSize { w: 3, h: 4 }.into();
        assert_eq!(with(vec![size]).resolution(), Some((3, 4)));
        assert_eq!(with(vec![audio()]).resolution(), None);
    }

    #[test]
    fn audio_tags_and_animation() {
        let d = with(vec![audio()]);
        assert_eq!((d.audio_title(), d.performer()), (Some("t"), Some("p")));
        assert!(with(vec![video()]).audio_title().is_none());
        assert!(with(vec![video()]).performer().is_none());
        assert!(with(vec![tl::enums::DocumentAttribute::Animated]).is_animated());
        assert!(!with(vec![video()]).is_animated());
    }

    #[test]
    fn input_forms_carry_the_identity() {
        let d = with(vec![]);
        let tl::enums::InputDocument::Document(i) = d.to_raw_input_document() else {
            panic!()
        };
        assert_eq!((i.id, i.access_hash, i.file_reference), (5, 6, vec![1]));
        let m = d.to_raw_input_media();
        assert_eq!(m.ttl_seconds, Some(9));
        let Some(tl::enums::InputFileLocation::InputDocumentFileLocation(l)) =
            d.to_raw_input_location()
        else {
            panic!()
        };
        assert_eq!((l.id, l.thumb_size.as_str()), (5, ""));
    }

    #[test]
    fn stickers_need_a_sticker_attribute() {
        assert!(Sticker::from_document(&with(vec![])).is_none());
        assert!(Sticker::from_document(&empty()).is_none());
        let s = Sticker::from_document(&with(vec![sticker_attr()])).unwrap();
        assert_eq!(s.emoji(), "😀");
        assert!(!s.is_animated());
        let s = Sticker::from_document(&with(vec![
            sticker_attr(),
            tl::enums::DocumentAttribute::Animated,
        ]))
        .unwrap();
        assert!(s.is_animated());
    }
}
