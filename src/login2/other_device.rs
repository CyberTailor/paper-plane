use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::gdk;
use gtk::glib;
use gtk::glib::clone;
use gtk::glib::closure;
use gtk::glib::WeakRef;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use crate::tdlib as model;
use crate::tdlib::BoxedAuthorizationStateWaitOtherDeviceConfirmation;
use crate::utils;
use crate::utils::spawn;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[template(resource = "/app/drey/paper-plane/ui/login-other-device.ui")]
    #[properties(wrapper_type = super::OtherDevice)]
    pub(crate) struct OtherDevice {
        #[property(get, set)]
        pub(super) model: WeakRef<model::ClientAuthWaitOtherDeviceConfirmation>,
        #[template_child]
        pub(super) image: TemplateChild<gtk::Image>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for OtherDevice {
        const NAME: &'static str = "LoginOtherDevice";
        type Type = super::OtherDevice;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();

            klass.install_action_async("login.exit", None, |widget, _, _| async move {
                widget.exit().await;
            });

            klass.install_action("login.use-phone-number", None, |widget, _, _| {
                widget.use_phone_number();
            });
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for OtherDevice {
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

    impl WidgetImpl for OtherDevice {}
}

glib::wrapper! {
    pub(crate) struct OtherDevice(ObjectSubclass<imp::OtherDevice>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl From<&model::ClientAuthWaitOtherDeviceConfirmation> for OtherDevice {
    fn from(model: &model::ClientAuthWaitOtherDeviceConfirmation) -> Self {
        glib::Object::builder().property("model", model).build()
    }
}

impl OtherDevice {
    pub(crate) async fn exit(&self) {
        if let Some(client) = self
            .model()
            .and_then(|model| model.auth())
            .and_then(|auth| auth.client())
        {
            client.log_out().await;
        }
    }

    pub(crate) fn use_phone_number(&self) {
        if let Some(client) = self
            .model()
            .and_then(|model| model.auth())
            .and_then(|auth| auth.client())
        {
            // We actually need to logout to stop tdlib sending us new links.
            // https://github.com/tdlib/td/issues/1645
            if let Some(client_manager) = client.manager() {
                client_manager.add_new_session(client.database_info().0.use_test_dc);
            }
            spawn(async move {
                client.log_out().await;
            });
        }
    }

    fn setup_expressions(&self) {
        Self::this_expression("model")
            .chain_property::<model::ClientAuthWaitOtherDeviceConfirmation>("data")
            .chain_closure::<gdk::MemoryTexture>(closure!(
                |obj: Self, data: BoxedAuthorizationStateWaitOtherDeviceConfirmation| {
                    let size = obj.imp().image.pixel_size() as usize;
                    let bytes_per_pixel = 3;

                    let data_luma = qrcode_generator::to_image_from_str(
                        data.0.link,
                        qrcode_generator::QrCodeEcc::Low,
                        size,
                    )
                    .unwrap();

                    let bytes = glib::Bytes::from_owned(
                        // gdk::Texture only knows 3 byte color spaces, thus convert Luma.
                        data_luma
                            .into_iter()
                            .flat_map(|p| (0..bytes_per_pixel).map(move |_| p))
                            .collect::<Vec<_>>(),
                    );

                    gdk::MemoryTexture::new(
                        size as i32,
                        size as i32,
                        gdk::MemoryFormat::R8g8b8,
                        &bytes,
                        size * bytes_per_pixel,
                    )
                }
            ))
            .bind(&*self.imp().image, "paintable", Some(self));

        if let Some(model) = self.model() {
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
}
