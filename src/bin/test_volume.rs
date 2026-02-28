// Simple test CLI for debugging PipeWire volume control

use pipewire::{
    context::Context,
    device::{Device, DeviceListener},
    main_loop::MainLoop,
    node::{Node, NodeListener},
    spa::{
        param::ParamType,
        pod::{
            deserialize::PodDeserializer,
            serialize::PodSerializer,
            Object, Property, PropertyFlags, Value, ValueArray,
        },
        utils::dict::DictRef,
    },
    types::ObjectType,
};
use std::{cell::RefCell, collections::HashMap, env, io::Cursor, rc::Rc};

struct NodeInfo {
    name: String,
    device_id: Option<u32>,
    card_profile_device: Option<i32>,
    _proxy: Node,
    _listener: NodeListener,
}

struct DeviceInfo {
    name: String,
    proxy: Device,
    _listener: DeviceListener,
}

struct RouteInfo {
    route_index: i32,
    route_device: i32,
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Usage:");
        println!("  {} list                    - List nodes and devices", args[0]);
        println!("  {} volume <node_id> <vol>  - Set volume (0.0-1.0)", args[0]);
        println!("  {} mute <node_id> <0|1>    - Set mute", args[0]);
        return;
    }

    let command = &args[1];

    pipewire::init();
    let mainloop = MainLoop::new(None).expect("Failed to create mainloop");
    let context = Context::new(&mainloop).expect("Failed to create context");
    let core = context
        .connect(None)
        .expect("Failed to connect to PipeWire");
    let registry = Rc::new(core.get_registry().expect("Failed to get registry"));

    let nodes: Rc<RefCell<HashMap<u32, NodeInfo>>> = Rc::new(RefCell::new(HashMap::new()));
    let devices: Rc<RefCell<HashMap<u32, DeviceInfo>>> = Rc::new(RefCell::new(HashMap::new()));
    let routes: Rc<RefCell<HashMap<(u32, i32), RouteInfo>>> = Rc::new(RefCell::new(HashMap::new()));

    let _listener = registry
        .add_listener_local()
        .global({
            let nodes = nodes.clone();
            let devices = devices.clone();
            let routes = routes.clone();
            let registry = registry.clone();
            move |global| {
                let props = match global.props {
                    Some(p) => p,
                    None => return,
                };

                match global.type_ {
                    ObjectType::Node => {
                        let name = get_name(props);
                        let device_id = props.get("device.id").and_then(|s| s.parse().ok());
                        let card_profile_device = props
                            .get("card.profile.device")
                            .and_then(|s| s.parse().ok());

                        // Bind the node proxy
                        let proxy: Node = registry.bind(global).expect("Failed to bind node");

                        // Set up listener to get node info (which has all properties)
                        let nodes_clone = nodes.clone();
                        let node_id = global.id;
                        let listener = proxy
                            .add_listener_local()
                            .info(move |info| {
                                if let Some(props) = info.props() {
                                    let device_id = props.get("device.id").and_then(|s| s.parse().ok());
                                    let card_profile_device = props
                                        .get("card.profile.device")
                                        .and_then(|s| s.parse().ok());
                                    // Update the stored info
                                    if let Some(node_info) = nodes_clone.borrow_mut().get_mut(&node_id) {
                                        if device_id.is_some() {
                                            node_info.device_id = device_id;
                                        }
                                        if card_profile_device.is_some() {
                                            node_info.card_profile_device = card_profile_device;
                                        }
                                    }
                                }
                            })
                            .register();

                        nodes.borrow_mut().insert(
                            global.id,
                            NodeInfo {
                                name,
                                device_id,
                                card_profile_device,
                                _proxy: proxy,
                                _listener: listener,
                            },
                        );
                    }
                    ObjectType::Device => {
                        let name = props
                            .get("device.description")
                            .or_else(|| props.get("device.name"))
                            .unwrap_or("Unknown")
                            .to_string();

                        let proxy: Device =
                            registry.bind(global).expect("Failed to bind device");

                        // Subscribe to Route params
                        let routes_clone = routes.clone();
                        let device_id = global.id;
                        let listener = proxy
                            .add_listener_local()
                            .param(move |_seq, param_type, _index, _next, param| {
                                if param_type == ParamType::Route {
                                    if let Some(route) = parse_route(param) {
                                        println!("Got route: device={}, route_device={}, index={}",
                                                 device_id, route.route_device, route.route_index);
                                        routes_clone.borrow_mut().insert(
                                            (device_id, route.route_device),
                                            route,
                                        );
                                    }
                                }
                            })
                            .register();

                        proxy.subscribe_params(&[ParamType::Route]);

                        devices.borrow_mut().insert(
                            global.id,
                            DeviceInfo {
                                name,
                                proxy,
                                _listener: listener,
                            },
                        );
                    }
                    _ => {}
                }
            }
        })
        .register();

    // Wait for objects to be enumerated
    let timer_mainloop = mainloop.clone();
    let _timer = mainloop.loop_().add_timer({
        move |_| {
            timer_mainloop.quit();
        }
    });
    _timer
        .update_timer(
            Some(std::time::Duration::from_millis(1000)),
            None,
        )
        .into_result()
        .unwrap();

    mainloop.run();

    match command.as_str() {
        "list" => {
            println!("\n=== Devices ===");
            for (id, dev) in devices.borrow().iter() {
                println!("Device {}: {}", id, dev.name);
            }

            println!("\n=== Routes ===");
            for ((dev_id, route_dev), route) in routes.borrow().iter() {
                println!(
                    "Device {} route_device={}: index={}",
                    dev_id, route_dev, route.route_index
                );
            }

            println!("\n=== Nodes ===");
            for (id, node) in nodes.borrow().iter() {
                let device_str = match (node.device_id, node.card_profile_device) {
                    (Some(d), Some(c)) => format!("device={}, card.profile.device={}", d, c),
                    (Some(d), None) => format!("device={}", d),
                    _ => "no device".to_string(),
                };
                println!("Node {}: {} ({})", id, node.name, device_str);
            }
        }
        "volume" => {
            if args.len() < 4 {
                println!("Usage: {} volume <node_id> <volume>", args[0]);
                return;
            }
            let node_id: u32 = args[2].parse().expect("Invalid node ID");
            let volume: f32 = args[3].parse().expect("Invalid volume");

            set_volume(
                node_id,
                volume,
                &nodes,
                &devices,
                &routes,
            );

            // Give PipeWire time to process
            let quit_mainloop = mainloop.clone();
            let _timer2 = mainloop.loop_().add_timer(move |_| {
                quit_mainloop.quit();
            });
            _timer2
                .update_timer(Some(std::time::Duration::from_millis(100)), None)
                .into_result()
                .unwrap();
            mainloop.run();

            println!("Volume set to {} for node {}", volume, node_id);
        }
        "mute" => {
            if args.len() < 4 {
                println!("Usage: {} mute <node_id> <0|1>", args[0]);
                return;
            }
            let node_id: u32 = args[2].parse().expect("Invalid node ID");
            let muted: bool = args[3] == "1";

            set_mute(
                node_id,
                muted,
                &nodes,
                &devices,
                &routes,
            );

            // Give PipeWire time to process
            let quit_mainloop = mainloop.clone();
            let _timer2 = mainloop.loop_().add_timer(move |_| {
                quit_mainloop.quit();
            });
            _timer2
                .update_timer(Some(std::time::Duration::from_millis(100)), None)
                .into_result()
                .unwrap();
            mainloop.run();

            println!("Mute set to {} for node {}", muted, node_id);
        }
        _ => {
            println!("Unknown command: {}", command);
        }
    }
}

