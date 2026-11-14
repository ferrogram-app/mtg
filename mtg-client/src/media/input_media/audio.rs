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
        self.duration = duration.as_secs().try_into().unwrap();
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
