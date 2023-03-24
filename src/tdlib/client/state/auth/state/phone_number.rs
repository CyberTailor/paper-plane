use std::cell::RefCell;

use glib::clone;
use glib::prelude::ObjectExt;
use glib::subclass::prelude::*;
use glib::ParamSpecBuilderExt;
use glib::Properties;
use glib::WeakRef;
use gtk::glib;
use tdlib::enums;
use tdlib::functions;
use tdlib::types;

use crate::tdlib::ClientAuth;
use crate::tdlib::ClientSession;
use crate::tdlib::CountryList;
use crate::utils::spawn;

pub(crate) enum SendPhoneNumberResult {
    AlreadyLoggedIn(ClientSession),
    Err(types::Error),
    Ok,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::WaitPhoneNumber)]
    pub(crate) struct WaitPhoneNumber {
        #[property(get, set, construct_only)]
        pub(super) auth: WeakRef<ClientAuth>,
        #[property(get, explicit_notify, nullable)]
        pub(super) country_list: RefCell<Option<CountryList>>,
        #[property(get, set)]
        pub(super) phone_number: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WaitPhoneNumber {
        const NAME: &'static str = "ClientAuthWaitPhoneNumber";
        type Type = super::WaitPhoneNumber;
    }

    impl ObjectImpl for WaitPhoneNumber {
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
            self.obj().load_country_codes();
        }
    }
}

glib::wrapper! {
    pub(crate) struct WaitPhoneNumber(ObjectSubclass<imp::WaitPhoneNumber>);
}

impl From<&ClientAuth> for WaitPhoneNumber {
    fn from(auth: &ClientAuth) -> Self {
        glib::Object::builder().property("auth", auth).build()
    }
}

impl WaitPhoneNumber {
    fn load_country_codes(&self) {
        if self.country_list().is_none() {
            let client = self.auth().unwrap().client().unwrap();
            let use_test_dc = client.database_info().0.use_test_dc;

            spawn(clone!(@weak self as obj => async move {
                let imp = obj.imp();
                match functions::get_countries(client.id()).await {
                    Ok(enums::Countries::Countries(countries)) => {
                        imp.country_list.replace(Some(CountryList::from_td_object(countries, use_test_dc)));
                        obj.notify("country-list");
                    }
                    Err(_) => {
                        // TODO: Show a toast notification.
                    }
                }
            }));
        }
    }

    pub(crate) async fn send_phone_number(&self, phone_number: &str) -> SendPhoneNumberResult {
        let client = self.auth().unwrap().client().unwrap();
        let client_id = client.id();

        // Check if we are already have an account logged in with that phone_number.
        let phone_number_digits = phone_number
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>();

        self.set_phone_number(phone_number_digits.clone());

        let client_manager = client.manager().unwrap();
        let sessions = client_manager.sessions();

        let on_test_dc = client.database_info().0.use_test_dc;

        let client_session = sessions.iter().find(|client| {
            on_test_dc == client.client().database_info().0.use_test_dc
                && client.me().phone_number().replace(' ', "") == phone_number_digits
        });

        match client_session {
            Some(client_session) => {
                // We just figured out that we already have an open session for that account.
                // Therefore we logout the client, with which we wanted to log in and delete its
                // just created database directory.

                // TODO: log out session and raise an special event.
                // client_session.log_out();
                SendPhoneNumberResult::AlreadyLoggedIn(client_session.to_owned())
            }
            None => {
                let result = functions::set_authentication_phone_number(
                    phone_number.into(),
                    Some(types::PhoneNumberAuthenticationSettings {
                        allow_flash_call: true,
                        allow_missed_call: true,
                        ..Default::default()
                    }),
                    client_id,
                )
                .await;

                match result {
                    Ok(_) => SendPhoneNumberResult::Ok,
                    Err(e) => SendPhoneNumberResult::Err(e),
                }
            }
        }
    }

    pub(crate) async fn request_qr_code(&self) -> Result<(), types::Error> {
        let client = self.auth().unwrap().client().unwrap();
        let other_user_ids = client
            .manager()
            .unwrap()
            .logged_in_users()
            .into_iter()
            .map(|user| user.id())
            .collect();

        functions::request_qr_code_authentication(other_user_ids, client.id()).await
    }
}
