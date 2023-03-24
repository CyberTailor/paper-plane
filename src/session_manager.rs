use std::borrow::Borrow;
use std::cell::RefCell;

use glib::clone;
use glib::Properties;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use tdlib::enums;
use tdlib::enums::Update;
use tdlib::functions;
use tdlib::types;

use crate::components::AnimatedBin;
use crate::tdlib::Client;
use crate::tdlib::ClientManager;
use crate::utils::spawn;
use crate::APPLICATION_OPTS;

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties, CompositeTemplate)]
    #[properties(wrapper_type = super::SessionManager)]
    #[template(string = r#"
        using Gtk 4.0;
        using Adw 1;
        template $SessionManager {
            layout-manager: BinLayout {};
            $AnimatedBin bin {
                transition-type: "crossfade";
            }
        }
    "#)]
    pub(crate) struct SessionManager {
        #[property(get)]
        pub(super) client_manager: ClientManager,
        /// The order of the recently used sessions. The string stored in the `Vec` represents the
        /// session's database directory name.
        pub(super) recently_used_sessions: RefCell<Vec<String>>,
        #[property(get)]
        pub(super) active_client: glib::WeakRef<Client>,
        #[template_child]
        pub(super) bin: TemplateChild<AnimatedBin>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionManager {
        const NAME: &'static str = "SessionManager";
        type Type = super::SessionManager;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SessionManager {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }
        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            self.derived_set_property(id, value, pspec)
        }
        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = &*self.obj();

            if let Some(client) = obj.active_client() {
                self.bin
                    .append(&crate::client::Client::from(&client).upcast());
            }

            let mut recently_used_sessions = gio::Settings::new(crate::config::APP_ID)
                .strv("recently-used-sessions")
                .into_iter()
                .map(glib::GString::into)
                .collect::<Vec<_>>();

            // Remove invalid database directory base names from recently used sessions.
            recently_used_sessions.retain(|directory_base_name| {
                self.client_manager.sessions().iter().any(|session| {
                    &session.client().database_info().0.directory_base_name == directory_base_name
                })
            });
            self.recently_used_sessions.replace(recently_used_sessions);

            self.client_manager
                .connect_client_removed(clone!(@weak obj => move |_, client| {
                    let directory_base_name = &client.database_info().0.directory_base_name;

                    remove_from_vec(
                        &mut *obj.imp().recently_used_sessions.borrow_mut(),
                        directory_base_name
                    );
                    obj.save_recently_used_sessions();

                    obj.set_recent_client();
                }));

            self.client_manager.connect_client_added(clone!(@weak obj => move |client_manager, client| {
                if client_manager
                    .client_by_directory_base_name(&client.database_info().0.directory_base_name)
                    .is_some()
                {
                    obj.set_active_client(client, true);

                    obj.imp()
                        .recently_used_sessions
                        .borrow_mut()
                        .push(client.database_info().0.directory_base_name);
                    obj.save_recently_used_sessions();
                }
            }));

            obj.set_recent_client();
        }

        fn dispose(&self) {
            self.bin.unparent();
        }
    }

    impl WidgetImpl for SessionManager {}
}

