use std::cell::RefCell;

use glib::prelude::ObjectExt;
use glib::subclass::prelude::*;
use glib::ParamSpecBuilderExt;
use glib::Properties;
use gtk::glib;
use tdlib::functions;
use tdlib::types;
use tdlib::types::AuthorizationStateWaitRegistration;

use crate::tdlib::BoxedAuthorizationStateWaitRegistration;
use crate::tdlib::ClientAuth;

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::WaitRegistration)]
    pub(crate) struct WaitRegistration {
        #[property(get, set, construct_only)]
        pub(super) auth: glib::WeakRef<ClientAuth>,
        #[property(get, set, construct_only)]
        pub(super) data: RefCell<Option<BoxedAuthorizationStateWaitRegistration>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WaitRegistration {
        const NAME: &'static str = "ClientAuthWaitRegistration";
        type Type = super::WaitRegistration;
    }

    impl ObjectImpl for WaitRegistration {
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
    pub(crate) struct WaitRegistration(ObjectSubclass<imp::WaitRegistration>);
}

impl WaitRegistration {
    pub(crate) fn new(auth: &ClientAuth, data: AuthorizationStateWaitRegistration) -> Self {
        glib::Object::builder()
            .property("auth", auth)
            .property("data", BoxedAuthorizationStateWaitRegistration(data))
            .build()
    }

    pub(crate) async fn send_registration(
        &self,
        first_name: String,
        last_name: String,
    ) -> Result<(), types::Error> {
        let client = self.auth().unwrap().client().unwrap();
        functions::register_user(first_name, last_name, client.id()).await
    }
}
