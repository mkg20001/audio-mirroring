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
use crate::ui::main::{Candidate, CandidateType, Dropdown, MainView};

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

        // Selected source node ID and type
        pub selected_source_id: Cell<Option<u32>>,
        pub selected_source_kind: Cell<Option<CandidateType>>,
        // Active target node IDs
        pub active_targets: RefCell<Vec<u32>>,
        // Active links (port_from, port_to) that we created
        pub active_links: RefCell<Vec<(u32, u32)>>,
        // Mapping from link ID to (port_from, port_to) for links we created
        pub link_id_to_ports: RefCell<HashMap<u32, (u32, u32)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GraphManager {
        const NAME: &'static str = "AudioMirroringGraphManager";
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
                    PipewireMessage::VolumeChanged { node_id, volume } => {
                        self.volume_changed(node_id, volume)
                    }
                    PipewireMessage::MuteChanged { node_id, muted } => {
                        self.mute_changed(node_id, muted)
                    }
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
                    Some(CandidateData::new(
                        node.get_id(),
                        CandidateType::Application,
                        node.get_name(),
                    ))
                } else if node.has_port_by_label("monitor_FL") && node.has_port_by_label("monitor_FR") {
                    Some(CandidateData::new(
                        node.get_id(),
                        CandidateType::Device,
                        node.get_name(),
                    ))
                } else {
                    None
                }
            }).collect()
        }

        fn get_candidates_target(&self) -> Vec<CandidateData> {
            self.nodes.borrow().iter().filter_map(|(_, node)| {
                if node.has_port_by_label("playback_FR") && node.has_port_by_label("playback_FL") {
                    Some(CandidateData::new(
                        node.get_id(),
                        CandidateType::Device,
                        node.get_name(),
                    ))
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
                if let Some(node) = nodes.get_mut(node_id) {
                    if let Some(port) = node.get_port_mut(id) {
                        port.set_media_type(media_type);
                    }
                } else {
                    log::warn!("Node (id: {node_id}) for port (id: {id}) not found in graph manager");
                    return;
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
            _active: bool,
            _media_type: MediaType,
        ) {
            // Check if this link is one we created (in our active_links)
            let ports = (output_port_id, input_port_id);
            let active_links = self.active_links.borrow();
            if active_links.contains(&ports) {
                // Track this link ID so we can detect if it gets removed
                self.link_id_to_ports.borrow_mut().insert(id, ports);
                log::info!("Tracking our link: id {} ({} -> {})", id, output_port_id, input_port_id);
            }
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

        // Create a link between the two specified ports (only if it doesn't exist).
        fn create_link_in_pw(&self, port_from: u32, port_to: u32) {
            let sender = self.pw_sender.get().expect("pw_sender shoud be set");
            sender
                .send(crate::GtkMessage::CreateLink { port_from, port_to })
                .expect("Failed to send message");
        }

        // Remove a link between the two specified ports (only if it exists).
        fn remove_link_from_pw(&self, port_from: u32, port_to: u32) {
            let sender = self.pw_sender.get().expect("pw_sender shoud be set");
            sender
                .send(crate::GtkMessage::RemoveLink { port_from, port_to })
                .expect("Failed to send message");
        }

        /// Remove the link with the specified id from the view.
        /// If this was one of our links, re-create it.
        fn remove_link(&self, id: u32) {
            // Check if this was one of our links
            let ports = self.link_id_to_ports.borrow_mut().remove(&id);
            if let Some((port_from, port_to)) = ports {
                // Check if this link is still supposed to be active
                let active_links = self.active_links.borrow();
                if active_links.contains(&(port_from, port_to)) {
                    drop(active_links);
                    log::warn!(
                        "Link {} ({} -> {}) was removed externally, re-creating it",
                        id, port_from, port_to
                    );
                    // Re-create the link
                    self.create_link_in_pw(port_from, port_to);
                }
            }
        }

        fn clear(&self) {
            //self.items.borrow_mut().clear();
            //self.obj().graph().clear();
        }

        pub fn set_source(&self, id: u32, kind: CandidateType) {
            self.selected_source_id.set(Some(id));
            self.selected_source_kind.set(Some(kind));
            log::info!("Source set to node {} ({:?})", id, kind);
            self.update_links();
        }

        pub fn add_target(&self, id: u32) {
            let mut targets = self.active_targets.borrow_mut();
            if !targets.contains(&id) {
                targets.push(id);
                log::info!("Target added: node {}", id);
            }
            drop(targets);
            self.update_links();
        }

        pub fn remove_target(&self, id: u32) {
            // Find and remove links to this target's ports
            let nodes = self.nodes.borrow();
            if let Some(target_node) = nodes.get(&id) {
                let target_fl = target_node.get_port_by_label("playback_FL");
                let target_fr = target_node.get_port_by_label("playback_FR");

                let mut links_to_remove = Vec::new();
                {
                    let active_links = self.active_links.borrow();
                    for &(port_from, port_to) in active_links.iter() {
                        if let Some(ref fl) = target_fl {
                            if port_to == fl.get_id() {
                                links_to_remove.push((port_from, port_to));
                            }
                        }
                        if let Some(ref fr) = target_fr {
                            if port_to == fr.get_id() {
                                links_to_remove.push((port_from, port_to));
                            }
                        }
                    }
                }

                // Remove from active links FIRST (so remove_link won't re-create them)
                self.active_links.borrow_mut().retain(|link| !links_to_remove.contains(link));

                // Remove the links from PipeWire
                for (port_from, port_to) in &links_to_remove {
                    self.remove_link_from_pw(*port_from, *port_to);
                }

                log::info!("Target removed: node {}, removed {} links", id, links_to_remove.len());
            }
            drop(nodes);

            let mut targets = self.active_targets.borrow_mut();
            targets.retain(|&t| t != id);
            drop(targets);

            self.update_status();
        }

        pub fn clear_source(&self) {
            // Remove all active links
            let links = self.active_links.borrow().clone();
            let link_count = links.len();

            // Clear active links FIRST (so remove_link won't re-create them)
            self.active_links.borrow_mut().clear();
            self.link_id_to_ports.borrow_mut().clear();

            // Then remove from PipeWire
            for (port_from, port_to) in links {
                self.remove_link_from_pw(port_from, port_to);
            }

            // Don't clear active_targets - the UI still shows them as selected
            // They will be re-linked when a new source is confirmed

            self.selected_source_id.set(None);
            self.selected_source_kind.set(None);
            log::info!("Source cleared, removed {} links", link_count);
            self.update_status();
        }

        pub fn cleanup(&self) {
            log::info!("Cleaning up - removing all links");
            // Remove all active links
            let links = self.active_links.borrow().clone();

            // Clear state FIRST (so remove_link won't re-create them)
            self.active_links.borrow_mut().clear();
            self.link_id_to_ports.borrow_mut().clear();
            self.active_targets.borrow_mut().clear();
            self.selected_source_id.set(None);
            self.selected_source_kind.set(None);

            // Then remove from PipeWire
            for (port_from, port_to) in links {
                self.remove_link_from_pw(port_from, port_to);
            }
        }

        fn update_status(&self) {
            let has_source = self.selected_source_id.get().is_some();
            let targets = self.active_targets.borrow();
            let target_count = targets.len() as u32;
            // Only show as mirroring if we have both a source and at least one target
            let is_mirroring = has_source && target_count > 0;
            self.obj().main().set_status(is_mirroring, target_count, None);
        }

        fn update_links(&self) {
            let Some(source_id) = self.selected_source_id.get() else {
                return;
            };
            let Some(source_kind) = self.selected_source_kind.get() else {
                return;
            };

            let nodes = self.nodes.borrow();
            let Some(source_node) = nodes.get(&source_id) else {
                log::warn!("Source node {} not found", source_id);
                return;
            };

            // Determine source port labels based on kind
            let (source_fl_label, source_fr_label) = match source_kind {
                CandidateType::Device => ("monitor_FL", "monitor_FR"),
                CandidateType::Application => ("output_FL", "output_FR"),
            };

            let source_fl = source_node.get_port_by_label(source_fl_label);
            let source_fr = source_node.get_port_by_label(source_fr_label);

            let (Some(source_fl), Some(source_fr)) = (source_fl, source_fr) else {
                log::warn!("Source ports not found for node {}", source_id);
                return;
            };

            let targets = self.active_targets.borrow();

            for target_id in targets.iter() {
                let Some(target_node) = nodes.get(target_id) else {
                    log::warn!("Target node {} not found", target_id);
                    continue;
                };

                let target_fl = target_node.get_port_by_label("playback_FL");
                let target_fr = target_node.get_port_by_label("playback_FR");

                let (Some(target_fl), Some(target_fr)) = (target_fl, target_fr) else {
                    log::warn!("Target ports not found for node {}", target_id);
                    continue;
                };

                let link_fl = (source_fl.get_id(), target_fl.get_id());
                let link_fr = (source_fr.get_id(), target_fr.get_id());

                // Only create links if they don't already exist in our tracking
                let mut active_links = self.active_links.borrow_mut();
                if !active_links.contains(&link_fl) {
                    self.create_link_in_pw(link_fl.0, link_fl.1);
                    active_links.push(link_fl);
                }
                if !active_links.contains(&link_fr) {
                    self.create_link_in_pw(link_fr.0, link_fr.1);
                    active_links.push(link_fr);
                }
                drop(active_links);

            }

            // Update status
            self.update_status();
        }

        fn volume_changed(&self, node_id: u32, volume: f32) {
            log::info!("Volume changed for node {}: {}", node_id, volume);
            self.obj().main().update_volume(node_id, volume);
        }

        fn mute_changed(&self, node_id: u32, muted: bool) {
            log::info!("Mute changed for node {}: {}", node_id, muted);
            self.obj().main().update_mute(node_id, muted);
        }

        pub fn get_volume(&self, node_id: u32) {
            let sender = self.pw_sender.get().expect("pw_sender should be set");
            sender
                .send(crate::GtkMessage::GetVolume { node_id })
                .expect("Failed to send get volume message");
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

        // Connect to source dropdown selection
        let source_dd = main.source_dd();
        source_dd.connect_selection_confirmed(glib::clone!(@weak res => move |_dropdown, id, kind| {
            res.imp().set_source(id, CandidateType::from_raw(kind));
            res.get_volume(id);
        }));

        // Connect to source dropdown volume changes
        source_dd.connect_volume_changed(glib::clone!(@weak res => move |_dropdown, node_id, volume| {
            res.set_volume(node_id, volume as f32);
        }));

        // Connect to source dropdown mute changes
        source_dd.connect_mute_changed(glib::clone!(@weak res => move |_dropdown, node_id, muted| {
            res.set_mute(node_id, muted);
        }));

        // Connect to source dropdown selection cancelled (edit mode)
        source_dd.connect_selection_cancelled(glib::clone!(@weak res => move |_dropdown, _source_id| {
            res.imp().clear_source();
        }));

        // Connect to target dropdown selection
        let target_dd = main.target_dd();
        target_dd.connect_selection_confirmed(glib::clone!(@weak res => move |_dropdown, id, _kind| {
            res.imp().add_target(id);
            res.get_volume(id);
        }));

        // Connect to target dropdown volume changes
        target_dd.connect_volume_changed(glib::clone!(@weak res => move |_dropdown, node_id, volume| {
            res.set_volume(node_id, volume as f32);
        }));

        // Connect to target dropdown mute changes
        target_dd.connect_mute_changed(glib::clone!(@weak res => move |_dropdown, node_id, muted| {
            res.set_mute(node_id, muted);
        }));

        // Connect to target dropdown selection cancelled (edit mode)
        target_dd.connect_selection_cancelled(glib::clone!(@weak res => move |_dropdown, target_id| {
            res.imp().remove_target(target_id);
        }));

        // Connect to dynamically added target dropdowns
        main.connect_target_dropdown_added(glib::clone!(@weak res => move |_main_view, dropdown| {
            dropdown.connect_selection_confirmed(glib::clone!(@weak res => move |_dropdown, id, _kind| {
                res.imp().add_target(id);
                res.get_volume(id);
            }));
            dropdown.connect_volume_changed(glib::clone!(@weak res => move |_dropdown, node_id, volume| {
                res.set_volume(node_id, volume as f32);
            }));
            dropdown.connect_mute_changed(glib::clone!(@weak res => move |_dropdown, node_id, muted| {
                res.set_mute(node_id, muted);
            }));
            dropdown.connect_selection_cancelled(glib::clone!(@weak res => move |_dropdown, target_id| {
                res.imp().remove_target(target_id);
            }));
        }));

        // Connect to target removal
        main.connect_target_removed(glib::clone!(@weak res => move |_main_view, target_id| {
            res.imp().remove_target(target_id);
        }));

        res
    }

    pub fn set_source(&self, id: u32, kind: CandidateType) {
        self.imp().set_source(id, kind);
    }

    pub fn add_target(&self, id: u32) {
        self.imp().add_target(id);
    }

    pub fn set_volume(&self, node_id: u32, volume: f32) {
        let sender = self.imp().pw_sender.get().expect("pw_sender should be set");
        sender
            .send(crate::GtkMessage::SetVolume { node_id, volume })
            .expect("Failed to send volume message");
    }

    pub fn set_mute(&self, node_id: u32, muted: bool) {
        let sender = self.imp().pw_sender.get().expect("pw_sender should be set");
        sender
            .send(crate::GtkMessage::SetMute { node_id, muted })
            .expect("Failed to send mute message");
    }

    pub fn get_volume(&self, node_id: u32) {
        self.imp().get_volume(node_id);
    }

    pub fn cleanup(&self) {
        self.imp().cleanup();
    }
}
