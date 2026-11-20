//! Profile photos: what is accepted and how it is served.
//!
//! The panel shrinks the chosen picture to a 256×256 square in the browser
//! and uploads it as WebP, PNG or JPEG. xBNC checks the real format of the
//! bytes (not only the name or the type the browser claims), the size and
//! the dimensions, and serves it back with headers that keep a browser from
//! ever running it as anything but an image.

use anyhow::{bail, Result};

/// Largest photo accepted, in bytes.
pub const MAX_BYTES: usize = 256 * 1024;
/// Largest width or height accepted, in pixels.
pub const MAX_SIDE: u32 = 1024;

/// The MIME type of a photo after checking its bytes, which must match the
/// type the browser sent.
pub fn check(claimed: &str, data: &[u8]) -> Result<&'static str> {
    if data.is_empty() {
        bail!("The image is empty");
    }
    if data.len() > MAX_BYTES {
        bail!("The image is larger than 256 KB");
    }
    let (mime, dims) = sniff(data).ok_or_else(|| anyhow::anyhow!("Only PNG, JPEG and WebP images are accepted"))?;
    let claimed = claimed.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    if claimed != mime {
        bail!("The image does not match its type");
    }
    match dims {
        Some((w, h)) if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE => {
            bail!("The image must be at most 1024×1024 pixels")
        }
        Some(_) => {}
        None => bail!("The image is damaged"),
    }
    Ok(mime)
}

/// Format and dimensions from the file's own header.
fn sniff(d: &[u8]) -> Option<(&'static str, Option<(u32, u32)>)> {
    if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        // IHDR is always the first chunk.
        let dims = (d.len() >= 24 && &d[12..16] == b"IHDR").then(|| (be32(&d[16..20]), be32(&d[20..24])));
        return Some(("image/png", dims));
    }
    if d.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(("image/jpeg", jpeg_dims(d)));
    }
    if d.len() >= 30 && &d[0..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        return Some(("image/webp", webp_dims(d)));
    }
    None
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn jpeg_dims(d: &[u8]) -> Option<(u32, u32)> {
    let mut i = 2;
    while i + 9 < d.len() {
        if d[i] != 0xFF {
            return None;
        }
        let marker = d[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let len = u16::from_be_bytes([d[i + 2], d[i + 3]]) as usize;
        // Start-of-frame markers carry the size (not DHT, JPG or DAC).
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([d[i + 5], d[i + 6]]) as u32;
            let w = u16::from_be_bytes([d[i + 7], d[i + 8]]) as u32;
            return Some((w, h));
        }
        if len < 2 {
            return None;
        }
        i += 2 + len;
    }
    None
}

fn webp_dims(d: &[u8]) -> Option<(u32, u32)> {
    match &d[12..16] {
        b"VP8 " if d.len() >= 30 && d[23..26] == [0x9D, 0x01, 0x2A] => {
            let w = u16::from_le_bytes([d[26], d[27]]) as u32 & 0x3FFF;
            let h = u16::from_le_bytes([d[28], d[29]]) as u32 & 0x3FFF;
            Some((w, h))
        }
        b"VP8L" if d.len() >= 25 && d[20] == 0x2F => {
            let b = u32::from_le_bytes([d[21], d[22], d[23], d[24]]);
            Some(((b & 0x3FFF) + 1, ((b >> 14) & 0x3FFF) + 1))
        }
        b"VP8X" if d.len() >= 30 => {
            let w = u32::from_le_bytes([d[24], d[25], d[26], 0]) + 1;
            let h = u32::from_le_bytes([d[27], d[28], d[29], 0]) + 1;
            Some((w, h))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        v
    }

    fn jpeg(w: u16, h: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF, 0xE0, 0, 16];
        v.extend_from_slice(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        v.extend_from_slice(&[0xFF, 0xC0, 0, 17, 8]);
        v.extend_from_slice(&h.to_be_bytes());
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1, 0xFF, 0xD9]);
        v
    }

    fn webp_lossless(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        let b = (w - 1) | ((h - 1) << 14);
        v.extend_from_slice(&b.to_le_bytes());
        v.extend_from_slice(&[0u8; 8]);
        v
    }

    #[test]
    fn accepts_real_images() {
        assert_eq!(check("image/png", &png(256, 256)).unwrap(), "image/png");
        assert_eq!(check("image/jpeg", &jpeg(256, 256)).unwrap(), "image/jpeg");
        assert_eq!(check("image/webp", &webp_lossless(256, 256)).unwrap(), "image/webp");
        assert_eq!(check("image/png; charset=binary", &png(1, 1)).unwrap(), "image/png");
    }

    #[test]
    fn refuses_everything_else() {
        // Not an image, or an image of another type than claimed.
        assert!(check("image/png", b"<svg onload=alert(1)>").is_err());
        assert!(check("image/svg+xml", b"<svg/>").is_err());
        assert!(check("image/jpeg", &png(10, 10)).is_err());
        assert!(check("text/html", &png(10, 10)).is_err());
        assert!(check("image/png", b"").is_err());
        // Too big, in bytes or in pixels; damaged headers.
        let mut big = png(10, 10);
        big.resize(MAX_BYTES + 1, 0);
        assert!(check("image/png", &big).is_err());
        assert!(check("image/png", &png(MAX_SIDE + 1, 10)).is_err());
        assert!(check("image/png", &png(0, 10)).is_err());
        assert!(check("image/jpeg", &[0xFF, 0xD8, 0xFF, 0x00, 1, 2, 3, 4, 5, 6, 7]).is_err());
        assert!(check("image/webp", &webp_lossless(2000, 10)).is_err());
    }
}
