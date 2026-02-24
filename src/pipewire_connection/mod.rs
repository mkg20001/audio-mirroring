// Copyright 2021 Tom A. Wagner <tom.a.wagner@protonmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License version 3 as published by
// the Free Software Foundation.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-only

mod state;

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use adw::glib::{self, clone};
use log::{debug, error, info, warn};
use pipewire::{
    context::Context,
    core::{Core, PW_ID_CORE},
    keys,
    link::{Link, LinkChangeMask, LinkInfoRef, LinkListener, LinkState},
    main_loop::MainLoop,
    node::{Node, NodeInfoRef, NodeListener},
    port::{Port, PortChangeMask, PortInfoRef, PortListener},
    properties::{properties, Properties},
    registry::{GlobalObject, Registry},
    spa::{
        param::{ParamInfoFlags, ParamType},
        utils::dict::DictRef,
        utils::result::SpaResult,
    },
    types::ObjectType,
};

use crate::{GtkMessage, MediaType, NodeType, PipewireMessage};
use state::{Item, State};

enum ProxyItem {
    Node {
        proxy: Node,
        _listener: NodeListener,
    },
    Port {
        proxy: Port,
        _listener: PortListener,
    },
    Link {
        _proxy: Link,
        _listener: LinkListener,
    },
}

struct LoopState {
    is_stopped: bool,
    props: Properties,
}

impl LoopState {
    fn handle_message(&mut self, msg: GtkMessage) -> bool {
        match msg {
            GtkMessage::Terminate => self.is_stopped = true,
            GtkMessage::Connect(remote) => match remote {
                Some(s) => self.props.insert(*keys::REMOTE_NAME, s),
                None => self.props.remove(*keys::REMOTE_NAME),
            },
            _ => return false,
        }
        true
    }
}

