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
use pipewire::spa::param::format::MediaType;

use super::{CandidateType, Port};
use glib::prelude::*;

mod imp {
    use super::*;

    use std::cell::Cell;

    use once_cell::sync::Lazy;

    pub struct CandidateData {
        pub id: Cell<u32>,
        pub kind: Cell<CandidateType>,
        pub label: Cell<String>,
    }

    impl Default for CandidateData {
        fn default() -> Self {
            Self {
                id: Cell::default(),
                kind: Cell::new(CandidateType::Application),
                label: Cell::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CandidateData {
        const NAME: &'static str = "AudioSharingCandidateData";
        type Type = super::CandidateData;
        type ParentType = glib::Object;
    }

    impl ObjectImpl for CandidateData {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecUInt::builder("id")
                        .flags(glib::ParamFlags::READWRITE)
                        .build(),
                    glib::ParamSpecEnum::builder::<CandidateType>("kind")
                        .flags(glib::ParamFlags::READWRITE)
                        .build(),
                    glib::ParamSpecString::builder("label")
                        .flags(glib::ParamFlags::READWRITE)
                        .build(),
                ]
            });

            PROPERTIES.as_ref()
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "id" => self.id.get().to_value(),
                "kind" => self.kind.get().as_raw().to_value(),
                "label" => self.label.get().to_value(),
                _ => unimplemented!(),
            }
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "id" => self.id.set(value.get().unwrap()),
                "kind" => self.kind.set(value.get().unwrap()),
                "label" => self.label.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub struct CandidateData(ObjectSubclass<imp::CandidateData>);
}

impl CandidateData {
    pub fn new(id: u32, kind: CandidateType, label: String) -> Self {
        glib::Object::builder()
            .property("id", id)
            .property("kind", kind.as_raw())
            .property("label", label)
            .build()
    }

    pub fn id(&self) -> Option<u32> {
        self.property("id")
    }

    pub fn set_id(&self, id: Option<&u32>) {
        self.set_property("id", id);
    }

    pub fn kind(&self) -> Option<CandidateType> {
        self.property("kind")
    }

    pub fn set_kind(&self, kind: Option<&CandidateType>) {
        self.set_property("kind", kind);
    }

    pub fn label(&self) -> Option<String> {
        self.property("label")
    }

    pub fn set_label(&self, label: Option<&String>) {
        self.set_property("label", label);
    }
}

impl Default for CandidateData {
    fn default() -> Self {
        Self::new(
            0,
            CandidateType::Application,
            "".to_string()
        )
    }
}
