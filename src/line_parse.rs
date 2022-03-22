use io::BufRead;
use std::io;

/// Line endings for lines of output to standard out or standard error
pub enum LineEnding {
    /// Linefeed line ending (`\n` or `0x0a`).
    /// This is the standard Linux ending
    Lf,
    /// Carriage return (`\r` or `0x0d`).
    /// This is used for some MacOs programs
    Cr,
    /// Carriage Return + Line feed (`\r\n` or `\0x0a\x0d`).
    /// This is the standard windows/internet ending
    Crlf,
}

impl LineEnding {
    /// Returns true if and only if `self` is a line feed
    pub fn is_line_feed(&self) -> bool {
        matches!(self, LineEnding::Lf)
    }

    /// Returns true if and only if `self` is a carriage return
    pub fn is_carriage_return(&self) -> bool {
        matches!(self, LineEnding::Cr)
    }

    /// Returns true if and only if `self` is a carriage return followed by a line feed
    pub fn is_carriage_return_line_feed(&self) -> bool {
        matches!(self, LineEnding::Crlf)
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
                }

                return Ok(Some((LineEnding::Lf, read_bytes)));
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
