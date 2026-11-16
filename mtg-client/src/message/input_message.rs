// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::time::{SystemTime, UNIX_EPOCH};

use mtg_tl_types as tl;

use super::input_rich_message::InputRichMessage;
use super::reply_markup::ReplyMarkup;
use crate::media::{Attribute, Media, Uploaded};

// https://github.com/telegramdesktop/tdesktop/blob/e7fbcce9d9f0a8944eb2c34e74bd01b8776cb891/Telegram/SourceFiles/data/data_scheduled_messages.h#L52
const SCHEDULE_ONCE_ONLINE: i32 = 0x7ffffffe;

/// Construct and send rich text messages with various options.
#[derive(Clone, Default)]
pub struct InputMessage {
    pub(crate) background: bool,
    pub(crate) clear_draft: bool,
    pub(crate) entities: Vec<tl::enums::MessageEntity>,
    pub(crate) invert_media: bool,
    pub(crate) link_preview: bool,
    pub(crate) reply_markup: Option<tl::enums::ReplyMarkup>,
    pub(crate) reply_to: Option<i32>,
    pub(crate) schedule_date: Option<i32>,
    pub(crate) silent: bool,
    pub(crate) text: String,
    pub(crate) media: Option<tl::enums::InputMedia>,
    pub(crate) rich_message: Option<tl::enums::InputRichMessage>,
    media_ttl: Option<i32>,
    mime_type: Option<String>,
}

impl InputMessage {
    /// Creates a new empty message for input.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether to "send this message as a background message".
    ///
    /// This description is taken from <https://core.telegram.org/method/messages.sendMessage>.
    pub fn background(mut self, background: bool) -> Self {
        self.background = background;
        self
    }

    /// Whether the draft in this peer, if any, should be cleared.
    pub fn clear_draft(mut self, clear_draft: bool) -> Self {
        self.clear_draft = clear_draft;
        self
    }

    /// Replaces the plaintext in the message.
    ///
    /// The caller must ensure that formatting entities remain valid for the given text.
    /// If you need to update formatting entities, call method [`InputMessage::fmt_entities`].
    ///
    /// <div class="warning">
    /// Note that this method does not modify formatting entities, which may break
    /// formatting or cause out-of-bounds errors if entities do not match the given text.
    /// </div>
    pub fn text<T>(mut self, s: T) -> Self
    where
        T: Into<String>,
    {
        self.text = s.into();
        self
    }

    /// The formatting entities within the message (such as bold, italics, etc.).
    pub fn fmt_entities<I>(mut self, entities: I) -> Self
    where
        I: IntoIterator<Item = tl::enums::MessageEntity>,
    {
        self.entities = entities.into_iter().collect();
        self
    }

    /// Builds a new message from the given markdown-formatted string as the
    /// message contents and entities.
    ///
    /// Note that Telegram only supports a very limited subset of entities:
    /// bold, italic, underline, strikethrough, code blocks, pre blocks and inline links (inline
    /// links with this format `tg://user?id=12345678` will be replaced with inline mentions when
    /// possible).
    #[cfg(feature = "markdown")]
    pub fn markdown<T>(mut self, s: T) -> Self
    where
        T: AsRef<str>,
    {
        let (text, entities) = crate::parsers::parse_markdown_message(s.as_ref());
        self.text = text;
        self.entities = entities;
        self
    }

    /// Builds a new message from the given HTML-formatted string as the
    /// message contents and entities.
    ///
    /// Note that Telegram only supports a very limited subset of entities:
    /// bold, italic, underline, strikethrough, code blocks, pre blocks and inline links (inline
    /// links with this format `tg://user?id=12345678` will be replaced with inline mentions when
    /// possible).
    #[cfg(feature = "html")]
    pub fn html<T>(mut self, s: T) -> Self
    where
        T: AsRef<str>,
    {
        let (text, entities) = crate::parsers::parse_html_message(s.as_ref());
        self.text = text;
        self.entities = entities;
        self
    }

    /// Whether the media will be inverted.
    ///
    /// If inverted, photos, videos, and documents will appear at the bottom and link previews at the top of the message.
    pub fn invert_media(mut self, invert_media: bool) -> Self {
        self.invert_media = invert_media;
        self
    }

