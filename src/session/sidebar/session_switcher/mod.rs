mod add_account;
mod avatar_with_selection;
mod item;
mod session_entry_row;

use std::convert::TryFrom;

use glib::subclass::InitializingObject;
use gtk::gio;
use gtk::gio::ListModel;
use gtk::gio::ListStore;
use gtk::glib;
use gtk::glib::clone;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use gtk::SelectionModel;

use super::session_switcher::item::ExtraItemObj;
use super::session_switcher::item::Item as SessionSwitcherItem;
use crate::session::Session;

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/app/drey/paper-plane/ui/sidebar-session-switcher.ui")]
    pub(crate) struct SessionSwitcher {
        #[template_child]
        pub(super) entries: TemplateChild<gtk::ListView>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SessionSwitcher {
        const NAME: &'static str = "SessionSwitcher";
        type Type = super::SessionSwitcher;
        type ParentType = gtk::Popover;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_accessible_role(gtk::AccessibleRole::Dialog);
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for SessionSwitcher {
        fn constructed(&self) {
            self.parent_constructed();

            self.entries.connect_activate(|list_view, index| {
                if let Some(Ok(item)) = list_view
                    .model()
                    .and_then(|model| model.item(index))
                    .map(SessionSwitcherItem::try_from)
                {
                    match item {
                        SessionSwitcherItem::Session(session, _) => {
                            session
                                .parent()
                                .unwrap()
                                .downcast::<gtk::Stack>()
                                .unwrap()
                                .set_visible_child(&session);
                        }
                        SessionSwitcherItem::AddAccount => {
                            /* ignored - as this is handled separately in the AddAccountItem */
                        }
                        other => unreachable!("Unexpected item: {:?}", other),
                    }
                }
            });
        }

        fn dispose(&self) {
            self.entries.unparent();
        }
    }

    impl WidgetImpl for SessionSwitcher {}
    impl PopoverImpl for SessionSwitcher {}
}

glib::wrapper! {
    pub(crate) struct SessionSwitcher(ObjectSubclass<imp::SessionSwitcher>)
        @extends gtk::Widget, gtk::Popover,
        @implements gtk::Accessible, gio::ListModel;
}

impl SessionSwitcher {
    pub(crate) fn set_sessions(&self, sessions: SelectionModel, this_session: &Session) {
        let entries = self.imp().entries.get();

        // There is no permanent stuff to take care of,
        // so only bind and unbind are connected.
        let factory = &gtk::SignalListItemFactory::new();
        factory.connect_bind(clone!(@weak this_session => move |_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            list_item.set_selectable(false);
            let child = list_item
                .item()
                .map(SessionSwitcherItem::try_from)
                .and_then(Result::ok)
                .map(|item| {
                    // Given that all the account switchers are built per-session widget
                    // there is no need for callbacks or data bindings; just set the hint
                    // when building the entries and they will show correctly marked in
                    // each session widget.
                    let item = item.set_hint(this_session);

                    if item == SessionSwitcherItem::Separator {
                        list_item.set_activatable(false);
                    }

                    item
                })
                .as_ref()
                .map(SessionSwitcherItem::build_widget);

            list_item.set_child(child.as_ref());
        }));

        factory.connect_unbind(|_, list_item| {
            let list_item = list_item.downcast_ref::<gtk::ListItem>().unwrap();
            list_item.set_child(gtk::Widget::NONE);
        });

        entries.set_factory(Some(factory));

        let session_sorter = gtk::CustomSorter::new(move |obj1, obj2| {
            let session1 = obj1
                .downcast_ref::<gtk::StackPage>()
                .unwrap()
                .child()
                .downcast::<Session>()
                .unwrap();
            let session2 = obj2
                .downcast_ref::<gtk::StackPage>()
                .unwrap()
                .child()
                .downcast::<Session>()
                .unwrap();

            session1
                .database_info()
                .0
                .directory_base_name
                .cmp(&session2.database_info().0.directory_base_name)
                .into()
        });

        let sessions_sort_model = gtk::SortListModel::new(Some(sessions), Some(session_sorter));

        let end_items = ExtraItemObj::list_store();

        let items_split = ListStore::new::<ListModel>();
        items_split.append(&sessions_sort_model);
        items_split.append(&end_items);

        let items = gtk::FlattenListModel::new(Some(items_split));
        let selectable_items = &gtk::NoSelection::new(Some(items));

        entries.set_model(Some(selectable_items));
    }
}
