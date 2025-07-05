use adw::{
    gio,
    glib::{self, clone},
    gtk::{
        self, cairo,
        graphene::{self, Point},
        gsk,
    },
    prelude::*,
    subclass::prelude::*,
};

use std::cmp::Ordering;

//use super::{Link, Node, Port};
use crate::NodeType;

mod imp {
    use super::*;

    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};

    use adw::gtk::gdk::{self};
    use log::warn;
    use once_cell::sync::Lazy;
    use pipewire::spa::param::format::MediaType;
    use pipewire::spa::utils::Direction;

    pub struct Colors;
    pub struct DragState;

    pub struct MainView;

    impl Default for MainView {
        fn default() -> Self {
            Self {}
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MainView {
        const NAME: &'static str = "AudioSharingMainView";
        type Type = super::MainView;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Scrollable,);

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("mainview");
        }
    }

    impl ObjectImpl for MainView {
        fn constructed(&self) {}
        fn dispose(&self) {}
        fn properties() -> &'static [glib::ParamSpec] {
            unimplemented!()
        }
        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            unimplemented!()
        }
        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {}
    }

    impl WidgetImpl for MainView {
        fn size_allocate(&self, _width: i32, _height: i32, baseline: i32) {}
        fn snapshot(&self, snapshot: &gtk::Snapshot) {}
    }

    impl ScrollableImpl for MainView {}

    impl MainView {

    }
}

glib::wrapper! {
    pub struct MainView(ObjectSubclass<imp::MainView>)
        @extends gtk::Widget;
}

impl MainView {
    pub fn new() -> Self {
        unimplemented!()
    }
}

impl Default for MainView {
    fn default() -> Self {
        unimplemented!()
    }
}