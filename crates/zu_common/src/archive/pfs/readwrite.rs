use crate::archive::{
    archive_error::ArchiveError,
    archive_trait::{IArchive, IReadableArchive, IWritableArchive},
    pfs::common::parse_filenames,
    pfs::constants::MAX_BLOCK_SIZE,
    pfs::constants::PFS_CRC_ALGO,
    pfs::{common::write_filenames, constants::FILENAMES_CRC_VALUE},
};
use bytes::{BufMut, BytesMut};
use crc::Crc;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use nom::{
    bytes::complete::{tag, take},
    multi::count,
    number::complete::le_u32,
    sequence::tuple,
    Err::Error,
    IResult,
};
use regex::Regex;
use std::{
    collections::HashMap,
    io::{Read, Write},
};

/// A readable + writable PFS archive
/// Less efficient than a strictly read or write archive because
/// it has to cache more things to be able to reconstruct the archive.
pub struct ReadWriteArchive {
    files: HashMap<String, ReadWriteArchiveFile>,
}

struct ReadWriteArchiveFile {
    blocks: Vec<ReadWriteArchiveFileBlock>,
}

#[derive(Clone)]
struct ReadWriteArchiveFileBlock {
    deflate_length: usize,
    inflate_length: usize,
    data: Vec<u8>,
}

impl ReadWriteArchive {
    fn do_parse(
        input: &[u8],
    ) -> IResult<&[u8], HashMap<String, ReadWriteArchiveFile>, ArchiveError> {
        let mut ret: HashMap<String, ReadWriteArchiveFile> = HashMap::new();
        let mut parsed_files: HashMap<u32, ReadWriteArchiveFile> = HashMap::new();

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
            let (_, blocks) = ReadWriteArchive::parse_pfs_file_blocks(
                &input[(*offset as usize)..],
                *size as usize,
            )?;

            parsed_files.insert(*crc, ReadWriteArchiveFile { blocks });
        }

        let mut filenames: Vec<String> = Vec::new();
        for (crc, f) in &parsed_files {
            if *crc == FILENAMES_CRC_VALUE {
                match f.inflate() {
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
        size: usize,
    ) -> IResult<&[u8], Vec<ReadWriteArchiveFileBlock>, ArchiveError> {
        let mut ret = Vec::new();
        let mut position: usize = 0;
        let mut inflate: usize = 0;

        while inflate < size {
            let current = &input[position..];
            let (_, block) = ReadWriteArchive::parse_pfs_file_block(current)?;

            inflate += block.inflate_length;
            position += block.deflate_length;
            position += 8;

            ret.push(block);
        }

        Ok((input, ret))
    }

    fn parse_pfs_file_block(
        input: &[u8],
    ) -> IResult<&[u8], ReadWriteArchiveFileBlock, ArchiveError> {
        let (input, deflate_length) = le_u32(input)?;
        let (input, inflate_length) = le_u32(input)?;
        let (input, data) = take(deflate_length as usize)(input)?;

        Ok((
            input,
            ReadWriteArchiveFileBlock {
                deflate_length: deflate_length as usize,
                inflate_length: inflate_length as usize,
                data: data.to_vec(),
            },
        ))
    }
}

impl ReadWriteArchiveFile {
    fn deflate<T>(input: T) -> Result<ReadWriteArchiveFile, ArchiveError>
    where
        T: AsRef<[u8]>,
    {
        let input_ref = input.as_ref();
        let mut pos = 0usize;
        let mut remain = input_ref.len();
        let mut blocks: Vec<ReadWriteArchiveFileBlock> = Vec::new();

        while remain > 0 {
            let sz: usize;
            if remain > MAX_BLOCK_SIZE {
                sz = MAX_BLOCK_SIZE;
                remain -= MAX_BLOCK_SIZE;
            } else {
                sz = remain;
                remain = 0;
            }

            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&input_ref[pos..pos + sz])?;
            let compressed = encoder.finish()?;

            let block = ReadWriteArchiveFileBlock {
                deflate_length: compressed.len(),
                inflate_length: sz,
                data: compressed,
            };

            pos += sz;
            blocks.push(block);
        }

        Ok(ReadWriteArchiveFile { blocks })
    }

    fn inflate(&self) -> Result<Vec<u8>, ArchiveError> {
        let mut ret: Vec<u8> = Vec::with_capacity(self.len());

        for block in self.blocks.iter() {
            let mut temp_buffer: Vec<u8> = vec![0; block.inflate_length + 1];
            let mut decoder = ZlibDecoder::new(&block.data[..]);
            let sz = decoder.read(&mut temp_buffer)?;

            ret.extend_from_slice(&temp_buffer[0..sz]);
        }

        Ok(ret)
    }

    fn len(&self) -> usize {
        self.blocks.iter().fold(0, |acc, b| acc + b.inflate_length)
    }
}

impl IArchive for ReadWriteArchive {
    fn new() -> Self {
        ReadWriteArchive {
            files: HashMap::new(),
        }
    }

