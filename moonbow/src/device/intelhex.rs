//! Loading Intel HEX files

use ihex;

#[derive(Debug)]
pub struct Segment {
    pub address: usize,
    pub data: Vec<u8>,
}

pub fn segments(hexdata: &[u8]) -> Result<Vec<Segment>, String> {
    let hexstr =
        core::str::from_utf8(hexdata).or_else(|e| Err(format!("Invalid UTF-8 string ({e:?})")))?;

    let reader = ihex::Reader::new(&hexstr);

    let mut segments = Vec::<Segment>::new();

    let mut address_base = 0_usize;

    let mut segment_buf = Vec::<u8>::new();
    let mut segment_start = 0_usize;

    for rec in reader {
        let rec = rec.or_else(|e| Err(format!("Invalid record: {e}")))?;
        match rec {
            ihex::Record::Data { offset, mut value } => {
                let segment_addr = segment_start + segment_buf.len();

                let addr = address_base + offset as usize;

                if addr != segment_addr {
                    if segment_buf.len() > 0 {
                        segments.push(Segment {
                            address: segment_start,
                            data: segment_buf,
                        });
                    }

                    segment_buf = Vec::<u8>::new();
                    segment_start = addr;
                }
                segment_buf.append(&mut value);
            }
            ihex::Record::EndOfFile => {
                if segment_buf.len() > 0 {
                    segments.push(Segment {
                        address: segment_start,
                        data: segment_buf,
                    });
                }
                return Ok(segments);
            }
            ihex::Record::ExtendedSegmentAddress(esa) => {
                address_base = (esa as usize) << 4;
            }
            ihex::Record::ExtendedLinearAddress(ela) => {
                address_base = (ela as usize) << 16;
            }
            _ => (),
        }
    }
    Err(String::from("Unexpected end of file"))
}
