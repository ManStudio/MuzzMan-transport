use bytes_kman::prelude::*;

use super::Packets;

#[derive(Bytes, Debug, PartialEq, Clone)]
pub struct Auth {
    pub name: String,
    pub path: String,
    pub secret: String,
}

#[derive(Bytes, Debug, PartialEq, Clone)]
pub struct AuthResponse {
    pub accepted: bool,
    pub session: u128,
}

#[cfg(test)]
mod test {
    use crate::packets::{Packet, Packets};
    use bytes_kman::prelude::*;

    #[test]
    fn auth() {
        let auth = super::Auth {
            name: "konkito".to_string(),
            path: "./data.txt".to_string(),
            secret: String::new(),
        };

        let mut b = auth.to_bytes();
        b.reverse();

        let other = super::Auth::from_bytes(&mut b).unwrap();

        assert_eq!(auth, other)
    }

    #[test]
    fn auth_pak() {
        let pak = Packet {
            id: 21,
            packets: vec![1; 32],
            packet: Packets::Auth(super::Auth {
                name: "konkito".to_string(),
                path: "./data.txt".to_string(),
                secret: String::new(),
            }),
        };

        let mut b = pak.to_bytes();
        b.reverse();

        println!("Bytes: {:?}", b);

        let other = Packet::from_bytes(&mut b).unwrap();

        assert_eq!(pak, other)
    }
}

impl Into<Packets> for Auth {
    fn into(self) -> Packets {
        Packets::Auth(self)
    }
}

impl Into<Packets> for AuthResponse {
    fn into(self) -> Packets {
        Packets::AuthResponse(self)
    }
}
