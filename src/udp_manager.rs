use std::{
    collections::HashMap,
    io::{Read, Seek, Write},
    mem::MaybeUninit,
    net::ToSocketAddrs,
    thread::{self, JoinHandle},
    time::{Duration, SystemTime},
};

use rand::{random, Rng};
use relay_man::{
    client::{ConnectionInfo, RelayClient},
    common::packets::Search,
};

use bytes_kman::prelude::*;
use muzzman_lib::prelude::*;

use crate::{
    connection::Connection,
    mesage::Message,
    packets::{Auth, AuthResponse, FileContent, Headers, Packet, Packets},
};

pub enum Should {
    Send,
    Recv,
    Sync,
}

pub struct UdpManager {
    connections: Vec<Connection>,
    relay: RelayClient,
    buffer: Vec<MaybeUninit<u8>>,
    path: String,
    secret: String,
    should: Should,
    buffer_size: usize,
    info: EInfo,
    pub messages: Vec<Message>,
    name: String,
    connecting: Option<JoinHandle<Result<Connection, ConnectingError>>>,
}

#[derive(Debug)]
pub enum ConnectingError {
    FailOnConnect,
    InvalidAuth,
    AuthFailed,
    DomainAdressCannotBeFound,
    InvalidPacket,
    InvalidFilePath,
}

impl UdpManager {
    pub fn new(
        buffer_size: usize,
        path: String,
        should: Should,
        secret: String,
        relays: Vec<String>,
        name: String,
        info: EInfo,
    ) -> Result<Self, String> {
        let relay = RelayClient::new(
            ConnectionInfo {
                client: "muzzman-transport".into(),
                name: name.clone(),
                public: vec![random(), random(), random(), random()],
                other: format!("File: {path}").to_bytes(),
                privacy: false,
            },
            relays,
        );
        let relay = match relay {
            Ok(relay) => relay,
            Err(err) => {
                return Err(format!("{:?}", err));
            }
        };

        let code = hex::encode(relay.info.public.clone());

        let mut buffer = Vec::with_capacity(buffer_size);
        buffer.resize(buffer_size, MaybeUninit::new(0));

        let messages = vec![Message::SetShare(format!(
            "mzt://{}/{}/{}",
            hex::encode(relay.info.public.clone()),
            secret.clone(),
            path.clone()
        ))];

        Ok(Self {
            connections: Vec::new(),
            buffer,
            // conn,
            buffer_size,
            secret,
            should,
            path,
            info,
            messages,
            name,
            relay,
            connecting: None,
        })
    }

    pub fn send_request(&mut self, url: String) -> Result<(), ()> {
        let segments = url.split('/').collect::<Vec<&str>>();
        let addr = segments[2];
        let adress = hex::decode(addr).unwrap();
        let secret = segments[3].to_string();
        let mut path = String::new();
        for (i, s) in segments[4..].iter().enumerate() {
            if i > 0 {
                path.push('/');
            }
            path.push_str(s);
        }

        self.relay
            .search(Search {
                client: relay_man::common::packets::SearchType::Exact("muzzman-transport".into()),
                ..Default::default()
            })
            .get();

        let where_is;
        if let Some(is) = self.relay.where_is_adress(&adress).iter().next() {
            where_is = *is
        } else {
            return Err(());
        };

        let res = self
            .relay
            .get(where_is)
            .unwrap()
            .request(&adress, String::new())
            .get();
        res.add_port(rand::thread_rng().gen_range(1025..u16::MAX));
        let req = res
            .accept(true, Some(Duration::from_secs(5).as_nanos()))
            .get();
        let Ok(mut addr) = req.to.to_socket_addrs() else{return Err(())};
        let Some(addr) = addr.next() else {return Err(())};

        let sock_addr = addr.into();
        let Ok(conn) =
                req.connect(Duration::from_secs(2), Duration::from_secs(1), false) else {return Err(())};
        println!("Connected");

        let pak = Packet {
            id: 0,
            packets: vec![0; 32],
            packet: Packets::Auth(Auth {
                name: self.name.clone(),
                path,
                secret,
            }),
        };

        println!("Pack: {:?}", pak);

        let mut b = pak.to_bytes();
        b.reverse();
        conn.send(&b).unwrap();

        self.connecting = Some(thread::spawn(move || {
            let mut buffer = [MaybeUninit::new(0); 1024];
            loop {
                if let Ok(len) = conn.recv(&mut buffer) {
                    let bytes = buffer[0..len].to_owned();
                    let mut bytes = unsafe { std::mem::transmute(bytes) };

                    if let Some(packet) = Packet::from_bytes(&mut bytes) {
                        println!("Packet: {:?}", packet);
                        if let Packets::AuthResponse(res) = packet.packet {
                            if res.accepted {
                                println!("Connection succesful");
                                let connection =
                                    Connection::new("Server", conn, sock_addr, res.session);
                                return Ok(connection);
                            } else {
                                println!("Connection Refuzed");
                                return Err(ConnectingError::AuthFailed);
                            }
                        }
                    } else {
                        return Err(ConnectingError::InvalidPacket);
                    }
                }
            }
        }));

        println!("Packet sent");
        Ok(())
    }

