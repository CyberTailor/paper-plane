use std::cell::Cell;

use glib::closure;
use glib::subclass::InitializingObject;
use glib::Properties;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

use super::avatar_with_selection::AvatarWithSelection;
use crate::expressions;
use crate::tdlib::ClientSession;
use crate::tdlib::User;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate, Properties)]
    #[properties(wrapper_type = super::SessionEntryRow)]
    #[template(resource = "/app/drey/paper-plane/ui/session-entry-row.ui")]
    pub(crate) struct SessionEntryRow {
        #[property(get, set, construct_only)]
        pub(super) session: glib::WeakRef<ClientSession>,
        #[property(get, set, construct_only)]
        pub(super) hint: Cell<bool>,
        #[template_child]
        pub(super) account_avatar: TemplateChild<AvatarWithSelection>,
        #[template_child]
        pub(super) center_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) display_name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) username_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) unread_count_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionEntryRow {
        const NAME: &'static str = "SessionEntryRow";
        type Type = super::SessionEntryRow;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            AvatarWithSelection::static_type();
            klass.bind_template();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SessionEntryRow {
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
            self.account_avatar.unparent();
            self.center_box.unparent();
            self.unread_count_label.unparent();
        }
    }

    impl WidgetImpl for SessionEntryRow {}
}

glib::wrapper! {
    pub(crate) struct SessionEntryRow(ObjectSubclass<imp::SessionEntryRow>)
        @extends gtk::Widget, @implements gtk::Accessible;
}

impl SessionEntryRow {
    pub(crate) fn new(session: &ClientSession, hinted: bool) -> Self {
        let obj: Self = glib::Object::builder().property("session", session).build();
        let imp = obj.imp();

        imp.account_avatar.set_selected(hinted);
        imp.display_name_label
            .set_css_classes(if hinted { &["bold"] } else { &[] });

        obj
    }

    fn setup_expressions(&self) {
        let imp = self.imp();
        let me_expression = Self::this_expression("session").chain_property::<ClientSession>("me");

        // Bind the name
        expressions::user_display_name(&me_expression).bind(
            &*imp.display_name_label,
            "label",
            Some(self),
        );

        // Bind the username
        let username_expression = me_expression.chain_property::<User>("username");
        username_expression
            .chain_closure::<String>(closure!(|_: Self, username: String| {
                format!("@{username}")
            }))
            .bind(&*imp.username_label, "label", Some(self));
        username_expression
            .chain_closure::<bool>(closure!(|_: Self, username: String| {
                !username.is_empty()
            }))
            .bind(&*imp.username_label, "visible", Some(self));
    }
}