fn get_name(props: &DictRef) -> String {
    props
        .get("node.description")
        .or_else(|| props.get("node.nick"))
        .or_else(|| props.get("node.name"))
        .unwrap_or("Unknown")
        .to_string()
}

fn parse_route(param: Option<&pipewire::spa::pod::Pod>) -> Option<RouteInfo> {
    let param = param?;
    let (_, value) = PodDeserializer::deserialize_any_from(param.as_bytes()).ok()?;

    const SPA_PARAM_ROUTE_INDEX: u32 = 1;
    const SPA_PARAM_ROUTE_DEVICE: u32 = 3;

    if let Value::Object(obj) = value {
        let mut route_index: Option<i32> = None;
        let mut route_device: Option<i32> = None;

        for prop in &obj.properties {
            match prop.key {
                SPA_PARAM_ROUTE_INDEX => {
                    if let Value::Int(idx) = prop.value {
                        route_index = Some(idx);
                    }
                }
                SPA_PARAM_ROUTE_DEVICE => {
                    if let Value::Int(dev) = prop.value {
                        route_device = Some(dev);
                    }
                }
                _ => {}
            }
        }

        if let (Some(index), Some(device)) = (route_index, route_device) {
            return Some(RouteInfo {
                route_index: index,
                route_device: device,
            });
        }
    }
    None
}