    pub fn step(&mut self) {
        self.relay.step();

        if let Some(conn) = self.connecting.take() {
            if conn.is_finished() {
                match conn.join().unwrap() {
                    Ok(mut conn) => {
                        println!("Connected");
                        conn.last_action = SystemTime::now();
                        self.messages.push(Message::New(
                            conn.name.clone(),
                            conn.session,
                            conn.sock_addr.clone(),
                        ));
                        self.connections.push(conn);
                    }
                    Err(err) => match err {
                        ConnectingError::InvalidFilePath => self
                            .messages
                            .push(Message::Error("Invalid file path!".into())),
                        ConnectingError::AuthFailed => self
                            .messages
                            .push(Message::Error("Invalid secret or path!".into())),
                        _ => {
                            println!("Connecting Error: {:?}", err);
                        }
                    },
                }
            } else {
                self.connecting = Some(conn)
            }
        }

        if self.connecting.is_none() {
            match self.should {
                Should::Send => {
                    if let Some((_, req)) = self.relay.has_new() {
                        match req {
                            relay_man::client::response::RequestStage::NewRequest(req) => {
                                if let Some(info) = req.connection.info(&req.from).get() {
                                    if info.client == "muzzman-transport".to_string() {
                                        req.accept(true);
                                    } else {
                                        req.accept(false);
                                    }
                                } else {
                                    req.accept(false);
                                }
                            }
                            relay_man::client::response::RequestStage::NewRequestFinal(req) => {
                                if req.accept {
                                    req.add_port(rand::thread_rng().gen_range(1025..u16::MAX));
                                }
                            }
                            relay_man::client::response::RequestStage::ConnectOn(req) => {
                                let path = self.path.clone();
                                let secret = self.secret.clone();
                                let info = self.info.clone();
                                self.connecting = Some(thread::spawn(move || {
                                    let Ok(mut addr) = req.to.to_socket_addrs() else{return Err(ConnectingError::DomainAdressCannotBeFound)};
                                    let Some(addr) = addr.next() else {return Err(ConnectingError::DomainAdressCannotBeFound)};

                                    let sock_addr = addr.into();

                                    let Ok(socket) = req.connect(
                                        Duration::from_secs(2),
                                        Duration::from_secs(1),
                                        true,
                                    ) else{
                                        println!("Cannot connect");
                                        return Err(ConnectingError::FailOnConnect)
                                    };

                                    let mut buffer = [MaybeUninit::new(0); 1024];

                                    loop {
                                        if let Ok(len) = socket.recv(&mut buffer) {
                                            let bytes = buffer[0..len].to_owned();
                                            let mut bytes = unsafe { std::mem::transmute(bytes) };

                                            if let Some(packet) = Packet::from_bytes(&mut bytes) {
                                                match packet.packet {
                                                    crate::packets::Packets::Auth(auth) => {
                                                        println!("Auth part: {}, path: {}, secret: {}, name: {}",auth.path, path, auth.secret, auth.name);
                                                        if auth.path != path
                                                            || auth.secret != secret
                                                        {
                                                            let mut pak = Packet {
                                                                id: 2,
                                                                packets: vec![0; 32],
                                                                packet: Packets::AuthResponse(
                                                                    AuthResponse {
                                                                        accepted: false,
                                                                        session: 0,
                                                                    },
                                                                ),
                                                            };
                                                            pak.packets[0] = packet.id;

                                                            let mut bytes = pak.to_bytes();
                                                            bytes.reverse();

                                                            let _ = socket.send(&bytes);
                                                            return Err(
                                                                ConnectingError::InvalidAuth,
                                                            );
                                                        }

                                                        let session = random();

                                                        let mut connection = Connection::new(
                                                            auth.name, socket, sock_addr, session,
                                                        );

                                                        connection.add_id(packet.id);
                                                        connection.add_packets(&packet.packets);

                                                        let pak = AuthResponse {
                                                            accepted: true,
                                                            session,
                                                        };

                                                        let len;
                                                        {
                                                            let mut ford = info.get_data().unwrap();
                                                            let current = match ford
                                                                .seek(std::io::SeekFrom::Current(0))
                                                            {
                                                                Ok(e) => e,
                                                                Err(_) => {
                                                                    let _ = connection.send(
                                                                        AuthResponse {
                                                                            accepted: false,
                                                                            session: 0,
                                                                        }
                                                                        .into(),
                                                                    );

                                                                    return Err(ConnectingError::InvalidFilePath);
                                                                }
                                                            };
                                                            len = ford
                                                                .seek(std::io::SeekFrom::End(0))
                                                                .unwrap();
                                                            let _ = ford.seek(
                                                                std::io::SeekFrom::Start(current),
                                                            );
                                                        }

                                                        connection.content_length = len as u128;

                                                        let _ = connection.send(pak.into());

                                                        let pak = Headers {
                                                            session,
                                                            content_length: len as u128,
                                                            others: HashMap::new(),
                                                        };

                                                        connection.content_length =
                                                            pak.content_length;

                                                        let _ = connection.send(pak.into());

                                                        return Ok(connection);
                                                    }
                                                    _ => {}
                                                }
                                            }
                                        }
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        for connection in self.connections.iter_mut() {
            if let Ok(size) = connection.conn.recv(&mut self.buffer) {
                let bytes = self.buffer[0..size].to_owned();
                let mut bytes = unsafe { std::mem::transmute(bytes) };

                if let Some(packet) = Packet::from_bytes(&mut bytes) {
                    match packet.packet {
                        crate::packets::Packets::Headers(headers) => match self.should {
                            Should::Sync | Should::Recv => {
                                println!("Recived headers: {}", headers.content_length);
                                // if let Some(conn) = self.get_conn(headers.session) {
                                connection.content_length = headers.content_length;
                                connection.add_id(packet.id);
                                connection.add_packets(&packet.packets);
                                connection.last_action = SystemTime::now();

                                connection.send(Packets::Tick(connection.session));

                                // }
                            }
                            _ => {}
                        },
                        crate::packets::Packets::FileContent(content) => match self.should {
                            Should::Recv | Should::Sync => {
                                let mut _do = false;
                                let mut coursor = 0;
                                let mut content_length = 0;
                                let mut packets = Vec::new();

                                // if let Some(conn) = self.get_conn(content.session) {
                                _do = true;
                                if connection.packets.contains(&packet.id) {
                                    _do = false;
                                }

                                if _do {
                                    connection.add_id(packet.id);
                                    connection.add_packets(&packet.packets);
                                    connection.last_action = SystemTime::now();
                                    connection.coursor = content.cursor;

                                    content_length = connection.content_length;
                                    coursor = connection.coursor;

                                    packets = connection.packets.clone();
                                }
                                // }

                                if _do {
                                    let mut ford = self.info.get_data().unwrap();
                                    let _ =
                                        ford.seek(std::io::SeekFrom::Start(content.cursor as u64));
                                    let _ = ford.write(&content.bytes).unwrap();

                                    connection.send(Packets::Tick(connection.session));

                                    let _ = self.info.set_progress(
                                        (coursor as f64 / content_length as f64) as f32,
                                    );
                                }
                            }
                            _ => {}
                        },
                        crate::packets::Packets::Finished(finished) => {
                            if connection.packets.contains(&packet.id) {
                                break;
                            }

                            connection.add_id(packet.id);
                            connection.add_packets(&packet.packets);
                            connection.active = false;

                            match self.should {
                                Should::Recv => {
                                    let _ = self.info.set_progress(1.0);
                                    let _ = self.info.set_status(4);
                                }
                                _ => {}
                            }

                            connection.send(Packets::Tick(connection.session));
                        }
                        crate::packets::Packets::Tick(session) => {
                            if !connection.packets.contains(&packet.id) {
                                connection.last_action = SystemTime::now();
                                connection.add_id(packet.id);
                                connection.add_packets(&packet.packets);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        self.tick();
    }

    fn get_conn(&mut self, session: u128) -> Option<&mut Connection> {
        for conn in self.connections.iter_mut() {
            if conn.session == session {
                return Some(conn);
            }
        }

        None
    }

    fn tick(&mut self) {
        for conn in self.connections.iter_mut() {
            if !conn.active {
                continue;
            }

            if conn.resolv() > 31 {
                continue;
            }

            match self.should {
                Should::Send => {
                    let pak = Packet {
                        id: 0,
                        packets: conn.packets.clone(),
                        packet: Packets::FileContent(FileContent {
                            session: conn.session,
                            cursor: 0,
                            bytes: Vec::new(),
                        }),
                    };

                    let mut buffer = Vec::new();
                    buffer.resize(self.buffer_size - (pak.size() + 0usize.size()), 0);

                    let mut ford = self.info.get_data().unwrap();
                    let _ = ford.seek(std::io::SeekFrom::Start(conn.coursor as u64));
                    let readed = ford.read(&mut buffer).unwrap();

                    let pak = if readed == 0 {
                        self.messages.push(Message::Destroy(conn.session));
                        Packets::Finished(conn.session)
                    } else {
                        self.messages.push(Message::SetProgress(
                            conn.session,
                            (conn.coursor as f64 / conn.content_length as f64) as f32,
                        ));
                        Packets::FileContent(FileContent {
                            session: conn.session,
                            cursor: conn.coursor,
                            bytes: buffer[0..readed].to_owned(),
                        })
                    };
                    conn.coursor += readed as u128;

                    conn.send(pak)
                    // self.pak_storage.send(&mut self.conn, pak, &conn.conn);
                }
                Should::Recv => {}
                Should::Sync => todo!(),
            }
        }

        for conn in self.connections.iter_mut() {
            let Ok(elapsed) = conn.last_action.elapsed()else{continue;};
            if elapsed > Duration::from_secs(20) {
                conn.active = false;
            }
        }

        self.connections.retain(|conn| {
            if !conn.active {
                self.messages.push(Message::Destroy(conn.session));
                false
            } else {
                true
            }
        });
    }
}
