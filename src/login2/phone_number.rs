use std::cell::RefCell;

use futures::future;
use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::phone_number_input::PhoneNumberInput;
use crate::session_manager::SessionManager;
use crate::tdlib as model;
use crate::tdlib::SendPhoneNumberResult;
use crate::utils;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(resource = "/app/drey/paper-plane/ui/login-phone-number.ui")]
    #[properties(wrapper_type = super::PhoneNumber)]
    pub(crate) struct PhoneNumber {
        pub(super) abort_handle: RefCell<Option<future::AbortHandle>>,
        #[property(get, set, construct_only)]
        pub(super) model: glib::WeakRef<model::ClientAuthWaitPhoneNumber>,
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub(super) input: TemplateChild<PhoneNumberInput>,
        #[template_child]
        pub(super) next_button_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) qr_code_spinner: TemplateChild<gtk::Spinner>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PhoneNumber {
        const NAME: &'static str = "LoginPhoneNumber";
        type Type = super::PhoneNumber;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action_async("login.exit", None, |widget, _, _| async move {
                widget.exit().await;
            });

            klass.install_action_async("login.next", None, |widget, _, _| async move {
                widget.next().await;
            });

            klass.install_action_async("login.use-qr-code", None, |widget, _, _| async move {
                widget.request_qr_code().await;
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PhoneNumber {
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
            self.obj().setup_expressions();
        }

        fn dispose(&self) {
            utils::unparent_children(self.obj().upcast_ref());
        }
    }

    impl WidgetImpl for PhoneNumber {}
}

glib::wrapper! {
    pub(crate) struct PhoneNumber(ObjectSubclass<imp::PhoneNumber>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&model::ClientAuthWaitPhoneNumber> for PhoneNumber {
    fn from(model: &model::ClientAuthWaitPhoneNumber) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl PhoneNumber {
    pub(crate) async fn exit(&self) {
        if let Some(client) = self
            .model()
            .and_then(|model| model.auth())
            .and_then(|auth| auth.client())
        {
            self.cancel();
            client.log_out().await;
        }
    }
    pub(crate) async fn next(&self) {
        if let Some(model) = self.model() {
            if let Some(client) = model.auth().and_then(|auth| auth.client()) {
                self.freeze(false, true);

                let imp = self.imp();

                let abort_registration = self.setup_abort_handle();
                let result = future::Abortable::new(
                    model.send_phone_number(imp.input.text().as_str()),
                    abort_registration,
                )
                .await;

                if let Ok(result) = result {
                    match result {
                        SendPhoneNumberResult::AlreadyLoggedIn(client_session) => {
                            self.ancestor(SessionManager::static_type())
                                .and_downcast::<SessionManager>()
                                .unwrap()
                                .set_active_client(&client_session.client(), false);

                            client.log_out().await;
                        }
                        SendPhoneNumberResult::Err(e) => {
                            log::error!("Failed to use phone number: {e:?}");
                            imp.toast_overlay.add_toast(
                                adw::Toast::builder()
                                    .title(e.message)
                                    .priority(adw::ToastPriority::High)
                                    .build(),
                            );
                        }
                        SendPhoneNumberResult::Ok => {}
                    }
                }

                self.freeze(false, false);
            }
        }
    }

    pub(crate) async fn request_qr_code(&self) {
        self.freeze(true, true);

        let abort_registration = self.setup_abort_handle();
        let result =
            future::Abortable::new(self.model().unwrap().request_qr_code(), abort_registration)
                .await;

        if let Err(_) = result {
            // TODO
        }

        self.freeze(true, false);
    }

    fn setup_expressions(&self) {
        let imp = self.imp();

        Self::this_expression("model")
            .chain_property::<model::ClientAuthWaitPhoneNumber>("country-list")
            .bind(&imp.input.get(), "model", Some(self));

        if let Some(model) = self.model() {
            imp.input.set_number(&self.model().unwrap().phone_number());

            if let Some(client_manager) = model
                .auth()
                .and_then(|auth| auth.client())
                .and_then(|client| client.manager())
            {
                self.action_set_enabled("login.exit", !client_manager.sessions().is_empty());
                client_manager.connect_items_changed(
                    clone!(@weak self as obj => move |client_manager, _, _, _| {
                        obj.action_set_enabled("login.exit", !client_manager.sessions().is_empty());
                    }),
                );
            }
        }
    }

    fn freeze(&self, qr: bool, freeze: bool) {
        let imp = self.imp();

        imp.input.set_sensitive(!freeze);
        imp.next_button_stack
            .set_visible_child_name(if !qr && freeze { "spinner" } else { "label" });
        imp.qr_code_spinner.set_spinning(qr && freeze);

        self.action_set_enabled("login.next", !freeze);
        self.action_set_enabled("login.use-qr-code", !qr || !freeze);
    }

    fn cancel(&self) {
        if let Some(handle) = &*self.imp().abort_handle.borrow() {
            handle.abort();
        }
    }

    fn setup_abort_handle(&self) -> future::AbortRegistration {
        let (abort_handle, abort_registration) = future::AbortHandle::new_pair();
        if let Some(handle) = self.imp().abort_handle.replace(Some(abort_handle)) {
            handle.abort();
        }

        abort_registration
    }
}
