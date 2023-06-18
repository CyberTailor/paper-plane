use adw::subclass::prelude::PreferencesRowImpl;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(string = r#"
    using Gtk 4.0;
    using Adw 1;

    template $StyleSelectionRow : Adw.PreferencesRow {
        activatable: false;
        title: _("Color Scheme");

        Box {
            halign: center;
            margin-top: 15;
            margin-end: 3;
            margin-bottom: 15;
            margin-start: 3;
            spacing: 12;

            Box {
                orientation: vertical;
                spacing: 6;

                ToggleButton {
                    action-name: "app.style-variant";
                    action-target: "'default'";

                    $StyleVariantPreview {
                        color-scheme: default;
                    }
                }

                Label {
                    label: _("Follow System Colors");
                }
            }

            Box {
                orientation: vertical;
                spacing: 6;

                ToggleButton {
                    action-name: "app.style-variant";
                    action-target: "'light'";

                    $StyleVariantPreview {
                        color-scheme: prefer-light;
                    }
                }

                Label {
                    label: _("Light");
                }
            }

            Box {
                orientation: vertical;
                spacing: 6;

                ToggleButton {
                    action-name: "app.style-variant";
                    action-target: "'dark'";

                    $StyleVariantPreview {
                        color-scheme: prefer-dark;
                    }
                }

                Label {
                    label: _("Dark");
                }
            }
        }
    }
    "#)]
    pub(crate) struct StyleSelectionRow;

    #[glib::object_subclass]
    impl ObjectSubclass for StyleSelectionRow {
        const NAME: &'static str = "StyleSelectionRow";
        type Type = super::StyleSelectionRow;
        type ParentType = adw::PreferencesRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_css_name("styleselectionrow");
            crate::components::StyleVariantPreview::static_type();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StyleSelectionRow {}
    impl WidgetImpl for StyleSelectionRow {}
    impl ListBoxRowImpl for StyleSelectionRow {}
    impl PreferencesRowImpl for StyleSelectionRow {}
}

glib::wrapper! {
    pub(crate) struct StyleSelectionRow(ObjectSubclass<imp::StyleSelectionRow>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Actionable;
}
