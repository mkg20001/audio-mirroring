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
use adw::gtk::{DropDown, ListItemFactory, ListStore, SignalListItemFactory, StringList};
use pipewire::spa::utils::Direction;
use crate::ui::main::candidate::CandidateType;
use super::{Candidate, CandidateData, Port};

mod imp {
    use crate::ui::main::dropdown::glib::clone;
use crate::gtk::{ListItemFactory, Box, Image, Label, Orientation};
use super::*;

    use std::{
        cell::{Cell, RefCell},
        collections::HashSet,
    };
    use crate::ui::main::Candidate;

    #[derive(glib::Properties, gtk::CompositeTemplate, Default)]
    #[properties(wrapper_type = super::Dropdown)]
    #[template(file = "dropdown.ui")]
    pub struct Dropdown {
        #[property(get, set, construct_only)]
        pub(super) pipewire_id: Cell<u32>,

        pub(super) candidates: RefCell<Vec<CandidateData>>,

        #[template_child]
        pub(super) dropdown: TemplateChild<gtk::DropDown>,

        #[template_child]
        pub(super) select_mode: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) confirm_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub(super) use_mode: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) edit_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) volume_slider: TemplateChild<gtk::Scale>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Dropdown {
        const NAME: &'static str = "AudioSharingDropdown";
        type Type = super::Dropdown;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_layout_manager_type::<gtk::BoxLayout>();

            klass.bind_template();

            klass.set_css_name("dropdown");
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    #[glib::derived_properties]
    impl ObjectImpl for Dropdown {
        fn constructed(&self) {
            self.parent_constructed();

            self.use_mode.hide();

            self.confirm_btn.connect_clicked(clone!(@weak self as imp => move |_| {
                imp.select_mode.hide();
                imp.use_mode.show();
            }));

            /*let name_expr = gtk::PropertyExpression::new(StringList::static_type(), None, "string");
            let factory = gtk::SignalListItemFactory::new();

            factory.connect_setup(|_, item| {
                let label = gtk::Label::new(None);
                item.set_child(Some(&label));
            });

            factory.connect_bind(move |_, item| {
                let obj = item.item().unwrap();
                let label = item.child().unwrap().downcast::<gtk::Label>().unwrap();

                let value = name_expr.evaluate(Some(&obj)).unwrap();
                let name = value.get::<String>().unwrap();
                label.set_text(&name);
            });

            self.dropdown.set_factory(Some(&factory));*/

            // Create a factory for custom list items
            /*let factory = ListItemFactory::new();
            factory.connect_setup(move |_, list_item| {
                let hbox = Box::new(Orientation::Horizontal, 6);

                let icon = Image::new();
                icon.set_pixel_size(16);

                let label = Label::new(None);
                hbox.append(&icon);
                hbox.append(&label);

                list_item.set_child(Some(&hbox));
            });

            factory.connect_bind(move |_, list_item| {
                let item = list_item
                    .item()
                    .and_downcast::<glib::Boxed<Item>>()
                    .expect("Expected an Item");

                let hbox = list_item
                    .child()
                    .and_downcast::<Box>()
                    .expect("Expected GtkBox");

                let icon = hbox
                    .first_child()
                    .and_downcast::<Image>()
                    .expect("Expected Image");

                let label = hbox
                    .last_child()
                    .and_downcast::<Label>()
                    .expect("Expected Label");

                icon.set_icon_name(Some(&item.icon_name));
                label.set_label(&item.label);
            });*/

            // Create item data and store in ListStore
            /*let items = vec![
                Item {
                    label: "Home".to_string(),
                    icon_name: "go-home-symbolic".to_string(),
                },
                Item {
                    label: "Settings".to_string(),
                    icon_name: "preferences-system-symbolic".to_string(),
                },
                Item {
                    label: "Help".to_string(),
                    icon_name: "help-browser-symbolic".to_string(),
                },
            ];

            let model = ListStore::new(Item::static_type());
            for item in items {
                model.append(&glib::Object::new::<glib::Object>(&[
                    ("label", &item.label),
                    ("icon-name", &item.icon_name),
                ])
                    .unwrap());
            }

            // Create a factory
            let factory = SignalListItemFactory::new();

            factory.connect_setup(|_, list_item| {
                let hbox = Box::new(Orientation::Horizontal, 6);

                let image = Image::new();
                image.set_pixel_size(16);
                let label = Label::new(None);

                hbox.append(&image);
                hbox.append(&label);

                list_item.set_child(Some(&hbox));
            });

            factory.connect_bind(|_, list_item| {
                let item = list_item
                    .item()
                    .and_downcast_ref::<Object>()
                    .expect("Item should be a glib::Object");

                let label_text = item.property::<String>("label");
                let icon_name = item.property::<String>("icon-name");

                if let Some(hbox) = list_item.child().and_downcast::<Box>() {
                    if let Some(image) = hbox.first_child().and_downcast::<Image>() {
                        image.set_icon_name(Some(&icon_name));
                    }
                    if let Some(label) = hbox.last_child().and_downcast::<Label>() {
                        label.set_label(&label_text);
                    }
                }
            });*/

            // self.dropdown.set_factory(Some(&factory));
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

        let model = ListStore::new(&[CandidateData::static_type()]);

        candidates_ref.iter().for_each(|f| {
            let iter = model.append();
            model.set(&iter, &[(0, f)]);
        });

        let factory = SignalListItemFactory::new();

        factory.connect_setup(move |_factory, list_item| {
            /*let candidate = Candidate::new(0, 0, "".into());
            list_item.set_child(Some(&candidate));*/
        });

        factory.connect_bind(move |_factory, list_item| {
            let item = list_item
                .item()
                .and_downcast::<CandidateData>()
                .expect("Expected CandidateData");
            list_item.set_child(Some(&Candidate::from(&item)));
            /*let item = list_item
                .item()
                .and_downcast::<Candidate>()
                .expect("Expected Candidate");

            list_item.set_child(Some(&item));*/
            /*let candidate = list_item.child().unwrap().downcast::<Candidate>().unwrap();
            let item = list_item.item().unwrap().downcast::<CandidateData>().unwrap();

            // Update candidate with new data
            candidate.set_property("pipewire-id", &item.id).unwrap();
            candidate.set_property("kind", &item.kind).unwrap();
            candidate.set_property("label", &item.label).unwrap();*/
            /*if let Some(item) = list_item.item().and_downcast::<Candidate>() {
                list_item.set_child(Some(&item));
            } else {
                log::warn!("did not work dropdown");
            }*/
            //list_item.child().unwrap().downcast::<Candidate>().unwrap();
        });

        // imp.dropdown.set_model(Some(&model));
        imp.dropdown.set_factory(Some(&factory));


        /*let labels: Vec<String> = candidates_ref.iter()
            .map(|c| c.label.clone() + match c.kind { // TODO: use icons - application=window, device=speaker
                CandidateType::Device => " (device)",
                CandidateType::Application => " (application)",
            })
            .collect();

        let string_list = StringList::new(&labels.iter()
            .map(|s| s.as_str())
            .collect::<Vec<&str>>());

        imp.dropdown.set_model(Some(&string_list));*/
    }
}