/// The "main" function of the pipewire thread.
pub(super) fn thread_main(
    gtk_sender: async_channel::Sender<PipewireMessage>,
    mut pw_receiver: pipewire::channel::Receiver<GtkMessage>,
) {
    let mainloop = MainLoop::new(None).expect("Failed to create mainloop");
    let context = Rc::new(Context::new(&mainloop).expect("Failed to create context"));
    let loop_state = Rc::new(RefCell::new(LoopState {
        is_stopped: false,
        props: properties! {
            "media.category" => "Manager",
        },
    }));
    let mut is_connecting = false;

    // Wait PipeWire service to connect from command line arguments.
    let receiver = pw_receiver.attach(mainloop.loop_(), {
        clone!(@strong mainloop, @strong loop_state => move |msg|
            if loop_state.borrow_mut().handle_message(msg) {
                mainloop.quit();
            }
        )
    });
    mainloop.run();
    pw_receiver = receiver.deattach();

    while !loop_state.borrow().is_stopped {
        // Try to connect
        let props = loop_state.borrow().props.clone();
        let core = match context.connect(Some(props)) {
            Ok(core) => Rc::new(core),
            Err(_) => {
                if !is_connecting {
                    is_connecting = true;
                    gtk_sender
                        .send_blocking(PipewireMessage::Connecting)
                        .expect("Failed to send message");
                }

                // If connection is failed, try to connect again in 200ms
                let interval = Some(Duration::from_millis(200));

                let timer = mainloop
                    .loop_()
                    .add_timer(clone!(@strong mainloop => move |_| {
                        mainloop.quit();
                    }));

                timer.update_timer(interval, None).into_result().unwrap();

                let receiver = pw_receiver.attach(mainloop.loop_(), {
                    clone!(@strong mainloop, @strong loop_state => move |msg|
                        if loop_state.borrow_mut().handle_message(msg) {
                            mainloop.quit();
                        }
                    )
                });

                mainloop.run();
                pw_receiver = receiver.deattach();

                continue;
            }
        };

        if is_connecting {
            is_connecting = false;
            gtk_sender
                .send_blocking(PipewireMessage::Connected)
                .expect("Failed to send message");
        }

        let registry = Rc::new(core.get_registry().expect("Failed to get registry"));

        // Keep proxies and their listeners alive so that we can receive info events.
        let proxies = Rc::new(RefCell::new(HashMap::new()));
        let state = Rc::new(RefCell::new(State::new()));

        let receiver = pw_receiver.attach(mainloop.loop_(), {
            clone!(@strong mainloop, @weak core, @weak registry, @strong state, @strong loop_state, @strong proxies, @strong gtk_sender => move |msg| match msg {
                GtkMessage::ToggleLink { port_from, port_to } => toggle_link(port_from, port_to, &core, &registry, &state),
                GtkMessage::SetVolume { node_id, volume } => set_volume(node_id, volume, &proxies),
                GtkMessage::GetVolume { node_id } => get_volume(node_id, &proxies),
                GtkMessage::Terminate | GtkMessage::Connect(_) => {
                    loop_state.borrow_mut().handle_message(msg);
                    mainloop.quit();
                }
            })
        });

        let _listener = core
            .add_listener_local()
            .error(
                clone!(@strong mainloop, @strong gtk_sender => move |id, _seq, res, message| {
                    if id != PW_ID_CORE {
                        return;
                    }

                    if res == -libc::EPIPE {
                        gtk_sender.send_blocking(PipewireMessage::Disconnected)
                            .expect("Failed to send message");
                        mainloop.quit();
                    } else {
                        let serr = SpaResult::from_c(res).into_result().unwrap_err();
                        error!("Pipewire Core received error {serr}: {message}");
                    }
                }),
            )
            .register();

        let _listener = registry
            .add_listener_local()
            .global(clone!(@strong gtk_sender, @weak registry, @strong proxies, @strong state =>
                move |global| match global.type_ {
                    ObjectType::Node => handle_node(global, &gtk_sender, &registry, &proxies, &state),
                    ObjectType::Port => handle_port(global, &gtk_sender, &registry, &proxies, &state),
                    ObjectType::Link => handle_link(global, &gtk_sender, &registry, &proxies, &state),
                    _ => {
                        // Other objects are not interesting to us
                    }
                }
            ))
            .global_remove(clone!(@strong gtk_sender, @strong proxies, @strong state => move |id| {
                if let Some(item) = state.borrow_mut().remove(id) {
                    gtk_sender.send_blocking(match item {
                        Item::Node { .. } => PipewireMessage::NodeRemoved {id},
                        Item::Port { node_id } => PipewireMessage::PortRemoved {id, node_id},
                        Item::Link { .. } => PipewireMessage::LinkRemoved {id},
                    }).expect("Failed to send message");
                } else {
                    warn!(
                        "Attempted to remove item with id {} that is not saved in state",
                        id
                    );
                }

                proxies.borrow_mut().remove(&id);
            }))
            .register();

        mainloop.run();
        pw_receiver = receiver.deattach();

        gtk_sender
            .send_blocking(PipewireMessage::Disconnected)
            .expect("Failed to send message");
    }
}

/// Get the nicest possible name for the node, using a fallback chain of possible name attributes
fn get_node_name(props: &DictRef) -> &str {
    props
        .get(&keys::NODE_DESCRIPTION)
        .or_else(|| props.get(&keys::NODE_NICK))
        .or_else(|| props.get(&keys::NODE_NAME))
        .unwrap_or_default()
}

/// Handle a new node being added
fn handle_node(
    node: &GlobalObject<&DictRef>,
    sender: &async_channel::Sender<PipewireMessage>,
    registry: &Rc<Registry>,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
    state: &Rc<RefCell<State>>,
) {
    let props = node
        .props
        .as_ref()
        .expect("Node object is missing properties");

    let name = get_node_name(props).to_string();
    let media_class = |class: &str| {
        if class.contains("Sink") || class.contains("Input") {
            Some(NodeType::Input)
        } else if class.contains("Source") || class.contains("Output") {
            Some(NodeType::Output)
        } else {
            None
        }
    };

    let node_type = props
        .get("media.category")
        .and_then(|class| {
            if class.contains("Duplex") {
                None
            } else {
                props.get("media.class").and_then(media_class)
            }
        })
        .or_else(|| props.get("media.class").and_then(media_class));

    state.borrow_mut().insert(node.id, Item::Node);

    sender
        .send_blocking(PipewireMessage::NodeAdded {
            id: node.id,
            name,
            node_type,
        })
        .expect("Failed to send message");

    let proxy: Node = registry.bind(node).expect("Failed to bind to node proxy");
    let node_id = node.id;
    let listener = proxy
        .add_listener_local()
        .info(clone!(@strong sender, @strong proxies => move |info| {
            handle_node_info(info, &sender, &proxies);
        }))
        .param(clone!(@strong sender => move |_seq, param_type, _index, _next, param| {
            if param_type == ParamType::Props || param_type == ParamType::Route {
                handle_node_props(node_id, param_type, param, &sender);
            }
        }))
        .register();

    // Subscribe to Props and Route params for volume changes
    proxy.subscribe_params(&[ParamType::Props, ParamType::Route]);

    proxies.borrow_mut().insert(
        node.id,
        ProxyItem::Node {
            proxy,
            _listener: listener,
        },
    );
}

