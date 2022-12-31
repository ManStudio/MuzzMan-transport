use std::time::{Duration, SystemTime};

use bytes_kman::TBytes;
use relay_man::client::response::Conn;
use socket2::SockAddr;

use crate::{
    packets::{Packet, Packets},
    pak_storage::PakStorage,
};

pub struct Connection {
    pub name: String,
    pub conn: Conn,
    pub sock_addr: SockAddr,
    pub packets: Vec<u16>,
    pub recv_packets: Vec<u16>,
    pub pak_cour: u8,
    pub coursor: u128,
    pub session: u128,
    pub active: bool,
    pub last_action: SystemTime,
    pub content_length: u128,
    pub storage: PakStorage,
}

// #[allow(unconditional_panic)]

impl Connection {
    pub fn new(name: impl Into<String>, conn: Conn, sock_addr: SockAddr, session: u128) -> Self {
        Self {
            name: name.into(),
            conn,
            sock_addr,
            packets: vec![0; 32],
            recv_packets: vec![0; 64],
            pak_cour: 0,
            coursor: 0,
            session,
            active: true,
            last_action: SystemTime::now(),
            content_length: 0,
            storage: PakStorage::default(),
        }
    }

    pub fn add_id(&mut self, id: u16) {
        self.packets[self.pak_cour as usize] = id;
        self.pak_cour = (self.pak_cour + 1) % 32;
    }

    pub fn add_packets(&mut self, packets: &[u16]) {
        {
            let mut has = Vec::with_capacity(64);
            for pak in self.recv_packets.iter_mut() {
                if has.contains(pak)
                /* || !self.packets.contains(pak) */
                {
                    *pak = 0;
                } else {
                    has.push(*pak);
                }
            }
        }

        let mut positions = Vec::with_capacity(64);
        for (i, has) in self.recv_packets.iter().enumerate() {
            if *has == 0 {
                positions.push(i)
            }
        }

        for i in 0..positions.capacity() - positions.len() {
            positions.push(i);
        }

        for pak in packets {
            if *pak != 0 {
                let pos = positions.pop().unwrap();
                self.recv_packets[pos] = *pak;
            }
        }
    }

    pub fn resolv(&mut self) -> usize {
        let mut not_recv = 0;
        self.storage.packets.retain_mut(|pak| {
            if self.recv_packets.contains(&pak.0.id) {
                return false;
            }

            if pak.1.elapsed().unwrap() < Duration::from_secs(1) {
                return true;
            }

            let mut bytes = pak.0.to_bytes();
            bytes.reverse();
            // let _ = conn.send_to(&bytes, &connection.conn);
            pak.1 = SystemTime::now();

            not_recv += 1;

            true
        });
        not_recv
    }

    pub fn send(&mut self, pak: Packets) {
        if self.storage.counter == 0 {
            self.storage.counter = 1;
        }

        let pak = Packet {
            id: self.storage.counter,
            packets: self.packets.clone(),
            packet: pak,
        };

        self.storage.counter = self.storage.counter.wrapping_add(1);

        let mut b = pak.to_bytes();
        b.reverse();

        self.storage.packets.push((pak, SystemTime::now()));

        let _ = self.conn.send(&b);
    }
}
