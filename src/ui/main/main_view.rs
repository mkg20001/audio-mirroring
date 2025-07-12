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

use std::collections::HashSet;
use adw::{glib, gtk, prelude::*, subclass::prelude::*};
use pipewire::spa::utils::Direction;
use crate::NodeType;
use crate::ui::main::{Node, Port, Dropdown};

mod imp {
    use super::*;

    use std::{
        collections::HashSet,
    };
    use std::cell::{Cell, OnceCell, RefCell};
    use glib::{List, Value};
    use crate::NodeType;

    #[derive(glib::Properties, gtk::CompositeTemplate, Default)]
    #[properties(wrapper_type = super::MainView)]
    #[template(file = "mainview.ui")]
    pub struct MainView {
        #[property(get, set, construct_only)]
        pub(super) pipewire_id: Cell<u32>,
        /// Stores nodes and their positions.
        pub(super) nodes: RefCell<HashSet<Node>>,
        pub(super) ports: RefCell<HashSet<Port>>,
        pub(super) source: Cell<u32>,
        // TODO: make list
        pub(super) targets: Cell<u32>,

        #[template_child]
        #[property(type = super::Dropdown, get = |_| self.source_dd.clone())]
        pub source_dd: TemplateChild<Dropdown>,

        #[template_child]
        #[property(type = super::Dropdown, get = |_| self.target_dd.clone())]
        pub target_dd: TemplateChild<Dropdown>,

        /*#[property(get, set, construct_only)]
        pub(super) pipewire_id: Cell<u32>,
        #[property(
            name = "node-name", type = String,
            get = |this: &Self| this.node_name.text().to_string(),
            set = |this: &Self, val| {
                this.node_name.set_text(val);
                this.node_name.set_tooltip_text(Some(val));
            }
        )]
        #[template_child]
        pub(super) node_name: TemplateChild<gtk::Label>,
        #[property(
            name = "media-name", type = String,
            get = |this: &Self| this.media_name.text().to_string(),
            set = |this: &Self, val| {
                this.media_name.set_text(val);
                this.media_name.set_tooltip_text(Some(val));
                this.media_name.set_visible(!val.is_empty());
            }
        )]
        #[template_child]
        pub(super) media_name: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) separator: TemplateChild<gtk::Separator>,
        #[template_child]
        pub(super) port_grid: TemplateChild<gtk::Grid>,
        pub(super) ports: RefCell<HashSet<Port>>,*/
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MainView {
        const NAME: &'static str = "AudioSharingMainView";
        type Type = super::MainView;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BoxLayout>();

            klass.bind_template();

            klass.set_css_name("mainview");
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for MainView {
        fn constructed(&self) {
            self.parent_constructed();
        }

        fn dispose(&self) {
            if let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for MainView {}

    impl MainView {
        pub fn add_node(&self, node: Node, node_type: Option<NodeType>) {

        }
    }
}

glib::wrapper! {
    pub struct MainView(ObjectSubclass<imp::MainView>)
        @extends gtk::Widget;
}

impl MainView {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn add_node(&self, node: Node, node_type: Option<NodeType>) {
        let imp = self.imp();
        let nodes = imp.nodes.borrow_mut();
        // nodes.add(node);
    }
}
