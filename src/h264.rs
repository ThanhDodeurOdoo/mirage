use crate::Error;

#[derive(Clone, Copy, Debug)]
pub struct NalUnit<'a> {
    bytes: &'a [u8],
}

impl<'a> NalUnit<'a> {
    pub fn bytes(self) -> &'a [u8] {
        self.bytes
    }

    pub fn nal_type(self) -> u8 {
        self.bytes[0] & 31
    }
}

pub struct NalUnits<'a> {
    remaining: Option<&'a [u8]>,
}

/// Borrow NALs from one Annex B picture, stripping prefixes and zero padding.
///
/// # Errors
///
/// Stops on [`Error::InvalidAnnexB`] for empty input or missing framing,
/// [`Error::EmptyNalUnit`] for an empty NAL or [`Error::InvalidNalHeader`] for a forbidden bit.
pub fn nal_units(bytes: &[u8]) -> NalUnits<'_> {
    NalUnits {
        remaining: Some(bytes),
    }
}

impl<'a> Iterator for NalUnits<'a> {
    type Item = Result<NalUnit<'a>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let remaining = self.remaining.take()?;
        let start = match start_code(remaining) {
            Some(start) if remaining[..start].iter().all(|&byte| byte == 0) => start + 3,
            _ => return Some(Err(Error::InvalidAnnexB)),
        };
        let payload = &remaining[start..];
        let end = start_code(payload).unwrap_or(payload.len());
        let mut bytes = &payload[..end];
        while bytes.last() == Some(&0) {
            bytes = &bytes[..bytes.len() - 1];
        }
        if bytes.is_empty() {
            return Some(Err(Error::EmptyNalUnit));
        }
        if bytes[0] & 128 != 0 {
            return Some(Err(Error::InvalidNalHeader));
        }
        if end < payload.len() {
            self.remaining = Some(&payload[end..]);
        }
        Some(Ok(NalUnit { bytes }))
    }
}

fn start_code(bytes: &[u8]) -> Option<usize> {
    bytes.windows(3).position(|window| window == [0, 0, 1])
}

#[derive(Clone, Copy, Debug)]
pub struct Sps<'a> {
    pub nal: NalUnit<'a>,
    pub profile_idc: u8,
    pub constraints: u8,
    pub level_idc: u8,
}

#[derive(Debug)]
pub struct PictureHeaders<'a> {
    pub sps: Option<Sps<'a>>,
    pub pps: Option<NalUnit<'a>>,
    pub has_idr: bool,
}

/// Header inspection does not validate decodability.
///
/// # Errors
///
/// Returns [`nal_units`] errors or [`Error::TruncatedSps`] for missing SPS fields.
pub fn inspect_picture(bytes: &[u8]) -> Result<PictureHeaders<'_>, Error> {
    let mut headers = PictureHeaders {
        sps: None,
        pps: None,
        has_idr: false,
    };
    for nal in nal_units(bytes) {
        let nal = nal?;
        match nal.nal_type() {
            7 => {
                let fields = nal.bytes().get(1..4).ok_or(Error::TruncatedSps)?;
                headers.sps.get_or_insert(Sps {
                    nal,
                    profile_idc: fields[0],
                    constraints: fields[1],
                    level_idc: fields[2],
                });
            }
            8 => {
                headers.pps.get_or_insert(nal);
            }
            5 => headers.has_idr = true,
            _ => {}
        }
    }
    Ok(headers)
}

pub(crate) fn confirm_refresh(bytes: &[u8]) -> Result<(), Error> {
    let headers = inspect_picture(bytes)?;
    if !headers.has_idr {
        return Err(Error::UnexpectedRefresh);
    }
    let mut sps = false;
    let mut pps = false;
    for nal in nal_units(bytes) {
        match nal?.nal_type() {
            7 => sps = true,
            8 => pps = sps,
            5 if pps => {}
            1..=5 => return Err(Error::UnexpectedRefresh),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrows_units_without_framing_or_padding() -> Result<(), Error> {
        let bytes = [
            0, 0, 0, 0, 1, 103, 66, 0, 0, 0, 1, 127, 0, 0, 3, 1, 0, 0, 1, 101, 128, 0,
        ];
        let units = nal_units(&bytes).collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            units.iter().map(|nal| nal.nal_type()).collect::<Vec<_>>(),
            [7, 31, 5]
        );
        for (nal, expected) in units
            .iter()
            .zip([&bytes[5..7], &bytes[11..16], &bytes[19..21]].iter())
        {
            assert_eq!(nal.bytes(), *expected);
            assert_eq!(nal.bytes().as_ptr(), expected.as_ptr());
        }
        Ok(())
    }

    #[test]
    fn malformed_input_ends_iteration() {
        for bytes in [&[][..], &[0, 0], &[2, 0, 0, 1, 101]] {
            let mut units = nal_units(bytes);
            assert!(matches!(units.next(), Some(Err(Error::InvalidAnnexB))));
            assert!(units.next().is_none());
        }
        for bytes in [&[0, 0, 1][..], &[0, 0, 1, 0, 0, 1, 101]] {
            let mut units = nal_units(bytes);
            assert!(matches!(units.next(), Some(Err(Error::EmptyNalUnit))));
            assert!(units.next().is_none());
        }
        assert!(matches!(
            nal_units(&[0, 0, 1, 255]).next(),
            Some(Err(Error::InvalidNalHeader))
        ));
    }

    #[test]
    fn inspects_only_the_available_headers() -> Result<(), Error> {
        let bytes = [
            0, 0, 1, 103, 66, 192, 31, 0, 0, 1, 104, 128, 0, 0, 1, 101, 128,
        ];
        let headers = inspect_picture(&bytes)?;
        let sps = headers.sps.unwrap();
        assert_eq!(
            (sps.profile_idc, sps.constraints, sps.level_idc),
            (66, 192, 31)
        );
        assert_eq!(sps.nal.bytes().as_ptr(), bytes[3..].as_ptr());
        assert_eq!(headers.pps.unwrap().bytes(), &[104, 128]);
        assert!(headers.has_idr);
        let ordinary_slice = inspect_picture(&[0, 0, 1, 65, 128])?;
        assert!(ordinary_slice.sps.is_none());
        assert!(ordinary_slice.pps.is_none());
        assert!(!ordinary_slice.has_idr);
        assert!(matches!(
            inspect_picture(&[0, 0, 1, 103, 66, 192]),
            Err(Error::TruncatedSps)
        ));
        Ok(())
    }
}
