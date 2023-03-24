mod contacts_window;
mod content;
mod preferences_window;
mod sidebar;

use adw::subclass::prelude::BinImpl;
use glib::clone;
use gtk::glib;
use gtk::glib::WeakRef;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use once_cell::sync::Lazy;
use tdlib::enums;
use tdlib::functions;

use self::contacts_window::ContactsWindow;
use self::content::Content;
use self::preferences_window::PreferencesWindow;
use self::sidebar::Sidebar;
use crate::tdlib as model;
use crate::tdlib::ClientSession;
use crate::utils::spawn;

// TODO: Vind it to
mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/app/drey/paper-plane/ui/session.ui")]
    pub(crate) struct Session {
        pub(super) model: WeakRef<model::ClientSession>,
        #[template_child]
        pub(super) leaflet: TemplateChild<adw::Leaflet>,
        #[template_child]
        pub(super) sidebar: TemplateChild<Sidebar>,
        #[template_child]
        pub(super) content: TemplateChild<Content>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Session {
        const NAME: &'static str = "Session";
        type Type = super::Session;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("content.go-back", None, move |widget, _, _| {
                widget
                    .imp()
                    .leaflet
                    .navigate(adw::NavigationDirection::Back);
            });
            klass.install_action_async("session.new", None, |widget, _, _| async move {
                widget
                    .model()
                    .unwrap()
                    .client()
                    .manager()
                    .unwrap()
                    .add_new_session(true);
            });
            klass.install_action_async("session.log-out", None, |widget, _, _| async move {
                // log_out(widget.model().unwrap().client().id()).await;
            });
            klass.install_action("session.show-preferences", None, move |widget, _, _| {
                let parent_window = widget.root().and_then(|r| r.downcast().ok());
                let preferences = PreferencesWindow::new(parent_window.as_ref(), widget);
                preferences.present();
            });
            klass.install_action("session.show-contacts", None, move |widget, _, _| {
                let parent = widget.root().and_then(|r| r.downcast().ok());
                let contacts = ContactsWindow::new(parent.as_ref(), widget.clone());

                contacts.connect_contact_activated(clone!(@weak widget => move |_, user_id| {
                    widget.select_chat(user_id);
                }));

                contacts.present();
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Session {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<model::ClientSession>("model")
                        .construct_only()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "model" => self.model.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "model" => self.obj().model().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();
            self.sidebar
                .connect_chat_selected(clone!(@weak obj => move |_| {
                    obj.imp().leaflet.navigate(adw::NavigationDirection::Forward);
                }));
        }
    }

    impl WidgetImpl for Session {}
    impl BinImpl for Session {}
}

glib::wrapper! {
    pub(crate) struct Session(ObjectSubclass<imp::Session>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&ClientSession> for Session {
    fn from(model: &ClientSession) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl Session {
    pub(crate) fn model(&self) -> Option<ClientSession> {
        self.imp().model.upgrade()
    }

    pub(crate) fn select_chat(&self, chat_id: i64) {
        match self.model().unwrap().try_chat(chat_id) {
            Some(chat) => self.imp().sidebar.select_chat(chat),
            None => spawn(clone!(@weak self as obj => async move {
                match functions::create_private_chat(chat_id, true, obj.model().unwrap().client().id()).await {
                    Ok(enums::Chat::Chat(data)) => obj.imp().sidebar.select_chat(obj.model().unwrap().chat(data.id)),
                    Err(e) => log::warn!("Failed to create private chat: {:?}", e),
                }
            })),
        }
    }

    pub(crate) fn handle_paste_action(&self) {
        self.imp().content.handle_paste_action();
    }

    pub(crate) fn begin_chats_search(&self) {
        let imp = self.imp();
        imp.leaflet.navigate(adw::NavigationDirection::Back);
        imp.sidebar.begin_chats_search();
    }
}
