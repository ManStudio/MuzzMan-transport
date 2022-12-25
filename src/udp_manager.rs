use std::{
    collections::HashMap,
    io::{Read, Seek, Write},
    mem::MaybeUninit,
    net::ToSocketAddrs,
    time::{Duration, SystemTime},
};

use socket2::{Domain, Protocol, SockAddr, Socket, Type};

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
    conn: Socket,
    buffer: Vec<MaybeUninit<u8>>,
    path: String,
    secret: String,
    should: Should,
    counter: u128,
    buffer_size: usize,
    info: EInfo,
    pub messages: Vec<Message>,
    pak_storage: PakStorage,
}

impl UdpManager {
    pub fn new(
        port: u16,
        buffer_size: usize,
        path: String,
        should: Should,
        secret: String,
        info: EInfo,
    ) -> Option<Self> {
        let sockaddr: SockAddr = (format!("localhost:{}", port)
            .to_socket_addrs()
            .unwrap()
            .nth(0)
            .unwrap())
        .into();
        let Ok(socket) = Socket::new(Domain::for_address(sockaddr.as_socket().unwrap()), Type::DGRAM, Some(Protocol::UDP))else{return None};
        // let Ok(conn) = UdpSocket::bind(format!("localhost:{}", port))else{return None};
        let Ok(_) = socket.bind(&sockaddr)else{return None};
        let _ = socket.set_nonblocking(true);

        let mut buffer = Vec::with_capacity(buffer_size);
        buffer.resize(buffer_size, MaybeUninit::new(0));

        // conn.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        // conn.set_nonblocking(true).unwrap();

        Some(Self {
            connections: Vec::new(),
            buffer,
            conn: socket,
            // conn,
            buffer_size,
            secret,
            should,
            path,
            counter: 1,
            info,
            messages: Vec::new(),
            pak_storage: Default::default(),
        })
    }

    pub fn send_request(&mut self, url: String) {
        let segments = url.split('/').collect::<Vec<&str>>();
        let addr = segments[2];
        let name = segments[3].to_string();
        let secret = segments[4].to_string();
        let mut path = String::new();
        for (i, s) in segments[5..].iter().enumerate() {
            if i > 0 {
                path.push('/');
            }
            path.push_str(s);
        }

        let pak = Packet {
            id: 43,
            packets: vec![0; 32],
            packet: Packets::Auth(Auth { name, path, secret }),
        };

        println!("Pack: {:?}", pak);

        let mut b = pak.to_bytes();
        b.reverse();

        let _ = self
            .conn
            .send_to(&b, &addr.to_socket_addrs().unwrap().nth(0).unwrap().into());
        println!("Packet sent");
    }

