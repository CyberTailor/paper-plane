use gettextrs::gettext;
use glib::closure;
use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use tdlib::enums::AuthenticationCodeType;

use crate::i18n::gettext_f;
use crate::tdlib as model;
use crate::tdlib::BoxedAuthorizationStateWaitCode;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(resource = "/app/drey/paper-plane/ui/login-code.ui")]
    #[properties(wrapper_type = super::Code)]
    pub(crate) struct Code {
        #[property(get, set)]
        pub(super) auth: glib::WeakRef<model::ClientAuth>,
        #[property(get, set)]
        pub(super) model: glib::WeakRef<model::ClientAuthWaitCode>,
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub(super) status_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub(super) input_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) entry_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) next_button_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) resend_link_button: TemplateChild<gtk::LinkButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Code {
        const NAME: &'static str = "LoginCode";
        type Type = super::Code;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("login.previous", None, |widget, _, _| widget.previous());

            klass.install_action_async("login.next", None, |widget, _, _| async move {
                widget.next().await;
            });

            klass.install_action_async("login.resend-auth-code", None, |widget, _, _| async move {
                widget.resend_auth_code().await;
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Code {
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
            self.obj().first_child().unwrap().unparent();
        }
    }

    impl WidgetImpl for Code {}
}

glib::wrapper! {
    pub(crate) struct Code(ObjectSubclass<imp::Code>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&model::ClientAuthWaitCode> for Code {
    fn from(model: &model::ClientAuthWaitCode) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl Code {
    pub(crate) fn previous(&self) {
        self.model().unwrap().auth().unwrap().reset();
    }

    pub(crate) async fn next(&self) {
        if let Some(model) = self.model() {
            self.freeze(true);

            let imp = self.imp();

            let result = model.send_code(imp.entry_row.text().into()).await;
            if let Err(e) = result {
                log::error!("Failed to authenticate: {e:?}");
                imp.toast_overlay.add_toast(
                    adw::Toast::builder()
                        .title(e.message)
                        .priority(adw::ToastPriority::High)
                        .build(),
                );
            }

            self.freeze(false);
        }
    }

    pub(crate) async fn resend_auth_code(&self) {
        if let Err(e) = self.model().unwrap().resend_auth_code().await {
            // TODO
        }
    }

    fn setup_expressions(&self) {
        let imp = self.imp();

        let model_expr = Self::this_expression("model");
        let data_expr = model_expr.chain_property::<model::ClientAuthWaitCode>("data");
        let countdown_expr = model_expr.chain_property::<model::ClientAuthWaitCode>("countdown");

        gtk::ClosureExpression::new::<String>(
            [data_expr.as_ref(), countdown_expr.as_ref()],
            closure!(
                |_: Self, data: BoxedAuthorizationStateWaitCode, countdown: i32| {
                    data.0
                        .code_info
                        .next_type
                        .map(|type_| {
                            if countdown > 0 {
                                gettext_f(
                                    "Send code via {type} (may still arive within {countdown} seconds)",
                                    &[("type", &stringify_auth_code_type(type_)), ("countdown", &countdown.to_string())],
                                )
                            } else {
                                gettext_f(
                                    "Send code via {type}",
                                    &[("type", &stringify_auth_code_type(type_))],
                                )
                            }
                        })
                        .unwrap_or_else(String::new)
                }
            ),
        )
        .bind(&*imp.resend_link_button, "label", Some(self));

        data_expr
            .chain_closure::<bool>(closure!(
                |_: Self, data: BoxedAuthorizationStateWaitCode| {
                    data.0.code_info.next_type.is_some()
                }
            ))
            .bind(&*imp.resend_link_button, "visible", Some(self));

        data_expr
            .chain_closure::<String>(closure!(
                |_: Self, data: BoxedAuthorizationStateWaitCode| {
                    gettext_f(
                        "The code will arrive to you via {type}.",
                        &[("type", &stringify_auth_code_type(data.0.code_info.r#type))],
                    )
                }
            ))
            .bind(&*imp.status_page, "description", Some(self));
    }

    fn freeze(&self, freeze: bool) {
        let imp = self.imp();

        imp.input_box.set_sensitive(!freeze);
        imp.next_button_stack
            .set_visible_child_name(if freeze { "spinner" } else { "label" });

        self.action_set_enabled("login.next", !freeze);
        self.action_set_enabled("login.resend-auth-code", !freeze);
    }
}

fn stringify_auth_code_type(code_type: AuthenticationCodeType) -> String {
    match code_type {
        // Translators: This is an authentication method
        AuthenticationCodeType::TelegramMessage(_) => gettext("Telegram"),
        // Translators: This is an authentication method
        AuthenticationCodeType::Sms(_)
        | AuthenticationCodeType::FirebaseAndroid(_)
        | AuthenticationCodeType::FirebaseIos(_) => gettext("SMS"),
        // Translators: This is an authentication method
        AuthenticationCodeType::Call(_) => gettext("Call"),
        // Translators: This is an authentication method
        AuthenticationCodeType::FlashCall(_) => gettext("Flash Call"),
        // Translators: This is an authentication method
        AuthenticationCodeType::MissedCall(_) => gettext("Missed Call"),
        // Translators: This is an authentication method
        AuthenticationCodeType::Fragment(_) => gettext("Fragment"),
    }
}
