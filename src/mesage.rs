use socket2::SockAddr;

pub enum Message {
    New(String, u128, SockAddr),
    SetProgress(u128, f32),
    SetStatus(u128, String),
    SetShare(String),
    Destroy(u128),
    Error(String),
}
