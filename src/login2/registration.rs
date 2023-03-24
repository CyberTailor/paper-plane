use adw::prelude::MessageDialogExtManual;
use adw::traits::MessageDialogExt;
use gettextrs::gettext;
use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::gio;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::tdlib as model;
use crate::utils::parse_formatted_text;
use crate::utils::spawn;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(resource = "/app/drey/paper-plane//ui/login-registration.ui")]
    #[properties(wrapper_type = super::Registration)]
    pub(crate) struct Registration {
        #[property(get, set)]
        pub(super) model: glib::WeakRef<model::ClientAuthWaitRegistration>,
        #[template_child]
        pub(super) first_name_entry_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) last_name_entry_row: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub(super) error_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) tos_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Registration {
        const NAME: &'static str = "LoginRegistration";
        type Type = super::Registration;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action("login.show-tos-dialog", None, move |widget, _, _| {
                widget.show_tos_dialog(false)
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Registration {
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
            let obj = &*self.obj();
            obj.first_child().unwrap().unparent();
            obj.first_child().unwrap().unparent();
        }
    }

    impl WidgetImpl for Registration {}
}

glib::wrapper! {
    pub(crate) struct Registration(ObjectSubclass<imp::Registration>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&model::ClientAuthWaitRegistration> for Registration {
    fn from(model: &model::ClientAuthWaitRegistration) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl Registration {
    fn show_tos_dialog(&self, user_needs_to_accept: bool) {
        if let Some(model) = self.model() {
            if let Some(data) = model.data() {
                let dialog = adw::MessageDialog::builder()
                    .body_use_markup(true)
                    .body(parse_formatted_text(data.0.terms_of_service.text))
                    .transient_for(self.root().unwrap().downcast_ref::<gtk::Window>().unwrap())
                    .build();

                if user_needs_to_accept {
                    dialog.set_heading(Some(&gettext("Do you accept the Terms of Service?")));
                    dialog.add_responses(&[("no", &gettext("_No")), ("yes", &gettext("_Yes"))]);
                    dialog.set_default_response(Some("no"));
                } else {
                    dialog.set_heading(Some(&gettext("Terms of Service")));
                    dialog.add_response("ok", &gettext("_OK"));
                    dialog.set_default_response(Some("ok"));
                }

                dialog.choose(
                    gio::Cancellable::NONE,
                    clone!(@weak self as obj => move |response| {
                        if response == "no" {
                            // If the user declines the ToS, don't proceed and just stay in
                            // the view but unfreeze it again.
                            // TODO: unfreeze
                            // obj.unfreeze();
                        } else if response == "yes" {
                            // User has accepted the ToS, so we can proceed in the login
                            // flow.
                            spawn(clone!(@weak obj, @weak model => async move {
                                let imp = obj.imp();
                                model.send_registration(
                                    imp.first_name_entry_row.text().to_string(),
                                    imp.last_name_entry_row.text().to_string()
                                )
                                .await;
                            }));
                        }
                    }),
                );
            }
        }
    }

    fn setup_expressions(&self) {
        // Self::this_expression("model")
        //     .chain_property::<ClientAuthPhaseWaitPhoneNumber>("country_list")
        //     .bind(&*self.imp().input, "model", Some(self));
    }
}
