use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;

use glib::clone;
use glib::prelude::ObjectExt;
use glib::prelude::ParamSpecBuilderExt;
use glib::prelude::ToValue;
use glib::subclass::prelude::*;
use gtk::glib;
use once_cell::sync::Lazy;
use once_cell::unsync::OnceCell;
use tdlib::enums::ChatList as TdChatList;
use tdlib::enums::NotificationSettingsScope;
use tdlib::enums::Update;
use tdlib::enums::{self};
use tdlib::functions;
use tdlib::types;
use tdlib::types::ChatPosition as TdChatPosition;
use tdlib::types::File;

use crate::tdlib::BasicGroup;
use crate::tdlib::BoxedScopeNotificationSettings;
use crate::tdlib::Chat;
use crate::tdlib::ChatList;
use crate::tdlib::Client;
use crate::tdlib::SecretChat;
use crate::tdlib::Supergroup;
use crate::tdlib::User;
use crate::utils::spawn;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub(crate) struct Session {
        pub(super) client: glib::WeakRef<Client>,

        pub(super) me: glib::WeakRef<User>,
        pub(super) main_chat_list: OnceCell<ChatList>,
        pub(super) archive_chat_list: OnceCell<ChatList>,
        pub(super) filter_chat_lists: RefCell<HashMap<i32, ChatList>>,
        pub(super) chats: RefCell<HashMap<i64, Chat>>,
        pub(super) users: RefCell<HashMap<i64, User>>,
        pub(super) basic_groups: RefCell<HashMap<i64, BasicGroup>>,
        pub(super) supergroups: RefCell<HashMap<i64, Supergroup>>,
        pub(super) secret_chats: RefCell<HashMap<i32, SecretChat>>,
        pub(super) private_chats_notification_settings:
            RefCell<Option<BoxedScopeNotificationSettings>>,
        pub(super) group_chats_notification_settings:
            RefCell<Option<BoxedScopeNotificationSettings>>,
        pub(super) channel_chats_notification_settings:
            RefCell<Option<BoxedScopeNotificationSettings>>,
        pub(super) downloading_files: RefCell<HashMap<i32, Vec<glib::Sender<File>>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Session {
        const NAME: &'static str = "ClientSession";
        type Type = super::Session;
    }

    impl ObjectImpl for Session {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![
                    glib::ParamSpecObject::builder::<Client>("client")
                        .construct_only()
                        .build(),
                    glib::ParamSpecObject::builder::<User>("me")
                        .read_only()
                        .build(),
                    glib::ParamSpecObject::builder::<ChatList>("main-chat-list")
                        .read_only()
                        .build(),
                    glib::ParamSpecBoxed::builder::<BoxedScopeNotificationSettings>(
                        "private-chats-notification-settings",
                    )
                    .read_only()
                    .build(),
                    glib::ParamSpecBoxed::builder::<BoxedScopeNotificationSettings>(
                        "group-chats-notification-settings",
                    )
                    .read_only()
                    .build(),
                    glib::ParamSpecBoxed::builder::<BoxedScopeNotificationSettings>(
                        "channel-chats-notification-settings",
                    )
                    .read_only()
                    .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "client" => self.client.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            let obj = self.obj();

            match pspec.name() {
                "client" => obj.client().to_value(),
                "me" => self.me.upgrade().to_value(),
                "main-chat-list" => obj.main_chat_list().to_value(),
                "private-chats-notification-settings" => {
                    obj.private_chats_notification_settings().to_value()
                }
                "group-chats-notification-settings" => {
                    obj.group_chats_notification_settings().to_value()
                }
                "channel-chats-notification-settings" => {
                    obj.channel_chats_notification_settings().to_value()
                }
                _ => unimplemented!(),
            }
        }
    }
}

glib::wrapper! {
    pub(crate) struct Session(ObjectSubclass<imp::Session>);
}

impl From<&Client> for Session {
    fn from(client: &Client) -> Self {
        glib::Object::builder().property("client", client).build()
    }
}

impl Session {
    pub(crate) fn client(&self) -> Client {
        self.imp().client.upgrade().unwrap()
    }

    pub(crate) fn me(&self) -> User {
        self.imp().me.upgrade().unwrap()
    }

    pub(crate) fn set_me(&self, me: &User) {
        let imp = self.imp();
        assert!(imp.me.upgrade().is_none());
        imp.me.set(Some(me));
        self.notify("me");
    }