    /// Whether the link preview be shown for the message.
    ///
    /// This has no effect when sending media, which cannot contain a link preview.
    pub fn link_preview(mut self, link_preview: bool) -> Self {
        self.link_preview = link_preview;
        self
    }

    /// Defines the suggested reply markup for the message (such as adding inline buttons).
    /// This will be displayed below the message.
    ///
    /// Only bot accounts can make use of the reply markup feature (a user attempting to send a
    /// message with a reply markup will result in the markup being ignored by Telegram).
    ///
    /// The user is free to ignore the markup and continue sending usual text messages.
    ///
    /// See [`crate::message::ReplyMarkup`] for the different available markups along with how
    /// they behave.
    pub fn reply_markup(mut self, markup: ReplyMarkup) -> Self {
        self.reply_markup = Some(markup.raw);
        self
    }

    /// The message identifier to which this message should reply to, if any.
    ///
    /// Otherwise, this message will not be a reply to any other.
    pub fn reply_to(mut self, reply_to: Option<i32>) -> Self {
        self.reply_to = reply_to;
        self
    }

    /// If set to a distant enough future time, the message won't be sent immediately,
    /// and instead it will be scheduled to be automatically sent at a later time.
    ///
    /// This scheduling is done server-side, and may not be accurate to the second.
    ///
    /// Bot accounts cannot schedule messages.
    pub fn schedule_date(mut self, schedule_date: Option<SystemTime>) -> Self {
        self.schedule_date = schedule_date.map(|t| {
            t.duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i32)
                .unwrap_or(0)
        });
        self
    }

    /// Schedule the message to be sent once the person comes online.
    ///
    /// This only works in private chats, and only if the person has their
    /// last seen visible.
    ///
    /// Bot accounts cannot schedule messages.
    pub fn schedule_once_online(mut self) -> Self {
        self.schedule_date = Some(SCHEDULE_ONCE_ONLINE);
        self
    }

    /// Whether the message should notify people or not.
    ///
    /// Defaults to `false`, which means it will notify them. Set it to `true`
    /// to alter this behaviour.
    pub fn silent(mut self, silent: bool) -> Self {
        self.silent = silent;
        self
    }

    /// Include the uploaded file as a photo in the message.
    ///
    /// The Telegram server will compress the image and convert it to JPEG format if necessary.
    ///
    /// The text will be the caption of the photo, which may be empty for no caption.
    pub fn photo(mut self, file: Uploaded) -> Self {
        self.media = Some(
            (tl::types::InputMediaUploadedPhoto {
                spoiler: false,
                file: file.raw,
                stickers: None,
                ttl_seconds: self.media_ttl,
                live_photo: false,
                video: None,
            })
            .into(),
        );
        self
    }

    /// Include an external photo in the message.
    ///
    /// The Telegram server will download and compress the image and convert it to JPEG format if
    /// necessary.
    ///
    /// The text will be the caption of the photo, which may be empty for no caption.
    pub fn photo_url(mut self, url: impl Into<String>) -> Self {
        self.media = Some(
            (tl::types::InputMediaPhotoExternal {
                spoiler: false,
                url: url.into(),
                ttl_seconds: self.media_ttl,
            })
            .into(),
        );
        self
    }

    /// Include the uploaded file as a document in the message.
    ///
    /// You can use this to send videos, stickers, audios, or uncompressed photos.
    ///
    /// The text will be the caption of the document, which may be empty for no caption.
    pub fn document(mut self, file: Uploaded) -> Self {
        let mime_type = self.get_file_mime(&file);
        self.media = Some(crate::media::uploaded_document(
            file,
            mime_type,
            false,
            self.media_ttl,
        ));
        self
    }

    /// Include a media in the message using the raw TL types.
    ///
    /// You can use this to send any media using the raw TL types that don't have
    /// a specific method in this builder such as Dice, Polls, etc.
    ///
    /// This can also be used to send media with a file reference, see `InputMediaDocument`
    /// and `InputMediaPhoto` in the `mtg-tl-types` crate.
    ///
    /// The text will be the caption of the media, which may be empty for no caption.
    pub fn media<M: Into<tl::enums::InputMedia>>(mut self, media: M) -> Self {
        self.media = Some(media.into());
        self
    }

    /// Include the video file with thumb in the message.
    ///
    /// The text will be the caption of the document, which may be empty for no caption.
    ///
    /// # Examples
    ///
    /// ```
    /// async fn f(client: &mut mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    ///     use mtg_client::message::InputMessage;
    ///
    ///     let video = client.upload_file("video.mp4").await?;
    ///     let thumb = client.upload_file("thumb.png").await?;
    ///     let message = InputMessage::new().text("").document(video).thumbnail(thumb);
    ///     Ok(())
    /// }
    /// ```
    pub fn thumbnail(mut self, thumb: Uploaded) -> Self {
        if let Some(tl::enums::InputMedia::UploadedDocument(document)) = &mut self.media {
            document.thumb = Some(thumb.raw);
        }
        self
    }

    /// Include an external file as a document in the message.
    ///
    /// You can use this to send videos, stickers, audios, or uncompressed photos.
    ///
    /// The Telegram server will be the one that downloads and includes the document as media.
    ///
    /// The text will be the caption of the document, which may be empty for no caption.
    pub fn document_url(mut self, url: impl Into<String>) -> Self {
        self.media = Some(
            (tl::types::InputMediaDocumentExternal {
                spoiler: false,
                url: url.into(),
                ttl_seconds: self.media_ttl,
                video_cover: None,
                video_timestamp: None,
            })
            .into(),
        );
        self
    }

    /// Add additional attributes to the message.
    ///
    /// This must be called *after* setting a file.
    ///
    /// # Examples
    ///
    /// ```
    /// # async fn f(client: &mut mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
    /// # let audio = client.upload_file("audio.flac").await?;
    /// #
    /// use std::time::Duration;
    /// use mtg_client::media::Attribute;
    /// use mtg_client::message::InputMessage;
    ///
    /// let message = InputMessage::new().text("").document(audio).attribute(
    ///    Attribute::Audio {
    ///        duration: Duration::new(123, 0),
    ///        title: Some("Hello".to_string()),
    ///        performer: Some("World".to_string()),
    ///    }
    /// );
    /// # Ok(())
    /// # }
    /// ```
    pub fn attribute(mut self, attr: Attribute) -> Self {
        if let Some(tl::enums::InputMedia::UploadedDocument(document)) = &mut self.media {
            document.attributes.push(attr.into());
        }
        self
    }

    /// Copy media from an existing message.
    ///
    /// You can use this to send media from another message without re-uploading it.
    pub fn copy_media(mut self, media: &Media) -> Self {
        self.media = media.to_raw_input_media();
        self
    }

    /// Include the uploaded file as a document file in the message.
    ///
    /// You can use this to send any type of media as a simple document file.
    ///
    /// The text will be the caption of the file, which may be empty for no caption.
    pub fn file(mut self, file: Uploaded) -> Self {
        let mime_type = self.get_file_mime(&file);
        self.media = Some(crate::media::uploaded_document(
            file,
            mime_type,
            true,
            self.media_ttl,
        ));
        self
    }

    /// Change the media's Time To Live (TTL).
    ///
    /// For example, this enables you to send a `photo` that can only be viewed for a certain
    /// amount of seconds before it expires.
    ///
    /// Not all media supports this feature.
    ///
    /// This method should be called before setting any media, else it won't have any effect.
    pub fn media_ttl(mut self, seconds: i32) -> Self {
        self.media_ttl = if seconds < 0 { None } else { Some(seconds) };
        self
    }

    /// Change the media's mime type.
    ///
    /// This method will override the mime type that would otherwise be automatically inferred
    /// from the extension of the used file
    ///
    /// If no mime type is set and it cannot be inferred, the mime type will be
    /// "application/octet-stream".
    ///
    /// This method should be called before setting any media, else it won't have any effect.
    pub fn mime_type(mut self, mime_type: &str) -> Self {
        self.mime_type = Some(mime_type.to_string());
        self
    }

    pub fn rich_message(mut self, rich_message: InputRichMessage) -> Self {
        self.rich_message = Some(rich_message.raw);
        self
    }

    /// Return the mime type string for the given file.
    fn get_file_mime(&self, file: &Uploaded) -> String {
        if let Some(mime) = self.mime_type.as_ref() {
            mime.clone()
        } else if let Some(mime) = mime_guess::from_path(file.name()).first() {
            mime.essence_str().to_string()
        } else {
            "application/octet-stream".to_string()
        }
    }
}

