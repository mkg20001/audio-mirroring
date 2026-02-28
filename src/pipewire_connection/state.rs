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

use std::collections::HashMap;

/// Route information for a device
#[derive(Clone, Debug)]
pub(super) struct RouteInfo {
    pub route_index: i32,
    pub route_device: i32,
}

/// Info about a node's device association
#[derive(Clone, Debug)]
pub(super) struct NodeDeviceInfo {
    pub device_id: u32,
    /// The card.profile.device property - identifies which route on the device applies to this node
    pub card_profile_device: i32,
}

/// Any pipewire item we need to keep track of.
/// These will be saved in the `State` struct associated with their id.
pub(super) enum Item {
    Node {
        /// Device association info if this node is associated with a device
        device_info: Option<NodeDeviceInfo>,
    },
    Port {
        // Save the id of the node this is on so we can remove the port from it
        // when it is deleted.
        node_id: u32,
    },
    Link {
        port_from: u32,
        port_to: u32,
    },
    Device,
}

/// This struct keeps track of any relevant items and stores them under their IDs.
///
/// Given two port ids, it can also efficiently find the id of the link that connects them.
#[derive(Default)]
pub(super) struct State {
    /// Map pipewire ids to items.
    items: HashMap<u32, Item>,
    /// Map `(output port id, input port id)` tuples to the id of the link that connects them.
    links: HashMap<(u32, u32), u32>,
    /// Map (device_id, route_device) to route info (used for volume control)
    /// Each device can have multiple routes, identified by route_device
    device_routes: HashMap<(u32, i32), RouteInfo>,
    /// Map node ids to their device info (for quick lookup)
    node_device_info: HashMap<u32, NodeDeviceInfo>,
}

impl State {
    /// Create a new, empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new item under the specified id.
    pub fn insert(&mut self, id: u32, item: Item) {
        if let Item::Link {
            port_from, port_to, ..
        } = item
        {
            self.links.insert((port_from, port_to), id);
        }

        self.items.insert(id, item);
    }

    /// Get the item that has the specified id.
    pub fn get(&self, id: u32) -> Option<&Item> {
        self.items.get(&id)
    }

    /// Get the id of the link that links the two specified ports.
    pub fn get_link_id(&self, output_port: u32, input_port: u32) -> Option<u32> {
        self.links.get(&(output_port, input_port)).copied()
    }

    /// Remove the item with the specified id, returning it if it exists.
    pub fn remove(&mut self, id: u32) -> Option<Item> {
        let removed = self.items.remove(&id);

        match &removed {
            Some(Item::Link { port_from, port_to }) => {
                self.links.remove(&(*port_from, *port_to));
            }
            Some(Item::Node { .. }) => {
                self.node_device_info.remove(&id);
            }
            Some(Item::Device) => {
                // Remove all routes for this device
                self.device_routes.retain(|(dev_id, _), _| *dev_id != id);
            }
            _ => {}
        }

        removed
    }

    /// Convenience function: Get the id of the node a port is on
    pub fn get_node_of_port(&self, port: u32) -> Option<u32> {
        if let Some(Item::Port { node_id }) = self.get(port) {
            Some(*node_id)
        } else {
            None
        }
    }

    /// Set the device association for a node
    pub fn set_node_device_info(&mut self, node_id: u32, info: NodeDeviceInfo) {
        self.node_device_info.insert(node_id, info.clone());
        // Update the Item if it exists
        if let Some(Item::Node { device_info }) = self.items.get_mut(&node_id) {
            *device_info = Some(info);
        }
    }

    /// Get the device info for a node
    pub fn get_node_device_info(&self, node_id: u32) -> Option<&NodeDeviceInfo> {
        self.node_device_info.get(&node_id)
    }

    /// Set the route info for a device route
    pub fn set_device_route(&mut self, device_id: u32, route_device: i32, route_info: RouteInfo) {
        self.device_routes.insert((device_id, route_device), route_info);
    }

    /// Get route info for a node (via its device and card.profile.device)
    pub fn get_node_route_info(&self, node_id: u32) -> Option<(u32, &RouteInfo)> {
        let node_info = self.node_device_info.get(&node_id)?;
        let route_info = self.device_routes.get(&(node_info.device_id, node_info.card_profile_device))?;
        Some((node_info.device_id, route_info))
    }
}