    pub fn step(&mut self) {
        if let Ok((size, from)) = self.conn.recv_from(&mut self.buffer) {
            let bytes = self.buffer[0..size].to_owned();
            let mut bytes = unsafe { std::mem::transmute(bytes) };

            if let Some(packet) = Packet::from_bytes(&mut bytes) {
                match packet.packet {
                    crate::packets::Packets::Auth(auth) => {
                        println!(
                            "Auth part: {}, path: {}, secret: {}, name: {}",
                            auth.path, self.path, auth.secret, auth.name
                        );
                        if auth.path != self.path || auth.secret != self.secret {
                            let mut pak = Packet {
                                id: 2,
                                packets: vec![0; 32],
                                packet: Packets::AuthResponse(AuthResponse {
                                    accepted: false,
                                    session: 0,
                                }),
                            };

                            pak.packets[0] = packet.id;

                            let mut bytes = pak.to_bytes();
                            bytes.reverse();

                            let _ = self.conn.send_to(&bytes, &from);
                            return;
                        }

                        let session = self.counter;
                        self.counter += 1;

                        let mut connection = Connection {
                            name: auth.name,
                            conn: from.clone(),
                            packets: vec![0; 32],
                            recv_packets: vec![0; 64],
                            coursor: 0,
                            session,
                            active: true,
                            last_action: SystemTime::now(),
                            content_length: 0,
                            pak_cour: 0,
                        };

                        connection.add_id(packet.id);
                        connection.add_packets(&packet.packets);

                        let pak = AuthResponse {
                            accepted: true,
                            session,
                        };

                        let len;
                        {
                            let mut ford = self.info.get_data().unwrap();
                            let current = ford.seek(std::io::SeekFrom::Current(0)).unwrap();
                            len = ford.seek(std::io::SeekFrom::End(0)).unwrap();
                            let _ = ford.seek(std::io::SeekFrom::Start(current));
                        }

                        connection.content_length = len as u128;

                        let pak = Packet {
                            id: 2,
                            packets: connection.packets.clone(),
                            packet: Packets::AuthResponse(pak),
                        };

                        let mut b = pak.to_bytes();
                        b.reverse();

                        let _ = self.conn.send_to(&b, &from);

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

                        let _ = self.conn.send_to(&b, &from);
                        self.messages.push(Message::New(
                            connection.name.clone(),
                            connection.session,
                            connection.conn.clone(),
                        ));
                        self.connections.push(connection);
                    }
                    crate::packets::Packets::AuthResponse(res) => {
                        // TODO: To implement if only requested
                        // WARING!

                        println!("Auth response recived: {}", res.accepted);

                        if res.accepted {
                            match self.should {
                                Should::Recv | Should::Sync => {
                                    // let session = self.counter;
                                    let mut connection = Connection {
                                        name: "Some conn".to_string(),
                                        conn: from,
                                        packets: vec![0; 32],
                                        recv_packets: vec![0; 64],
                                        coursor: 0,
                                        session: res.session,
                                        active: true,
                                        last_action: SystemTime::now(),
                                        content_length: 0,
                                        pak_cour: 0,
                                    };

                                    connection.add_id(packet.id);
                                    connection.add_packets(&packet.packets);

                                    self.connections.push(connection);
                                }
                                _ => {}
                            }
                        }
                    }
                    crate::packets::Packets::Headers(headers) => match self.should {
                        Should::Sync | Should::Recv => {
                            println!("Recived headers: {}", headers.content_length);
                            if let Some(conn) = self.get_conn(headers.session) {
                                if conn.conn.as_socket().unwrap() == from.as_socket().unwrap() {
                                    conn.content_length = headers.content_length;
                                    conn.add_id(packet.id);
                                    conn.add_packets(&packet.packets);
                                    conn.last_action = SystemTime::now();
                                }
                            }
                        }
                        _ => {}
                    },
                    crate::packets::Packets::FileContent(content) => match self.should {
                        Should::Recv | Should::Sync => {
                            let mut _do = false;
                            let mut coursor = 0;
                            let mut content_length = 0;
                            let mut packets = Vec::new();

                            if let Some(conn) = self.get_conn(content.session) {
                                if conn.conn.as_socket().unwrap() == from.as_socket().unwrap() {
                                    _do = true;
                                    if conn.packets.contains(&packet.id) {
                                        _do = false;
                                    }

                                    if _do {
                                        conn.add_id(packet.id);
                                        conn.add_packets(&packet.packets);
                                        conn.last_action = SystemTime::now();
                                        conn.coursor = content.cursor;

                                        content_length = conn.content_length;
                                        coursor = conn.coursor;

                                        packets = conn.packets.clone();
                                    }
                                }
                            }

                            if _do {
                                let mut ford = self.info.get_data().unwrap();
                                let _ = ford.seek(std::io::SeekFrom::Start(content.cursor as u64));
                                let _ = ford.write(&content.bytes).unwrap();

                                let pak = Packet {
                                    id: 0,
                                    packets: packets.clone(),
                                    packet: Packets::Tick(content.session),
                                };

                                self.pak_storage.send(&mut self.conn, pak, &from);

                                let _ = self
                                    .info
                                    .set_progress((coursor as f64 / content_length as f64) as f32);
                            }
                        }
                        _ => {}
                    },
                    crate::packets::Packets::Finished(finished) => {
                        let mut finded = None;
                        for (i, conn) in self.connections.iter_mut().enumerate() {
                            if conn.session == finished {
                                if conn.packets.contains(&packet.id) {
                                    break;
                                }

                                finded = Some(i);
                                conn.add_id(packet.id);
                                conn.add_packets(&packet.packets);
                                conn.last_action = SystemTime::now();
                                break;
                            }
                        }
                        if let Some(finded) = finded {
                            self.connections.remove(finded);
                            match self.should {
                                Should::Recv => {
                                    let _ = self.info.set_progress(1.0);
                                    let _ = self.info.set_status(4);
                                }
                                _ => {}
                            }
                        }
                    }
                    crate::packets::Packets::Tick(session) => {
                        if let Some(conn) = self.get_conn(session) {
                            if !conn.packets.contains(&packet.id) {
                                if conn.conn.as_socket().unwrap() == from.as_socket().unwrap() {
                                    conn.last_action = SystemTime::now();
                                    conn.add_id(packet.id);
                                    conn.add_packets(&packet.packets);
                                }
                            }
                        }
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

            if self.pak_storage.resolv(&mut self.conn, conn) > 31 {
                continue;
            }

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

                    self.pak_storage.send(&mut self.conn, pak, &conn.conn);
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
