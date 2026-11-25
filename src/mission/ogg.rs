//! The length of an Ogg Vorbis file, read from its pages (docs/phase5.md T10), so a speech entry's `duration` can be filled from the
//! author's own `.ogg` without a new dependency: the last page's granule position over the sample rate of the identification header.
//!
//! PROVISIONAL, pending the maintainer's choice between this reader and an engine `sound_duration` (docs/phase5.md engine
//! requirement 7, "reads `duration` the way the game would"): it understands Ogg pages (RFC 3533: `OggS`, version 0, the granule
//! position at byte 6, the segment table at 26) and the Vorbis identification header (Vorbis I, section 4.2.2: packet type 1, `vorbis`,
//! version, channels, sample rate). It does not check page CRCs, does not read Opus or FLAC, does not subtract a pre-skip and takes
//! the first logical stream's rate for every page (a chained file is read as its first stream plus the granule of its last page).
//! A file cut short is read up to its last whole page.

/// The sample rate and the last known granule position (the number of samples) of an Ogg Vorbis file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Length {
    pub sample_rate: u32,
    pub samples: u64,
}

impl Length {
    /// The length in seconds.
    pub fn seconds(self) -> f64 {
        self.samples as f64 / f64::from(self.sample_rate)
    }
}

/// Reads the sample rate and the sample count. Errors say what is wrong (not an Ogg file, not Vorbis, no page with a position).
pub fn length(bytes: &[u8]) -> Result<Length, String> {
    let mut at = 0;
    let mut rate = None;
    let mut last = None;
    while let Some(page) = Page::read(bytes, at) {
        if rate.is_none() {
            if at != 0 || page.header_type & 0x02 == 0 {
                return Err("not an Ogg file (no first page)".into());
            }
            rate = Some(vorbis_rate(page.body)?);
        }
        // `-1` (all ones) says no packet ends on the page.
        if page.granule != u64::MAX {
            last = Some(page.granule);
        }
        at = page.end;
    }
    let sample_rate = rate.ok_or("not an Ogg file")?;
    let samples = last.ok_or("no page of the file has a position")?;
    Ok(Length { sample_rate, samples })
}

/// The duration in seconds ([`length`]).
pub fn duration(bytes: &[u8]) -> Result<f64, String> {
    length(bytes).map(Length::seconds)
}

fn vorbis_rate(first_body: &[u8]) -> Result<u32, String> {
    // packet type 1, "vorbis", version u32, channels u8, rate u32
    if first_body.len() < 16 || first_body[0] != 1 || &first_body[1..7] != b"vorbis" {
        return Err("not Ogg Vorbis (the first packet is not a Vorbis identification header)".into());
    }
    let rate = u32::from_le_bytes([first_body[12], first_body[13], first_body[14], first_body[15]]);
    if rate == 0 { Err("the Vorbis header says a sample rate of 0".into()) } else { Ok(rate) }
}

struct Page<'a> {
    header_type: u8,
    granule: u64,
    body: &'a [u8],
    end: usize,
}

impl<'a> Page<'a> {
    /// The whole page at `at`, `None` at the end of the data, at something that is no page or at a page cut short.
    fn read(bytes: &'a [u8], at: usize) -> Option<Page<'a>> {
        let head = bytes.get(at..at.checked_add(27)?)?;
        if &head[..4] != b"OggS" || head[4] != 0 {
            return None;
        }
        let segments = usize::from(head[26]);
        let table = bytes.get(at + 27..at + 27 + segments)?;
        let len: usize = table.iter().map(|s| usize::from(*s)).sum();
        let start = at + 27 + segments;
        let body = bytes.get(start..start.checked_add(len)?)?;
        let granule = u64::from_le_bytes(head[6..14].try_into().ok()?);
        Some(Page { header_type: head[5], granule, body, end: start + len })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One Ogg page (CRC left 0: the reader does not check it).
    fn page(header_type: u8, granule: u64, seq: u32, body: &[u8]) -> Vec<u8> {
        assert!(body.len() < 255 * 255);
        let mut out = b"OggS".to_vec();
        out.push(0);
        out.push(header_type);
        out.extend(granule.to_le_bytes());
        out.extend(7u32.to_le_bytes());
        out.extend(seq.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        let mut segs = vec![255u8; body.len() / 255];
        segs.push((body.len() % 255) as u8);
        out.push(segs.len() as u8);
        out.extend(segs);
        out.extend(body);
        out
    }

    fn id_packet(rate: u32) -> Vec<u8> {
        let mut p = vec![1];
        p.extend(b"vorbis");
        p.extend(0u32.to_le_bytes());
        p.push(2);
        p.extend(rate.to_le_bytes());
        p.extend([0; 12]);
        p
    }

    /// A file of `granules` as the positions of its audio pages.
    fn file(rate: u32, granules: &[u64]) -> Vec<u8> {
        let mut f = page(0x02, 0, 0, &id_packet(rate));
        for (i, g) in granules.iter().enumerate() {
            f.extend(page(0, *g, i as u32 + 1, &[0; 300]));
        }
        f
    }

    #[test]
    fn the_last_page_over_the_rate_is_the_length() {
        let f = file(44100, &[4096, 88200, 132_300]);
        assert_eq!(length(&f), Ok(Length { sample_rate: 44100, samples: 132_300 }));
        assert!((duration(&f).unwrap() - 3.0).abs() < 1e-9);
        assert!((duration(&file(22050, &[11025])).unwrap() - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_page_without_a_position_is_skipped_and_a_cut_file_is_read_to_its_last_whole_page() {
        let mut f = file(48000, &[48000, u64::MAX]);
        assert_eq!(length(&f).unwrap().samples, 48000);
        f.extend(page(0, 96000, 9, &[0; 100]));
        f.truncate(f.len() - 10);
        assert_eq!(length(&f).unwrap().samples, 48000);
    }

    #[test]
    fn what_is_not_ogg_vorbis_is_refused_with_a_reason() {
        assert!(length(b"").unwrap_err().contains("not an Ogg"));
        assert!(length(b"RIFF....WAVEfmt ").unwrap_err().contains("not an Ogg"));
        assert!(length(&page(0x02, 0, 0, b"OpusHead........")).unwrap_err().contains("not Ogg Vorbis"));
        assert!(length(&page(0x02, 0, 0, &id_packet(0))).unwrap_err().contains("rate of 0"));
        // headers only: no page has a position
        assert!(length(&page(0x02, u64::MAX, 0, &id_packet(44100))).unwrap_err().contains("no page"));
        // the first page must begin the stream
        assert!(length(&page(0x00, 10, 0, &id_packet(44100))).is_err());
    }

    #[test]
    fn a_long_packet_spans_several_segments() {
        let mut f = page(0x02, 0, 0, &id_packet(8000));
        f.extend(page(0, 16000, 1, &[0; 1000]));
        assert!((duration(&f).unwrap() - 2.0).abs() < 1e-9);
    }
}
