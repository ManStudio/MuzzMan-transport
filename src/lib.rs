use std::{path::PathBuf, str::FromStr};

use muzzman_lib::prelude::*;
use udp_manager::{Should, UdpManager};

mod connection;
mod mesage;
mod packets;
mod pak_storage;
mod udp_manager;

#[module_link]
pub struct ModuleMuzzManTransport;

pub fn action_share(info: MRef, args: Vec<Type>) {
    let Some(path) = args.get(0)else{return};
    let Ok(path) = path.clone().try_into() else {return};
    let Some(should_enable) = args.get(1)else{return};
    let Ok(should_enable) = should_enable.clone().try_into() else{return};

    let path: String = path;
    let should_enable: bool = should_enable;

    let filename;
    #[cfg(any(target_os = "unix", target_os = "linux", target_os = "android"))]
    {
        filename = path.split('/').last()
    }
    #[cfg(target_os = "windows")]
    {
        filename = path.split('\\').last()
    }

    let Some(filename) = filename else{return};

    let Ok(session) = info.get_session() else {return};
    let Ok(location) = session.get_default_location() else {return};
    let Ok(element) = session.create_element(filename, &location.id()) else{return};
    let _ = element.set_module(Some(info.id()));
    let _ = element.init();
    let Ok(path) = PathBuf::from_str(&path) else {return};
    let _ = element.set_data(FileOrData::File(path, None));
    let _ = element.set_enabled(should_enable, None);
}

pub fn action_recive(info: MRef, args: Vec<Type>) {
    let Some(url) = args.get(0) else{return};
    let Ok(url) = url.clone().try_into() else {return};
    let url: String = url;

    let Some(should_enable) = args.get(1) else{return};
    let Ok(should_enable) = should_enable.clone().try_into() else {return};
    let should_enable: bool = should_enable;

    let filename;
    #[cfg(any(target_os = "unix", target_os = "linux", target_os = "android"))]
    {
        filename = url.split('/').last()
    }
    #[cfg(target_os = "windows")]
    {
        filename = url.split('\\').last()
    }

    let Some(filename) = filename else{return};

    let Ok(session) = info.get_session() else {return};
    let Ok(location) = session.get_default_location() else {return};
    let Ok(element) = session.create_element(filename, &location.id()) else {return};
    element.set_module(Some(info.id())).unwrap();
    element.set_url(Some(url)).unwrap();
    element.init().unwrap();

    element.set_enabled(should_enable, None).unwrap();
}

impl TModule for ModuleMuzzManTransport {
    fn init(&self, info: MRef) -> Result<(), String> {
        let _ = info.register_action(
            "share".into(),
            vec![
                (
                    String::from("path"),
                    Value::new(
                        Type::None,
                        vec![TypeTag::String],
                        vec![],
                        true,
                        "The filepath that will be shared",
                    ),
                ),
                (
                    String::from("auto_start"),
                    Value::new(
                        Type::Bool(true),
                        vec![TypeTag::Bool],
                        vec![],
                        true,
                        "If should auto enable",
                    ),
                ),
            ],
            action_share,
        );
        let _ = info.register_action(
            "recive".into(),
            vec![
                (
                    String::from("url"),
                    Value::new(
                        Type::None,
                        vec![TypeTag::String],
                        vec![],
                        true,
                        "The mzt url for reciving",
                    ),
                ),
                (
                    String::from("auto_start"),
                    Value::new(
                        Type::Bool(true),
                        vec![TypeTag::Bool],
                        vec![],
                        true,
                        "If should auto enable",
                    ),
                ),
            ],
            action_recive,
        );
        Ok(())
    }

    fn get_name(&self) -> String {
        String::from("MuzzMan Transport")
    }

    fn get_desc(&self) -> String {
        String::from("Transfer library")
    }

    fn init_settings(&self, data: &mut Data) {
        data.add(
            "buffer_size",
            Value::new(
                Type::USize(8192),
                vec![TypeTag::USize],
                vec![],
                true,
                "The buffer size with will recive or send file",
            ),
        );

        data.add(
            "relays",
            Value::new(
                Type::Vec(vec![
                    Type::String("w.konkito.com".into()),
                    Type::String("localhost".into()),
                ]),
                vec![TypeTag::Vec(Box::new(TypeTag::String))],
                vec![],
                true,
                "Relays to find clients",
            ),
        );

        data.add(
            "name",
            Value::new(
                Type::String(whoami::username()),
                vec![TypeTag::String],
                vec![],
                true,
                "The name of the client",
            ),
        );
    }

