mod state;

use std::cell::RefCell;

use glib::prelude::Cast;
use glib::prelude::ObjectExt;
use glib::subclass::prelude::*;
use glib::ParamSpecBuilderExt;
use glib::Properties;
use gtk::glib;
use once_cell::unsync::OnceCell;
pub(crate) use state::SendPasswordRecoveryCodeResult;
pub(crate) use state::SendPhoneNumberResult;
pub(crate) use state::WaitCode;
pub(crate) use state::WaitOtherDeviceConfirmation;
pub(crate) use state::WaitPassword;
pub(crate) use state::WaitPhoneNumber;
pub(crate) use state::WaitRegistration;
use tdlib::enums::AuthorizationState;
use tdlib::types::UpdateAuthorizationState;

use crate::tdlib::BoxedAuthorizationStateWaitCode;
use crate::tdlib::BoxedAuthorizationStateWaitOtherDeviceConfirmation;
use crate::tdlib::Client;

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::Auth)]
    pub(crate) struct Auth {
        pub(super) wait_phone_number: OnceCell<WaitPhoneNumber>,
        #[property(get, set, construct_only)]
        pub(super) client: glib::WeakRef<Client>,
        #[property(get)]
        pub(super) state: RefCell<Option<glib::Object>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Auth {
        const NAME: &'static str = "ClientAuth";
        type Type = super::Auth;
    }

    impl ObjectImpl for Auth {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }
        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            self.derived_set_property(id, value, pspec)
        }
        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }
    }
}

glib::wrapper! {
    pub(crate) struct Auth(ObjectSubclass<imp::Auth>);
}

impl From<&Client> for Auth {
    fn from(client: &Client) -> Self {
        glib::Object::builder().property("client", client).build()
    }
}

impl Auth {
    fn wait_phone_number(&self) -> &WaitPhoneNumber {
        self.imp()
            .wait_phone_number
            .get_or_init(|| WaitPhoneNumber::from(self))
    }

    pub(crate) fn reset(&self) {
        self.set_state(Some(self.wait_phone_number().to_owned().upcast()));
    }

    pub(crate) fn set_state(&self, state: Option<glib::Object>) {
        if self.state() == state {
            return;
        }
        self.imp().state.replace(state);
        self.notify("state");
    }

    pub(crate) fn handle_update(&self, update: UpdateAuthorizationState) {
        self.set_state(Some(match update.authorization_state {
            AuthorizationState::WaitPhoneNumber => self.wait_phone_number().to_owned().upcast(),

            AuthorizationState::WaitCode(data) => match self
                .state()
                .and_then(|state| state.downcast::<WaitCode>().ok())
            {
                Some(state) => {
                    state.set_data(BoxedAuthorizationStateWaitCode(data));
                    state
                }
                None => WaitCode::new(self, data),
            }
            .upcast(),

            AuthorizationState::WaitOtherDeviceConfirmation(other_device) => match self
                .state()
                .and_then(|state| state.downcast::<WaitOtherDeviceConfirmation>().ok())
            {
                Some(state) => {
                    state.set_data(BoxedAuthorizationStateWaitOtherDeviceConfirmation(
                        other_device,
                    ));
                    state
                }
                None => WaitOtherDeviceConfirmation::new(self, other_device),
            }
            .upcast(),

            AuthorizationState::WaitRegistration(registration) => {
                WaitRegistration::new(self, registration).upcast()
            }

            AuthorizationState::WaitPassword(password) => {
                WaitPassword::new(self, password).upcast()
            }

            other => unreachable!("{other:?}"),
        }));
    }
}
