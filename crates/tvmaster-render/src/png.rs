//! A tiny PNG writer: RGBA, no compression (stored deflate blocks). Enough to
//! hand FFmpeg a transparent graphic without pulling in an image library.

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c = table[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for x in chunk {
            a += *x as u32;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend(data);
    out.extend(&body);
    out.extend(crc32(&body).to_be_bytes());
}

/// Encode `rgba` (`w * h * 4` bytes) as a PNG.
pub fn rgba_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(
        rgba.len(),
        (w * h * 4) as usize,
        "pixel data does not match the size"
    );
    // Each row is prefixed by filter type 0 (none).
    let mut raw = Vec::with_capacity(((w * 4 + 1) * h) as usize);
    for row in rgba.chunks((w * 4) as usize) {
        raw.push(0);
        raw.extend(row);
    }
    // zlib stream of stored blocks.
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65_535).peekable();
    if blocks.peek().is_none() {
        z.extend([1, 0, 0, 0xFF, 0xFF]);
    }
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend((b.len() as u16).to_le_bytes());
        z.extend((!(b.len() as u16)).to_le_bytes());
        z.extend(b);
    }
    z.extend(adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend(w.to_be_bytes());
    ihdr.extend(h.to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]); // 8-bit RGBA
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_the_known_values() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082, "the CRC every PNG ends with");
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn a_png_has_the_right_signature_and_chunks() {
        let png = rgba_png(
            2,
            2,
            &[255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 1, 2, 3, 4],
        );
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 13, 10, 26, 10]);
        assert!(png.windows(4).any(|w| w == b"IHDR") && png.windows(4).any(|w| w == b"IDAT"));
        assert_eq!(
            &png[png.len() - 8..],
            &[b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82],
            "ends with IEND and its CRC"
        );
        assert_eq!(&png[16..24], &[0, 0, 0, 2, 0, 0, 0, 2], "2x2");
    }

    #[test]
    fn large_images_span_several_stored_blocks() {
        let (w, h) = (300, 300); // 360 KB of pixels > one 64 KB block
        let png = rgba_png(w, h, &vec![7u8; (w * h * 4) as usize]);
        assert!(png.len() > 360_000);
    }
}