    /// Returns the filter chat list of the specified id.
    pub(crate) fn filter_chat_list(&self, chat_filter_id: i32) -> ChatList {
        self.imp()
            .filter_chat_lists
            .borrow_mut()
            .entry(chat_filter_id)
            .or_insert_with(|| ChatList::from(self))
            .clone()
    }

    pub(crate) fn private_chats_notification_settings(
        &self,
    ) -> Option<BoxedScopeNotificationSettings> {
        self.imp()
            .private_chats_notification_settings
            .borrow()
            .clone()
    }

    /// Returns the `Chat` of the specified id, if present.
    pub(crate) fn try_chat(&self, chat_id: i64) -> Option<Chat> {
        self.imp().chats.borrow().get(&chat_id).cloned()
    }

    /// Returns the `Chat` of the specified id. Panics if the chat is not present.
    ///
    /// Note that TDLib guarantees that types are always returned before their ids,
    /// so if you use an id returned by TDLib, it should be expected that the
    /// relative `Chat` exists in the list.
    pub(crate) fn chat(&self, chat_id: i64) -> Chat {
        self.try_chat(chat_id).expect("Failed to get expected Chat")
    }

    pub(crate) fn upsert_user(&self, td_user: tdlib::types::User) -> User {
        let mut users = self.imp().users.borrow_mut();

        match users.entry(td_user.id) {
            Entry::Occupied(entry) => {
                let user = entry.get();
                user.update(td_user);
                user.to_owned()
            }
            Entry::Vacant(entry) => {
                let user = User::from_td_object(td_user, self);
                entry.insert(user.clone());
                user
            }
        }
    }

    /// Returns the `User` of the specified id. Panics if the user is not present.
    ///
    /// Note that TDLib guarantees that types are always returned before their ids,
    /// so if you use an id returned by TDLib, it should be expected that the
    /// relative `User` exists in the list.
    pub(crate) fn user(&self, user_id: i64) -> User {
        self.imp().users.borrow().get(&user_id).unwrap().clone()
    }

    /// Returns the `BasicGroup` of the specified id. Panics if the basic group is not present.
    ///
    /// Note that TDLib guarantees that types are always returned before their ids,
    /// so if you use an id returned by TDLib, it should be expected that the
    /// relative `BasicGroup` exists in the list.
    pub(crate) fn basic_group(&self, basic_group_id: i64) -> BasicGroup {
        self.imp()
            .basic_groups
            .borrow()
            .get(&basic_group_id)
            .expect("Failed to get expected BasicGroup")
            .clone()
    }

    /// Returns the `Supergroup` of the specified id. Panics if the supergroup is not present.
    ///
    /// Note that TDLib guarantees that types are always returned before their ids,
    /// so if you use an id returned by TDLib, it should be expected that the
    /// relative `Supergroup` exists in the list.
    pub(crate) fn supergroup(&self, supergroup_id: i64) -> Supergroup {
        self.imp()
            .supergroups
            .borrow()
            .get(&supergroup_id)
            .expect("Failed to get expected Supergroup")
            .clone()
    }

    /// Returns the `SecretChat` of the specified id. Panics if the secret chat is not present.
    ///
    /// Note that TDLib guarantees that types are always returned before their ids,
    /// so if you use an id returned by TDLib, it should be expected that the
    /// relative `SecretChat` exists in the list.
    pub(crate) fn secret_chat(&self, secret_chat_id: i32) -> SecretChat {
        self.imp()
            .secret_chats
            .borrow()
            .get(&secret_chat_id)
            .expect("Failed to get expected SecretChat")
            .clone()
    }

    /// Returns the main chat list.
    pub(crate) fn main_chat_list(&self) -> &ChatList {
        self.imp()
            .main_chat_list
            .get_or_init(|| ChatList::from(self))
    }

    /// Returns the list of archived chats.
    pub(crate) fn archive_chat_list(&self) -> &ChatList {
        self.imp()
            .archive_chat_list
            .get_or_init(|| ChatList::from(self))
    }

    pub(crate) fn fetch_chats(&self) {
        self.main_chat_list().fetch();
    }

    /// Fetches the contacts of the user.
    pub(crate) async fn fetch_contacts(&self) -> Result<Vec<User>, types::Error> {
        let result = functions::get_contacts(self.client().id()).await;

        result.map(|data| {
            let tdlib::enums::Users::Users(users) = data;
            users.user_ids.into_iter().map(|id| self.user(id)).collect()
        })
    }

