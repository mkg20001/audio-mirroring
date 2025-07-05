use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, LinkedList};
use std::hash::{Hash, Hasher};
use pipewire::spa::param::format::MediaType;
use pipewire::spa::utils::Direction;

pub struct Node {
    id: u32,
    name: String,
    media_name: String,
    ports: HashMap<u32, Port>
}

impl Node {
    pub fn new(name: &str, id: u32) -> Node {
        Node {
            id,
            name: name.to_string(),
            media_name: String::new(),
            ports: HashMap::new(),
        }
    }
    pub fn get_id(&self) -> u32 { self.id }
    pub fn get_name(&self) -> String { self.name.clone() }

    // pub fn get_ports(&self) -> HashMap<u32, Port> { self.ports. }
    pub fn get_port(&self, port_id: u32) -> Option<&Port> { self.ports.get(&port_id) }
    pub fn get_port_mut(&mut self, port_id: u32) -> Option<&mut Port> { self.ports.get_mut(&port_id) }
    pub fn get_port_by_label(&self, label: &str) -> Option<&Port> {
        self.ports.iter().find(|(_, port)| port.get_label() == label).map(|(_, port)| port)
    }
    pub fn has_port_by_label(&self, label: &str) -> bool {
        self.get_port_by_label(label).is_some()
    }
    pub fn get_port_labels(&self) -> Vec<String> {
        self.ports.iter().map(|(_, port)| port.get_label()).collect()
    }

    pub fn set_name(&mut self, name: &str) { self.name = name.to_string() }
    pub fn set_media_name(&mut self, media_name: &str) { self.media_name = media_name.to_string() }

    pub fn add_port(&mut self, port: Port) { self.ports.insert(port.id, port); }
    pub fn remove_port(&mut self, port_id: u32) {
        self.ports.remove(&port_id);
    }
}

pub struct Port {
    id: u32,
    media_type: MediaType,
    direction: Direction,
    label: String,
    links: HashSet<Link>,
}

impl Port {
    pub fn new(label: &str, id: u32, direction: Direction) -> Port {
        Port {
            id,
            label: label.to_string(),
            direction: direction,
            links: HashSet::new(),
            media_type: MediaType::Unknown,
        }
    }
    pub fn get_id(&self) -> u32 { self.id }
    pub fn get_media_type(&self) -> MediaType { self.media_type }
    pub fn get_direction(&self) -> Direction { self.direction }
    pub fn get_label(&self) -> String { self.label.clone() }
    // pub fn get_links(&self) -> HashSet<Link> { self.links.clone() }

    pub fn set_media_type(&mut self, media_type: MediaType) { self.media_type = media_type }
    pub fn set_direction(&mut self, direction: Direction) { self.direction = direction }
    pub fn set_label(&mut self, label: String) { self.label = label }

    /* pub fn add_link(&mut self, link: Link) { self.links.insert(link.hash(), link); }
    pub fn remove_link(&mut self, link_id: Link) { self.links.remove(&link_id); }*/
}

pub struct Link {
    from: u32,
    to: u32,
    media_type: MediaType,
    active: bool,
}

impl Hash for Node {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Hash for Port {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Hash for Link {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.from.hash(state);
        self.to.hash(state);
    }
}