fn set_volume(
    node_id: u32,
    volume: f32,
    nodes: &Rc<RefCell<HashMap<u32, NodeInfo>>>,
    devices: &Rc<RefCell<HashMap<u32, DeviceInfo>>>,
    routes: &Rc<RefCell<HashMap<(u32, i32), RouteInfo>>>,
) {
    let volume = volume.clamp(0.0, 1.0);
    let cubic_volume = volume * volume * volume;

    let nodes = nodes.borrow();
    let node = match nodes.get(&node_id) {
        Some(n) => n,
        None => {
            println!("Node {} not found", node_id);
            return;
        }
    };

    println!("Node {}: device_id={:?}, card_profile_device={:?}",
             node_id, node.device_id, node.card_profile_device);

    // Check if this is a device node
    if let (Some(device_id), Some(card_profile_device)) = (node.device_id, node.card_profile_device) {
        let devices = devices.borrow();
        let routes = routes.borrow();

        // Find the device
        let device = match devices.get(&device_id) {
            Some(d) => d,
            None => {
                println!("Device {} not found", device_id);
                return;
            }
        };

        // Find the route
        let route = match routes.get(&(device_id, card_profile_device)) {
            Some(r) => r,
            None => {
                println!("Route not found for device {} card_profile_device {}",
                         device_id, card_profile_device);
                println!("Available routes: {:?}", routes.keys().collect::<Vec<_>>());
                return;
            }
        };

        println!("Setting volume via Device {} Route (index={}, device={})",
                 device_id, route.route_index, route.route_device);

        // Build Route param
        set_device_volume(&device.proxy, route, cubic_volume);
    } else {
        // Stream node - set Props directly
        println!("Setting volume via Node Props");
        set_node_volume(&node._proxy, cubic_volume);
    }
}