fn handle_node_info(
    info: &NodeInfoRef,
    sender: &async_channel::Sender<PipewireMessage>,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
) {
    debug!("Received node info: {:?}", info);

    let id = info.id();
    let proxies = proxies.borrow();
    let Some(ProxyItem::Node { .. }) = proxies.get(&id) else {
        error!("Received info on unknown node with id {id}");
        return;
    };

    let props = info.props().expect("NodeInfo object is missing properties");
    if let Some(media_name) = props.get(&keys::MEDIA_NAME) {
        let name = get_node_name(props).to_string();

        sender
            .send_blocking(PipewireMessage::NodeNameChanged {
                id,
                name,
                media_name: media_name.to_string(),
            })
            .expect("Failed to send message");
    }
}

/// Handle a new port being added
fn handle_port(
    port: &GlobalObject<&DictRef>,
    sender: &async_channel::Sender<PipewireMessage>,
    registry: &Rc<Registry>,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
    state: &Rc<RefCell<State>>,
) {
    let port_id = port.id;
    let proxy: Port = registry.bind(port).expect("Failed to bind to port proxy");
    let listener = proxy
        .add_listener_local()
        .info(
            clone!(@strong proxies, @strong state, @strong sender => move |info| {
                handle_port_info(info, &proxies, &state, &sender);
            }),
        )
        .param(clone!(@strong sender => move |_, param_id, _, _, param| {
            if param_id == ParamType::EnumFormat {
                handle_port_enum_format(port_id, param, &sender)
            }
        }))
        .register();

    proxies.borrow_mut().insert(
        port.id,
        ProxyItem::Port {
            proxy,
            _listener: listener,
        },
    );
}

fn handle_port_info(
    info: &PortInfoRef,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
    state: &Rc<RefCell<State>>,
    sender: &async_channel::Sender<PipewireMessage>,
) {
    debug!("Received port info: {:?}", info);

    let id = info.id();
    let proxies = proxies.borrow();
    let Some(ProxyItem::Port { proxy, .. }) = proxies.get(&id) else {
        log::error!("Received info on unknown port with id {id}");
        return;
    };

    let mut state = state.borrow_mut();

    if let Some(Item::Port { .. }) = state.get(id) {
        // Info was an update, figure out if we should notify the GTK thread
        if info.change_mask().contains(PortChangeMask::PARAMS) {
            // TODO: React to param changes
        }
    } else {
        // First time we get info. We can now notify the gtk thread of a new link.
        let props = info.props().expect("Port object is missing properties");
        let name = props.get("port.name").unwrap_or_default().to_string();
        let node_id: u32 = props
            .get("node.id")
            .expect("Port has no node.id property!")
            .parse()
            .expect("Could not parse node.id property");

        state.insert(id, Item::Port { node_id });

        let params = info.params();
        let enum_format_info = params
            .iter()
            .find(|param| param.id() == ParamType::EnumFormat);
        if let Some(enum_format_info) = enum_format_info {
            if enum_format_info.flags().contains(ParamInfoFlags::READ) {
                proxy.enum_params(0, Some(ParamType::EnumFormat), 0, u32::MAX);
            }
        }

        sender
            .send_blocking(PipewireMessage::PortAdded {
                id,
                node_id,
                name,
                direction: info.direction(),
            })
            .expect("Failed to send message");
    }
}

fn handle_port_enum_format(
    port_id: u32,
    param: Option<&pipewire::spa::pod::Pod>,
    sender: &async_channel::Sender<PipewireMessage>,
) {
    let media_type = param
        .and_then(|param| pipewire::spa::param::format_utils::parse_format(param).ok())
        .map(|(media_type, _media_subtype)| media_type)
        .unwrap_or(MediaType::Unknown);

    sender
        .send_blocking(PipewireMessage::PortFormatChanged {
            id: port_id,
            media_type,
        })
        .expect("Failed to send message")
}

