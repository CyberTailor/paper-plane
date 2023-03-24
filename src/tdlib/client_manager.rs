//! Widget for managing sessions.
//!
//! This widget can be considered as the main view. It is directly subordinate to the application
//! window and takes care of managing sessions. In the following sections it is described what this
//! includes.
//!
//! # Adding new sessions
//! The `ClientManager` directs it to [`Login`](struct@crate::login::Login) as soon as the user
//! wants to add a new session within the function `add_new_session`. Login returns control to the
//! session manager if either the new session is ready, logging into the new session is aborted by
//! the user, or it is noticed that the phone number is already logged in. In order ro prevent
//! logging in twice in the same account by an qr code, the function
//! [`ClientManager::logged_in_users()`] is provided that is used by `login.rs` to extract user
//! ids and pass them to tdlib.
//!
//! # Adding existing sessions
//! The `ClientManager` analyzes the individual database directories in the Telegrand data
//! directory to see which sessions can be logged in directly using
//! [`ClientManager::add_existing_session()`]. To do this, it checks the presence of a `td.binlog`
//! or a `td_test.binlog` file.
//!
//! # Destroying sessions
//! This is realized by first logging out the client and then deleting the database directory once
//! the `AuthorizationState::Closed` event has been received for that session.
//! Destroying sessions happens in different places: When the login is canceled, When the QR code
//! is canceled, when a logged in session is logged out, and when the session is removed from
//! another device.
//!
//! # Remembering recently used sessions
//! In order to remember the order in which the user selected the sessions, the `ClientManager`
//! uses a gsettings key value pair.

use std::cell::RefCell;
use std::fs;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use gio::subclass::prelude::ListModelImpl;
use glib::subclass::prelude::*;
use glib::subclass::Signal;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use indexmap::map::Entry;
use indexmap::IndexMap;
use once_cell::sync::Lazy;
use tdlib::enums::AuthorizationState;
use tdlib::enums::Update;

use crate::tdlib::Client;
use crate::tdlib::ClientSession;
use crate::tdlib::DatabaseInfo;
use crate::tdlib::User;
use crate::utils;
use crate::APPLICATION_OPTS;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub(crate) struct ClientManager(pub(super) RefCell<IndexMap<i32, Client>>);

    #[glib::object_subclass]
    impl ObjectSubclass for ClientManager {
        const NAME: &'static str = "ClientManager";
        type Type = super::ClientManager;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ClientManager {
        fn signals() -> &'static [Signal] {
            static SIGNALS: Lazy<Vec<Signal>> = Lazy::new(|| {
                vec![
                    Signal::builder("client-added")
                        .param_types([Client::static_type()])
                        .build(),
                    Signal::builder("client-removed")
                        .param_types([Client::static_type()])
                        .build(),
                ]
            });
            SIGNALS.as_ref()
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = &*self.obj();

            // ####################################################################################
            // # Load the sessions from the data directory.                                       #
            // ####################################################################################
            match analyze_data_dir() {
                Err(e) => panic!("Could not initialize data directory: {e}"),
                Ok(database_infos) => {
                    if database_infos.is_empty() {
                        obj.add_new_session(APPLICATION_OPTS.get().unwrap().test_dc);
                    } else {
                        database_infos.into_iter().for_each(|database_info| {
                            obj.add_existing_session(database_info);
                        });
                    }
                }
            }
        }
    }

    impl ListModelImpl for ClientManager {
        fn item_type(&self) -> glib::Type {
            Client::static_type()
        }

        fn n_items(&self) -> u32 {
            self.0.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.0
                .borrow()
                .get_index(position as usize)
                .map(|(_, client)| client.to_owned().upcast())
        }
    }
}

glib::wrapper! {
    pub(crate) struct ClientManager(ObjectSubclass<imp::ClientManager>) @implements gio::ListModel;
}

impl Default for ClientManager {
    fn default() -> Self {
        glib::Object::builder().build()
    }
}

impl ClientManager {
    /// Returns the `Session` for the given client id.
    pub(crate) fn session(&self, client_id: i32) -> Option<ClientSession> {
        self.imp()
            .0
            .borrow()
            .get(&client_id)
            .and_then(|client| client.state())
            .and_then(|client_state| client_state.downcast::<ClientSession>().ok())
    }

    /// Function that returns all currently logged in users.
    pub(crate) fn sessions(&self) -> Vec<ClientSession> {
        self.imp()
            .0
            .borrow()
            .values()
            .cloned()
            .filter_map(|client| client.state())
            .filter_map(|client_state| client_state.downcast::<ClientSession>().ok())
            .collect()
    }

    /// Function that returns all currently logged in users.
    pub(crate) fn logged_in_users(&self) -> Vec<User> {
        self.sessions().iter().map(ClientSession::me).collect()
    }

    /// Returns the `Client` for the given client id.
    pub(crate) fn client(&self, client_id: i32) -> Option<Client> {
        self.imp().0.borrow().get(&client_id).cloned()
    }

    pub(crate) fn first_client(&self) -> Option<Client> {
        self.imp().0.borrow().values().next().cloned()
    }

