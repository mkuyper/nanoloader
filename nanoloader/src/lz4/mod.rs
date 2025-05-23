pub trait Sink {
    fn literal(&mut self, data: &[u8]) -> Option<()>;
    fn backref(&mut self, offset: usize, length: usize) -> Option<()>;
}

fn extend_length<'a>(len: usize, it: &mut impl Iterator<Item = &'a u8>) -> Option<usize> {
    let mut length: usize = len;
    if length == 15 {
        loop {
            let len = it.next().map(|x| *x as usize)?;
            length = length.checked_add(len)?;
            if len != 255 {
                break;
            }
        }
    }
    Some(length)
}

pub fn decompress(source: &[u8], sink: &mut impl Sink) -> Option<()> {
    let mut it = source.iter();

    loop {
        let token = it.next().map(|x| *x as usize)?;

        let literal_len = token >> 4;
        let match_len = token & 0x0f;

        let literal_len = extend_length(literal_len, &mut it)?;

        let (literals, more) = it.as_slice().split_at_checked(literal_len)?;

        sink.literal(literals)?;

        it = more.iter();

        let Some(offset_lsb) = it.next().map(|x| *x as usize) else {
            // The last block only contains literals, so we're done here.
            return Some(());
        };

        let offset_msb = it.next().map(|x| *x as usize)?;

        let offset = (offset_msb << 8) | offset_lsb;

        let match_len = extend_length(match_len, &mut it)?.checked_add(4)?;

        sink.backref(offset, match_len)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BufferSink<'a, const SIZE: usize> {
        length: usize,
        buffer: [u8; SIZE],
        dict: &'a [u8],
    }

    impl<const SIZE: usize> BufferSink<'_, SIZE> {
        fn as_slice(&self) -> &[u8] {
            &self.buffer[..self.length]
        }

        fn get(&mut self, idx: isize) -> u8 {
            if idx < 0 {
                self.dict[(self.dict.len() as isize + idx) as usize]
            } else {
                self.buffer[idx as usize]
            }
        }
    }

    impl<const SIZE: usize> Sink for BufferSink<'_, SIZE> {
        fn literal(&mut self, data: &[u8]) -> Option<()> {
            self.buffer[self.length..self.length + data.len()].copy_from_slice(data);
            self.length += data.len();
            Some(())
        }

        fn backref(&mut self, offset: usize, length: usize) -> Option<()> {
            let offset = self.length as isize - offset as isize;

            for i in 0..length {
                self.buffer[self.length + i] = self.get(offset + i as isize);
            }
            self.length += length;
            Some(())
        }
    }

    fn do_test(data: &[u8], compressed: &[u8], dict: &[u8]) {
        let mut sink = BufferSink::<1024> {
            length: 0,
            buffer: [0; 1024],
            dict,
        };

        let result = decompress(compressed, &mut sink);

        assert!(result.is_some());
        assert_eq!(sink.as_slice(), data)
    }

    #[test]
    fn empty() {
        do_test(b"", b"\0", &[]);
    }

    #[test]
    fn lorem1() {
        do_test(
            include_bytes!("testdata/lorem1.dat"),
            include_bytes!("testdata/lorem1.lz4"),
            &[],
        );
    }

    #[test]
    fn lorem2() {
        do_test(
            include_bytes!("testdata/lorem2.dat"),
            include_bytes!("testdata/lorem2.lz4"),
            include_bytes!("testdata/lorem2.dct"),
        );
    }
}
