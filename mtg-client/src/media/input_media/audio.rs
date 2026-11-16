use super::*;

/// Convenience builder for an audio [`InputMedia`].
///
/// # Examples
///
/// ```
/// # async fn f(client: &mut mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
/// use std::time::Duration;
/// use mtg_client::media::InputAudio;
/// use mtg_client::message::InputMessage;
///
/// let uploaded = client.upload_file("target/input.mp3").await?;
/// let thumb = client.upload_file("target/cover.mp3").await?;
/// let media = InputAudio::new(uploaded)
///     .mime_type("audio/mpeg")
///     .thumb(Some(thumb))
///     .duration(Duration::from_secs(128))
///     .title(Some("SongName"))
///     .performer(Some("SongAuthor"));
/// let media = InputMessage::new().media(media);
/// # Ok(())
/// # }
pub struct InputAudio {
    file: Uploaded,
    force_file: bool,
    mime_type: String,
    thumb: Option<Uploaded>,
    voice: bool,
    duration: i32,
    title: Option<String>,
    performer: Option<String>,
    waveform: Option<Vec<u8>>,
}

impl InputAudio {
    pub fn new(uploaded: Uploaded) -> Self {
        Self {
            file: uploaded,
            force_file: false,
            mime_type: String::new(),
            thumb: None,
            voice: false,
            duration: 0,
            title: None,
            performer: None,
            waveform: None,
        }
    }

    /// Sets whether force the media file to be uploaded as document.
    pub fn force_file(mut self, force_file: bool) -> Self {
        self.force_file = force_file;
        self
    }

    /// Sets the mime type of the video.
    ///
    /// Note: if the mime_type is not audio, it will not be displayed as audio.
    pub fn mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = mime_type.into();
        self
    }

    /// Sets the thumbnail of the audio.
    pub fn thumb(mut self, thumb: Option<Uploaded>) -> Self {
        self.thumb = thumb;
        self
    }

    /// Sets whether this is a voice message.
    pub fn voice(mut self, voice: bool) -> Self {
        self.voice = voice;
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration.as_secs().try_into().unwrap_or(i32::MAX);
        self
    }

    /// Sets the name of the song.
    pub fn title(mut self, title: Option<impl Into<String>>) -> Self {
        self.title = title.map(Into::into);
        self
    }

    /// Sets the performer of the song.
    pub fn performer(mut self, performer: Option<impl Into<String>>) -> Self {
        self.performer = performer.map(Into::into);
        self
    }

    /// Sets the waveform of the song.
    pub fn waveform(mut self, waveform: Option<Vec<u8>>) -> Self {
        self.waveform = waveform;
        self
    }
}

impl From<InputAudio> for tl::enums::InputMedia {
    fn from(x: InputAudio) -> Self {
        let InputAudio {
            file,
            force_file,
            mime_type,
            thumb,
            voice,
            duration,
            title,
            performer,
            waveform,
        } = x;
        let file_name = file.name().to_string();
        tl::types::InputMediaUploadedDocument {
            nosound_video: false,
            force_file,
            spoiler: false,
            file: file.raw,
            thumb: thumb.map(|t| t.raw),
            mime_type,
            attributes: vec![
                tl::types::DocumentAttributeFilename { file_name }.into(),
                tl::types::DocumentAttributeAudio {
                    voice,
                    duration,
                    title,
                    performer,
                    waveform,
                }
                .into(),
            ],
            stickers: None,
            ttl_seconds: None,
            video_cover: None,
            video_timestamp: None,
        }
        .into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> Uploaded {
        Uploaded {
            raw: tl::types::InputFile {
                id: 1,
                parts: 1,
                name: "a.ogg".into(),
                md5_checksum: String::new(),
            }
            .into(),
        }
    }

    fn doc(a: InputAudio) -> tl::types::InputMediaUploadedDocument {
        let tl::enums::InputMedia::UploadedDocument(d) = a.into() else {
            panic!()
        };
        d
    }

    fn audio_attr(d: &tl::types::InputMediaUploadedDocument) -> &tl::types::DocumentAttributeAudio {
        d.attributes
            .iter()
            .find_map(|a| match a {
                tl::enums::DocumentAttribute::Audio(a) => Some(a),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn defaults_make_a_plain_audio_document() {
        let d = doc(InputAudio::new(file()));
        assert!(!d.force_file && d.thumb.is_none() && d.mime_type.is_empty());
        let a = audio_attr(&d);
        assert!(!a.voice && a.duration == 0 && a.title.is_none() && a.waveform.is_none());
        assert!(
            matches!(&d.attributes[0], tl::enums::DocumentAttribute::Filename(f) if f.file_name == "a.ogg")
        );
    }

    #[test]
    fn every_setter_reaches_the_document() {
        let d = doc(InputAudio::new(file())
            .force_file(true)
            .mime_type("audio/ogg")
            .thumb(Some(file()))
            .voice(true)
            .duration(Duration::from_secs(12))
            .title(Some("t"))
            .performer(Some("p"))
            .waveform(Some(vec![1, 2])));
        assert!(d.force_file && d.thumb.is_some());
        assert_eq!(d.mime_type, "audio/ogg");
        let a = audio_attr(&d);
        assert!(a.voice && a.duration == 12);
        assert_eq!(
            (a.title.as_deref(), a.performer.as_deref()),
            (Some("t"), Some("p"))
        );
        assert_eq!(a.waveform.as_deref(), Some(&[1u8, 2][..]));
    }

    #[test]
    fn a_huge_duration_saturates() {
        let d = doc(InputAudio::new(file()).duration(Duration::from_secs(u64::MAX)));
        assert_eq!(audio_attr(&d).duration, i32::MAX);
    }
}
