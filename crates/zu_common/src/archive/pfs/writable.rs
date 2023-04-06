use crate::archive::{
    archive_error::ArchiveError,
    archive_trait::{IArchive, IWritableArchive},
    pfs::constants::{FILENAMES_CRC_VALUE, PFS_CRC_ALGO},
    pfs::{common::write_filenames, constants::MAX_BLOCK_SIZE},
};
use bytes::{BufMut, Bytes, BytesMut};
use crc::Crc;
use flate2::{write::ZlibEncoder, Compression};
use std::{collections::HashMap, io::Write};

/// A writable PFS archive
/// Simplier than the read+write variant
/// as it's not concerned with being able to
/// read data back just being able to write it
/// at save time.
pub struct WritableArchive {
    files: HashMap<String, WritableArchiveFile>,
}

struct WritableArchiveFile {
    data: Vec<u8>,
}

impl WritableArchiveFile {
    fn deflate(&self) -> Result<Bytes, ArchiveError> {
        let mut buffer = BytesMut::with_capacity(1024);
        let mut remain = self.data.len();
        let mut pos = 0usize;

        while remain > 0 {
            let sz;
            if remain > MAX_BLOCK_SIZE {
                sz = MAX_BLOCK_SIZE;
                remain -= MAX_BLOCK_SIZE;
            } else {
                sz = remain;
                remain = 0;
            }

            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&self.data[pos..pos + sz])?;
            let compressed = encoder.finish()?;

            buffer.put_u32_le(compressed.len() as u32);
            buffer.put_u32_le(sz as u32);
            buffer.put(&compressed[..]);
            pos += sz;
        }

        Ok(buffer.freeze())
    }
}

impl IArchive for WritableArchive {
    fn new() -> Self {
        WritableArchive {
            files: HashMap::new(),
        }
    }

    fn close(&mut self) {
        self.files.clear();
    }
}

impl IWritableArchive for WritableArchive {
    fn save_to_bytes(&self) -> Result<Vec<u8>, ArchiveError> {
        let mut data = BytesMut::with_capacity(1024);
        let mut directory = BytesMut::with_capacity(1024);
        directory.put_u32_le(self.files.len() as u32 + 1);

        let crc_provider = Crc::<u32>::new(&PFS_CRC_ALGO);
        let mut filenames = Vec::new();
        for (filename, file) in &self.files {
            let blocks = file.deflate()?;
            let offset = data.len() + 12;
            let mut digest = crc_provider.digest();
            digest.update(filename.to_lowercase().as_bytes());
            digest.update(b"\0");

            let crc = digest.finalize();

            data.put(blocks);
            directory.put_u32_le(crc);
            directory.put_u32_le(offset as u32);
            directory.put_u32_le(file.data.len() as u32);
            filenames.push(filename.clone());
        }

        //do filename file
        let filenames_data = write_filenames(&filenames);
        let filenames_file = WritableArchiveFile {
            data: filenames_data.to_vec(),
        };

        let blocks = filenames_file.deflate()?;
        let offset = data.len() + 12;
        data.put(blocks);
        directory.put_u32_le(FILENAMES_CRC_VALUE);
        directory.put_u32_le(offset as u32);
        directory.put_u32_le(filenames_file.data.len() as u32);

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
        if self.files.contains_key(&in_archive_path_lower) {
            return Err(ArchiveError::DestFileAlreadyExists);
        }

        let input_ref = input.as_ref();
        let new_file = WritableArchiveFile {
            data: input_ref.to_vec(),
        };

        self.files.insert(in_archive_path_lower, new_file);
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
        let new_file: WritableArchiveFile;

        if let Some(f) = existing {
            new_file = WritableArchiveFile {
                data: f.data.to_vec(),
            }
        } else {
            return Err(ArchiveError::SrcFileNotFound);
        }

        self.files.insert(new_in_archive_path_lower, new_file);
        Ok(())
    }
}
