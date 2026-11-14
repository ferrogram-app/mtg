use super::*;

/// Convenience builder for an video [`InputMedia`].
///
/// # Examples
///
/// ```
/// # async fn f(client: &mut mtg_client::Client) -> Result<(), Box<dyn std::error::Error>> {
/// use std::time::Duration;
/// use mtg_client::media::InputVideo;
/// use mtg_client::message::InputMessage;
///
/// let uploaded = client.upload_file("target/input.mp4").await?;
/// let thumb = client.upload_file("target/first_frame.jpg").await?;
/// let media = InputVideo::new(uploaded)
///     .mime_type("video/mp4")
///     .thumb(Some(thumb))
///     .duration(Duration::from_secs_f64(13.78))
///     .size(1920, 1080);
/// let media = InputMessage::new().media(media);
/// # Ok(())
/// # }
pub struct InputVideo {
    file: Uploaded,
    force_file: bool,
    spoiler: bool,
    nosound: bool,
    nosound_video: bool,
    animated: bool,
    mime_type: String,
    thumb: Option<Uploaded>,
    video_cover: Option<tl::enums::InputPhoto>,
    video_timestamp: Option<i32>,
    ttl: Option<i32>,
    stickers: Option<Vec<tl::enums::InputDocument>>,
    round_message: bool,
    supports_streaming: bool,
    duration: f64,
    w: i32,
    h: i32,
    preload_prefix_size: Option<i32>,
    video_start_ts: Option<f64>,
    video_codec: Option<String>,
}

impl InputVideo {
    pub fn new(uploaded: Uploaded) -> Self {
        Self {
            file: uploaded,
            force_file: false,
            spoiler: false,
            nosound: false,
            nosound_video: false,
            animated: false,
            mime_type: String::new(),
            thumb: None,
            video_cover: None,
            video_timestamp: None,
            ttl: None,
            stickers: None,
            round_message: false,
            supports_streaming: true,
            duration: 0.0,
            w: 0,
            h: 0,
            preload_prefix_size: None,
            video_start_ts: None,
            video_codec: None,
        }
    }

    /// Sets whether force the media file to be uploaded as document.
    pub fn force_file(mut self, force_file: bool) -> Self {
        self.force_file = force_file;
        self
    }

    /// Sets whether this video should be hidden behind a spoiler warning.
    pub fn spoiler(mut self, spoiler: bool) -> Self {
        self.spoiler = spoiler;
        self
    }

    /// Sets whether this video does not contain an audio track.
    pub fn nosound(mut self, nosound: bool) -> Self {
        self.nosound = nosound;
        self
    }

    /// Sets whether to keep videos without an audio track as videos.
    ///
    /// * If `true` (default), such videos are still sent as videos.
    /// * If `false`, such videos are sent as animations (`DocumentAttributeAnimated`) instead.
    pub fn nosound_video(mut self, nosound_video: bool) -> Self {
        self.nosound_video = nosound_video;
        self
    }

    /// Sets whether the video is displayed as an animated GIF.
    pub fn animated(mut self, animated: bool) -> Self {
        self.animated = animated;
        self.nosound_video = false;
        self
    }

    /// Sets the mime type of the video.
    ///
    /// Note: if the mime_type is not video, it will not be displayed as video.
    pub fn mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = mime_type.into();
        self
    }

    /// Sets the thumbnail of the video.
    pub fn thumb(mut self, thumb: Option<Uploaded>) -> Self {
        self.thumb = thumb;
        self
    }

    /// Sets the cover of the video.
    ///
    /// tip: [`tl::enums::InputPhoto`] can get from [`super::Photo::to_raw_input_photo`].
    pub fn video_cover(mut self, video_cover: Option<tl::enums::InputPhoto>) -> Self {
        self.video_cover = video_cover;
        self
    }

    /// Sets the timestamp at which to start playing the video.
    pub fn video_timestamp(mut self, video_timestamp: Option<Duration>) -> Self {
        self.video_timestamp = video_timestamp.map(|t| t.as_secs().try_into().unwrap());
        self
    }

    /// Change the video's Time To Live (TTL).
    ///
    /// This enables you to send a `video` that can only be viewed for a certain
    /// amount of seconds before it expires.
    pub fn ttl(mut self, ttl: TimeToLive) -> Self {
        self.ttl = ttl.into();
        self
    }

    /// Attached stickers.
    pub fn stickers(mut self, stickers: Option<Vec<tl::enums::InputDocument>>) -> Self {
        self.stickers = stickers;
        self
    }

    /// Sets whether this is a round video.
    pub fn round_message(mut self, round_message: bool) -> Self {
        self.round_message = round_message;
        self
    }

    /// Sets whether the video supports streaming
    pub fn supports_streaming(mut self, supports_streaming: bool) -> Self {
        self.supports_streaming = supports_streaming;
        self
    }

    /// Sets the duration of the video.
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = duration.as_secs_f64();
        self
    }

    // Sets the width and height of the video.
    pub fn size(mut self, w: i32, h: i32) -> Self {
        self.w = w;
        self.h = h;
        self
    }

    /// Number of bytes to preload when preloading videos.
    pub fn preload_prefix_size(mut self, preload_prefix_size: Option<i32>) -> Self {
        self.preload_prefix_size = preload_prefix_size;
        self
    }

    /// Floating point UNIX timestamp in seconds, indicating the frame of the video that
    /// should be used as static preview and thumbnail.
    pub fn video_start_ts(mut self, video_start_ts: Option<Duration>) -> Self {
        self.video_start_ts = video_start_ts.map(|d| d.as_secs_f64());
        self
    }

    /// Sets the codec used for the video, i.e. "h264", "h265", or "av1".
    pub fn video_codec(mut self, video_codec: Option<impl Into<String>>) -> Self {
        self.video_codec = video_codec.map(Into::into);
        self
    }
}

impl From<InputVideo> for tl::enums::InputMedia {
    fn from(x: InputVideo) -> Self {
        let InputVideo {
            file,
            force_file,
            spoiler,
            nosound,
            nosound_video,
            animated,
            mime_type,
            thumb,
            video_cover,
            video_timestamp,
            ttl,
            stickers,
            round_message,
            supports_streaming,
            duration,
            w,
            h,
            preload_prefix_size,
            video_start_ts,
            video_codec,
        } = x;
        let file_name = file.name().to_string();
        let mut attributes = vec![
            tl::types::DocumentAttributeFilename { file_name }.into(),
            tl::types::DocumentAttributeVideo {
                round_message,
                supports_streaming,
                nosound,
                duration,
                w,
                h,
                preload_prefix_size,
                video_start_ts,
                video_codec,
            }
            .into(),
        ];
        if animated {
            attributes.push(tl::types::DocumentAttributeAnimated {}.into())
        }

        tl::types::InputMediaUploadedDocument {
            nosound_video,
            force_file,
            spoiler,
            file: file.raw,
            thumb: thumb.map(|t| t.raw),
            mime_type,
            attributes,
            stickers,
            ttl_seconds: ttl,
            video_cover,
            video_timestamp,
        }
        .into()
    }
}
