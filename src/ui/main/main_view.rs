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

use crate::ui::main::{CandidateData, Dropdown, Node, Port};
use crate::NodeType;
use adw::{glib, gtk, prelude::*, subclass::prelude::*};
use std::borrow::BorrowMut;
use std::collections::HashSet;
use std::error::Error;

mod imp {
    use super::*;

    use crate::NodeType;
    use glib::clone;
    use glib::subclass::Signal;
    use glib::{List, Value};
    use once_cell::sync::Lazy;
    use std::cell::{Cell, OnceCell, RefCell};
    use std::collections::HashSet;

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

        #[template_child]
        pub targets_container: TemplateChild<gtk::Box>,
        #[template_child]
        pub add_target_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub status_icon: TemplateChild<gtk::Image>,

        pub target_candidates: RefCell<Vec<CandidateData>>,
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
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![Signal::builder("target-dropdown-added")
                    .param_types([Dropdown::static_type()])
                    .build()]
            });
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.add_target_btn
                .connect_clicked(clone!(@weak self as imp => move |_| {
                    imp.obj().add_target_dropdown();
                }));
        }

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for MainView {}

    impl MainView {
        pub fn add_node(&self, node: Node, node_type: Option<NodeType>) {}
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

    pub fn update_candidates(&self, source: Vec<CandidateData>, target: Vec<CandidateData>) {
        let imp = self.imp();
        imp.source_dd.update_candidates(source);

        // Store target candidates for new dropdowns
        imp.target_candidates.replace(target.clone());

        // Update all target dropdowns
        let container = &*imp.targets_container;
        let mut child = container.first_child();
        while let Some(widget) = child {
            if let Some(dropdown) = widget.downcast_ref::<Dropdown>() {
                dropdown.update_candidates(target.clone());
            }
            child = widget.next_sibling();
        }
    }

    pub fn add_target_dropdown(&self) {
        let imp = self.imp();
        let dropdown = Dropdown::new();
        dropdown.set_hexpand(true);

        // Populate with current candidates
        let candidates = imp.target_candidates.borrow().clone();
        dropdown.update_candidates(candidates);

        imp.targets_container.append(&dropdown);

        // Emit signal so GraphManager can connect to the new dropdown
        self.emit_by_name::<()>("target-dropdown-added", &[&dropdown]);
    }

    pub fn connect_target_dropdown_added<F: Fn(&Self, &Dropdown) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_closure(
            "target-dropdown-added",
            false,
            glib::closure_local!(move |main_view: &MainView, dropdown: &Dropdown| {
                f(main_view, dropdown);
            }),
        )
    }

    pub fn set_status(&self, mirroring: bool, device_count: u32, last_event: Option<&str>) {
        let imp = self.imp();
        if mirroring {
            imp.status_icon.set_icon_name(Some("media-playback-start-symbolic"));
            let status = match last_event {
                Some(event) => format!("Mirroring to {} device(s) - {}", device_count, event),
                None => format!("Mirroring to {} device(s)", device_count),
            };
            imp.status_label.set_text(&status);
        } else {
            imp.status_icon.set_icon_name(Some("media-playback-stop-symbolic"));
            imp.status_label.set_text("Not mirroring");
        }
    }
}