glib::wrapper! {
    pub(crate) struct SessionManager(ObjectSubclass<imp::SessionManager>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl SessionManager {
    pub(crate) fn set_active_client(&self, client: &Client, append: bool) {
        let old_active_client = self.active_client();
        if old_active_client.as_ref() == Some(client) {
            return;
        }

        if let Some(client) = old_active_client {
            spawn(clone!(@weak client => async move {
                _ = client.set_online(false).await;
            }));
        }

        spawn(clone!(@weak client => async move {
            _ = client.set_online(true).await;
        }));

        if append {
            self.imp()
                .bin
                .append(&crate::client::Client::from(client).upcast());
        } else {
            self.imp()
                .bin
                .prepend(&crate::client::Client::from(client).upcast());
        }

        self.imp().active_client.set(Some(client));
        self.notify("active-client");
    }

    fn set_recent_client(&self) {
        let imp = self.imp();

        match imp.client_manager.first_client() {
            Some(client) => {
                match imp
                    .recently_used_sessions
                    .borrow()
                    .last()
                    .and_then(|directory_base_name| {
                        imp.client_manager
                            .client_by_directory_base_name(directory_base_name)
                    }) {
                    Some(client) => self.set_active_client(&client, false),
                    None => self.set_active_client(&client, false),
                }
            }
            None => imp
                .client_manager
                .add_new_session(APPLICATION_OPTS.get().unwrap().test_dc),
        }
    }

    /// Sets the online status for the active logged in client. This will be called from the
    /// application `Window` when its active state has changed.
    pub(crate) async fn set_active_client_online(&self, value: bool) {
        if let Some(client) = self.active_client() {
            client.set_online(value).await;
        }
    }

    /// Function cleaning up, which is called by the application windows on closing. It sets all
    /// clients offline.
    pub(crate) fn close_clients(&self) {
        // Create a future to close the sessions.
        // let close_sessions_future = futures::future::join_all(
        //     self.imp()
        //         .clients
        //         .borrow()
        //         .iter()
        //         .filter_map(|(client_id, client)| match client.state {
        //             ClientState::Auth { .. } | ClientState::LoggedIn => Some(client_id),
        //             _ => None,
        //         })
        //         .cloned()
        //         .map(|client_id| {
        //             set_online(client_id, false).and_then(move |_| functions::close(client_id))
        //         }),
        // );

        // // Block on that future, else the window closes before they are finished!!!
        // block_on(async {
        //     close_sessions_future.await.into_iter().for_each(|result| {
        //         if let Err(e) = result {
        //             log::warn!("Error on closing client: {:?}", e);
        //         }
        //     });
        // });
    }

    pub(crate) fn handle_update(&self, update: Update, client_id: i32) {
        self.imp().client_manager.handle_update(update, client_id);
    }

    /// Function that is used to overwrite the recently used sessions file.
    fn save_recently_used_sessions(&self) {
        let settings = gio::Settings::new(crate::config::APP_ID);
        if let Err(e) = settings.set_strv(
            "recently-used-sessions",
            self.imp()
                .recently_used_sessions
                .borrow()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice(),
        ) {
            log::warn!(
                "Failed to save value for gsettings key 'recently-used-sessions': {}",
                e
            );
        }
    }

    async fn enable_notifications(&self, client_id: i32) {
        let result = functions::set_option(
            "notification_group_count_max".to_string(),
            Some(enums::OptionValue::Integer(types::OptionValueInteger {
                value: 5,
            })),
            client_id,
        )
        .await;

        if let Err(e) = result {
            log::warn!(
                "Error setting the notification_group_count_max option: {:?}",
                e
            );
        }
    }

    pub(crate) fn select_chat(&self, client_id: i32, chat_id: i64) {
        // if let Some(session) = self.imp().client_manager.session(client_id) {
        //     session.select_chat(chat_id);
        // }

        // TODO:
        // if let Some(client) = self.imp().clients.borrow().get(&client_id) {
        //     if let ClientState::LoggedIn = client.state {
        //         client.session.select_chat(chat_id);
        //     }
        // }
    }

    pub(crate) fn handle_paste_action(&self) {
        // TODO
        // if let Some(client_id) = self.active_logged_in_client_id() {
        //     let clients = self.imp().clients.borrow();
        //     let client = clients.get(&client_id).unwrap();
        //     if let ClientState::LoggedIn = client.state {
        //         client.session.handle_paste_action();
        //     }
        // }
    }

    pub(crate) fn begin_chats_search(&self) {
        // TODO
        // if let Some(client_id) = self.active_logged_in_client_id() {
        //     let clients = self.imp().clients.borrow();
        //     let client = clients.get(&client_id).unwrap();
        //     if let ClientState::LoggedIn = client.state {
        //         client.session.begin_chats_search();
        //     }
        // }
    }
}

/// Helper function for removing an element from a [`Vec`] based on an equality comparison.
fn remove_from_vec<T, Q: ?Sized>(vec: &mut Vec<T>, to_remove: &Q) -> bool
where
    T: Borrow<Q>,
    Q: Eq,
{
    match vec.iter().position(|elem| elem.borrow() == to_remove) {
        Some(pos) => {
            vec.remove(pos);
            true
        }
        None => false,
    }
}
