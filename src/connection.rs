use std::time::SystemTime;

use socket2::{SockAddr, Socket};

pub struct Connection {
    pub name: String,
    pub conn: Socket,
    pub sock_addr: SockAddr,
    pub packets: Vec<u16>,
    pub recv_packets: Vec<u16>,
    pub pak_cour: u8,
    pub coursor: u128,
    pub session: u128,
    pub active: bool,
    pub last_action: SystemTime,
    pub content_length: u128,
}

impl Connection {
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
}