/// Handle a new link being added
fn handle_link(
    link: &GlobalObject<&DictRef>,
    sender: &async_channel::Sender<PipewireMessage>,
    registry: &Rc<Registry>,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
    state: &Rc<RefCell<State>>,
) {
    debug!(
        "New link (id:{}) appeared, setting up info listener.",
        link.id
    );

    let proxy: Link = registry.bind(link).expect("Failed to bind to link proxy");
    let listener = proxy
        .add_listener_local()
        .info(clone!(@strong state, @strong sender => move |info| {
            handle_link_info(info, &state, &sender);
        }))
        .register();

    proxies.borrow_mut().insert(
        link.id,
        ProxyItem::Link {
            _proxy: proxy,
            _listener: listener,
        },
    );
}

fn handle_link_info(
    info: &LinkInfoRef,
    state: &Rc<RefCell<State>>,
    sender: &async_channel::Sender<PipewireMessage>,
) {
    debug!("Received link info: {:?}", info);

    let id = info.id();

    let mut state = state.borrow_mut();
    if let Some(Item::Link { .. }) = state.get(id) {
        // Info was an update - figure out if we should notify the gtk thread
        if info.change_mask().contains(LinkChangeMask::STATE) {
            sender
                .send_blocking(PipewireMessage::LinkStateChanged {
                    id,
                    active: matches!(info.state(), LinkState::Active),
                })
                .expect("Failed to send message");
        }
        if info.change_mask().contains(LinkChangeMask::FORMAT) {
            sender
                .send_blocking(PipewireMessage::LinkFormatChanged {
                    id,
                    media_type: get_link_media_type(info),
                })
                .expect("Failed to send message");
        }
    } else {
        // First time we get info. We can now notify the gtk thread of a new link.
        let port_from = info.output_port_id();
        let port_to = info.input_port_id();

        state.insert(id, Item::Link { port_from, port_to });

        sender
            .send_blocking(PipewireMessage::LinkAdded {
                id,
                port_from,
                port_to,
                active: matches!(info.state(), LinkState::Active),
                media_type: get_link_media_type(info),
            })
            .expect("Failed to send message");
    }
}

/// Toggle a link between the two specified ports.
fn toggle_link(
    port_from: u32,
    port_to: u32,
    core: &Rc<Core>,
    registry: &Rc<Registry>,
    state: &Rc<RefCell<State>>,
) {
    let state = state.borrow_mut();
    if let Some(id) = state.get_link_id(port_from, port_to) {
        info!("Requesting removal of link with id {}", id);

        // FIXME: Handle error
        registry.destroy_global(id);
    } else {
        info!(
            "Requesting creation of link from port id:{} to port id:{}",
            port_from, port_to
        );

        let node_from = state
            .get_node_of_port(port_from)
            .expect("Requested port not in state");
        let node_to = state
            .get_node_of_port(port_to)
            .expect("Requested port not in state");

        if let Err(e) = core.create_object::<Link>(
            "link-factory",
            &properties! {
                "link.output.node" => node_from.to_string(),
                "link.output.port" => port_from.to_string(),
                "link.input.node" => node_to.to_string(),
                "link.input.port" => port_to.to_string(),
                "object.linger" => "1"
            },
        ) {
            warn!("Failed to create link: {}", e);
        }
    }
}

