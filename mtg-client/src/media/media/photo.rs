// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Photo and its download.

use super::*;

impl Photo {
    pub fn from_raw(photo: tl::enums::Photo) -> Self {
        Self {
            raw: tl::types::MessageMediaPhoto {
                spoiler: false,
                photo: Some(photo),
                ttl_seconds: None,
                live_photo: false,
                video: None,
            },
        }
    }

    pub fn from_raw_media(photo: tl::types::MessageMediaPhoto) -> Self {
        Self { raw: photo }
    }

    pub fn to_raw_input_media(&self) -> tl::types::InputMediaPhoto {
        tl::types::InputMediaPhoto {
            spoiler: false,
            id: self.to_raw_input_photo(),
            ttl_seconds: self.raw.ttl_seconds,
            live_photo: false,
            video: None,
        }
    }

    pub fn to_raw_input_photo(&self) -> tl::enums::InputPhoto {
        use tl::{
            enums::{InputPhoto as eInputPhoto, Photo},
            types::InputPhoto,
        };

        match self.raw.photo {
            Some(Photo::Photo(ref photo)) => InputPhoto {
                id: photo.id,
                access_hash: photo.access_hash,
                file_reference: photo.file_reference.clone(),
            }
            .into(),
            _ => eInputPhoto::Empty,
        }
    }

    /// The globally unique identifier for this media, as seen by any account.
    pub fn id(&self) -> i64 {
        use tl::enums::Photo as P;

        match self.raw.photo.as_ref().unwrap() {
            P::Empty(photo) => photo.id,
            P::Photo(photo) => photo.id,
        }
    }

    /// The size of the photo, if not empty.
    pub fn size(&self) -> Option<usize> {
        self.thumbs()
            .iter()
            .max_by_key(|x| x.size())
            .map(|thumb| thumb.size())
    }

    /// Get photo thumbs.
    ///
    /// Since Telegram doesn't store the original photo, it can be presented in different sizes
    /// and quality, a.k.a. thumbnails. Each photo preview has a specific type, indicating
    /// the resolution and image transform that was applied server-side. Some low-resolution
    /// thumbnails already contain all necessary information that can be shown to the user, but
    /// for other types an additional request to the Telegram should be performed.
    /// Check the description of [`PhotoSize``] to get an information about each particular thumbnail.
    ///
    /// <https://core.telegram.org/api/files#image-thumbnail-types>
    pub fn thumbs(&self) -> Vec<PhotoSize> {
        use tl::enums::Photo as P;

        let photo = match self.raw.photo.as_ref() {
            Some(photo) => photo,
            None => return vec![],
        };

        match photo {
            P::Empty(_) => vec![],
            P::Photo(photo) => photo
                .sizes
                .iter()
                .map(|x| PhotoSize::make_from(x, photo))
                .collect(),
        }
    }

    /// Returns true if the photo is a spoiler.
    pub fn is_spoiler(&self) -> bool {
        self.raw.spoiler
    }

    /// Returns TTL seconds if the photo is self-destructive, None otherwise
    pub fn ttl_seconds(&self) -> Option<i32> {
        self.raw.ttl_seconds
    }
}

impl Downloadable for Photo {
    fn to_raw_input_location(&self) -> Option<tl::enums::InputFileLocation> {
        use tl::enums::Photo as P;

        self.raw.photo.as_ref().and_then(|p| match p {
            P::Empty(_) => None,
            P::Photo(photo) => Some(
                tl::types::InputPhotoFileLocation {
                    id: photo.id,
                    access_hash: photo.access_hash,
                    file_reference: photo.file_reference.clone(),
                    thumb_size: self
                        .thumbs()
                        .iter()
                        .max_by_key(|x| x.size())
                        .map(|ps| ps.photo_type())
                        .unwrap_or(String::from("w")),
                }
                .into(),
            ),
        })
    }

    fn size(&self) -> Option<usize> {
        self.size()
    }

    fn dc_id(&self) -> Option<i32> {
        use tl::enums::Photo as P;
        match self.raw.photo.as_ref()? {
            P::Empty(_) => None,
            P::Photo(photo) => Some(photo.dc_id),
        }
    }
}
