//! Portable image data returned by platform icon providers.

/// An owned RGBA image buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Row-major RGBA pixel data.
    pub rgba: Vec<u8>,
}

impl Image {
    /// Returns a simple generic application icon.
    #[must_use]
    pub fn generic() -> Self {
        let mut rgba = vec![0_u8; 16 * 16 * 4];
        for y in 2_usize..14 {
            for x in 2_usize..14 {
                let offset = (y * 16 + x) * 4;
                let border = !(4..12).contains(&x) || !(4..12).contains(&y);
                let color = if border {
                    [120, 130, 148, 255]
                } else {
                    [72, 92, 120, 255]
                };
                rgba[offset..offset + 4].copy_from_slice(&color);
            }
        }
        Self {
            width: 16,
            height: 16,
            rgba,
        }
    }
}
