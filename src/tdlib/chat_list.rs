use std::cell::Cell;
use std::cell::RefCell;
use std::cell::RefMut;
use std::collections::BTreeMap;

use glib::clone;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use once_cell::sync::Lazy;
use tdlib::functions;
use tdlib::types::ChatPosition as TdChatPosition;

use super::ClientSession;
use crate::tdlib::Chat;
use crate::tdlib::ChatListItem;
use crate::utils::spawn;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub(crate) struct ChatList {
        // order -> item
        pub(super) list: RefCell<BTreeMap<i64, ChatListItem>>,
        pub(super) unread_count: Cell<i32>,
        pub(super) client_session: glib::WeakRef<ClientSession>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChatList {
        const NAME: &'static str = "ChatList";
        type Type = super::ChatList;
        type Interfaces = (gio::ListModel,);
    }

    impl ObjectImpl for ChatList {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecInt::builder("unread-count")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<ClientSession>("client-session")
                        .construct_only()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "client-session" => self.client_session.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "unread-count" => obj.unread_count().to_value(),
                "client-session" => obj.client_session().to_value(),
                _ => unimplemented!(),
            }
        }
    }

    impl ListModelImpl for ChatList {
        fn item_type(&self) -> glib::Type {
            ChatListItem::static_type()
        }

        fn n_items(&self) -> u32 {
            self.list.borrow().len() as u32
        }

        fn item(&self, position: u32) -> Option<glib::Object> {
            self.list
                .borrow()
                .iter()
                .nth(position as usize)
                .map(|(_, c)| c.upcast_ref())
                .cloned()
        }
    }
}

glib::wrapper! {
    pub(crate) struct ChatList(ObjectSubclass<imp::ChatList>)
        @implements gio::ListModel;
}

impl From<&ClientSession> for ChatList {
    fn from(client_session: &ClientSession) -> Self {
        glib::Object::builder()
            .property("client-session", client_session)
            .build()
    }
}

impl ChatList {
    pub(crate) fn client_session(&self) -> ClientSession {
        self.imp().client_session.upgrade().unwrap()
    }

    pub(crate) fn fetch(&self) {
        spawn(clone!(@weak self as obj => async move {
            let result = functions::load_chats(None, 20, obj.client_session().client().id()).await;

            if let Err(err) = result {
                // Error 404 means that all chats have been loaded
                if err.code != 404 {
                    log::error!("Received an error for LoadChats: {}", err.code);
                }
            } else {
                obj.fetch();
            }
        }));
    }

    pub(crate) fn find_chat_item(&self, chat_id: i64) -> Option<ChatListItem> {
        self.imp()
            .list
            .borrow()
            .iter()
            .find(|(_, item)| item.chat().id() == chat_id)
            .map(|(_, item)| item)
            .cloned()
    }

    pub(crate) fn update_chat_position(&self, chat: &Chat, position: &TdChatPosition) {
        let imp = self.imp();
        let mut list = imp.list.borrow_mut();

        match find_chat_item_position(&list, chat.id()) {
            Some((old_pos, old_order)) => {
                let item = list.remove(&old_order).unwrap();

                drop(list);
                self.items_changed(old_pos as u32, 1, 0);

                if position.order != 0 {
                    item.update(position);

                    self.insert_item(imp.list.borrow_mut(), item, position.order);
                }
            }
            None => self.insert_item(list, ChatListItem::new(chat, position), position.order),
        }
    }

    fn insert_item(
        &self,
        mut list: RefMut<BTreeMap<i64, ChatListItem>>,
        item: ChatListItem,
        order: i64,
    ) {
        let chat_id = item.chat().id();

        // Invert the sign to have a descending order
        list.insert(-order, item);

        let position = find_chat_item_position(&list, chat_id).unwrap().0;

        drop(list);
        self.items_changed(position as u32, 0, 1);
    }

    pub(crate) fn update_unread_message_count(&self, unread_count: i32) {
        if self.unread_count() == unread_count {
            return;
        }
        self.imp().unread_count.set(unread_count);
        self.notify("unread-count");
    }

    pub(crate) fn unread_count(&self) -> i32 {
        self.imp().unread_count.get()
    }
}

fn find_chat_item_position(
    list: &RefMut<BTreeMap<i64, ChatListItem>>,
    chat_id: i64,
) -> Option<(usize, i64)> {
    list.iter()
        .enumerate()
        .find(|(_, (_, item))| item.chat().id() == chat_id)
        .map(|(pos, (order, _))| (pos, *order))
}
