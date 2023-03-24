use glib::closure;
use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::tdlib as model;
use crate::tdlib::BoxedAuthorizationStateWaitPassword;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(resource = "/app/drey/paper-plane/ui/login-password.ui")]
    #[properties(wrapper_type = super::Password)]
    pub(crate) struct Password {
        #[property(get, set)]
        pub(super) model: glib::WeakRef<model::ClientAuthWaitPassword>,
        #[template_child]
        pub(super) toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub(super) input_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) entry_row: TemplateChild<adw::PasswordEntryRow>,
        #[template_child]
        pub(super) hint_action_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) hint_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) next_button_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) forgot_password_link_button: TemplateChild<gtk::LinkButton>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Password {
        const NAME: &'static str = "LoginPassword";
        type Type = super::Password;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("login.previous", None, |widget, _, _| widget.previous());

            klass.install_action_async("login.next", None, |widget, _, _| async move {
                widget.next().await;
            });

            klass.install_action_async("login.go-to-forgot-password-page", None, |widget, _, _| async move {
                widget.forgot_password().await;
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Password {
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

    impl WidgetImpl for Password {}
}

glib::wrapper! {
    pub(crate) struct Password(ObjectSubclass<imp::Password>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&model::ClientAuthWaitPassword> for Password {
    fn from(model: &model::ClientAuthWaitPassword) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl Password {
    pub(crate) fn previous(&self) {
        self.model().unwrap().auth().unwrap().reset();
    }

    pub(crate) async fn next(&self) {
        if let Some(model) = self.model() {
            self.freeze(true);

            let imp = self.imp();

            let result = model.send_password(imp.entry_row.text().into()).await;
            if let Err(e) = result {
                log::error!("Failed to verify password: {e:?}");
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

    pub(crate) async fn forgot_password(&self) {
        // TODO
    }

    fn setup_expressions(&self) {
        let imp = self.imp();

        let data_expr =
            Self::this_expression("model").chain_property::<model::ClientAuthWaitPassword>("data");

        data_expr
            .chain_closure::<bool>(closure!(
                |_: Self, data: BoxedAuthorizationStateWaitPassword| {
                    !data.0.password_hint.is_empty()
                }
            ))
            .bind(&*imp.hint_action_row, "visible", Some(self));

        data_expr
            .chain_closure::<String>(closure!(
                |_: Self, data: BoxedAuthorizationStateWaitPassword| data.0.password_hint
            ))
            .bind(&*imp.hint_label, "label", Some(self));
    }

    fn freeze(&self, freeze: bool) {
        let imp = self.imp();

        imp.input_box.set_sensitive(!freeze);
        imp.next_button_stack
            .set_visible_child_name(if freeze { "spinner" } else { "label" });

        self.action_set_enabled("login.next", !freeze);
        self.action_set_enabled("login.go-to-forgot-password-page", !freeze);
    }
}
