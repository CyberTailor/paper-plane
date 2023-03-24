mod code;
mod other_device;
mod password;
mod phone_number;
mod registration;

use std::cell::Cell;

use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::components::AnimatedBin;
use crate::login2::code::Code;
use crate::login2::other_device::OtherDevice;
use crate::login2::password::Password;
use crate::login2::phone_number::PhoneNumber;
use crate::login2::registration::Registration;
use crate::tdlib as model;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(string = r#"
        using Gtk 4.0;

        template $Login2 {
            layout-manager: BoxLayout {
                orientation: vertical;
            };

            $AnimatedBin bin { }
        }
    "#)]
    #[properties(wrapper_type = super::Login2)]
    pub(crate) struct Login2 {
        pub(super) phone_number_entered: Cell<bool>,
        #[property(get, set = Self::set_model, construct, explicit_notify)]
        pub(super) model: glib::WeakRef<model::ClientAuth>,
        #[template_child]
        pub(super) bin: TemplateChild<AnimatedBin>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Login2 {
        const NAME: &'static str = "Login2";
        type Type = super::Login2;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Login2 {
        fn properties() -> &'static [glib::ParamSpec] {
            Self::derived_properties()
        }
        fn set_property(&self, id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            self.derived_set_property(id, value, pspec)
        }
        fn property(&self, id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            self.derived_property(id, pspec)
        }

        fn dispose(&self) {
            self.obj().first_child().unwrap().unparent();
        }
    }

    impl WidgetImpl for Login2 {}

    impl Login2 {
        fn set_model(&self, model: &model::ClientAuth) {
            let obj = &*self.obj();
            if obj.model().as_ref() == Some(model) {
                return;
            }

            if let Some(state) = model.state() {
                obj.update_state(state);
            }
            model.connect_state_notify(clone!(@weak obj => move |auth| {
                if let Some(state) = auth.state() {
                    obj.update_state(state);
                }
            }));

            self.model.set(Some(model));
            obj.notify("model");
        }
    }
}

glib::wrapper! {
    pub(crate) struct Login2(ObjectSubclass<imp::Login2>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&model::ClientAuth> for Login2 {
    fn from(model: &model::ClientAuth) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl Login2 {
    fn update_state(&self, state: glib::Object) {
        let imp = self.imp();
        if let Some(state) = state.downcast_ref::<model::ClientAuthWaitPhoneNumber>() {
            let phone_number = PhoneNumber::from(state).upcast();
            if imp.phone_number_entered.get() {
                imp.bin.prepend(&phone_number);
            } else {
                imp.bin.append(&phone_number);
            }
        } else {
            imp.phone_number_entered.set(true);

            imp.bin.append(&if let Some(state) =
                state.downcast_ref::<model::ClientAuthWaitOtherDeviceConfirmation>()
            {
                OtherDevice::from(state).upcast()
            } else if let Some(state) = state.downcast_ref::<model::ClientAuthWaitCode>() {
                Code::from(state).upcast()
            } else if let Some(state) = state.downcast_ref::<model::ClientAuthWaitRegistration>() {
                Registration::from(state).upcast()
            } else if let Some(state) = state.downcast_ref::<model::ClientAuthWaitPassword>() {
                Password::from(state).upcast()
            } else {
                unreachable!()
            });
        }
    }
}
