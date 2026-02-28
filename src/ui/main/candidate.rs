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

use crate::ui::main::CandidateData;
use adw::{
    gdk,
    glib::{self, subclass::Signal},
    gtk::{self, graphene},
    prelude::*,
    subclass::prelude::*,
};
use glib::property::PropertyGet;

#[derive(Clone, Copy, Debug, glib::Enum)]
#[enum_type(name = "CandidateType")]
#[repr(u32)]
pub enum CandidateType {
    Application,
    Device,
}

impl Default for CandidateType {
    fn default() -> Self {
        CandidateType::Application
    }
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

    use once_cell::sync::Lazy;
    use pipewire::spa::{param::format::MediaType, utils::Direction};
    use std::cell::{Cell, OnceCell};

    /// Graphical representation of a pipewire port.
    #[derive(gtk::CompositeTemplate, glib::Properties)]
    #[properties(wrapper_type = super::Candidate)]
    #[template(file = "candidate.ui")]
    pub struct Candidate {
        #[property(name = "pipewire-id", get, set)]
        pub(super) pipewire_id: Cell<u32>,
        #[property(
            type = u32,
            get = |_| self.kind.get().as_raw(),
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
        #[template_child]
        pub(super) icon: TemplateChild<gtk::Image>,
        #[property(
            name = "disabled", type = bool,
            get = |this: &Self| this.disabled.get(),
            set = Self::set_disabled
        )]
        pub(super) disabled: Cell<bool>,
    }

    impl Default for Candidate {
        fn default() -> Self {
            Self {
                pipewire_id: Cell::default(),
                kind: Cell::new(CandidateType::Application),
                label: TemplateChild::default(),
                icon: TemplateChild::default(),
                disabled: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Candidate {
        const NAME: &'static str = "AudioMirroringCandidate";
        type Type = super::Candidate;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BoxLayout>();
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

        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Candidate {}

    impl Candidate {
        fn set_kind(&self, candidate: u32) {
            let kind = CandidateType::from_raw(candidate);
            self.kind.set(kind);

            // Update icon based on candidate type
            let icon_name = match kind {
                CandidateType::Application => "application-x-executable-symbolic",
                CandidateType::Device => "audio-card-symbolic",
            };
            self.icon.set_icon_name(Some(icon_name));
        }

        fn set_disabled(&self, disabled: bool) {
            self.disabled.set(disabled);
            // Apply visual styling for disabled state
            if disabled {
                self.obj().add_css_class("dim-label");
                self.label.add_css_class("dim-label");
                self.icon.set_opacity(0.5);
            } else {
                self.obj().remove_css_class("dim-label");
                self.label.remove_css_class("dim-label");
                self.icon.set_opacity(1.0);
            }
        }
    }
}

glib::wrapper! {
    pub struct Candidate(ObjectSubclass<imp::Candidate>)
        @extends gtk::Widget;
}

impl Candidate {
    pub fn new(id: u32, kind: CandidateType, label: String) -> Self {
        glib::Object::builder()
            .property("pipewire-id", id)
            .property("kind", kind.as_raw())
            .property("name", label)
            .build()
    }
}

impl From<&CandidateData> for Candidate {
    fn from(data: &CandidateData) -> Self {
        Candidate::new(data.id(), data.kind(), data.label())
    }
}