    fn close(&mut self) {
        self.files.clear();
    }
}

impl IReadableArchive for ReadWriteArchive {
    fn open_from_bytes<T>(&mut self, input: T) -> Result<(), ArchiveError>
    where
        T: AsRef<[u8]>,
    {
        let input_ref = input.as_ref();
        self.close();
        match ReadWriteArchive::do_parse(input_ref) {
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
                let res = ent.inflate()?;
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

impl IWritableArchive for ReadWriteArchive {
    fn save_to_bytes(&self) -> Result<Vec<u8>, ArchiveError> {
        let mut data = BytesMut::with_capacity(1024);
        let mut directory = BytesMut::with_capacity(1024);
        directory.put_u32_le(self.files.len() as u32 + 1);

        let crc_provider = Crc::<u32>::new(&PFS_CRC_ALGO);
        let mut filenames = Vec::new();
        for (filename, file) in &self.files {
            let offset = data.len() + 12;
            let mut digest = crc_provider.digest();
            digest.update(filename.to_lowercase().as_bytes());
            digest.update(b"\0");

            let crc = digest.finalize();

            for block in &file.blocks {
                data.put_u32_le(block.deflate_length as u32);
                data.put_u32_le(block.inflate_length as u32);
                data.put(&block.data[..]);
            }

            directory.put_u32_le(crc);
            directory.put_u32_le(offset as u32);
            directory.put_u32_le(file.len() as u32);
            filenames.push(filename.clone());
        }

        let offset = data.len() + 12;
        let filenames_data = write_filenames(&filenames);
        let filenames_file = ReadWriteArchiveFile::deflate(filenames_data)?;

        for block in &filenames_file.blocks {
            data.put_u32_le(block.deflate_length as u32);
            data.put_u32_le(block.inflate_length as u32);
            data.put(&block.data[..]);
        }

        directory.put_u32_le(FILENAMES_CRC_VALUE);
        directory.put_u32_le(offset as u32);
        directory.put_u32_le(filenames_file.len() as u32);

        let data = data.freeze();
        let directory = directory.freeze();

        let mut final_data = BytesMut::with_capacity(12 + data.len() + directory.len());
        final_data.put_u32_le(data.len() as u32 + 12);
        final_data.put_u8(b'P');
        final_data.put_u8(b'F');
        final_data.put_u8(b'S');
        final_data.put_u8(b' ');
        final_data.put_u32_le(131072);
        final_data.put(data);
        final_data.put(directory);

        let final_data = final_data.freeze();
        Ok(final_data.to_vec())
    }

    fn save_to_file(&self, filename: &str) -> Result<(), ArchiveError> {
        let data = self.save_to_bytes()?;
        std::fs::write(filename, data)?;
        Ok(())
    }

    fn set<T>(&mut self, in_archive_path: &str, input: T) -> Result<(), ArchiveError>
    where
        T: AsRef<[u8]>,
    {
        let in_archive_path_lower = in_archive_path.to_lowercase();
        let file = ReadWriteArchiveFile::deflate(input)?;
        self.files.insert(in_archive_path_lower, file);
        Ok(())
    }

    fn remove(&mut self, in_archive_path: &str) -> Result<(), ArchiveError> {
        let in_archive_path_lower = in_archive_path.to_lowercase();
        match self.files.remove(&in_archive_path_lower) {
            Some(_) => Ok(()),
            None => Err(ArchiveError::SrcFileNotFound),
        }
    }

    fn rename(
        &mut self,
        in_archive_path: &str,
        new_in_archive_path: &str,
    ) -> Result<(), ArchiveError> {
        let in_archive_path_lower = in_archive_path.to_lowercase();
        let new_in_archive_path_lower = new_in_archive_path.to_lowercase();

        if self.files.contains_key(&new_in_archive_path_lower) {
            return Err(ArchiveError::DestFileAlreadyExists);
        }

        match self.files.remove(&in_archive_path_lower) {
            Some(f) => {
                self.files.insert(new_in_archive_path_lower, f);
                Ok(())
            }
            None => Err(ArchiveError::SrcFileNotFound),
        }
    }

    fn copy(
        &mut self,
        in_archive_path: &str,
        new_in_archive_path: &str,
    ) -> Result<(), ArchiveError> {
        let in_archive_path_lower = in_archive_path.to_lowercase();
        let new_in_archive_path_lower = new_in_archive_path.to_lowercase();

        if self.files.contains_key(&new_in_archive_path_lower) {
            return Err(ArchiveError::DestFileAlreadyExists);
        }

        let existing = self.files.get(&in_archive_path_lower);
        let new_file: ReadWriteArchiveFile;

        if let Some(f) = existing {
            new_file = ReadWriteArchiveFile {
                blocks: f.blocks.to_vec(),
            }
        } else {
            return Err(ArchiveError::SrcFileNotFound);
        }

        self.files.insert(new_in_archive_path_lower, new_file);
        Ok(())
    }
}
