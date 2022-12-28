use std::time::{Duration, SystemTime};

use bytes_kman::TBytes;
use socket2::{SockAddr, Socket};

use crate::{
    connection::Connection,
    packets::{Packet, Packets},
};

pub struct PakStorage {
    packets: Vec<(Packet, SystemTime)>,
    counter: u16,
}

impl PakStorage {
    pub fn send(&mut self, conn: &mut Socket, pak: Packet, to: &SockAddr) {
        let mut pak = pak;

        if self.counter == 0 {
            self.counter = 1;
        }
        pak.id = self.counter;
        self.counter = self.counter.wrapping_add(1);

        let mut b = pak.to_bytes();
        b.reverse();

        self.packets.push((pak, SystemTime::now()));

        let _ = conn.send_to(&b, to);
    }

    pub fn resolv(&mut self, conn: &mut Socket, connection: &mut Connection) -> usize {
        let mut not_recv = 0;
        self.packets.retain_mut(|pak| {
            let session = match &pak.0.packet {
                Packets::Auth(_) => None,
                Packets::AuthResponse(_) => None,
                Packets::Headers(headers) => Some(headers.session),
                Packets::FileContent(content) => Some(content.session),
                Packets::Finished(finished) => Some(*finished),
                Packets::Tick(tick) => Some(*tick),
            };

            let Some(session) = session else {return false};

            if session != connection.session {
                return true;
            }

            if connection.recv_packets.contains(&pak.0.id) {
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
}

impl Default for PakStorage {
    fn default() -> Self {
        Self {
            packets: Vec::new(),
            counter: 2121,
        }
    }
}
