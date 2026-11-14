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

        match self.raw.document.as_ref().unwrap() {
            D::Empty(document) => document.id,
            D::Document(document) => document.id,
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

        match self.raw.document.as_ref().unwrap() {
            D::Empty(_) => None,
            D::Document(document) => document.attributes.iter().find_map(|attr| match attr {
                tl::enums::DocumentAttribute::Filename(attr) => Some(attr.file_name.as_ref()),
                _ => None,
            }),
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
