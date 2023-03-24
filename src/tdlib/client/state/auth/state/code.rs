use std::cell::Cell;
use std::cell::RefCell;

use glib::clone;
use glib::subclass::prelude::*;
use glib::ObjectExt;
use glib::ParamSpecBuilderExt;
use glib::Properties;
use glib::WeakRef;
use gtk::glib;
use tdlib::functions;
use tdlib::types;
use tdlib::types::AuthorizationStateWaitCode;

use crate::tdlib::BoxedAuthorizationStateWaitCode;
use crate::tdlib::ClientAuth;

mod imp {
    use super::*;

    #[derive(Debug, Default, Properties)]
    #[properties(wrapper_type = super::WaitCode)]
    pub(crate) struct WaitCode {
        #[property(get, set, construct_only)]
        pub(super) auth: WeakRef<ClientAuth>,
        #[property(get, set, construct)]
        pub(super) data: RefCell<Option<BoxedAuthorizationStateWaitCode>>,
        #[property(get, explicit_notify)]
        pub(super) countdown: Cell<i32>,
        pub(super) countdown_source_id: RefCell<Option<glib::SourceId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WaitCode {
        const NAME: &'static str = "ClientAuthWaitCode";
        type Type = super::WaitCode;
    }

    impl ObjectImpl for WaitCode {
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
            self.obj()
                .connect_data_notify(Self::Type::update_code_resend_state);
        }
    }
}

glib::wrapper! {
    pub(crate) struct WaitCode(ObjectSubclass<imp::WaitCode>);
}

impl WaitCode {
    pub(crate) fn new(auth: &ClientAuth, data: AuthorizationStateWaitCode) -> Self {
        glib::Object::builder()
            .property("auth", auth)
            .property("data", BoxedAuthorizationStateWaitCode(data))
            .build()
    }

    pub(crate) fn set_countdown(&self, countdown: i32) {
        if self.countdown() == countdown {
            return;
        }
        self.imp().countdown.set(countdown);
        self.notify("countdown");
    }

    pub(crate) async fn send_code(&self, code: String) -> Result<(), types::Error> {
        let client_id = self.auth().unwrap().client().unwrap().id();

        let result = functions::check_authentication_code(code, client_id).await;
        if let Ok(_) = result {
            self.stop_code_next_type_countdown();
        }

        result
    }

    pub(crate) async fn resend_auth_code(&self) -> Result<(), types::Error> {
        let result =
            functions::resend_authentication_code(self.auth().unwrap().client().unwrap().id())
                .await;

        if let Err(ref err) = result {
            if err.code == 8 {
                // Sometimes the user may get a FLOOD_WAIT when he/she wants to resend the
                // authorization code. But then tdlib blocks the resend function for the
                // user, but does not inform us about it by sending an
                // 'AuthorizationState::WaitCode'. Consequently, the user interface would
                // still indicate that we are allowed to resend the code. However, we
                // always get code 8 when we try, indicating that resending does not work.
                // In this case, we automatically disable the resend feature.
                self.stop_code_next_type_countdown();
                self.data().unwrap().0.code_info.next_type = None;
                self.notify("data");
            }
        }

        result
    }

    fn update_code_resend_state(&self) {
        // Always stop the resend countdown first.
        self.stop_code_next_type_countdown();

        let code_info = self.data().unwrap().0.code_info;
        if let Some(code_type) = code_info.next_type {
            if code_info.timeout > 0 {
                self.set_countdown(code_info.timeout);

                let source_id = glib::timeout_add_seconds_local(
                    1,
                    clone!(@weak self as obj => @default-return glib::Continue(false), move || {
                        obj.set_countdown(obj.countdown() - 1);
                        glib::Continue(if obj.countdown() == 0 {
                            obj.stop_code_next_type_countdown();
                            false
                        } else {
                            true
                        })
                    }),
                );
                self.imp().countdown_source_id.replace(Some(source_id));
            }
        }
    }

    fn stop_code_next_type_countdown(&self) {
        if let Some(source_id) = self.imp().countdown_source_id.take() {
            source_id.remove();
        }
    }
}