    fn init_element_settings(&self, data: &mut Data) {
        data.add(
            "url",
            Value::new(
                Type::None,
                vec![TypeTag::String, TypeTag::None],
                vec![],
                true,
                "Url from where to download or upload!",
            ),
        );

        data.add(
            "secret",
            Value::new(
                Type::String("".to_string()),
                vec![TypeTag::String],
                vec![],
                true,
                "is the password for the file",
            ),
        );

        let mut should = CustomEnum::default();
        should.add("Send");
        should.add("Recv");
        should.add("Sync");
        should.set_active(Some(0));
        should.lock();

        data.add(
            "should",
            Value::new(
                Type::CustomEnum(should.clone()),
                vec![TypeTag::CustomEnum(should)],
                vec![],
                true,
                "Should upload,download or sync",
            ),
        );

        data.add(
            "share",
            Value::new(
                Type::String(String::new()),
                vec![TypeTag::String],
                vec![],
                false,
                "This you need to send to your friend to be able to download",
            ),
        );
    }

    fn init_element(&self, element: ERow) {
        let statuses = vec![
            "Initializeiting and validating".to_string(),
            "Waiting for connections".to_string(),
            "Sending".to_string(),
            "Recivind".to_string(),
            "Finished".to_string(),
            "Error".to_string(),
        ];
        element.write().unwrap().statuses = statuses;
        element.set_status(0);
    }

