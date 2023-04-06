use crc::Algorithm;

pub const FILENAMES_CRC_VALUE: u32 = 0x61580ac9;
pub const MAX_BLOCK_SIZE: usize = 8192;
pub const PFS_CRC_ALGO: Algorithm<u32> = Algorithm {
    poly: 0x04c11db7,
    init: 0x00000000,
    refin: false,
    refout: false,
    xorout: 0,
    check: 0,
    residue: 0,
    width: 32,
};

#[cfg(test)]
mod tests {
    use crate::archive::pfs::constants::PFS_CRC_ALGO;
    use crc::Crc;

    #[test]
    fn file_crc_test() {
        let crc_provider = Crc::<u32>::new(&PFS_CRC_ALGO);
        let mut digest = crc_provider.digest();
        digest.update("innch0003.bmp".to_string().as_bytes());
        digest.update(b"\0");

        let mut digest2 = crc_provider.digest();
        digest2.update("innhe0004.bmp".to_string().as_bytes());
        digest2.update(b"\0");

        let mut digest3 = crc_provider.digest();
        digest3.update("beahe0204.bmp".to_string().as_bytes());
        digest3.update(b"\0");

        let crc = digest.finalize();
        let crc2 = digest2.finalize();
        let crc3 = digest3.finalize();

        assert_eq!(crc, 0xD32DA54A);
        assert_eq!(crc2, 0xD33312A3);
        assert_eq!(crc3, 0xD46B03A5);
    }
}
