use bytes_kman::prelude::*;
use std::collections::HashMap;

use super::Packets;

#[derive(Bytes, Debug, PartialEq, Clone)]
pub struct Headers {
    pub session: u128,
    pub content_length: u128,
    pub others: HashMap<String, String>,
}

impl Into<Packets> for Headers {
    fn into(self) -> Packets {
        Packets::Headers(self)
    }
}
