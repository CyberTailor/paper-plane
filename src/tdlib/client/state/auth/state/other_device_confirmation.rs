use std::cell::RefCell;

use glib::prelude::ObjectExt;
use glib::subclass::prelude::*;
use glib::ParamSpecBuilderExt;
use glib::Properties;
use gtk::glib;
use tdlib::types::AuthorizationStateWaitOtherDeviceConfirmation;

use crate::tdlib::BoxedAuthorizationStateWaitOtherDeviceConfirmation;
use crate::tdlib::ClientAuth;

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::WaitOtherDeviceConfirmation)]
    pub(crate) struct WaitOtherDeviceConfirmation {
        #[property(get, set, construct_only)]
        pub(super) auth: glib::WeakRef<ClientAuth>,
        #[property(get, set)]
        pub(super) data: RefCell<Option<BoxedAuthorizationStateWaitOtherDeviceConfirmation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WaitOtherDeviceConfirmation {
        const NAME: &'static str = "ClientAuthWaitOtherDeviceConfirmation";
        type Type = super::WaitOtherDeviceConfirmation;
    }

    impl ObjectImpl for WaitOtherDeviceConfirmation {
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
    pub(crate) struct WaitOtherDeviceConfirmation(ObjectSubclass<imp::WaitOtherDeviceConfirmation>);
}

impl WaitOtherDeviceConfirmation {
    pub(crate) fn new(
        auth: &ClientAuth,
        data: AuthorizationStateWaitOtherDeviceConfirmation,
    ) -> Self {
        glib::Object::builder()
            .property("auth", auth)
            .property(
                "data",
                BoxedAuthorizationStateWaitOtherDeviceConfirmation(data),
            )
            .build()
    }
}