    fn set_private_chats_notification_settings(
        &self,
        settings: Option<BoxedScopeNotificationSettings>,
    ) {
        if self.private_chats_notification_settings() == settings {
            return;
        }
        self.imp()
            .private_chats_notification_settings
            .replace(settings);
        self.notify("private-chats-notification-settings")
    }

    pub(crate) fn group_chats_notification_settings(
        &self,
    ) -> Option<BoxedScopeNotificationSettings> {
        self.imp()
            .group_chats_notification_settings
            .borrow()
            .clone()
    }

    fn set_group_chats_notification_settings(
        &self,
        settings: Option<BoxedScopeNotificationSettings>,
    ) {
        if self.group_chats_notification_settings() == settings {
            return;
        }
        self.imp()
            .group_chats_notification_settings
            .replace(settings);
        self.notify("group-chats-notification-settings")
    }

    pub(crate) fn channel_chats_notification_settings(
        &self,
    ) -> Option<BoxedScopeNotificationSettings> {
        self.imp()
            .channel_chats_notification_settings
            .borrow()
            .clone()
    }

    fn set_channel_chats_notification_settings(
        &self,
        settings: Option<BoxedScopeNotificationSettings>,
    ) {
        if self.channel_chats_notification_settings() == settings {
            return;
        }
        self.imp()
            .channel_chats_notification_settings
            .replace(settings);
        self.notify("channel-chats-notification-settings")
    }

    /// Downloads a file of the specified id. This will only return when the file
    /// downloading has completed or has failed.
    pub(crate) async fn download_file(&self, file_id: i32) -> Result<File, types::Error> {
        let client_id = self.client().id();
        let result = functions::download_file(file_id, 5, 0, 0, true, client_id).await;

        result.map(|data| {
            let tdlib::enums::File::File(file) = data;
            file
        })
    }

    /// Downloads a file of the specified id and calls a closure every time there's an update
    /// about the progress or when the download has completed.
    pub(crate) fn download_file_with_updates<F: Fn(File) + 'static>(&self, file_id: i32, f: F) {
        let (sender, receiver) = glib::MainContext::channel::<File>(glib::PRIORITY_DEFAULT);
        receiver.attach(None, move |file| {
            let is_downloading_active = file.local.is_downloading_active;
            f(file);
            glib::Continue(is_downloading_active)
        });