fn handle_node_props(
    node_id: u32,
    param_type: ParamType,
    param: Option<&pipewire::spa::pod::Pod>,
    sender: &async_channel::Sender<PipewireMessage>,
) {
    use pipewire::spa::pod::deserialize::PodDeserializer;
    use pipewire::spa::pod::{Value, ValueArray};

    let Some(param) = param else {
        return;
    };

    // Try to deserialize the pod
    let Ok((_, value)) = PodDeserializer::deserialize_any_from(param.as_bytes()) else {
        return;
    };

    const SPA_PROP_VOLUME: u32 = 3;
    const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x10008;
    const SPA_PARAM_ROUTE_props: u32 = 5;

    // Handle both Props and Route params
    if let Value::Object(obj) = value {
        let properties = if param_type == ParamType::Route {
            // For Route, look inside the props sub-object
            obj.properties.iter()
                .find(|p| p.key == SPA_PARAM_ROUTE_props)
                .and_then(|p| {
                    if let Value::Object(props_obj) = &p.value {
                        Some(props_obj.properties.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        } else {
            obj.properties
        };

        for prop in properties {
            // Try channelVolumes first
            if prop.key == SPA_PROP_CHANNEL_VOLUMES {
                if let Value::ValueArray(ValueArray::Float(ref volumes)) = prop.value {
                    if let Some(&volume) = volumes.first() {
                        // Convert from cubic to linear for display
                        let linear_volume = volume.cbrt();
                        info!("Volume changed for node {} (channelVolumes): {} (linear: {})", node_id, volume, linear_volume);
                        let _ = sender.send_blocking(PipewireMessage::VolumeChanged {
                            node_id,
                            volume: linear_volume,
                        });
                        return;
                    }
                }
            }
            // Fallback to single volume
            if prop.key == SPA_PROP_VOLUME {
                if let Value::Float(volume) = prop.value {
                    let linear_volume = volume.cbrt();
                    info!("Volume changed for node {} (volume): {} (linear: {})", node_id, volume, linear_volume);
                    let _ = sender.send_blocking(PipewireMessage::VolumeChanged {
                        node_id,
                        volume: linear_volume,
                    });
                    return;
                }
            }
        }
    }
}

fn set_volume(node_id: u32, volume: f32, proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>) {
    use pipewire::spa::pod::serialize::PodSerializer;
    use pipewire::spa::pod::{Object, Property, PropertyFlags, Value, ValueArray};
    use std::io::Cursor;

    // Clamp volume between 0.0 and 1.0
    let volume = volume.clamp(0.0, 1.0);

    // Convert linear volume to cubic (perceptual) scale
    let cubic_volume = volume * volume * volume;

    info!("Setting volume for node {} to {} (cubic: {})", node_id, volume, cubic_volume);

    // Try native PipeWire first for stream nodes
    let native_success = {
        let proxies = proxies.borrow();
        if let Some(ProxyItem::Node { proxy, .. }) = proxies.get(&node_id) {
            // SPA constants
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
                    properties: vec![
                        Property {
                            key: SPA_PROP_CHANNEL_VOLUMES,
                            flags: PropertyFlags::empty(),
                            value: Value::ValueArray(ValueArray::Float(
                                vec![cubic_volume, cubic_volume],
                            )),
                        },
                    ],
                }),
            );

            if let Ok((cursor, _size)) = result {
                let pod_data = cursor.into_inner();
                let pod = unsafe {
                    &*(pod_data.as_ptr() as *const pipewire::spa::pod::Pod)
                };
                proxy.set_param(ParamType::Props, 0, pod);
                info!("Volume set for node {} via native Props", node_id);
                true
            } else {
                false
            }
        } else {
            warn!("Node {} not found for volume control", node_id);
            false
        }
    };

    // Always also try wpctl for device nodes (it handles device routing properly)
    if native_success {
        set_volume_wpctl(node_id, volume);
    }
}

fn set_volume_wpctl(node_id: u32, volume: f32) {
    let volume_str = format!("{:.2}", volume);

    std::thread::spawn(move || {
        match std::process::Command::new("wpctl")
            .args(["set-volume", &node_id.to_string(), &volume_str])
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    info!("Volume set for node {} via wpctl", node_id);
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    debug!("wpctl set-volume for node {} (may be expected): {}", node_id, stderr);
                }
            }
            Err(e) => {
                warn!("Failed to run wpctl: {}", e);
            }
        }
    });
}

fn get_volume(node_id: u32, proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>) {
    let proxies = proxies.borrow();
    let Some(ProxyItem::Node { proxy, .. }) = proxies.get(&node_id) else {
        warn!("Node {} not found for get volume", node_id);
        return;
    };

    // Request both Props and Route params - the response will come via the param callback
    proxy.enum_params(0, Some(ParamType::Props), 0, u32::MAX);
    proxy.enum_params(0, Some(ParamType::Route), 0, u32::MAX);
    info!("Requested volume for node {}", node_id);
}

fn get_link_media_type(link_info: &LinkInfoRef) -> MediaType {
    let media_type = link_info
        .format()
        .and_then(|format| pipewire::spa::param::format_utils::parse_format(format).ok())
        .map(|(media_type, _media_subtype)| media_type)
        .unwrap_or(MediaType::Unknown);

    media_type
}
