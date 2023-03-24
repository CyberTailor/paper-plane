use std::cell::Cell;
use std::cell::RefCell;

use glib::prelude::ObjectExt;
use glib::subclass::prelude::*;
use glib::ParamSpecBuilderExt;
use glib::Properties;
use gtk::glib;
use tdlib::functions;
use tdlib::types;
use tdlib::types::AuthorizationStateWaitPassword;

use crate::tdlib::BoxedAuthorizationStateWaitPassword;
use crate::tdlib::ClientAuth;

pub(crate) enum SendPasswordRecoveryCodeResult {
    Expired,
    Err(types::Error),
    Ok,
}

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::WaitPassword)]
    pub(crate) struct WaitPassword {
        #[property(get, set, construct_only)]
        pub(super) auth: glib::WeakRef<ClientAuth>,
        #[property(get, set, construct_only)]
        pub(super) data: RefCell<Option<BoxedAuthorizationStateWaitPassword>>,
        pub(super) password_recovery_expired: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WaitPassword {
        const NAME: &'static str = "ClientAuthWaitPassword";
        type Type = super::WaitPassword;
    }

    impl ObjectImpl for WaitPassword {
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
    pub(crate) struct WaitPassword(ObjectSubclass<imp::WaitPassword>);
}

impl WaitPassword {
    pub(crate) fn new(auth: &ClientAuth, password: AuthorizationStateWaitPassword) -> Self {
        glib::Object::builder()
            .property("auth", auth)
            .property("data", BoxedAuthorizationStateWaitPassword(password))
            .build()
    }

    pub(crate) async fn send_password(&self, password: String) -> Result<(), types::Error> {
        let client = self.auth().unwrap().client().unwrap();
        functions::check_authentication_password(password, client.id()).await
    }

    pub(crate) async fn recover_password(&self) -> Result<(), types::Error> {
        let imp = self.imp();

        if !imp.password_recovery_expired.get() {
            return Ok(());
        }

        // We need to tell tdlib to send us the recovery code via mail (again).
        let client = self.auth().unwrap().client().unwrap();

        let result = functions::request_authentication_password_recovery(client.id()).await;

        // Save that we do not need to resend the mail when we enter the recovery
        // page the next time.
        imp.password_recovery_expired.set(result.is_err());

        result
    }

    pub(crate) async fn send_password_recovery_code(
        &self,
        recovery_code: String,
    ) -> SendPasswordRecoveryCodeResult {
        let client = self.auth().unwrap().client().unwrap();

        let result = functions::recover_authentication_password(
            recovery_code,
            String::new(),
            String::new(),
            client.id(),
        )
        .await;

        match result {
            Ok(_) => SendPasswordRecoveryCodeResult::Ok,
            Err(e) => {
                if e.message == "PASSWORD_RECOVERY_EXPIRED" {
                    // The same procedure is used as for the official client (as far as I
                    // understood from the code). Alternatively, we could send the user a new
                    // code, indicate that and stay on the recovery page.
                    self.imp().password_recovery_expired.set(true);

                    SendPasswordRecoveryCodeResult::Expired
                } else {
                    SendPasswordRecoveryCodeResult::Err(e)
                }
            }
        }
    }
}
