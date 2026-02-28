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
use std::collections::HashSet;

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
        rc::Rc,
    };

    #[derive(glib::Properties, gtk::CompositeTemplate)]
    #[properties(wrapper_type = super::Dropdown)]
    #[template(file = "dropdown.ui")]
    pub struct Dropdown {
        #[property(get, set, construct_only)]
        pub(super) pipewire_id: Cell<u32>,

        pub(super) candidates: RefCell<Vec<CandidateData>>,
        pub(super) confirmed_target_id: Cell<Option<u32>>,
        pub(super) updating_volume: Cell<bool>,
        pub(super) updating_mute: Cell<bool>,
        pub(super) disabled_ids: Rc<RefCell<HashSet<u32>>>,

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
        #[template_child]
        pub(super) mute_btn: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub(super) mute_icon: TemplateChild<gtk::Image>,
    }

    impl Default for Dropdown {
        fn default() -> Self {
            Self {
                pipewire_id: Cell::default(),
                candidates: RefCell::default(),
                confirmed_target_id: Cell::default(),
                updating_volume: Cell::default(),
                updating_mute: Cell::default(),
                disabled_ids: Rc::new(RefCell::new(HashSet::new())),
                dropdown: TemplateChild::default(),
                select_mode: TemplateChild::default(),
                confirm_btn: TemplateChild::default(),
                remove_btn: TemplateChild::default(),
                use_mode: TemplateChild::default(),
                selected_icon: TemplateChild::default(),
                selected_label: TemplateChild::default(),
                edit_btn: TemplateChild::default(),
                volume_slider: TemplateChild::default(),
                mute_btn: TemplateChild::default(),
                mute_icon: TemplateChild::default(),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Dropdown {
        const NAME: &'static str = "AudioMirroringDropdown";
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
                    Signal::builder("mute-changed")
                        .param_types([u32::static_type(), bool::static_type()])
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
                        // Don't allow confirming disabled items
                        if selected.disabled() {
                            return;
                        }

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

            // Update confirm button sensitivity when selection changes
            self.dropdown.connect_selected_notify(clone!(@weak self as imp => move |dropdown| {
                let selected = dropdown.selected();
                if selected == gtk::INVALID_LIST_POSITION {
                    imp.confirm_btn.set_sensitive(false);
                    return;
                }
                let candidates = imp.candidates.borrow();
                if let Some(candidate) = candidates.get(selected as usize) {
                    imp.confirm_btn.set_sensitive(!candidate.disabled());
                } else {
                    imp.confirm_btn.set_sensitive(false);
                }
            }));

            self.edit_btn
                .connect_clicked(clone!(@weak self as imp => move |_| {
                    // Clear confirmed ID first, then emit signal so update_disabled_states works correctly
                    let target_id = imp.confirmed_target_id.get();
                    imp.confirmed_target_id.set(None);
                    imp.use_mode.hide();
                    imp.select_mode.show();
                    if let Some(id) = target_id {
                        imp.obj().emit_by_name::<()>("selection-cancelled", &[&id]);
                    }
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

            self.mute_btn
                .connect_toggled(clone!(@weak self as imp => move |btn| {
                    // Don't emit signal if we're updating programmatically
                    if imp.updating_mute.get() {
                        return;
                    }
                    let muted = btn.is_active();
                    // Update icon based on mute state
                    let icon_name = if muted {
                        "audio-volume-muted-symbolic"
                    } else {
                        "audio-volume-high-symbolic"
                    };
                    imp.mute_icon.set_icon_name(Some(icon_name));

                    if let Some(node_id) = imp.confirmed_target_id.get() {
                        imp.obj().emit_by_name::<()>("mute-changed", &[&node_id, &muted]);
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
        self.imp().remove_btn.set_visible(removable);
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

    pub fn set_muted(&self, muted: bool) {
        let imp = self.imp();
        // Set flag to prevent feedback loop
        imp.updating_mute.set(true);
        imp.mute_btn.set_active(muted);
        // Update icon based on mute state
        let icon_name = if muted {
            "audio-volume-muted-symbolic"
        } else {
            "audio-volume-high-symbolic"
        };
        imp.mute_icon.set_icon_name(Some(icon_name));
        imp.updating_mute.set(false);
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

    pub fn connect_mute_changed<F: Fn(&Self, u32, bool) + 'static>(&self, f: F) -> glib::SignalHandlerId {
        self.connect_closure(
            "mute-changed",
            false,
            glib::closure_local!(move |dropdown: &Dropdown, node_id: u32, muted: bool| {
                f(dropdown, node_id, muted);
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

        // Capture disabled_ids Rc for use in bind closure
        let disabled_ids = imp.disabled_ids.clone();
        factory.connect_bind(move |_factory, list_item| {
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
            // Check disabled state from the shared disabled_ids set
            let is_disabled = disabled_ids.borrow().contains(&item.id());
            candidate.set_property("disabled", &is_disabled);
        });

        imp.dropdown.set_factory(Some(&factory));
        imp.dropdown.set_model(Some(&model));
    }

    pub fn set_disabled_ids(&self, ids: HashSet<u32>) {
        let imp = self.imp();
        let selected = imp.dropdown.selected();
        imp.disabled_ids.replace(ids);

        // Force refresh by rebuilding the model
        let candidates_ref = imp.candidates.borrow();
        let model = ListStore::new::<CandidateData>();
        for candidate in candidates_ref.iter() {
            model.append(candidate);
        }
        imp.dropdown.set_model(Some(&model));

        // Restore selection
        if selected != gtk::INVALID_LIST_POSITION {
            imp.dropdown.set_selected(selected);
        }

        // Update confirm button sensitivity based on current selection
        if selected != gtk::INVALID_LIST_POSITION {
            if let Some(candidate) = candidates_ref.get(selected as usize) {
                let is_disabled = imp.disabled_ids.borrow().contains(&candidate.id());
                imp.confirm_btn.set_sensitive(!is_disabled);
            }
        }
    }
}