fn set_device_volume(proxy: &Device, route: &RouteInfo, cubic_volume: f32) {
    // Route param object type and id
    const SPA_TYPE_OBJECT_PARAM_ROUTE: u32 = 0x40009; // 262153
    const SPA_PARAM_ROUTE: u32 = 13;
    // Route property keys
    const SPA_PARAM_ROUTE_INDEX: u32 = 1;
    const SPA_PARAM_ROUTE_DEVICE: u32 = 3;
    const SPA_PARAM_ROUTE_PROPS: u32 = 10;
    const SPA_PARAM_ROUTE_SAVE: u32 = 13;

    // Props object type
    const SPA_TYPE_OBJECT_PROPS: u32 = 0x40002; // 262146
    const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x10008; // 65544

    let props_object = Object {
        type_: SPA_TYPE_OBJECT_PROPS,
        id: SPA_PARAM_ROUTE, // Inner Props uses Route param id
        properties: vec![Property {
            key: SPA_PROP_CHANNEL_VOLUMES,
            flags: PropertyFlags::empty(),
            value: Value::ValueArray(ValueArray::Float(vec![cubic_volume, cubic_volume])),
        }],
    };

    let route_object = Object {
        type_: SPA_TYPE_OBJECT_PARAM_ROUTE,
        id: SPA_PARAM_ROUTE,
        properties: vec![
            Property {
                key: SPA_PARAM_ROUTE_INDEX,
                flags: PropertyFlags::empty(),
                value: Value::Int(route.route_index),
            },
            Property {
                key: SPA_PARAM_ROUTE_DEVICE,
                flags: PropertyFlags::empty(),
                value: Value::Int(route.route_device),
            },
            Property {
                key: SPA_PARAM_ROUTE_PROPS,
                flags: PropertyFlags::empty(),
                value: Value::Object(props_object),
            },
            Property {
                key: SPA_PARAM_ROUTE_SAVE,
                flags: PropertyFlags::empty(),
                value: Value::Bool(true),
            },
        ],
    };

    let pod_vec: Vec<u8> = Vec::new();
    let cursor = Cursor::new(pod_vec);

    let result = PodSerializer::serialize(cursor, &Value::Object(route_object));

    if let Ok((cursor, _size)) = result {
        let pod_data = cursor.into_inner();
        let pod = unsafe { &*(pod_data.as_ptr() as *const pipewire::spa::pod::Pod) };
        proxy.set_param(ParamType::Route, 0, pod);
        println!("Route param sent to device");
    } else {
        println!("Failed to serialize Route param");
    }
}

fn set_node_volume(proxy: &Node, cubic_volume: f32) {
    const SPA_TYPE_OBJECT_PROPS: u32 = 0x40002;
    const SPA_PARAM_PROPS: u32 = 2;
    const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x10008;

    let pod_vec: Vec<u8> = Vec::new();
    let cursor = Cursor::new(pod_vec);

    let result = PodSerializer::serialize(
        cursor,
        &Value::Object(Object {
            type_: SPA_TYPE_OBJECT_PROPS,
            id: SPA_PARAM_PROPS,
            properties: vec![Property {
                key: SPA_PROP_CHANNEL_VOLUMES,
                flags: PropertyFlags::empty(),
                value: Value::ValueArray(ValueArray::Float(vec![cubic_volume, cubic_volume])),
            }],
        }),
    );

    if let Ok((cursor, _size)) = result {
        let pod_data = cursor.into_inner();
        let pod = unsafe { &*(pod_data.as_ptr() as *const pipewire::spa::pod::Pod) };
        proxy.set_param(ParamType::Props, 0, pod);
        println!("Props param sent to node");
    } else {
        println!("Failed to serialize Props param");
    }
}

fn set_mute(
    node_id: u32,
    muted: bool,
    nodes: &Rc<RefCell<HashMap<u32, NodeInfo>>>,
    devices: &Rc<RefCell<HashMap<u32, DeviceInfo>>>,
    routes: &Rc<RefCell<HashMap<(u32, i32), RouteInfo>>>,
) {
    let nodes = nodes.borrow();
    let node = match nodes.get(&node_id) {
        Some(n) => n,
        None => {
            println!("Node {} not found", node_id);
            return;
        }
    };

    println!("Node {}: device_id={:?}, card_profile_device={:?}",
             node_id, node.device_id, node.card_profile_device);

    if let (Some(device_id), Some(card_profile_device)) = (node.device_id, node.card_profile_device) {
        let devices = devices.borrow();
        let routes = routes.borrow();

        let device = match devices.get(&device_id) {
            Some(d) => d,
            None => {
                println!("Device {} not found", device_id);
                return;
            }
        };

        let route = match routes.get(&(device_id, card_profile_device)) {
            Some(r) => r,
            None => {
                println!("Route not found for device {} card_profile_device {}",
                         device_id, card_profile_device);
                return;
            }
        };

        println!("Setting mute via Device {} Route", device_id);
        set_device_mute(&device.proxy, route, muted);
    } else {
        println!("Setting mute via Node Props");
        set_node_mute(&node._proxy, muted);
    }
}

