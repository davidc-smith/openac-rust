//! Positional reads: fill a buffer from a file offset without touching any
//! shared cursor, so one open file can serve many threads through `&File`.

use std::fs::File;
use std::io;

/// Fill `buf` from `file` starting at byte `offset`.
#[cfg(unix)]
pub(crate) fn read_exact_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    use std::os::unix::fs::FileExt;
    file.read_exact_at(buf, offset)
}

/// Fill `buf` from `file` starting at byte `offset`.
///
/// Windows positional reads may return fewer bytes than asked; loop until
/// the buffer is full. They also move the handle's cursor, which nothing
/// here relies on.
#[cfg(windows)]
pub(crate) fn read_exact_at(file: &File, mut buf: &mut [u8], mut offset: u64) -> io::Result<()> {
    use std::os::windows::fs::FileExt;
    while !buf.is_empty() {
        match file.seek_read(buf, offset) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(read) => {
                buf = &mut buf[read..];
                offset += read as u64;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
