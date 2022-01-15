use glib::closure;
use gtk::{glib, prelude::*, subclass::prelude::*, CompositeTemplate};

use crate::session::User;

mod imp {
    use super::*;
    use adw::subclass::prelude::AdwWindowImpl;
    use once_cell::sync::{Lazy, OnceCell};

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/com/github/melix99/telegrand/ui/content-user-dialog.ui")]
    pub struct UserDialog {
        pub user: OnceCell<User>,
        #[template_child]
        pub name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub mobile_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub username_row: TemplateChild<adw::ActionRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UserDialog {
        const NAME: &'static str = "ContentUserDialog";
        type Type = super::UserDialog;
        type ParentType = adw::Window;

        fn class_init(klass: &mut Self::Class) {
            Self::bind_template(klass);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for UserDialog {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::new(
                    "user",
                    "User",
                    "The user displayed by this dialog",
                    User::static_type(),
                    glib::ParamFlags::READWRITE | glib::ParamFlags::CONSTRUCT_ONLY,
                )]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(
            &self,
            _obj: &Self::Type,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "user" => self.user.set(value.get().unwrap()).unwrap(),
                _ => unimplemented!(),
            }
        }

        fn property(&self, obj: &Self::Type, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "user" => obj.user().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self, obj: &Self::Type) {
            self.parent_constructed(obj);
            obj.setup_expressions();
        }
    }

    impl WidgetImpl for UserDialog {}
    impl WindowImpl for UserDialog {}
    impl AdwWindowImpl for UserDialog {}
}

glib::wrapper! {
    pub struct UserDialog(ObjectSubclass<imp::UserDialog>)
        @extends gtk::Widget, gtk::Window, adw::Window;
}

impl UserDialog {
    pub fn new(parent_window: &Option<gtk::Window>, user: &User) -> Self {
        glib::Object::new(&[("transient-for", parent_window), ("user", user)])
            .expect("Failed to create UserDialog")
    }

    fn setup_expressions(&self) {
        let self_ = imp::UserDialog::from_instance(self);
        let user_expression = UserDialog::this_expression("user");

        // Bind the name
        User::full_name_expression(&user_expression).bind(&*self_.name_label, "label", Some(self));

        // Bind the phone number
        let phone_number_expression = user_expression.chain_property::<User>("phone-number");
        phone_number_expression
            .chain_closure::<String>(closure!(|_: UserDialog, phone_number: String| {
                format!("+{}", phone_number)
            }))
            .bind(&*self_.mobile_row, "title", Some(self));
        phone_number_expression
            .chain_closure::<bool>(closure!(|_: UserDialog, phone_number: String| {
                !phone_number.is_empty()
            }))
            .bind(&*self_.mobile_row, "visible", Some(self));

        // Bind the username
        let username_expression = user_expression.chain_property::<User>("username");
        username_expression
            .chain_closure::<String>(closure!(|_: UserDialog, username: String| {
                format!("@{}", username)
            }))
            .bind(&*self_.username_row, "title", Some(self));
        username_expression
            .chain_closure::<bool>(closure!(|_: UserDialog, username: String| {
                !username.is_empty()
            }))
            .bind(&*self_.username_row, "visible", Some(self));
    }

    pub fn user(&self) -> Option<&User> {
        let self_ = imp::UserDialog::from_instance(self);
        self_.user.get()
    }
}
