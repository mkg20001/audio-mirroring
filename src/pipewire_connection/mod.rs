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
    device::{Device, DeviceListener},
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
use state::{Item, NodeDeviceInfo, RouteInfo, State};

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
    Device {
        proxy: Device,
        _listener: DeviceListener,
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
                GtkMessage::CreateLink { port_from, port_to } => create_link(port_from, port_to, &core, &state),
                GtkMessage::RemoveLink { port_from, port_to } => remove_link(port_from, port_to, &registry, &state),
                GtkMessage::SetVolume { node_id, volume } => set_volume(node_id, volume, &proxies, &state),
                GtkMessage::GetVolume { node_id } => get_volume(node_id, &proxies),
                GtkMessage::SetMute { node_id, muted } => set_mute(node_id, muted, &proxies, &state),
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
                    ObjectType::Device => handle_device(global, &registry, &proxies, &state),
                    _ => {
                        // Other objects are not interesting to us
                    }
                }
            ))
            .global_remove(clone!(@strong gtk_sender, @strong proxies, @strong state => move |id| {
                if let Some(item) = state.borrow_mut().remove(id) {
                    match item {
                        Item::Node { .. } => {
                            gtk_sender.send_blocking(PipewireMessage::NodeRemoved {id})
                                .expect("Failed to send message");
                        }
                        Item::Port { node_id } => {
                            gtk_sender.send_blocking(PipewireMessage::PortRemoved {id, node_id})
                                .expect("Failed to send message");
                        }
                        Item::Link { .. } => {
                            gtk_sender.send_blocking(PipewireMessage::LinkRemoved {id})
                                .expect("Failed to send message");
                        }
                        Item::Device => {
                            // Device removed - no message needed for now
                        }
                    }
                }
                // Objects we don't track (params, metadata, etc.) are silently ignored

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

    // Extract device.id and card.profile.device if present (for device nodes like sinks/sources)
    let device_info = props
        .get("device.id")
        .and_then(|id| id.parse::<u32>().ok())
        .and_then(|device_id| {
            // card.profile.device identifies which route on the device applies to this node
            let card_profile_device = props
                .get("card.profile.device")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0); // Default to 0 if not present
            Some(NodeDeviceInfo {
                device_id,
                card_profile_device,
            })
        });

    {
        let mut state = state.borrow_mut();
        state.insert(node.id, Item::Node { device_info: device_info.clone() });
        if let Some(info) = device_info {
            state.set_node_device_info(node.id, info);
        }
    }

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
        .info(clone!(@strong sender, @strong proxies, @strong state => move |info| {
            handle_node_info(info, &sender, &proxies, &state);
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
    state: &Rc<RefCell<State>>,
) {
    debug!("Received node info: {:?}", info);

    let id = info.id();
    let proxies = proxies.borrow();
    let Some(ProxyItem::Node { .. }) = proxies.get(&id) else {
        error!("Received info on unknown node with id {id}");
        return;
    };

    let props = info.props().expect("NodeInfo object is missing properties");

    // Update device info from node info props (card.profile.device is often not in global props)
    if let Some(device_id_str) = props.get("device.id") {
        if let Ok(device_id) = device_id_str.parse::<u32>() {
            let card_profile_device = props
                .get("card.profile.device")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0);
            debug!(
                "Node {} device info: device_id={}, card.profile.device={}",
                id, device_id, card_profile_device
            );
            state.borrow_mut().set_node_device_info(
                id,
                NodeDeviceInfo {
                    device_id,
                    card_profile_device,
                },
            );
        }
    }

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

/// Handle a new device being added
fn handle_device(
    device: &GlobalObject<&DictRef>,
    registry: &Rc<Registry>,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
    state: &Rc<RefCell<State>>,
) {
    let device_id = device.id;
    debug!("New device (id:{}) appeared", device_id);

    state.borrow_mut().insert(device_id, Item::Device);

    let proxy: Device = registry.bind(device).expect("Failed to bind to device proxy");

    let listener = proxy
        .add_listener_local()
        .param(clone!(@strong state => move |_seq, param_type, _index, _next, param| {
            if param_type == ParamType::Route {
                handle_device_route(device_id, param, &state);
            }
        }))
        .register();

    // Subscribe to Route params for volume routing info
    proxy.subscribe_params(&[ParamType::Route]);

    proxies.borrow_mut().insert(
        device_id,
        ProxyItem::Device {
            proxy,
            _listener: listener,
        },
    );
}

/// Handle Route param from a device - extract route info for volume control
fn handle_device_route(
    device_id: u32,
    param: Option<&pipewire::spa::pod::Pod>,
    state: &Rc<RefCell<State>>,
) {
    use pipewire::spa::pod::deserialize::PodDeserializer;
    use pipewire::spa::pod::Value;

    let Some(param) = param else {
        return;
    };

    let Ok((_, value)) = PodDeserializer::deserialize_any_from(param.as_bytes()) else {
        return;
    };

    // Route param structure:
    // - index: route index
    // - device: device index
    // - props: the props object (with volume, mute, etc.)
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
            debug!("Device {} route: index={}, device={}", device_id, index, device);
            state.borrow_mut().set_device_route(
                device_id,
                device, // route_device is the key to match with card.profile.device
                RouteInfo {
                    route_index: index,
                    route_device: device,
                },
            );
        }
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

/// Remove a link between the two specified ports (only if it exists).
fn remove_link(
    port_from: u32,
    port_to: u32,
    registry: &Rc<Registry>,
    state: &Rc<RefCell<State>>,
) {
    let state = state.borrow_mut();
    if let Some(id) = state.get_link_id(port_from, port_to) {
        info!("Requesting removal of link with id {}", id);
        registry.destroy_global(id);
    } else {
        info!(
            "Link from port {} to port {} does not exist, skipping removal",
            port_from, port_to
        );
    }
}

/// Create a link between the two specified ports (only if it doesn't exist).
fn create_link(
    port_from: u32,
    port_to: u32,
    core: &Rc<Core>,
    state: &Rc<RefCell<State>>,
) {
    let state = state.borrow_mut();
    if state.get_link_id(port_from, port_to).is_some() {
        info!(
            "Link from port {} to port {} already exists, skipping creation",
            port_from, port_to
        );
        return;
    }

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

    const SPA_PROP_VOLUME: u32 = 0x10003;
    const SPA_PROP_MUTE: u32 = 0x10004;
    const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x10008;
    const SPA_PARAM_ROUTE_PROPS: u32 = 10;

    // Handle both Props and Route params
    if let Value::Object(obj) = value {
        let properties = if param_type == ParamType::Route {
            // For Route, look inside the props sub-object
            obj.properties.iter()
                .find(|p| p.key == SPA_PARAM_ROUTE_PROPS)
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

        // First pass: look for channelVolumes (preferred) and mute
        let mut found_channel_volumes = false;
        let mut mute_sent = false;

        for prop in &properties {
            // Look for channelVolumes (this is the actual volume level)
            if prop.key == SPA_PROP_CHANNEL_VOLUMES {
                match &prop.value {
                    Value::ValueArray(ValueArray::Float(volumes)) => {
                        if let Some(&volume) = volumes.first() {
                            // Convert from cubic to linear for display
                            let linear_volume = volume.cbrt();
                            debug!(
                                "Node {} channelVolumes: raw={}, linear={}",
                                node_id, volume, linear_volume
                            );
                            let _ = sender.send_blocking(PipewireMessage::VolumeChanged {
                                node_id,
                                volume: linear_volume,
                            });
                            found_channel_volumes = true;
                        }
                    }
                    Value::ValueArray(ValueArray::Double(volumes)) => {
                        if let Some(&volume) = volumes.first() {
                            let linear_volume = (volume as f32).cbrt();
                            debug!(
                                "Node {} channelVolumes (double): raw={}, linear={}",
                                node_id, volume, linear_volume
                            );
                            let _ = sender.send_blocking(PipewireMessage::VolumeChanged {
                                node_id,
                                volume: linear_volume,
                            });
                            found_channel_volumes = true;
                        }
                    }
                    _ => {}
                }
            }
            // Check mute state
            if prop.key == SPA_PROP_MUTE && !mute_sent {
                if let Value::Bool(muted) = prop.value {
                    let _ = sender.send_blocking(PipewireMessage::MuteChanged {
                        node_id,
                        muted,
                    });
                    mute_sent = true;
                }
            }
        }

        // Note: We intentionally skip SPA_PROP_VOLUME (single volume multiplier)
        // and softVolumes - these are software multipliers, not the actual volume level.
        // If channelVolumes is not found, we don't send a volume update.
    }
}

fn set_volume(
    node_id: u32,
    volume: f32,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
    state: &Rc<RefCell<State>>,
) {
    use pipewire::spa::pod::serialize::PodSerializer;
    use pipewire::spa::pod::{Object, Property, PropertyFlags, Value, ValueArray};
    use std::io::Cursor;

    // Clamp volume between 0.0 and 1.0
    let volume = volume.clamp(0.0, 1.0);

    // Convert linear volume to cubic (perceptual) scale
    let cubic_volume = volume * volume * volume;

    info!("set_volume: node={}, linear={}, cubic={}", node_id, volume, cubic_volume);

    // Check if this node is associated with a device
    let device_info = state.borrow().get_node_route_info(node_id).map(|(d, r)| (d, r.clone()));

    let proxies = proxies.borrow();

    if let Some((device_id, route_info)) = device_info {
        // Device node - set volume via Route param on the Device
        info!(
            "set_volume: node {} is device node (device={}, route_index={}, route_device={})",
            node_id, device_id, route_info.route_index, route_info.route_device
        );
        if let Some(ProxyItem::Device { proxy, .. }) = proxies.get(&device_id) {
            set_device_volume(proxy, &route_info, cubic_volume);
            info!("Volume set for node {} via Device {} Route", node_id, device_id);
        } else {
            warn!("Device {} not found for node {}", device_id, node_id);
        }
    } else {
        // Stream node - set volume via Props on the Node
        info!("set_volume: node {} is stream node (no device info)", node_id);
        if let Some(ProxyItem::Node { proxy, .. }) = proxies.get(&node_id) {
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
                debug!("Volume set for node {} via native Props", node_id);
            }
        } else {
            warn!("Node {} not found for volume control", node_id);
        }
    }
}

fn set_device_volume(proxy: &Device, route_info: &RouteInfo, cubic_volume: f32) {
    use pipewire::spa::pod::serialize::PodSerializer;
    use pipewire::spa::pod::{Object, Property, PropertyFlags, Value, ValueArray};
    use std::io::Cursor;

    // SPA constants for Route param
    const SPA_TYPE_OBJECT_PARAM_ROUTE: u32 = 0x40009; // 262153
    const SPA_PARAM_ROUTE: u32 = 13;
    const SPA_PARAM_ROUTE_INDEX: u32 = 1;
    const SPA_PARAM_ROUTE_DEVICE: u32 = 3;
    const SPA_PARAM_ROUTE_PROPS: u32 = 10;
    const SPA_PARAM_ROUTE_SAVE: u32 = 13;

    const SPA_TYPE_OBJECT_PROPS: u32 = 0x40002; // 262146
    const SPA_PROP_CHANNEL_VOLUMES: u32 = 0x10008; // 65544

    // Build the props sub-object
    let props_object = Object {
        type_: SPA_TYPE_OBJECT_PROPS,
        id: SPA_PARAM_ROUTE, // Inner Props uses Route param id
        properties: vec![Property {
            key: SPA_PROP_CHANNEL_VOLUMES,
            flags: PropertyFlags::empty(),
            value: Value::ValueArray(ValueArray::Float(vec![cubic_volume, cubic_volume])),
        }],
    };

    // Build the Route param
    let route_object = Object {
        type_: SPA_TYPE_OBJECT_PARAM_ROUTE,
        id: SPA_PARAM_ROUTE,
        properties: vec![
            Property {
                key: SPA_PARAM_ROUTE_INDEX,
                flags: PropertyFlags::empty(),
                value: Value::Int(route_info.route_index),
            },
            Property {
                key: SPA_PARAM_ROUTE_DEVICE,
                flags: PropertyFlags::empty(),
                value: Value::Int(route_info.route_device),
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
    } else {
        warn!("Failed to serialize Route param for device volume");
    }
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

fn set_mute(
    node_id: u32,
    muted: bool,
    proxies: &Rc<RefCell<HashMap<u32, ProxyItem>>>,
    state: &Rc<RefCell<State>>,
) {
    use pipewire::spa::pod::serialize::PodSerializer;
    use pipewire::spa::pod::{Object, Property, PropertyFlags, Value};
    use std::io::Cursor;

    debug!("Setting mute for node {} to {}", node_id, muted);

    // Check if this node is associated with a device
    let device_info = state.borrow().get_node_route_info(node_id).map(|(d, r)| (d, r.clone()));

    let proxies = proxies.borrow();

    if let Some((device_id, route_info)) = device_info {
        // Device node - set mute via Route param on the Device
        if let Some(ProxyItem::Device { proxy, .. }) = proxies.get(&device_id) {
            set_device_mute(proxy, &route_info, muted);
            debug!("Mute set for node {} via Device {} Route", node_id, device_id);
        } else {
            warn!("Device {} not found for node {}", device_id, node_id);
        }
    } else {
        // Stream node - set mute via Props on the Node
        if let Some(ProxyItem::Node { proxy, .. }) = proxies.get(&node_id) {
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
                debug!("Mute set for node {} to {} via native Props", node_id, muted);
            }
        } else {
            warn!("Node {} not found for mute control", node_id);
        }
    }
}

fn set_device_mute(proxy: &Device, route_info: &RouteInfo, muted: bool) {
    use pipewire::spa::pod::serialize::PodSerializer;
    use pipewire::spa::pod::{Object, Property, PropertyFlags, Value};
    use std::io::Cursor;

    // SPA constants for Route param
    const SPA_TYPE_OBJECT_PARAM_ROUTE: u32 = 0x40009; // 262153
    const SPA_PARAM_ROUTE: u32 = 13;
    const SPA_PARAM_ROUTE_INDEX: u32 = 1;
    const SPA_PARAM_ROUTE_DEVICE: u32 = 3;
    const SPA_PARAM_ROUTE_PROPS: u32 = 10;
    const SPA_PARAM_ROUTE_SAVE: u32 = 13;

    const SPA_TYPE_OBJECT_PROPS: u32 = 0x40002; // 262146
    const SPA_PROP_MUTE: u32 = 0x10004; // 65540

    // Build the props sub-object
    let props_object = Object {
        type_: SPA_TYPE_OBJECT_PROPS,
        id: SPA_PARAM_ROUTE, // Inner Props uses Route param id
        properties: vec![Property {
            key: SPA_PROP_MUTE,
            flags: PropertyFlags::empty(),
            value: Value::Bool(muted),
        }],
    };

    // Build the Route param
    let route_object = Object {
        type_: SPA_TYPE_OBJECT_PARAM_ROUTE,
        id: SPA_PARAM_ROUTE,
        properties: vec![
            Property {
                key: SPA_PARAM_ROUTE_INDEX,
                flags: PropertyFlags::empty(),
                value: Value::Int(route_info.route_index),
            },
            Property {
                key: SPA_PARAM_ROUTE_DEVICE,
                flags: PropertyFlags::empty(),
                value: Value::Int(route_info.route_device),
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
    } else {
        warn!("Failed to serialize Route param for device mute");
    }
}


fn get_link_media_type(link_info: &LinkInfoRef) -> MediaType {
    let media_type = link_info
        .format()
        .and_then(|format| pipewire::spa::param::format_utils::parse_format(format).ok())
        .map(|(media_type, _media_subtype)| media_type)
        .unwrap_or(MediaType::Unknown);

    media_type
}
