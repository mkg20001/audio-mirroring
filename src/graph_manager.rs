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

use adw::{glib, prelude::*, subclass::prelude::*};

use pipewire::channel::Sender as PwSender;

use crate::{ui::main, GtkMessage, PipewireMessage};
use crate::types::{};
use crate::ui::main::{Candidate, CandidateType, MainView};

use glib::subclass::prelude::*;
use glib::{glib_object_wrapper, Object, ParamSpec, ParamSpecUInt, ParamSpecString, Value};
use std::cell::{Cell, OnceCell, RefCell};

mod imp {
    use std::option::Option;
    use super::*;

    use std::{cell::OnceCell, cell::RefCell, collections::HashMap};
    use log::warn;
    use crate::{types, MediaType, NodeType};
    use crate::types::Node;
    use crate::ui::main::CandidateData;

    #[derive(Default, glib::Properties)]
    #[properties(wrapper_type = super::GraphManager)]
    pub struct GraphManager {
        #[property(get, set, construct_only)]
        pub main: OnceCell<main::MainView>,

        #[property(get, set, construct_only)]
        pub connection_banner: OnceCell<adw::Banner>,

        pub pw_sender: OnceCell<PwSender<GtkMessage>>,
        pub nodes: RefCell<HashMap<u32, Node>>,
        pub port2node: RefCell<HashMap<u32, u32>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphManager {
        const NAME: &'static str = "HelvumGraphManager";
        type Type = super::GraphManager;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for GraphManager {}

    impl GraphManager {
        pub async fn receive(&self, receiver: async_channel::Receiver<crate::PipewireMessage>) {
            loop {
                let Ok(msg) = receiver.recv().await else {
                    continue;
                };
                match msg {
                    PipewireMessage::NodeAdded {
                        id,
                        name,
                        node_type,
                    } => self.add_node(id, name.as_str(), node_type),
                    PipewireMessage::NodeNameChanged {
                        id,
                        name,
                        media_name,
                    } => self.node_name_changed(id, &name, &media_name),
                    PipewireMessage::PortAdded {
                        id,
                        node_id,
                        name,
                        direction,
                    } => self.add_port(id, name.as_str(), node_id, direction),
                    PipewireMessage::PortFormatChanged { id, media_type } => {
                        self.port_media_type_changed(id, media_type)
                    }
                    PipewireMessage::LinkAdded {
                        id,
                        port_from,
                        port_to,
                        active,
                        media_type,
                    } => self.add_link(id, port_from, port_to, active, media_type),
                    PipewireMessage::LinkStateChanged { id, active } => {
                        self.link_state_changed(id, active)
                    }
                    PipewireMessage::LinkFormatChanged { id, media_type } => {
                        self.link_format_changed(id, media_type)
                    }
                    PipewireMessage::NodeRemoved { id } => self.remove_node(id),
                    PipewireMessage::PortRemoved { id, node_id } => self.remove_port(id, node_id),
                    PipewireMessage::LinkRemoved { id } => self.remove_link(id),
                    PipewireMessage::Connecting => {
                        self.obj().connection_banner().set_revealed(true);
                    }
                    PipewireMessage::Connected => {
                        self.obj().connection_banner().set_revealed(false);
                    }
                    PipewireMessage::Disconnected => {
                        self.clear();
                    }
                };
            }
        }

        fn update_candidates(&self) {
            self.obj().main().update_candidates(self.get_candidates_source(), self.get_candidates_target());
        }

        fn get_candidates_source(&self) -> Vec<CandidateData> {
            self.nodes.borrow().iter().filter_map(|(_, node)| {
                if node.has_port_by_label("output_FL") && node.has_port_by_label("output_FR") {
                    Some(CandidateData {
                        id: node.get_id(),
                        kind: CandidateType::Application,
                        label: node.get_name(),
                    })
                } else if node.has_port_by_label("monitor_FL") && node.has_port_by_label("monitor_FR") {
                    Some(CandidateData {
                        id: node.get_id(),
                        kind: CandidateType::Device,
                        label: node.get_name(),
                    })
                } else {
                    None
                }
            }).collect()
        }

        fn get_candidates_target(&self) -> Vec<CandidateData> {
            self.nodes.borrow().iter().filter_map(|(_, node)| {
                if node.has_port_by_label("playback_FR") && node.has_port_by_label("playback_FL") {
                    Some(CandidateData {
                        id: node.get_id(),
                        kind: CandidateType::Device,
                        label: node.get_name(),
                    })
                } else {
                    None
                }
            }).collect()
        }

        /// Add a new node to the view.
        fn add_node(&self, id: u32, name: &str, node_type: Option<NodeType>) {
            // Add node to main, update selectables for source and mirror

            log::info!("Adding node to graph: id {}", id);

            let node = Node::new(name, id);

            self.nodes.borrow_mut().insert(id, node);

            self.nodes.borrow().iter().for_each(|(_, node)| {
                let name = node.get_name();
                let labels = node.get_port_labels().join(", ");
                log::warn!("{name}: {labels}")
            });

            self.update_candidates()
        }

        /// Update a node tooltip to the view.
        fn node_name_changed(&self, id: u32, node_name: &str, media_name: &str) {
            // Update node name

            if let Some(node) = self.nodes.borrow_mut().get_mut(&id) {
                node.set_name(node_name);
                node.set_media_name(media_name);
            } else {
                log::warn!("Node (id: {id}) for changed name not found in graph manager");
                return;
            }

            self.update_candidates()
        }

        /// Remove the node with the specified id from the view.
        fn remove_node(&self, id: u32) {
            // Remove node from main
            // Do something if the node is currently being used as source or mirror target

            log::info!("Removing node from graph: id {}", id);

            let Some(_) = self.nodes.borrow_mut().remove(&id) else {
                log::warn!("Unknown node (id={id}) removed from graph");
                return;
            };

            self.update_candidates()
        }

        /// Add a new port to the view.
        fn add_port(
            &self,
            id: u32,
            name: &str,
            node_id: u32,
            direction: pipewire::spa::utils::Direction,
        ) {
            log::info!("Adding port to graph: id {}", id);

            if let Some(mut node) = self.nodes.borrow_mut().get_mut(&node_id) {
                node.add_port(types::Port::new(name, id, direction));
                self.port2node.borrow_mut().insert(node_id, id);
            } else {
                log::warn!("Node (id: {node_id}) for port (id: {id}) not found in graph manager");
                return;
            }

            self.update_candidates()
        }

        fn port_media_type_changed(&self, id: u32, media_type: MediaType) {
            let mut nodes = self.nodes.borrow_mut();
            let port2node = self.port2node.borrow();
            if let Some(node_id) = port2node.get(&id) {
                let node = nodes.get_mut(node_id).expect("");
                if let Some(port) = node.get_port_mut(id) {
                    port.set_media_type(media_type);
                }
            } else {
                log::warn!("Node for port (id: {id}) not found in graph manager");
                return;
            }

            self.update_candidates()
        }

        /// Remove the port with the id `id` from the node with the id `node_id`
        /// from the view.
        fn remove_port(&self, id: u32, node_id: u32) {
            log::info!("Removing port from graph: id {}, node_id: {}", id, node_id);

            {
                let mut nodes = self.nodes.borrow_mut();
                let mut node = nodes.get_mut(&node_id);
                if let Some(node) = node {
                    node.remove_port(id);
                } else {
                    log::warn!("Node (id: {node_id}) for port (id: {id}) not found in graph manager");
                    return;
                }
            }

            self.update_candidates()

            /*let mut items = self.items.borrow_mut();

            let Some(node) = items.get(&node_id) else {
                return;
            };
            let Ok(node) = node.clone().dynamic_cast::<graph::Node>() else {
                log::warn!("Graph Manager item under node id {node_id} is not a node");
                return;
            };
            let Some(port) = items.remove(&id) else {
                log::warn!("Unknown Port (id: {id}) removed from graph");
                return;
            };
            let Ok(port) = port.dynamic_cast::<graph::Port>() else {
                log::warn!("Graph Manager item under port id {id} is not a port");
                return;
            };

            node.remove_port(&port); */
        }

        /// Add a new link to the view.
        fn add_link(
            &self,
            id: u32,
            output_port_id: u32,
            input_port_id: u32,
            active: bool,
            media_type: MediaType,
        ) {
            /*log::info!("Adding link to graph: id {}", id);

            let mut items = self.items.borrow_mut();

            let Some(output_port) = items.get(&output_port_id) else {
                log::warn!("Output port (id: {output_port_id}) for link (id: {id}) not found in graph manager");
                return;
            };
            let Ok(output_port) = output_port.clone().dynamic_cast::<graph::Port>() else {
                log::warn!("Graph Manager item under port id {output_port_id} is not a port");
                return;
            };
            let Some(input_port) = items.get(&input_port_id) else {
                log::warn!("Output port (id: {input_port_id}) for link (id: {id}) not found in graph manager");
                return;
            };
            let Ok(input_port) = input_port.clone().dynamic_cast::<graph::Port>() else {
                log::warn!("Graph Manager item under port id {input_port_id} is not a port");
                return;
            };

            let link = graph::Link::new();
            link.set_output_port(Some(&output_port));
            link.set_input_port(Some(&input_port));
            link.set_active(active);
            link.set_media_type(media_type);

            items.insert(id, link.clone().upcast());

            // Update graph to contain the new link.
            self.graph
                .get()
                .expect("graph should be set")
                .add_link(link);*/
        }

        fn link_state_changed(&self, id: u32, active: bool) {
            /*log::info!(
                "Link state changed: Link (id={id}) is now {}",
                if active { "active" } else { "inactive" }
            );

            let items = self.items.borrow();

            let Some(link) = items.get(&id) else {
                log::warn!("Link state changed on unknown link (id={id})");
                return;
            };
            let Some(link) = link.dynamic_cast_ref::<graph::Link>() else {
                log::warn!("Graph Manager item under link id {id} is not a link");
                return;
            };

            link.set_active(active);*/
        }

        fn link_format_changed(
            &self,
            id: u32,
            media_type: pipewire::spa::param::format::MediaType,
        ) {
            /*let items = self.items.borrow();

            let Some(link) = items.get(&id) else {
                log::warn!("Link (id: {id}) for changed media type not found in graph manager");
                return;
            };
            let Some(link) = link.dynamic_cast_ref::<main::Link>() else {
                log::warn!("Graph Manager item under link id {id} is not a link");
                return;
            };
            link.set_media_type(media_type);*/
        }

        // Toggle a link between the two specified ports on the remote pipewire server.
        fn toggle_link(&self, port_from: u32, port_to: u32) {
            let sender = self.pw_sender.get().expect("pw_sender shoud be set");
            sender
                .send(crate::GtkMessage::ToggleLink { port_from, port_to })
                .expect("Failed to send message");
        }

        /// Remove the link with the specified id from the view.
        fn remove_link(&self, id: u32) {
            /*log::info!("Removing link from graph: id {}", id);

            let Some(link) = self.items.borrow_mut().remove(&id) else {
                log::warn!("Unknown Link (id={id}) removed from graph");
                return;
            };
            let Ok(link) = link.dynamic_cast::<main::Link>() else {
                log::warn!("Graph Manager item under link id {id} is not a link");
                return;
            };*/

            // self.obj().main().remove_link(&link);
        }

        fn clear(&self) {
            //self.items.borrow_mut().clear();
            //self.obj().graph().clear();
        }
    }
}

glib::wrapper! {
    pub struct GraphManager(ObjectSubclass<imp::GraphManager>);
}

async fn receive(graph_manager: GraphManager, receiver: async_channel::Receiver<PipewireMessage>) {
    graph_manager.imp().receive(receiver).await
}

impl GraphManager {
    pub fn new(
        main: &MainView,
        connection_banner: &adw::Banner,
        sender: PwSender<GtkMessage>,
        receiver: async_channel::Receiver<PipewireMessage>,
    ) -> Self {
        let res: Self = glib::Object::builder()
            .property("main", main)
            .property("connection-banner", connection_banner)
            .build();

        glib::MainContext::default().spawn_local(receive(res.clone(), receiver));
        assert!(
            res.imp().pw_sender.set(sender).is_ok(),
            "Should be able to set pw_sender)"
        );

        res
    }
}
