// Copyright 2020 - developers of the `grammers` project.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::time::Duration;

use mtg_tl_types as tl;

/// Document media attributes.
#[derive(Clone)]
pub enum Attribute {
    Audio {
        duration: Duration,
        title: Option<String>,
        performer: Option<String>,
    },
    Voice {
        duration: Duration,
        waveform: Option<Vec<u8>>,
    },
    Video {
        round_message: bool,
        supports_streaming: bool,
        duration: Duration,
        w: i32,
        h: i32,
    },
    FileName(String),
}

impl From<Attribute> for tl::enums::DocumentAttribute {
    fn from(attr: Attribute) -> Self {
        use Attribute::*;
        match attr {
            Audio {
                duration,
                title,
                performer,
            } => Self::Audio(tl::types::DocumentAttributeAudio {
                voice: false,
                duration: i32::try_from(duration.as_secs()).unwrap_or(i32::MAX),
                title,
                performer,
                waveform: None,
            }),
            Voice { duration, waveform } => Self::Audio(tl::types::DocumentAttributeAudio {
                voice: true,
                duration: i32::try_from(duration.as_secs()).unwrap_or(i32::MAX),
                title: None,
                performer: None,
                waveform,
            }),
            Video {
                round_message,
                supports_streaming,
                duration,
                w,
                h,
            } => Self::Video(tl::types::DocumentAttributeVideo {
                round_message,
                supports_streaming,
                nosound: false,
                duration: duration.as_secs_f64(),
                w,
                h,
                preload_prefix_size: None,
                video_start_ts: None,
                video_codec: None,
            }),
            FileName(file_name) => {
                Self::Filename(tl::types::DocumentAttributeFilename { file_name })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Da = tl::enums::DocumentAttribute;

    #[test]
    fn audio_and_voice() {
        let a: Da = Attribute::Audio {
            duration: Duration::from_secs(7),
            title: Some("t".into()),
            performer: Some("p".into()),
        }
        .into();
        let Da::Audio(a) = a else { panic!("not audio") };
        assert!(!a.voice && a.duration == 7);
        assert_eq!(
            (a.title.as_deref(), a.performer.as_deref()),
            (Some("t"), Some("p"))
        );

        let v: Da = Attribute::Voice {
            duration: Duration::from_secs(u64::MAX),
            waveform: Some(vec![1, 2]),
        }
        .into();
        let Da::Audio(v) = v else { panic!("not voice") };
        // an absurd duration saturates instead of panicking
        assert!(v.voice && v.duration == i32::MAX && v.waveform == Some(vec![1, 2]));
    }

    #[test]
    fn video_and_file_name() {
        let v: Da = Attribute::Video {
            round_message: true,
            supports_streaming: true,
            duration: Duration::from_millis(1500),
            w: 3,
            h: 4,
        }
        .into();
        let Da::Video(v) = v else { panic!("not video") };
        assert!(v.round_message && v.supports_streaming && !v.nosound);
        assert_eq!((v.duration, v.w, v.h), (1.5, 3, 4));

        let f: Da = Attribute::FileName("a.txt".into()).into();
        assert!(matches!(f, Da::Filename(f) if f.file_name == "a.txt"));
    }
}