    fn step_element(&self, element: ERow, control_flow: &mut ControlFlow, storage: &mut Storage) {
        let status = element.read().unwrap().status;
        let info = element.read().unwrap().info.clone();
        let mut logger = element.get_logger(None);
        let s = if let Some(session) = &info.read().unwrap().session {
            session.c()
        } else {
            error(&info, "Element Ref has in session");
            return;
        };

        match status {
            0 => {
                if let Some(err) = element.read().unwrap().element_data.validate() {
                    error(&info, format!("Error: element data {}", err));
                    return;
                }

                let buffer_size;
                let path;
                let secret;
                let should;
                let mut relays = vec![];
                let name;

                {
                    let element = element.read().unwrap();

                    match &element.data {
                        FileOrData::File(file_path, _) => {
                            if let Some(p) = file_path.to_str() {
                                path = p.to_string()
                            } else {
                                return;
                            }
                        }
                        FileOrData::Bytes(_) => {
                            return;
                        }
                    }

                    let Some(data) = element.module_data.get("buffer_size")else{return}; // in posibile
                                                                                         // because validation
                    if let Type::USize(p) = data {
                        buffer_size = *p;
                    } else {
                        return; // in posibile because validation
                    }

                    let Some(data) = element.element_data.get("secret")else{return}; // in posibile
                                                                                     // because validation
                    if let Type::String(p) = data {
                        secret = p.clone();
                    } else {
                        return; // in posibile because validation
                    }

                    let Some(data) = element.element_data.get("should")else{return}; // in posibile
                                                                                     // because validation
                    if let Type::CustomEnum(p) = data {
                        should = if let Some(active) = p.get_active() {
                            match active.trim() {
                                "Send" => Should::Send,
                                "Recv" => Should::Recv,
                                "Sync" => Should::Sync,
                                _ => Should::Send,
                            }
                        } else {
                            Should::Send
                        };
                    } else {
                        return; // in posibile because validation
                    }

                    let Some(data) = element.module_data.get("relays")else{return};

                    if let Type::Vec(data) = data {
                        for element in data {
                            if let Type::String(element) = element {
                                relays.push(element.clone())
                            }
                        }
                    }

                    logger.info(format!("Relays: {:?}", relays));

                    if relays.is_empty() {
                        error(&info, "module_data has no relays");
                        return;
                    }

                    let Some(data) = element.module_data.get("name")else{return};

                    if let Type::String(data) = data {
                        name = data.clone();
                    } else {
                        return;
                    }
                }

                let mut manager = match UdpManager::new(
                    buffer_size,
                    path,
                    should,
                    secret,
                    relays,
                    name,
                    info.clone(),
                ) {
                    Ok(manager) => manager,
                    Err(err) => {
                        error(&info, err);
                        return;
                    }
                };

                {
                    let mut err = None;
                    {
                        let element = element.read().unwrap();
                        if let Some(url) = element.url.clone() {
                            if manager.send_request(url.clone()).is_err() {
                                manager.messages.reverse();
                                while !manager.messages.is_empty() {
                                    let message = manager.messages.pop().unwrap();
                                    if let mesage::Message::Error(msg) = message {
                                        logger.error(msg.clone());
                                        err = Some(msg);
                                    }
                                }
                            }
                        }
                    }
                    if let Some(err) = err {
                        error(&info, err);
                        return;
                    }
                }
                storage.set(manager);
                storage.set(Vec::<u128>::new());

                element.set_status(1);
            }
            1 => {
                let Some(sessions) = storage.get::<Vec<u128>>() else {return};
                let mut sessions = sessions.clone();
                let Some(manager) = storage.get_mut::<UdpManager>()else{element.set_status(0); return;};
                manager.step();

                manager.messages.reverse();
                while !manager.messages.is_empty() {
                    let message = manager.messages.pop().unwrap();
                    match message {
                        mesage::Message::New(name, session, conn) => {
                            let id = info.read().unwrap().id.location_id.clone();
                            let location_info = s.get_location_ref(&id).unwrap();
                            let element = location_info.create_element(&name).unwrap();
                            let mut data = element.get_element_data().unwrap();

                            // data.add("parent", Value::new(Type::EInfo(info.clone(), vec![], vec![], false, "Parent")));
                            data.add(
                                "session",
                                Value::new(
                                    Type::U128(session),
                                    vec![],
                                    vec![],
                                    false,
                                    "Session for MZTransport",
                                ),
                            );
                            data.add(
                                "conn",
                                Value::new(
                                    Type::String(conn.as_socket().unwrap().to_string()),
                                    vec![],
                                    vec![],
                                    false,
                                    "IP of the client of MZTransport",
                                ),
                            );
                            element.set_element_data(data).unwrap();
                            sessions.push(session);
                        }
                        mesage::Message::SetProgress(session, progress) => {
                            let id = info.read().unwrap().id.location_id.clone();
                            let location_info = s.get_location_ref(&id).unwrap();
                            let len = location_info.get_elements_len().unwrap();
                            let elements = location_info.get_elements(0..len).unwrap();

                            for element in elements {
                                let data = element.get_element_data().unwrap();
                                if let Some(Type::U128(s)) = data.get("session") {
                                    if *s == session {
                                        element.set_progress(progress).unwrap();
                                    }
                                }
                            }
                        }
                        mesage::Message::SetStatus(session, status) => {
                            let id = info.read().unwrap().id.location_id.clone();
                            let location_info = s.get_location_ref(&id).unwrap();
                            let len = location_info.get_elements_len().unwrap();
                            let elements = location_info.get_elements(0..len).unwrap();

                            for element in elements {
                                let data = element.get_element_data().unwrap();
                                if let Some(Type::U128(s)) = data.get("session") {
                                    if *s == session {
                                        let _ = element.set_status(0);
                                        let _ = element.set_statuses(vec![status.clone()]);
                                    }
                                }
                            }
                        }
                        mesage::Message::Destroy(session) => {
                            let id = info.read().unwrap().id.location_id.clone();
                            let location_info = s.get_location_ref(&id).unwrap();
                            let len = location_info.get_elements_len().unwrap();
                            let elements = location_info.get_elements(0..len).unwrap();

                            let mut finded = None;

                            for element in elements {
                                let data = element.get_element_data().unwrap();
                                if let Some(Type::U128(s)) = data.get("session") {
                                    if *s == session {
                                        finded = Some(element.clone())
                                    }
                                }
                            }

                            if let Some(finded) = finded {
                                finded.destroy().unwrap();
                            }

                            sessions.retain(|s| *s != session);
                        }
                        mesage::Message::SetShare(share) => {
                            if let Ok(mut data) = info.get_element_data() {
                                data.set("share", Type::String(share));
                                let _ = info.set_element_data(data);
                            }
                        }
                        mesage::Message::Error(msg) => {
                            let id = info.read().unwrap().id.location_id.clone();
                            let location_info = s.get_location_ref(&id).unwrap();
                            let len = location_info.get_elements_len().unwrap();
                            let elements = location_info.get_elements(0..len).unwrap();

                            let mut finded = Vec::new();

                            for element in elements {
                                let data = element.get_element_data().unwrap();
                                if let Some(Type::U128(s)) = data.get("session") {
                                    if sessions.contains(s) {
                                        finded.push(element.clone())
                                    }
                                }
                            }

                            for finded in finded {
                                finded.destroy().unwrap();
                            }

                            sessions.clear();
                            error(&info, msg);
                            return;
                        }
                    }
                }

                storage.set(sessions);
            }

            4 | 5 => {
                *control_flow = ControlFlow::Break;
            }
            _ => {}
        }
    }

    fn accept_extension(&self, _filename: &str) -> bool {
        // to do form mzt files
        false
    }

    fn accept_url(&self, url: String) -> bool {
        if let Some(protocol) = url.split(':').next() {
            if protocol == "mzt" {
                return true;
            }
        }
        false
    }

    fn accepted_protocols(&self) -> Vec<String> {
        vec!["mzt".into()]
    }

    fn init_location(&self, _location: LRef, _data: FileOrData) {}

    fn step_location(
        &self,
        _location: LRow,
        _control_flow: &mut ControlFlow,
        _storage: &mut Storage,
    ) {
    }

    fn notify(&self, info: Ref, event: Event) {}

    fn c(&self) -> Box<dyn TModule> {
        Box::new(ModuleMuzzManTransport)
    }
}

pub fn error(element: &ERef, error: impl Into<String>) {
    let mut statuses = element.get_statuses().unwrap();
    statuses[5] = error.into();
    element.set_statuses(statuses).unwrap();
    element.set_status(5).unwrap();
}
