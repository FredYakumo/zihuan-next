use serde::{Deserialize, Serialize};

use crate::ims_bot_adapter::models::message::{PersistedMedia, PersistedMediaSource};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    Text { text: String },
    Image { media: PersistedMedia },
    Video { media: PersistedMedia },
}

impl MessagePart {
    /// Build a plain text content part from any string-like input.
    pub fn text<S: Into<String>>(s: S) -> Self {
        MessagePart::Text { text: s.into() }
    }

    /// Wrap an existing persisted image media record as an image part.
    pub fn image_media(media: PersistedMedia) -> Self {
        MessagePart::Image { media }
    }

    /// Wrap an existing persisted video media record as a video part.
    pub fn video_media(media: PersistedMedia) -> Self {
        MessagePart::Video { media }
    }

    /// Build an upload-scoped image part from a direct URL or locator string.
    pub fn image_url_string<S: Into<String>>(url: S) -> Self {
        MessagePart::image_media(PersistedMedia::new(
            PersistedMediaSource::Upload,
            url.into(),
            "",
            None,
            None,
            None,
        ))
    }

    /// Build an inline image part by encoding MIME metadata into a data URL wrapper.
    pub fn image_data_url<M: AsRef<str>, B: AsRef<str>>(mime: M, base64_payload: B) -> Self {
        MessagePart::image_media(PersistedMedia::new(
            PersistedMediaSource::Upload,
            format!(
                "data:{};base64,{}",
                mime.as_ref(),
                base64_payload.as_ref()
            ),
            "",
            None,
            None,
            Some(mime.as_ref().to_string()),
        ))
    }

    /// Build an upload-scoped video part from a direct URL or locator string.
    pub fn video_url_string<S: Into<String>>(url: S) -> Self {
        MessagePart::video_media(PersistedMedia::new(
            PersistedMediaSource::Upload,
            url.into(),
            "",
            None,
            None,
            None,
        ))
    }

    /// Build an inline video part by encoding MIME metadata into a data URL wrapper.
    pub fn video_data_url<M: AsRef<str>, B: AsRef<str>>(mime: M, base64_payload: B) -> Self {
        MessagePart::video_media(PersistedMedia::new(
            PersistedMediaSource::Upload,
            format!(
                "data:{};base64,{}",
                mime.as_ref(),
                base64_payload.as_ref()
            ),
            "",
            None,
            None,
            Some(mime.as_ref().to_string()),
        ))
    }

    /// Return the underlying persisted media when this part is image or video.
    pub fn media(&self) -> Option<&PersistedMedia> {
        match self {
            MessagePart::Text { .. } => None,
            MessagePart::Image { media } | MessagePart::Video { media } => Some(media),
        }
    }

    /// Resolve the best provider-facing media locator, preferring inline and remote URLs first.
    pub fn media_locator(&self) -> Option<&str> {
        let media = self.media()?;
        if media.original_source.starts_with("data:")
            || media.original_source.starts_with("http://")
            || media.original_source.starts_with("https://")
        {
            return Some(media.original_source.as_str());
        }
        if media.rustfs_path.starts_with("data:")
            || media.rustfs_path.starts_with("http://")
            || media.rustfs_path.starts_with("https://")
        {
            return Some(media.rustfs_path.as_str());
        }
        media.primary_locator()
    }
}
