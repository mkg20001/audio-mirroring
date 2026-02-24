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

use super::{Candidate, CandidateData};
use crate::ui::main::candidate::CandidateType;
use adw::gio::ListStore;
use adw::gtk::SignalListItemFactory;
use adw::{glib, gtk, prelude::*, subclass::prelude::*};

mod imp {
    use super::*;
    use crate::gtk::{Box, Image, Label, ListItemFactory, Orientation};
    use crate::ui::main::dropdown::glib::clone;
    use glib::subclass::Signal;
    use once_cell::sync::Lazy;

    use crate::ui::main::Candidate;
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

        pub(super) candidates: RefCell<Vec<CandidateData>>,
        pub(super) confirmed_target_id: Cell<Option<u32>>,
        pub(super) updating_volume: Cell<bool>,

        #[template_child]
        pub(super) dropdown: TemplateChild<gtk::DropDown>,

        #[template_child]
        pub(super) select_mode: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) confirm_btn: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) remove_btn: TemplateChild<gtk::Button>,

        #[template_child]
        pub(super) use_mode: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) selected_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) selected_label: TemplateChild<gtk::Label>,
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
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("selection-confirmed")
                        .param_types([u32::static_type(), u32::static_type()])
                        .build(),
                    Signal::builder("remove-requested")
                        .param_types([u32::static_type()])
                        .build(),
                    Signal::builder("volume-changed")
                        .param_types([u32::static_type(), f64::static_type()])
                        .build(),
                    Signal::builder("selection-cancelled")
                        .param_types([u32::static_type()])
                        .build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.use_mode.hide();

            self.confirm_btn
                .connect_clicked(clone!(@weak self as imp => move |_| {
                    // Update selected label and icon before switching modes
                    if let Some(selected) = imp.obj().selected_candidate() {
                        let id = selected.id();
                        let kind = selected.kind();
                        let label = selected.label();

                        imp.selected_label.set_text(&label);
                        imp.selected_label.set_tooltip_text(Some(&label));

                        let icon_name = match kind {
                            CandidateType::Application => "application-x-executable-symbolic",
                            CandidateType::Device => "audio-card-symbolic",
                        };
                        imp.selected_icon.set_icon_name(Some(icon_name));

                        imp.confirmed_target_id.set(Some(id));

                        imp.select_mode.hide();
                        imp.use_mode.show();

                        imp.obj().emit_by_name::<()>(
                            "selection-confirmed",
                            &[&id, &kind.as_raw()],
                        );
                    }
                }));

            self.edit_btn
                .connect_clicked(clone!(@weak self as imp => move |_| {
                    // Emit cancellation signal before switching modes
                    if let Some(target_id) = imp.confirmed_target_id.get() {
                        imp.obj().emit_by_name::<()>("selection-cancelled", &[&target_id]);
                    }
                    imp.confirmed_target_id.set(None);
                    imp.use_mode.hide();
                    imp.select_mode.show();
                }));

            self.remove_btn
                .connect_clicked(clone!(@weak self as imp => move |_| {
                    let target_id = imp.confirmed_target_id.get().unwrap_or(0);
                    imp.obj().emit_by_name::<()>("remove-requested", &[&target_id]);
                }));

            self.volume_slider
                .connect_value_changed(clone!(@weak self as imp => move |scale| {
                    // Don't emit signal if we're updating programmatically
                    if imp.updating_volume.get() {
                        return;
                    }
                    if let Some(node_id) = imp.confirmed_target_id.get() {
                        let volume = scale.value() / 100.0; // Convert 0-100 to 0.0-1.0
                        imp.obj().emit_by_name::<()>("volume-changed", &[&node_id, &volume]);
                    }
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

    impl Dropdown {}
}

glib::wrapper! {
    pub struct Dropdown(ObjectSubclass<imp::Dropdown>)
        @extends gtk::Widget;
}

impl Dropdown {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn selected_candidate(&self) -> Option<CandidateData> {
        let imp = self.imp();
        let selected = imp.dropdown.selected();
        if selected == gtk::INVALID_LIST_POSITION {
            return None;
        }
        imp.candidates.borrow().get(selected as usize).cloned()
    }

    pub fn connect_selection_confirmed<F: Fn(&Self, u32, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "selection-confirmed",
            false,
            glib::closure_local!(move |dropdown: &Dropdown, id: u32, kind: u32| {
                f(dropdown, id, kind);
            }),
        )
    }

    pub fn connect_remove_requested<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "remove-requested",
            false,
            glib::closure_local!(move |dropdown: &Dropdown, target_id: u32| {
                f(dropdown, target_id);
            }),
        )
    }

    pub fn set_removable(&self, removable: bool) {
        let btn = &self.imp().remove_btn;
        btn.set_opacity(if removable { 1.0 } else { 0.0 });
        btn.set_sensitive(removable);
    }

    pub fn confirmed_node_id(&self) -> Option<u32> {
        self.imp().confirmed_target_id.get()
    }

    pub fn set_volume(&self, volume: f32) {
        let imp = self.imp();
        // Set flag to prevent feedback loop
        imp.updating_volume.set(true);
        // Convert 0.0-1.0 to 0-100 for the slider
        let slider_value = (volume * 100.0).clamp(0.0, 100.0) as f64;
        imp.volume_slider.set_value(slider_value);
        imp.updating_volume.set(false);
    }

    pub fn connect_volume_changed<F: Fn(&Self, u32, f64) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "volume-changed",
            false,
            glib::closure_local!(move |dropdown: &Dropdown, node_id: u32, volume: f64| {
                f(dropdown, node_id, volume);
            }),
        )
    }

    pub fn connect_selection_cancelled<F: Fn(&Self, u32) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "selection-cancelled",
            false,
            glib::closure_local!(move |dropdown: &Dropdown, target_id: u32| {
                f(dropdown, target_id);
            }),
        )
    }

    pub fn update_candidates(&self, c: Vec<CandidateData>) {
        let imp = self.imp();
        imp.candidates.replace(c);
        // Create a list of strings
        let candidates_ref = imp.candidates.borrow();

        let model = ListStore::new::<CandidateData>();

        candidates_ref.iter().for_each(|f| {
            model.append(f);
        });

        let factory = SignalListItemFactory::new();

        factory.connect_setup(move |_factory, list_item| {
            let candidate = Candidate::new(0, CandidateType::Application, "".into());
            list_item.set_child(Some(&candidate));
        });

        factory.connect_bind(move |_factory, list_item| {
            /*let item = list_item
                .item()
                .and_downcast::<CandidateData>()
                .expect("Expected CandidateData");
            list_item.set_child(Some(&Candidate::from(&item)));*/
            /*let item = list_item
                .item()
                .and_downcast::<CandidateData>()
                .expect("Expected CandidateData");

            let candidate = Candidate::from(&item);
            list_item.set_child(Some(&candidate));*/
            let candidate = list_item.child().unwrap().downcast::<Candidate>().unwrap();
            let item = list_item
                .item()
                .unwrap()
                .downcast::<CandidateData>()
                .unwrap();

            // Update candidate with new data
            candidate.set_property("pipewire-id", &item.id());
            candidate.set_property("kind", &item.kind().as_raw());
            candidate.set_property("name", &item.label());
            /*if let Some(item) = list_item.item().and_downcast::<Candidate>() {
                list_item.set_child(Some(&item));
            } else {
                log::warn!("did not work dropdown");
            }*/
            //list_item.child().unwrap().downcast::<Candidate>().unwrap();
        });

        imp.dropdown.set_factory(Some(&factory));
        imp.dropdown.set_model(Some(&model));

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
