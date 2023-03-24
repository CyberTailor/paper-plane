use glib::prelude::ObjectExt;
use glib::subclass::prelude::*;
use glib::ParamSpecBuilderExt;
use gtk::glib;
use gtk::prelude::ToValue;
use gtk::subclass::widget::CompositeTemplateClass;
use gtk::subclass::widget::CompositeTemplateInitializingExt;
use gtk::subclass::widget::WidgetClassSubclassExt;
use gtk::subclass::widget::WidgetImpl;
use gtk::traits::WidgetExt;
use gtk::CompositeTemplate;
use gtk::TemplateChild;
use once_cell::sync::Lazy;

mod imp {
    use super::*;

    #[derive(Debug, CompositeTemplate, Default)]
    #[template(string = r#"
        using Gtk 4.0;
        using Adw 1;

        template $AnimatedBin {
            layout-manager: BinLayout { };

            vexpand: true;
            Adw.Leaflet stack {
                can-unfold: false;
            }
        }
    "#)]
    pub(crate) struct AnimatedBin {
        #[template_child]
        pub(super) stack: TemplateChild<adw::Leaflet>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AnimatedBin {
        const NAME: &'static str = "AnimatedBin";
        type Type = super::AnimatedBin;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for AnimatedBin {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecEnum::builder_with_default::<adw::LeafletTransitionType>(
                        "transition-type",
                        adw::LeafletTransitionType::Over,
                    )
                    .explicit_notify()
                    .build(),
                ]
            });
            PROPERTIES.as_ref()
        }
        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "transition-type" => self.obj().set_transition_type(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }
        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "transition-type" => self.obj().transition_type().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            self.stack.connect_child_transition_running_notify(|stack| {
                if !stack.is_child_transition_running() {
                    let mut child = stack.first_child();
                    while let Some(child_) = child {
                        child = child_.next_sibling();
                        if stack.visible_child().as_ref() != Some(&child_) {
                            stack.remove(&child_);
                        }
                    }
                }
            });
        }

        fn dispose(&self) {
            self.stack.unparent();
        }
    }

    impl WidgetImpl for AnimatedBin {}
}

glib::wrapper! {
    pub(crate) struct AnimatedBin(ObjectSubclass<imp::AnimatedBin>) @extends gtk::Widget;
}

impl AnimatedBin {
    pub(crate) fn append(&self, child: &gtk::Widget) {
        self.imp().stack.append(child);
        self.imp().stack.set_visible_child(child);
    }

    pub(crate) fn prepend(&self, child: &gtk::Widget) {
        self.imp().stack.prepend(child);
        self.imp().stack.set_visible_child(child);
    }

    pub(crate) fn transition_type(&self) -> adw::LeafletTransitionType {
        self.imp().stack.transition_type()
    }

    pub(crate) fn set_transition_type(&self, transition_type: adw::LeafletTransitionType) {
        self.imp().stack.set_transition_type(transition_type);
        self.notify("transition-type");
    }
}
