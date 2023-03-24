use glib::closure;
use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::login2::Login2;
use crate::session::Session;
use crate::tdlib;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(resource = "/app/drey/paper-plane/ui/client.ui")]
    #[properties(wrapper_type = super::Client)]
    pub(crate) struct Client {
        #[property(get, set)]
        pub(super) model: glib::WeakRef<tdlib::Client>,
        #[template_child]
        pub(super) bin: TemplateChild<adw::Bin>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Client {
        const NAME: &'static str = "ClientView";
        type Type = super::Client;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action_async("login.previous", None, |widget, _, _| async move {
                // widget.previous().await;
            });
            klass.install_action_async("login.next", None, |widget, _, _| async move {
                // widget.next().await;
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Client {
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

    impl WidgetImpl for Client {}
}

glib::wrapper! {
    pub(crate) struct Client(ObjectSubclass<imp::Client>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&crate::tdlib::Client> for Client {
    fn from(model: &crate::tdlib::Client) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl Client {
    fn setup_expressions(&self) {
        Self::this_expression("model")
            .chain_property::<crate::tdlib::Client>("state")
            .chain_closure::<gtk::Widget>(closure!(|_: Self, state: glib::Object| {
                if let Some(state) = state.downcast_ref::<tdlib::ClientAuth>() {
                    Login2::from(state).upcast::<gtk::Widget>()
                } else if let Some(state) = state.downcast_ref::<tdlib::ClientSession>() {
                    Session::from(state).upcast::<gtk::Widget>()
                } else {
                    panic!();
                }
            }))
            .bind(&*self.imp().bin, "child", Some(self));
    }
}
