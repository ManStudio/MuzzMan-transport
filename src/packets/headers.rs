use bytes_kman::prelude::*;
use std::collections::HashMap;

#[derive(Bytes, Debug, PartialEq, Clone)]
pub struct Headers {
    pub session: u128,
    pub content_length: u128,
    pub others: HashMap<String, String>,
}
