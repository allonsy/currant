use io::BufRead;
use std::io;

pub enum LineEnding {
    Lf,
    Cr,
    Crlf,
}

impl LineEnding {
    pub fn is_carriage_return(&self) -> bool {
        matches!(self, LineEnding::Cr)
    }
}

pub fn get_line<R>(reader: &mut R) -> io::Result<Option<(LineEnding, Vec<u8>)>>
where
    R: BufRead,
{
    let mut read_bytes = Vec::new();
    let mut buf = reader.fill_buf()?;
    let mut num_consumed = 0;
    let mut seen_cr = false;
    loop {
        if buf.is_empty() {
            if !read_bytes.is_empty() {
                return Ok(Some((LineEnding::Lf, read_bytes)));
            }
            return Ok(None);
        }
        for byte in buf {
            num_consumed += 1;
            if *byte == b'\r' {
                seen_cr = true;
            } else if *byte == b'\n' {
                reader.consume(num_consumed);
                if seen_cr {
                    return Ok(Some((LineEnding::Crlf, read_bytes)));
                } else {
                    return Ok(Some((LineEnding::Lf, read_bytes)));
                }
            } else if seen_cr {
                reader.consume(num_consumed - 1);
                return Ok(Some((LineEnding::Cr, read_bytes)));
            } else {
                seen_cr = false;
                read_bytes.push(*byte);
            }
        }

        reader.consume(num_consumed);
        num_consumed = 0;
        buf = reader.fill_buf()?;
    }
}
