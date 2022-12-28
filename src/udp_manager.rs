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
    pak_storage::PakStorage,
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
    counter: u128,
    buffer_size: usize,
    info: EInfo,
    pub messages: Vec<Message>,
    pak_storage: PakStorage,
    name: String,
    connecting: Option<JoinHandle<Result<Connection, ()>>>,
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
    ) -> Option<Self> {
        let Ok(relay) = RelayClient::new(
            ConnectionInfo {
                client: "muzzman-transport".into(),
                name: name.clone(),
                public: vec![random(), random(), random(), random()],
                other: format!("File: {path}").to_bytes(),
                privacy: false,
            },
            relays,
        ) else{ return None};

        let mut buffer = Vec::with_capacity(buffer_size);
        buffer.resize(buffer_size, MaybeUninit::new(0));

        // conn.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        // conn.set_nonblocking(true).unwrap();

        Some(Self {
            connections: Vec::new(),
            buffer,
            // conn,
            buffer_size,
            secret,
            should,
            path,
            counter: 1,
            info,
            messages: Vec::new(),
            pak_storage: Default::default(),
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

        self.relay.search(Search {
            client: relay_man::common::packets::SearchType::Exact("muzzman-transport".into()),
            ..Default::default()
        });

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
        let socket = res.accept(true).get().connect();

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

        socket.send(&b);

        println!("Packet sent");
        Ok(())
    }

    pub fn step(&mut self) {
        self.relay.step();

        if self.connecting.is_none() {
            match self.should {
                Should::Send => {
                    if let Some((index, req)) = self.relay.has_new() {
                        match req {
                            relay_man::client::response::RequestStage::NewRequest(req) => {
                                if let Some(info) = req.connection.info(&req.from).get() {
                                    if info.other == "muzzman-transport".to_string().to_bytes() {
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
                                    let from_adress = req.adress.clone();
                                    let Ok(mut addr) = req.to.to_socket_addrs() else{return Err(())};
                                    let Some(addr) = addr.next() else {return Err(())};

                                    let sock_addr = addr.into();

                                    let socket = req.connect();
                                    socket.set_nonblocking(false).unwrap();

                                    let mut buffer = [MaybeUninit::new(0); 1024];

                                    if let Ok(len) = socket.recv(&mut buffer) {
                                        let bytes = buffer[0..len].to_owned();
                                        let mut bytes = unsafe { std::mem::transmute(bytes) };

                                        if let Some(packet) = Packet::from_bytes(&mut bytes) {
                                            match packet.packet {
                                                crate::packets::Packets::Auth(auth) => {
                                                    println!("Auth part: {}, path: {}, secret: {}, name: {}",auth.path, path, auth.secret, auth.name);
                                                    if auth.path != path || auth.secret != secret {
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
                                                        return Err(());
                                                    }

                                                    let session = random();

                                                    let mut connection = Connection {
                                                        name: auth.name,
                                                        conn: socket,
                                                        packets: vec![0; 32],
                                                        recv_packets: vec![0; 64],
                                                        coursor: 0,
                                                        session,
                                                        active: true,
                                                        last_action: SystemTime::now(),
                                                        content_length: 0,
                                                        pak_cour: 0,
                                                        sock_addr,
                                                    };

                                                    connection.add_id(packet.id);
                                                    connection.add_packets(&packet.packets);

                                                    let pak = AuthResponse {
                                                        accepted: true,
                                                        session,
                                                    };

                                                    let len;
                                                    {
                                                        let mut ford = info.get_data().unwrap();
                                                        let current = ford
                                                            .seek(std::io::SeekFrom::Current(0))
                                                            .unwrap();
                                                        len = ford
                                                            .seek(std::io::SeekFrom::End(0))
                                                            .unwrap();
                                                        let _ = ford.seek(
                                                            std::io::SeekFrom::Start(current),
                                                        );
                                                    }

                                                    connection.content_length = len as u128;

                                                    let pak = Packet {
                                                        id: 2,
                                                        packets: connection.packets.clone(),
                                                        packet: Packets::AuthResponse(pak),
                                                    };

                                                    let mut b = pak.to_bytes();
                                                    b.reverse();

                                                    let _ = connection.conn.send(&b);

                                                    let pak = Headers {
                                                        session,
                                                        content_length: len as u128,
                                                        others: HashMap::new(),
                                                    };

                                                    connection.content_length = pak.content_length;

                                                    let pak = Packet {
                                                        id: 3,
                                                        packets: connection.packets.clone(),
                                                        packet: Packets::Headers(pak),
                                                    };

                                                    let mut b = pak.to_bytes();
                                                    b.reverse();

                                                    let _ = connection.conn.send(&b);
                                                    // self.messages.push(Message::New(
                                                    //     connection.name.clone(),
                                                    //     connection.session,
                                                    //     connection.sock_addr,
                                                    // ));
                                                    // self.connections.push(connection);
                                                }
                                                _ => {}
                                            }
                                        }
                                    }

                                    Err(())
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
                        crate::packets::Packets::AuthResponse(res) => {
                            // TODO: To implement if only requested
                            // WARING!

                            println!("Auth response recived: {}", res.accepted);

                            if res.accepted {
                                match self.should {
                                    Should::Recv | Should::Sync => {
                                        // let session = self.counter;
                                        // let mut connection = Connection {
                                        //     name: "Some conn".to_string(),
                                        //     conn: from,
                                        //     packets: vec![0; 32],
                                        //     recv_packets: vec![0; 64],
                                        //     coursor: 0,
                                        //     session: res.session,
                                        //     active: true,
                                        //     last_action: SystemTime::now(),
                                        //     content_length: 0,
                                        //     pak_cour: 0,
                                        // };

                                        // connection.add_id(packet.id);
                                        // connection.add_packets(&packet.packets);

                                        // self.connections.push(connection);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        crate::packets::Packets::Headers(headers) => match self.should {
                            Should::Sync | Should::Recv => {
                                println!("Recived headers: {}", headers.content_length);
                                // if let Some(conn) = self.get_conn(headers.session) {
                                connection.content_length = headers.content_length;
                                connection.add_id(packet.id);
                                connection.add_packets(&packet.packets);
                                connection.last_action = SystemTime::now();
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

                                    let pak = Packet {
                                        id: 0,
                                        packets: packets.clone(),
                                        packet: Packets::Tick(content.session),
                                    };

                                    // self.pak_storage.send(&mut self.conn, pak, &from);

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

            // if self.pak_storage.resolv(&mut self.conn, conn) > 31 {
            //     continue;
            // }

            match self.should {
                Should::Send => {
                    let mut pak = Packet {
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

                    pak.packet = if readed == 0 {
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
