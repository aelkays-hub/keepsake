//! What each rendition is, and the recipe hash that names it in the cache.
//!
//! The recipe is canonical JSON (keys sorted) with a version number. Any
//! change to how a variant is made must bump [`RECIPE_VERSION`] or change
//! its parameters, so new renditions get new names and old ones age out.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::catalog::{Blake3Hash, RenditionKind};

/// Bump when a renderer's output changes for the same parameters.
pub const RECIPE_VERSION: u32 = 1;
/// JPEG quality of thumbnails and previews.
pub const JPEG_QUALITY: u8 = 82;
/// JPEG quality of full-size renditions (zoom to 100%).
pub const FULL_QUALITY: u8 = 92;
/// Proxy height (spike: `scale=-2:720`); smaller sources are not enlarged.
pub const PROXY_HEIGHT: u32 = 720;
/// Proxy audio (spike: AAC 128 kb/s), stereo.
pub const PROXY_AUDIO_KBPS: u32 = 128;

/// A kind of rendition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    /// 256 px long edge, JPEG.
    Thumb256,
    /// 1024 px long edge, JPEG.
    Thumb1024,
    /// 2560 px long edge (or the source's size if smaller), JPEG.
    Preview2560,
    /// 720p H.264 + AAC MP4 with a GOP of at most one second.
    Proxy720,
    /// The camera's own H.264 proxy (`PRIVATE/M4ROOT/SUB`), copied unchanged.
    CameraProxy,
    /// Full size, for zooming to 100%: the JPEG/PNG/TIFF/HEIF decoded, as
    /// JPEG. Made on demand.
    Full,
    /// Full size from the RAW, decoded by LibRaw (camera white balance,
    /// sRGB). Made on demand.
    FullRaw,
}

impl Variant {
    pub const STILLS: [Variant; 3] = [Variant::Preview2560, Variant::Thumb1024, Variant::Thumb256];
    pub const ALL: [Variant; 7] = [
        Variant::Thumb256,
        Variant::Thumb1024,
        Variant::Preview2560,
        Variant::Proxy720,
        Variant::CameraProxy,
        Variant::Full,
        Variant::FullRaw,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Variant::Thumb256 => "thumb-256",
            Variant::Thumb1024 => "thumb-1024",
            Variant::Preview2560 => "preview-2560",
            Variant::Proxy720 => "proxy-720",
            Variant::CameraProxy => "camera-proxy",
            Variant::Full => "full",
            Variant::FullRaw => "full-raw",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Variant::ALL.into_iter().find(|v| v.name() == s)
    }

    pub fn kind(self) -> RenditionKind {
        match self {
            Variant::Thumb256 | Variant::Thumb1024 => RenditionKind::Thumbnail,
            Variant::Preview2560 | Variant::Full | Variant::FullRaw => RenditionKind::Preview,
            Variant::Proxy720 | Variant::CameraProxy => RenditionKind::Proxy,
        }
    }

    /// Longest edge in pixels, for stills.
    pub fn long_edge(self) -> Option<u32> {
        match self {
            Variant::Thumb256 => Some(256),
            Variant::Thumb1024 => Some(1024),
            Variant::Preview2560 => Some(2560),
            _ => None,
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            Variant::Proxy720 | Variant::CameraProxy => "mp4",
            _ => "jpg",
        }
    }

    /// The recipe as canonical JSON.
    pub fn recipe(self) -> serde_json::Value {
        match self {
            Variant::Thumb256 | Variant::Thumb1024 | Variant::Preview2560 => json!({
                "v": RECIPE_VERSION,
                "variant": self.name(),
                "long_edge": self.long_edge(),
                "format": "jpeg",
                "quality": JPEG_QUALITY,
                "orientation": "baked",
                "resize": "lanczos3",
                "video_frame_at": 0.10,
            }),
            Variant::Proxy720 => json!({
                "v": RECIPE_VERSION,
                "variant": self.name(),
                "video": "h264",
                "height": PROXY_HEIGHT,
                "gop_s": 1,
                "pix_fmt": "yuv420p",
                "audio": "aac",
                "audio_kbps": PROXY_AUDIO_KBPS,
                "audio_channels": 2,
                "faststart": true,
                "deinterlace": "yadif",
            }),
            Variant::CameraProxy => json!({
                "v": RECIPE_VERSION,
                "variant": self.name(),
                "copy": "camera SUB proxy, H.264 only",
            }),
            Variant::Full => json!({
                "v": RECIPE_VERSION,
                "variant": self.name(),
                "format": "jpeg",
                "quality": FULL_QUALITY,
                "orientation": "baked",
            }),
            Variant::FullRaw => json!({
                "v": RECIPE_VERSION,
                "variant": self.name(),
                "decoder": "libraw 0.22 dcraw_emu -w -o 1 -q 3",
                "format": "jpeg",
                "quality": FULL_QUALITY,
            }),
        }
    }

    pub fn recipe_hash(self) -> Blake3Hash {
        // serde_json objects keep keys sorted, so this is canonical.
        Blake3Hash::of(self.recipe().to_string().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recipes_are_distinct_and_stable() {
        let hashes: std::collections::HashSet<_> =
            Variant::ALL.iter().map(|v| v.recipe_hash()).collect();
        assert_eq!(hashes.len(), Variant::ALL.len());
        assert_eq!(
            Variant::Thumb256.recipe_hash(),
            Variant::Thumb256.recipe_hash()
        );
        let text = Variant::Thumb256.recipe().to_string();
        assert!(
            text.find("\"format\"") < text.find("\"long_edge\""),
            "{text}"
        );
        for v in Variant::ALL {
            assert_eq!(Variant::parse(v.name()), Some(v));
        }
    }
}