fn set_device_mute(proxy: &Device, route: &RouteInfo, muted: bool) {
    // Route param object type and id
    const SPA_TYPE_OBJECT_PARAM_ROUTE: u32 = 0x40009; // 262153
    const SPA_PARAM_ROUTE: u32 = 13;
    // Route property keys
    const SPA_PARAM_ROUTE_INDEX: u32 = 1;
    const SPA_PARAM_ROUTE_DEVICE: u32 = 3;
    const SPA_PARAM_ROUTE_PROPS: u32 = 10;
    const SPA_PARAM_ROUTE_SAVE: u32 = 13;

    // Props object type
    const SPA_TYPE_OBJECT_PROPS: u32 = 0x40002; // 262146
    const SPA_PROP_MUTE: u32 = 0x10004; // 65540

    let props_object = Object {
        type_: SPA_TYPE_OBJECT_PROPS,
        id: SPA_PARAM_ROUTE, // Inner Props uses Route param id
        properties: vec![Property {
            key: SPA_PROP_MUTE,
            flags: PropertyFlags::empty(),
            value: Value::Bool(muted),
        }],
    };

    let route_object = Object {
        type_: SPA_TYPE_OBJECT_PARAM_ROUTE,
        id: SPA_PARAM_ROUTE,
        properties: vec![
            Property {
                key: SPA_PARAM_ROUTE_INDEX,
                flags: PropertyFlags::empty(),
                value: Value::Int(route.route_index),
            },
            Property {
                key: SPA_PARAM_ROUTE_DEVICE,
                flags: PropertyFlags::empty(),
                value: Value::Int(route.route_device),
            },
            Property {
                key: SPA_PARAM_ROUTE_PROPS,
                flags: PropertyFlags::empty(),
                value: Value::Object(props_object),
            },
            Property {
                key: SPA_PARAM_ROUTE_SAVE,
                flags: PropertyFlags::empty(),
                value: Value::Bool(true),
            },
        ],
    };

    let pod_vec: Vec<u8> = Vec::new();
    let cursor = Cursor::new(pod_vec);

    let result = PodSerializer::serialize(cursor, &Value::Object(route_object));

    if let Ok((cursor, _size)) = result {
        let pod_data = cursor.into_inner();
        let pod = unsafe { &*(pod_data.as_ptr() as *const pipewire::spa::pod::Pod) };
        proxy.set_param(ParamType::Route, 0, pod);
        println!("Route param (mute) sent to device");
    } else {
        println!("Failed to serialize Route param for mute");
    }
}

fn set_node_mute(proxy: &Node, muted: bool) {
    const SPA_TYPE_OBJECT_PROPS: u32 = 0x40002;
    const SPA_PARAM_PROPS: u32 = 2;
    const SPA_PROP_MUTE: u32 = 0x10004;

    let pod_vec: Vec<u8> = Vec::new();
    let cursor = Cursor::new(pod_vec);

    let result = PodSerializer::serialize(
        cursor,
        &Value::Object(Object {
            type_: SPA_TYPE_OBJECT_PROPS,
            id: SPA_PARAM_PROPS,
            properties: vec![Property {
                key: SPA_PROP_MUTE,
                flags: PropertyFlags::empty(),
                value: Value::Bool(muted),
            }],
        }),
    );

    if let Ok((cursor, _size)) = result {
        let pod_data = cursor.into_inner();
        let pod = unsafe { &*(pod_data.as_ptr() as *const pipewire::spa::pod::Pod) };
        proxy.set_param(ParamType::Props, 0, pod);
        println!("Props param (mute) sent to node");
    } else {
        println!("Failed to serialize Props param for mute");
    }
}
