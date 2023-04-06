use crate::archive::archive_error::ArchiveError;
use bytes::{BufMut, Bytes, BytesMut};
use nom::Err::Error;
use nom::{bytes::complete::take, number::complete::le_u32, IResult};

pub fn parse_filenames(input: &[u8]) -> Result<Vec<String>, ArchiveError> {
    match _parse_filenames(input) {
        Ok((_, filenames)) => Ok(filenames),
        Err(e) => {
            if let Error(ae) = e {
                Err(ae)
            } else {
                Err(ArchiveError::Unknown)
            }
        }
    }
}

fn _parse_filenames(input: &[u8]) -> IResult<&[u8], Vec<String>, ArchiveError> {
    let mut ret = Vec::new();
    let (mut current, count) = le_u32(input)?;

    for _ in 0..count {
        let (pos, len) = le_u32(current)?;
        let (pos, str) = take(len as usize)(pos)?;

        match std::str::from_utf8(&str[..(len as usize - 1)]) {
            Ok(utf_str) => ret.push(utf_str.to_string()),
            Err(e) => return Err(Error(ArchiveError::Utf8(e))),
        }
        current = pos;
    }

    Ok((current, ret))
}

pub fn write_filenames(filenames: &[String]) -> Bytes {
    let mut buffer = BytesMut::with_capacity(1024);
    buffer.put_u32_le(filenames.len() as u32);

    for filename in filenames {
        let filename_bytes = filename.as_bytes();
        buffer.put_u32_le(filename_bytes.len() as u32 + 1);
        buffer.put(filename_bytes);
        buffer.put_u8(0);
    }

    buffer.freeze()
}