        let mut downloading_files = self.imp().downloading_files.borrow_mut();
        match downloading_files.entry(file_id) {
            Entry::Occupied(mut entry) => {
                entry.get_mut().push(sender);
            }
            Entry::Vacant(entry) => {
                entry.insert(vec![sender]);

                let client_id = self.client().id();
                spawn(clone!(@weak self as obj => async move {
                    let result = functions::download_file(file_id, 5, 0, 0, false, client_id).await;
                    match result {
                        Ok(enums::File::File(file)) => {
                            obj.handle_file_update(file);
                        }
                        Err(e) => {
                            log::warn!("Error downloading a file: {:?}", e);
                        }
                    }
                }));
            }
        }
    }

    pub(crate) fn cancel_download_file(&self, file_id: i32) {
        let client_id = self.client().id();
        spawn(async move {
            if let Err(e) = functions::cancel_download_file(file_id, false, client_id).await {
                log::warn!("Error canceling a file: {:?}", e);
            }
        });
    }

    fn handle_chat_position_update(&self, chat: &Chat, position: &TdChatPosition) {
        match &position.list {
            TdChatList::Main => {
                self.main_chat_list().update_chat_position(chat, position);
            }
            TdChatList::Archive => {
                self.archive_chat_list()
                    .update_chat_position(chat, position);
            }
            TdChatList::Folder(data) => {
                self.filter_chat_list(data.chat_folder_id)
                    .update_chat_position(chat, position);
            }
        }
    }

    fn handle_file_update(&self, file: File) {
        let mut downloading_files = self.imp().downloading_files.borrow_mut();
        if let Entry::Occupied(mut entry) = downloading_files.entry(file.id) {
            // Keep only the senders with which it was possible to send successfully.
            // It is indeed possible that the object that created the sender and receiver and
            // attached it to the default main context has been disposed in the meantime.
            // This is problematic if it is now tried to upgrade a weak reference of this object in
            // the receiver closure.
            // It will either panic directly if `@default-panic` is used or the sender will return
            // an error in the `SyncSender::send()` function if
            // `default-return glib::Continue(false)` is used. In the latter case, the Receiver
            // will be detached from the main context, which will cause the sending to fail.
            entry
                .get_mut()
                .retain(|sender| sender.send(file.clone()).is_ok());

            if !file.local.is_downloading_active || entry.get().is_empty() {
                entry.remove();
            }
        }
    }

    pub(crate) fn handle_update(&self, update: Update) {
        match update {
            Update::NewChat(data) => {
                // No need to update the chat positions here, tdlib sends
                // the correct chat positions in other updates later
                let chat = Chat::new(data.chat, self);
                self.imp().chats.borrow_mut().insert(chat.id(), chat);
            }
            Update::ChatTitle(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::ChatPhoto(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::ChatPermissions(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::ChatLastMessage(ref data) => {
                let chat = self.chat(data.chat_id);
                for position in &data.positions {
                    self.handle_chat_position_update(&chat, position);
                }
                chat.handle_update(update);
            }
            Update::ChatPosition(ref data) => {
                self.handle_chat_position_update(&self.chat(data.chat_id), &data.position)
            }
            Update::ChatReadInbox(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::ChatReadOutbox(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::ChatDraftMessage(ref data) => {
                let chat = self.chat(data.chat_id);
                for position in &data.positions {
                    self.handle_chat_position_update(&chat, position);
                }
                chat.handle_update(update);
            }
            Update::ChatNotificationSettings(ref data) => {
                self.chat(data.chat_id).handle_update(update)
            }
            Update::ChatUnreadMentionCount(ref data) => {
                self.chat(data.chat_id).handle_update(update)
            }
            Update::ChatIsBlocked(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::ChatIsMarkedAsUnread(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::DeleteMessages(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::ChatAction(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::MessageContent(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::MessageEdited(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::MessageMentionRead(ref data) => self.chat(data.chat_id).handle_update(update),
            Update::MessageSendSucceeded(ref data) => {
                self.chat(data.message.chat_id).handle_update(update)
            }
            Update::NewMessage(ref data) => self.chat(data.message.chat_id).handle_update(update),
            Update::BasicGroup(data) => {
                let mut basic_groups = self.imp().basic_groups.borrow_mut();
                match basic_groups.entry(data.basic_group.id) {
                    Entry::Occupied(entry) => entry.get().update(data.basic_group),
                    Entry::Vacant(entry) => {
                        entry.insert(BasicGroup::from_td_object(data.basic_group));
                    }
                }
            }
            Update::File(update) => {
                self.handle_file_update(update.file);
            }
            Update::ScopeNotificationSettings(update) => {
                let settings = Some(BoxedScopeNotificationSettings(update.notification_settings));
                match update.scope {
                    NotificationSettingsScope::PrivateChats => {
                        self.set_private_chats_notification_settings(settings);
                    }
                    NotificationSettingsScope::GroupChats => {
                        self.set_group_chats_notification_settings(settings);
                    }
                    NotificationSettingsScope::ChannelChats => {
                        self.set_channel_chats_notification_settings(settings);
                    }
                }
            }
            Update::SecretChat(data) => {
                let mut secret_chats = self.imp().secret_chats.borrow_mut();
                match secret_chats.entry(data.secret_chat.id) {
                    Entry::Occupied(entry) => entry.get().update(data.secret_chat),
                    Entry::Vacant(entry) => {
                        let user = self.user(data.secret_chat.user_id);
                        entry.insert(SecretChat::from_td_object(data.secret_chat, user));
                    }
                }
            }
            Update::Supergroup(data) => {
                let mut supergroups = self.imp().supergroups.borrow_mut();
                match supergroups.entry(data.supergroup.id) {
                    Entry::Occupied(entry) => entry.get().update(data.supergroup),
                    Entry::Vacant(entry) => {
                        entry.insert(Supergroup::from_td_object(data.supergroup));
                    }
                }
            }
            Update::UnreadMessageCount(data) => match data.chat_list {
                TdChatList::Main => {
                    self.main_chat_list()
                        .update_unread_message_count(data.unread_count);
                }
                TdChatList::Archive => {
                    self.archive_chat_list()
                        .update_unread_message_count(data.unread_count);
                }
                TdChatList::Folder(data_) => {
                    self.filter_chat_list(data_.chat_folder_id)
                        .update_unread_message_count(data.unread_count);
                }
            },
            Update::User(data) => {
                self.upsert_user(data.user);
            }
            Update::UserStatus(data) => {
                self.user(data.user_id).update_status(data.status);
            }
            _ => {}
        }
    }
}
