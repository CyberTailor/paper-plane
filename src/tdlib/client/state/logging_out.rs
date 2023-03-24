use glib::subclass::prelude::*;
use gtk::glib;
use tdlib::enums::Update;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub(crate) struct LoggingOut;

    #[glib::object_subclass]
    impl ObjectSubclass for LoggingOut {
        const NAME: &'static str = "ClientLoggingOut";
        type Type = super::LoggingOut;
    }

    impl ObjectImpl for LoggingOut {}
}

glib::wrapper! {
    pub(crate) struct LoggingOut(ObjectSubclass<imp::LoggingOut>);
}

impl Default for LoggingOut {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

impl LoggingOut {
    pub(crate) fn handle_update(&self, update: Update) {}
}
