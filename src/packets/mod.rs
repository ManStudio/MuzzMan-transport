mod auth;
mod file_content;
mod headers;

pub use auth::*;
use bytes_kman::prelude::*;
pub use file_content::FileContent;
pub use headers::Headers;

#[derive(Bytes, Debug, PartialEq, Clone)]
pub struct Packet {
    pub id: u16,
    pub packets: Vec<u16>,
    pub packet: Packets,
}

#[derive(Bytes, Debug, PartialEq, Clone)]
pub enum Packets {
    Auth(Auth),
    AuthResponse(AuthResponse),
    Headers(Headers),
    FileContent(FileContent),
    Finished(u128),
    Tick(u128),
}

#[cfg(test)]
mod test {
    use bytes_kman::TBytes;

    use super::{Auth, Packet, Packets};

    #[test]
    fn packet() {
        let pak = Packet {
            id: 21,
            packets: vec![1; 32],
            packet: Packets::Auth(Auth {
                name: "konkito".to_string(),
                path: "./data.txt".to_string(),
                secret: String::new(),
            }),
        };

        let mut bytes = pak.to_bytes();
        bytes.reverse();

        let other = Packet::from_bytes(&mut bytes).unwrap();

        assert_eq!(pak, other)
    }
}
