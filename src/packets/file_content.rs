use bytes_kman::prelude::*;

#[derive(Bytes, Debug, PartialEq, Clone)]
pub struct FileContent {
    pub session: u128,
    pub cursor: u128,
    pub bytes: Vec<u8>,
}

#[cfg(test)]
mod test {
    use bytes_kman::TBytes;

    use crate::packets::{Packet, Packets};

    use super::FileContent;

    #[test]
    fn file_content() {
        let file_content = FileContent {
            session: 1,
            cursor: 0,
            bytes: vec![1; 53],
        };

        let mut bytes = file_content.to_bytes();
        bytes.reverse();

        let other = FileContent::from_bytes(&mut bytes).unwrap();

        assert_eq!(file_content, other)
    }

    #[test]
    fn file_content_pak() {
        let pak = Packet {
            id: 21,
            packets: vec![0; 32],
            packet: Packets::FileContent(FileContent {
                session: 1,
                cursor: 0,
                bytes: vec![1; 53],
            }),
        };

        let mut bytes = pak.to_bytes();
        bytes.reverse();

        let other = Packet::from_bytes(&mut bytes).unwrap();

        assert_eq!(pak, other);
    }
}
