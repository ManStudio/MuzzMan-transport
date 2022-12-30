use std::time::SystemTime;

use crate::packets::Packet;

pub struct PakStorage {
    pub packets: Vec<(Packet, SystemTime)>,
    pub counter: u16,
}

impl Default for PakStorage {
    fn default() -> Self {
        Self {
            packets: Vec::new(),
            counter: 2121,
        }
    }
}
