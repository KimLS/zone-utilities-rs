use super::{common::parse_filenames, constants::FILENAMES_CRC_VALUE, constants::PFS_CRC_ALGO};
use crate::archive::{
    archive_error::ArchiveError,
    archive_trait::{IArchive, IReadableArchive},
};
use crc::Crc;
use flate2::read::ZlibDecoder;
use nom::Err::Error;
use nom::{
    bytes::complete::{tag, take},
    multi::count,
    number::complete::le_u32,
    sequence::tuple,
    IResult,
};
use regex::Regex;
use std::{collections::HashMap, io::Read};

pub struct ReadableArchive {
    data: Vec<u8>,
    files: HashMap<String, ArchiveFile>,
}

struct ArchiveFile {
    size: usize,
    blocks: Vec<ArchiveFileBlock>,
}

struct ArchiveFileBlock {
    deflate_length: usize,
    inflate_length: usize,
    offset: usize,
}

/// A readable PFS archive
/// The most efficient of the three archive types but can only read data.
impl ReadableArchive {
    fn do_parse(input: &[u8]) -> IResult<&[u8], HashMap<String, ArchiveFile>, ArchiveError> {
        let mut ret: HashMap<String, ArchiveFile> = HashMap::new();
        let mut parsed_files: HashMap<u32, ArchiveFile> = HashMap::new();

        let (current, dir_offset) = le_u32(input)?;
        let (current, _) = tag("PFS ")(current)?;
        let (_, version) = le_u32(current)?;

        if version != 131072 {
            return Err(Error(ArchiveError::WrongVersion { version }));
        }

        let current = &input[dir_offset as usize..];
        let (current, dir_count) = le_u32(current)?;
        let (_, directory_entries) =
            count(tuple((le_u32, le_u32, le_u32)), dir_count as usize)(current)?;

        parsed_files.reserve(dir_count as usize);
        for entry in directory_entries.iter() {
            let (crc, offset, size) = entry;
            let (_, blocks) = ReadableArchive::parse_pfs_file_blocks(
                &input[(*offset as usize)..],
                *offset as usize,
                *size as usize,
            )?;

            parsed_files.insert(
                *crc,
                ArchiveFile {
                    size: *size as usize,
                    blocks,
                },
            );
        }

        let mut filenames: Vec<String> = Vec::new();
        for (crc, f) in &parsed_files {
            if *crc == FILENAMES_CRC_VALUE {
                match ReadableArchive::inflate_file_entry(input, f) {
                    Ok(data) => {
                        filenames = parse_filenames(&data[..]).unwrap_or_default();
                        break;
                    }
                    Err(e) => return Err(Error(e)),
                }
            }
        }

        let crc = Crc::<u32>::new(&PFS_CRC_ALGO);
        for filename in &filenames {
            let mut digest = crc.digest();
            digest.update(filename.as_bytes());
            digest.update(b"\0");
            let crc = digest.finalize();

            if let Some(f) = parsed_files.remove(&crc) {
                ret.insert(filename.clone(), f);
            }
        }

        Ok((input, ret))
    }

    fn parse_pfs_file_blocks(
        input: &[u8],
        offset: usize,
        size: usize,
    ) -> IResult<&[u8], Vec<ArchiveFileBlock>, ArchiveError> {
        let mut ret = Vec::new();
        let mut position: usize = 0;
        let mut inflate: usize = 0;

        while inflate < size {
            let current = &input[position..];
            let (_, block) = ReadableArchive::parse_pfs_file_block(current, offset + position)?;

            inflate += block.inflate_length;
            position += block.deflate_length;
            position += 8;

            ret.push(block);
        }

        Ok((input, ret))
    }

    fn parse_pfs_file_block(
        input: &[u8],
        offset: usize,
    ) -> IResult<&[u8], ArchiveFileBlock, ArchiveError> {
        let (input, deflate_length) = le_u32(input)?;
        let (input, inflate_length) = le_u32(input)?;
        let (input, _) = take(deflate_length as usize)(input)?;

        Ok((
            input,
            ArchiveFileBlock {
                deflate_length: deflate_length as usize,
                inflate_length: inflate_length as usize,
                offset: offset + 8,
            },
        ))
    }

    fn inflate_file_entry(data: &[u8], entry: &ArchiveFile) -> Result<Vec<u8>, ArchiveError> {
        let mut ret = Vec::with_capacity(entry.size);

        for block in entry.blocks.iter() {
            let mut temp_buffer = vec![0; block.inflate_length + 1];
            let mut decoder =
                ZlibDecoder::new(&data[block.offset..(block.offset + block.deflate_length)]);
            let sz = decoder.read(&mut temp_buffer)?;

            ret.extend_from_slice(&temp_buffer[0..sz]);
        }

        Ok(ret)
    }
}

impl IArchive for ReadableArchive {
    fn new() -> Self {
        ReadableArchive {
            data: Vec::new(),
            files: HashMap::new(),
        }
    }

    fn close(&mut self) {
        self.data.clear();
        self.files.clear();
    }
}

impl IReadableArchive for ReadableArchive {
    fn open_from_bytes<T>(&mut self, input: T) -> Result<(), ArchiveError>
    where
        T: AsRef<[u8]>,
    {
        let input_ref = input.as_ref();
        self.close();

        self.data.extend_from_slice(input_ref);
        match ReadableArchive::do_parse(&self.data[..]) {
            Ok((_, files)) => {
                self.files = files;
                Ok(())
            }
            Err(e) => {
                if let Error(ae) = e {
                    Err(ae)
                } else {
                    Err(ArchiveError::Unknown)
                }
            }
        }
    }

    fn open_file(&mut self, filename: &str) -> Result<(), ArchiveError> {
        let data = std::fs::read(filename)?;
        self.open_from_bytes(&data[..])
    }

    fn get(&self, in_archive_path: &str) -> Result<Vec<u8>, ArchiveError> {
        let in_archive_path_lower = in_archive_path.to_lowercase();
        match self.files.get(&in_archive_path_lower) {
            Some(ent) => {
                let res = ReadableArchive::inflate_file_entry(&self.data[..], ent)?;
                Ok(res)
            }
            None => Err(ArchiveError::SrcFileNotFound),
        }
    }

    fn exists(&self, in_archive_path: &str) -> Result<bool, ArchiveError> {
        let in_archive_path_lower = in_archive_path.to_lowercase();
        Ok(self.files.contains_key(&in_archive_path_lower))
    }

    fn search(&self, search_regex: &str) -> Result<Vec<String>, ArchiveError> {
        let regex = Regex::new(search_regex)?;
        let mut ret = Vec::new();

        for filename in self.files.keys() {
            if regex.is_match(filename) {
                ret.push(filename.clone());
            }
        }

        Ok(ret)
    }
}
