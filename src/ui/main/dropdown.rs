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

use adw::{glib, gtk, prelude::*, subclass::prelude::*};
use adw::gtk::{ListStore, StringList};
use pipewire::spa::utils::Direction;
use crate::graph_manager::Candidate;
use super::Port;

mod imp {
    use super::*;

    use std::{
        cell::{Cell, RefCell},
        collections::HashSet,
    };

    #[derive(glib::Properties, gtk::CompositeTemplate, Default)]
    #[properties(wrapper_type = super::Dropdown)]
    #[template(file = "dropdown.ui")]
    pub struct Dropdown {
        #[property(get, set, construct_only)]
        pub(super) pipewire_id: Cell<u32>,

        pub(super) candidates: RefCell<Vec<Candidate>>,

        #[template_child]
        pub(super) dropdown: TemplateChild<gtk::DropDown>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Dropdown {
        const NAME: &'static str = "AudioSharingDropdown";
        type Type = super::Dropdown;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BoxLayout>();

            klass.bind_template();

            klass.set_css_name("Dropdown");
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Dropdown {
        fn constructed(&self) {
            self.parent_constructed();
        }

        fn dispose(&self) {
            if let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Dropdown {}

    impl Dropdown {
    }
}

glib::wrapper! {
    pub struct Dropdown(ObjectSubclass<imp::Dropdown>)
        @extends gtk::Widget;
}

impl Dropdown {
    pub fn new(/*name: &str, pipewire_id: u32*/) -> Self {
        glib::Object::builder()
            /*.property("node-name", name)
            .property("pipewire-id", pipewire_id)*/
            .build()
    }

    pub fn update_candidates(&self, c: Vec<Candidate>) {
        let imp = self.imp();
        imp.candidates.replace(c);
        // Create a list of strings
        let candidates_ref = imp.candidates.borrow();

        let labels: Vec<String> = candidates_ref.iter()
            .map(|c| c.label.clone())
            .collect();

        let string_list = StringList::new(&labels.iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>());

        imp.dropdown.set_model(Some(&string_list));
    }
}
