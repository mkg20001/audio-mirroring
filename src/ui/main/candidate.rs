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

use adw::{
    gdk,
    glib::{self, subclass::Signal},
    gtk::{self, graphene},
    prelude::*,
    subclass::prelude::*,
};
use crate::graph_manager::CandidateData;

#[derive(Copy, Clone)]
pub enum CandidateType {
    Application,
    Device,
}

impl CandidateType {
    pub fn as_raw(&self) -> u32 {
        match self {
            CandidateType::Application => 0,
            CandidateType::Device => 1,
        }
    }

    pub fn from_raw(raw: u32) -> Self {
        match raw {
            0 => CandidateType::Application,
            1 => CandidateType::Device,
            _ => CandidateType::Application,
        }
    }
}

mod imp {
use super::*;

    use std::cell::{Cell, OnceCell};

    use once_cell::sync::Lazy;
    use pipewire::spa::{param::format::MediaType, utils::Direction};

    /// Graphical representation of a pipewire port.
    #[derive(gtk::CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::Candidate)]
    #[template(file = "candidate.ui")]
    pub struct Candidate {
        #[property(get, set, construct_only)]
        pub(super) pipewire_id: OnceCell<u32>,
        #[property(
            type = u32,
            get = |this: &Self| this.kind.get().as_raw(),
            set = Self::set_kind
        )]
        pub(super) kind: Cell<CandidateType>,
        #[property(
            name = "name", type = String,
            get = |this: &Self| this.label.text().to_string(),
            set = |this: &Self, val| {
                this.label.set_text(val);
                this.label.set_tooltip_text(Some(val));
            }
        )]
        #[template_child]
        pub(super) label: TemplateChild<gtk::Label>,
    }

    impl Default for Candidate {
        fn default() -> Self {
            Self {
                pipewire_id: OnceCell::default(),
                kind: Cell::new(CandidateType::Application),
                label: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Candidate {
        const NAME: &'static str = "AudioSharingCandidate";
        type Type = super::Candidate;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("candidate");

            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Candidate {
        fn constructed(&self) {
            self.parent_constructed();
        }
    }

    impl WidgetImpl for Candidate {
    }

    impl Candidate {
        fn set_kind(&self, candidate: u32) {
            let candidate_type = CandidateType::from_raw(candidate);

            self.kind.set(candidate_type);

            /*for css_class in ["application", "device"] {
                self.handle.remove_css_class(css_class)
            }

            // Color the port according to its media type.
            match candidate_type {
                CandidateType::Application => self.handle.add_css_class("application"),
                CandidateType::Device => self.handle.add_css_class("device"),
                _ => {}
            }*/
        }
    }
}

glib::wrapper! {
    pub struct Candidate(ObjectSubclass<imp::Candidate>)
        @extends gtk::Widget;
}

impl Candidate {
    pub fn new(id: u32, kind: u32, label: String) -> Self {
        glib::Object::builder()
            .property("pipewire-id", id)
            .property("kind", kind)
            .property("name", label)
            .build()
    }
}

impl From<&CandidateData> for Candidate {
    fn from(data: &CandidateData) -> Self {
        Candidate::new(data.id, data.kind.as_raw(), data.label.clone())
    }
}