    pub(crate) fn client_by_directory_base_name(
        &self,
        directory_base_name: &str,
    ) -> Option<Client> {
        self.imp()
            .0
            .borrow()
            .values()
            .find(|client| directory_base_name == client.database_info().0.directory_base_name)
            .cloned()
    }

    /// This function is used to add/load an existing session that already had the
    /// `AuthorizationState::Ready` state from a previous application run.
    fn add_existing_session(&self, database_info: DatabaseInfo) {
        self.add_client(tdlib::create_client(), database_info)
    }

    /// This function is used to add a new session for a so far unknown account. This means it will
    /// go through the login process.
    pub(crate) fn add_new_session(&self, use_test_dc: bool) {
        self.add_client(
            tdlib::create_client(),
            DatabaseInfo {
                directory_base_name: generate_database_dir_base_name(),
                use_test_dc,
            },
        );
    }

    fn add_client(&self, client_id: i32, database_info: DatabaseInfo) {
        let client = Client::new(self, client_id, database_info);
        let (position, _) = self.imp().0.borrow_mut().insert_full(
            client_id,
            // Important: Here, we basically say that we just want to wait for
            // `AuthorizationState::Ready` and skip the login process.
            client.clone(),
        );
        self.items_changed(position as u32, 0, 1);
        self.client_added(&client)
    }

    fn client_added(&self, client: &Client) {
        self.emit_by_name::<()>("client-added", &[client]);
    }

    fn client_removed(&self, client: &Client) {
        self.emit_by_name::<()>("client-removed", &[&client]);
    }

    pub(crate) fn connect_client_added<F: Fn(&Self, &Client) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("client-added", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let client = values[1].get::<Client>().unwrap();
            f(&obj, &client);

            None
        })
    }

    pub(crate) fn connect_client_removed<F: Fn(&Self, &Client) + 'static>(
        &self,
        f: F,
    ) -> glib::SignalHandlerId {
        self.connect_local("client-removed", true, move |values| {
            let obj = values[0].get::<Self>().unwrap();
            let client = values[1].get::<Client>().unwrap();
            f(&obj, &client);

            None
        })
    }

    fn remove_client_(&self, client: &Client, position: u32) {
        let database_dir_base_name = client.database_info().0.directory_base_name;

        if let Err(e) = fs::remove_dir_all(utils::data_dir().join(database_dir_base_name)) {
            log::error!("Error on on removing database directory: {}", e);
        }

        self.items_changed(position, 1, 0);
        self.client_removed(client);
    }

    pub(crate) fn remove_client(&self, client: &Client) {
        let mut list = self.imp().0.borrow_mut();
        if let Some((position, _, _)) = list.swap_remove_full(&client.id()) {
            drop(list);
            self.remove_client_(client, position as u32);
        }
    }

    pub(crate) fn handle_update(&self, update: Update, client_id: i32) {
        let mut list = self.imp().0.borrow_mut();
        if let Entry::Occupied(entry) = list.entry(client_id) {
            if matches!(
                &update,
                Update::AuthorizationState(state)
                    if state.authorization_state == AuthorizationState::Closed)
            {
                let position = entry.index();
                let client = entry.shift_remove();
                drop(list);

                self.remove_client_(&client, position as u32);
            } else {
                let client = entry.get().to_owned();
                drop(list);
                client.handle_update(update);
            }
        }
    }
}

/// This function analyzes the data directory.
///
/// First, it checks whether the directory exists. It will create it and return immediately if
/// it doesn't.
///
/// If the data directory exists, information about the sessions is gathered. This is reading the
/// recently used sessions file and checking the individual session's database directory.
fn analyze_data_dir() -> Result<Vec<DatabaseInfo>, anyhow::Error> {
    if !utils::data_dir().exists() {
        // Create the Telegrand data directory if it does not exist and return.
        fs::create_dir_all(utils::data_dir())?;
        return Ok(Vec::new());
    }

    // All directories with the result of reading the session info file.
    let database_infos = fs::read_dir(utils::data_dir())?
        // Remove entries with error
        .filter_map(|res| res.ok())
        // Only consider directories.
        .filter(|entry| entry.path().is_dir())
        // Only consider directories with a "*.binlog" file
        .filter_map(|entry| {
            if entry.path().join("td.binlog").is_file() {
                return Some((entry, false));
            } else if entry.path().join("td_test.binlog").is_file() {
                return Some((entry, true));
            }
            None
        })
        .map(|(entry, use_test_dc)| DatabaseInfo {
            directory_base_name: entry
                .path()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_owned(),
            use_test_dc,
        })
        .collect::<Vec<_>>();

    Ok(database_infos)
}

/// This function generates a new database directory name based on the current UNIX system time
/// (e.g. db1638487692420). In the very unlikely case that a name is already taken it tries to
/// append a number at the end.
fn generate_database_dir_base_name() -> String {
    let database_dir_base_name = format!(
        "db{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis()
    );

    // Just to be sure!
    if utils::data_dir().join(&database_dir_base_name).exists() {
        (2..)
            .map(|count| format!("{database_dir_base_name}_{count}"))
            .find(|alternative_base_name| !utils::data_dir().join(alternative_base_name).exists())
            .unwrap()
    } else {
        database_dir_base_name
    }
}