impl<S> From<S> for InputMessage
where
    S: Into<String>,
{
    fn from(text: S) -> Self {
        Self::new().text(text)
    }
}

impl From<super::Message> for InputMessage {
    fn from(message: super::Message) -> Self {
        let (text, entities, media) = match message.raw {
            tl::enums::Message::Empty(_) => (None, None, None),
            tl::enums::Message::Message(message) => {
                (Some(message.message), message.entities, message.media)
            }
            tl::enums::Message::Service(_) => (None, None, None),
        };

        Self {
            text: text.unwrap_or_default(),
            entities: entities.unwrap_or_default(),
            media: media
                .and_then(Media::from_raw)
                .and_then(|m| m.to_raw_input_media()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use mtg_testkit::FakeServer;
    use mtg_tl_types::Deserializable;

    use super::*;
    use crate::message::Message;
    use crate::test_util::{client, message, user_ref};

    fn file(name: &str) -> Uploaded {
        Uploaded {
            raw: tl::types::InputFile {
                id: 1,
                parts: 1,
                name: name.into(),
                md5_checksum: String::new(),
            }
            .into(),
        }
    }

    fn doc(m: &InputMessage) -> &tl::types::InputMediaUploadedDocument {
        let Some(tl::enums::InputMedia::UploadedDocument(d)) = &m.media else {
            panic!("not an uploaded document")
        };
        d
    }

    #[test]
    fn plain_flags_and_text_are_stored() {
        let m = InputMessage::new()
            .background(true)
            .clear_draft(true)
            .invert_media(true)
            .link_preview(true)
            .silent(true)
            .reply_to(Some(7))
            .text("hi")
            .fmt_entities([tl::enums::MessageEntity::Bold(
                tl::types::MessageEntityBold {
                    offset: 0,
                    length: 2,
                },
            )]);
        assert!(m.background && m.clear_draft && m.invert_media && m.link_preview && m.silent);
        assert_eq!(
            (m.reply_to, m.text.as_str(), m.entities.len()),
            (Some(7), "hi", 1)
        );
        assert_eq!(InputMessage::from("a").text, "a");
    }

    #[test]
    fn reply_markup_and_rich_message_are_kept() {
        let m = InputMessage::new().reply_markup(ReplyMarkup::hide());
        assert!(matches!(
            m.reply_markup,
            Some(tl::enums::ReplyMarkup::ReplyKeyboardHide(_))
        ));
        let m = InputMessage::new().rich_message(InputRichMessage::blocks(vec![]));
        assert!(m.rich_message.is_some());
    }

    #[test]
    fn scheduling() {
        let at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(
            InputMessage::new().schedule_date(Some(at)).schedule_date,
            Some(1_700_000_000)
        );
        assert_eq!(InputMessage::new().schedule_date(None).schedule_date, None);
        // a time before the epoch cannot be represented
        let before = UNIX_EPOCH - Duration::from_secs(5);
        assert_eq!(
            InputMessage::new()
                .schedule_date(Some(before))
                .schedule_date,
            Some(0)
        );
        assert_eq!(
            InputMessage::new().schedule_once_online().schedule_date,
            Some(SCHEDULE_ONCE_ONLINE)
        );
    }

    #[test]
    fn photos_carry_the_ttl_only_when_set_before() {
        let m = InputMessage::new().media_ttl(10).photo(file("a.jpg"));
        assert!(
            matches!(m.media, Some(tl::enums::InputMedia::UploadedPhoto(p)) if p.ttl_seconds == Some(10))
        );
        let m = InputMessage::new().media_ttl(-1).photo(file("a.jpg"));
        assert!(
            matches!(m.media, Some(tl::enums::InputMedia::UploadedPhoto(p)) if p.ttl_seconds.is_none())
        );
        let m = InputMessage::new()
            .media_ttl(3)
            .photo_url("https://x/a.jpg");
        assert!(
            matches!(m.media, Some(tl::enums::InputMedia::PhotoExternal(p)) if p.url == "https://x/a.jpg" && p.ttl_seconds == Some(3))
        );
        let m = InputMessage::new()
            .media_ttl(3)
            .document_url("https://x/a.bin");
        assert!(
            matches!(m.media, Some(tl::enums::InputMedia::DocumentExternal(d)) if d.url == "https://x/a.bin" && d.ttl_seconds == Some(3))
        );
    }

    #[test]
    fn documents_guess_or_override_the_mime_type() {
        let m = InputMessage::new().document(file("a.png"));
        assert_eq!(doc(&m).mime_type, "image/png");
        assert!(!doc(&m).force_file);
        let m = InputMessage::new().document(file("noext"));
        assert_eq!(doc(&m).mime_type, "application/octet-stream");
        let m = InputMessage::new()
            .mime_type("text/x-custom")
            .file(file("a.png"));
        assert_eq!(doc(&m).mime_type, "text/x-custom");
        assert!(doc(&m).force_file);
    }

    #[test]
    fn thumbnail_and_attributes_need_an_uploaded_document() {
        let m = InputMessage::new()
            .document(file("a.mp3"))
            .thumbnail(file("t.png"))
            .attribute(Attribute::FileName("x".into()));
        assert!(doc(&m).thumb.is_some());
        assert!(
            doc(&m).attributes.iter().any(
                |a| matches!(a, tl::enums::DocumentAttribute::Filename(f) if f.file_name == "x")
            )
        );

        // on anything else both are ignored
        let m = InputMessage::new()
            .photo_url("u")
            .thumbnail(file("t.png"))
            .attribute(Attribute::FileName("x".into()));
        assert!(matches!(
            m.media,
            Some(tl::enums::InputMedia::PhotoExternal(_))
        ));
        let m = InputMessage::new().thumbnail(file("t.png"));
        assert!(m.media.is_none());
    }

    #[test]
    fn raw_media_is_stored_as_is() {
        let m = InputMessage::new().media(tl::types::InputMediaEmpty {});
        assert!(matches!(m.media, Some(tl::enums::InputMedia::Empty)));
    }

    #[test]
    fn copy_media_of_a_dice_and_of_a_web_page() {
        let dice = Media::Dice(crate::media::Dice {
            raw: tl::types::MessageMediaDice {
                value: 3,
                emoticon: "🎲".into(),
                game_outcome: None,
            },
        });
        assert!(matches!(
            InputMessage::new().copy_media(&dice).media,
            Some(tl::enums::InputMedia::Dice(_))
        ));
    }

    #[cfg(feature = "markdown")]
    #[test]
    fn markdown_fills_text_and_entities() {
        let m = InputMessage::new().markdown("**b**");
        assert_eq!(m.text, "b");
        assert_eq!(m.entities.len(), 1);
    }

    #[cfg(feature = "html")]
    #[test]
    fn html_fills_text_and_entities() {
        let m = InputMessage::new().html("<b>b</b>");
        assert_eq!(m.text, "b");
        assert_eq!(m.entities.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_received_message_becomes_an_input_message() {
        let server = FakeServer::start().await;
        let client = client(&server);
        let peer = user_ref(5);
        let raw = message(1, peer, "hello", 0);
        let m = Message::from_raw(&client, raw, Some(peer), client.empty_peer_map().handle());
        let input = InputMessage::from(m);
        assert_eq!(input.text, "hello");
        assert!(input.media.is_none() && input.entities.is_empty());

        // an empty message has no text at all
        let empty = tl::enums::Message::from_bytes(
            &[
                &0x90a6ca84u32.to_le_bytes()[..],
                &0u32.to_le_bytes(),
                &9i32.to_le_bytes(),
            ]
            .concat(),
        )
        .unwrap();
        let m = Message::from_raw(&client, empty, Some(peer), client.empty_peer_map().handle());
        assert_eq!(InputMessage::from(m).text, "");
    }
}
