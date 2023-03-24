mod state;

use std::cell::Cell;
use std::cell::RefCell;

use glib::Properties;
use gtk::glib::clone;
use gtk::glib::WeakRef;
use gtk::glib::{self};
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use locale_config::Locale;
use once_cell::unsync::OnceCell;
use tdlib::enums::AuthorizationState;
use tdlib::enums::Update;
use tdlib::enums::{self};
use tdlib::functions;
use tdlib::types;

pub(crate) use self::state::Auth as ClientAuth;
pub(crate) use self::state::AuthWaitCode as ClientAuthWaitCode;
pub(crate) use self::state::AuthWaitOtherDeviceConfirmation as ClientAuthWaitOtherDeviceConfirmation;
pub(crate) use self::state::AuthWaitPassword as ClientAuthWaitPassword;
pub(crate) use self::state::AuthWaitPhoneNumber as ClientAuthWaitPhoneNumber;
pub(crate) use self::state::AuthWaitRegistration as ClientAuthWaitRegistration;
pub(crate) use self::state::LoggingOut as ClientLoggingOut;
pub(crate) use self::state::SendPasswordRecoveryCodeResult;
pub(crate) use self::state::SendPhoneNumberResult;
pub(crate) use self::state::Session as ClientSession;
use crate::config;
use crate::tdlib::BoxedDatabaseInfo;
use crate::tdlib::ClientManager;
use crate::utils::spawn;
use crate::utils::{self};

/// A struct for storing information about a session's database.
#[derive(Clone, Debug)]
pub(crate) struct DatabaseInfo {
    // The base name of the database directory.
    pub(crate) directory_base_name: String,
    // Whether this database uses a test dc.
    pub(crate) use_test_dc: bool,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::Client)]
    pub(crate) struct Client {
        #[property(get, set, construct_only)]
        pub(super) manager: WeakRef<ClientManager>,
        #[property(get, set, construct_only)]
        pub(super) id: Cell<i32>,
        #[property(get, set, construct_only)]
        pub(super) database_info: OnceCell<BoxedDatabaseInfo>,
        #[property(get)]
        pub(super) state: RefCell<Option<glib::Object>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Client {
        const NAME: &'static str = "Client";
        type Type = super::Client;
    }

    impl ObjectImpl for Client {
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

            spawn(clone!(@weak obj => async move {
                obj.init().await;
            }));
            obj.set_state(ClientAuth::from(obj).upcast());
        }
    }
}

glib::wrapper! { pub(crate) struct Client(ObjectSubclass<imp::Client>); }

impl Client {
    pub(crate) fn new(
        manager: &ClientManager,
        client_id: i32,
        database_info: DatabaseInfo,
    ) -> Self {
        glib::Object::builder()
            .property("manager", manager)
            .property("id", client_id)
            .property("database-info", BoxedDatabaseInfo(database_info))
            .build()
    }

    async fn init(&self) {
        let result = functions::set_log_verbosity_level(
            if log::log_enabled!(log::Level::Trace) {
                5
            } else if log::log_enabled!(log::Level::Debug) {
                4
            } else if log::log_enabled!(log::Level::Info) {
                3
            } else if log::log_enabled!(log::Level::Warn) {
                2
            } else {
                0
            },
            self.id(),
        )
        .await;

        if let Err(e) = result {
            log::warn!("Error setting the tdlib log level: {:?}", e);
        }

        // TODO: Hopefully we'll support animated emoji at some point
        let result = self.disable_animated_emoji(true).await;
        if let Err(e) = result {
            log::warn!("Error disabling animated emoji: {:?}", e);
        }
    }

    /// Helper function to enable/disable animated emoji for a client.
    async fn disable_animated_emoji(&self, value: bool) -> Result<(), types::Error> {
        functions::set_option(
            "disable_animated_emoji".to_string(),
            Some(enums::OptionValue::Boolean(types::OptionValueBoolean {
                value,
            })),
            self.id(),
        )
        .await
    }

    pub(crate) async fn send_tdlib_parameters(&self) -> Result<(), types::Error> {
        let system_language_code = {
            let locale = Locale::current().to_string();
            if !locale.is_empty() {
                locale
            } else {
                "en_US".to_string()
            }
        };

        let database_info = &self.database_info().0;

        let database_directory = utils::data_dir()
            .join(&database_info.directory_base_name)
            .to_str()
            .expect("Data directory path is not a valid unicode string")
            .into();

        functions::set_tdlib_parameters(
            // database_info.use_test_dc,
            true,
            database_directory,
            String::new(),
            String::new(),
            true,
            true,
            true,
            true,
            config::TG_API_ID,
            config::TG_API_HASH.into(),
            system_language_code,
            "Desktop".into(),
            String::new(),
            config::VERSION.into(),
            true,
            false,
            self.id(),
        )
        .await
    }

    fn set_state(&self, state: glib::Object) {
        if self.state().as_ref() == Some(&state) {
            return;
        }
        self.imp().state.replace(Some(state));
        self.notify("state");
    }

    async fn set_ready(&self) {
        let client_session = ClientSession::from(self);
        self.set_state(client_session.clone().upcast());

        let enums::User::User(me) = functions::get_me(self.id()).await.unwrap();
        client_session.set_me(&client_session.upsert_user(me));

        client_session.fetch_chats();

        let result = functions::set_option(
            "notification_group_count_max".to_string(),
            Some(enums::OptionValue::Integer(types::OptionValueInteger {
                value: 5,
            })),
            self.id(),
        )
        .await;

        if let Err(e) = result {
            log::warn!(
                "Error setting the notification_group_count_max option: {:?}",
                e
            );
        }
    }

    pub(crate) async fn set_online(&self, value: bool) -> Result<(), types::Error> {
        functions::set_option(
            "online".to_string(),
            Some(enums::OptionValue::Boolean(types::OptionValueBoolean {
                value,
            })),
            self.id(),
        )
        .await
    }

    pub(crate) async fn log_out(&self) {
        if let Some(client_manager) = self.manager() {
            client_manager.remove_client(self);
        }

        if let Err(e) = functions::log_out(self.id()).await {
            log::error!("Could not logout client with id={}: {:?}", self.id(), e);
        }
    }

    pub(crate) fn handle_update(&self, update: Update) {
        match update {
            Update::AuthorizationState(state) => match state.authorization_state {
                AuthorizationState::WaitTdlibParameters => {
                    spawn(clone!(@weak self as obj => async move {
                        _ = obj.send_tdlib_parameters().await;
                    }));
                }
                AuthorizationState::Ready => {
                    spawn(clone!(@weak self as obj => async move {
                        obj.set_ready().await;
                    }));
                }
                AuthorizationState::Closing => {
                    self.set_state(ClientLoggingOut::default().upcast());
                }
                AuthorizationState::LoggingOut => {
                    println!("logging out");
                }
                _ => self
                    .state()
                    .and_downcast::<ClientAuth>()
                    .unwrap()
                    .handle_update(state),
            },
            _ => {
                let state = self.state().unwrap();

                if let Some(state) = state.downcast_ref::<ClientSession>() {
                    state.handle_update(update);
                } else if let Some(state) = state.downcast_ref::<ClientLoggingOut>() {
                    state.handle_update(update);
                }
            }
        }
    }
}
