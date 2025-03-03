use std::cell::Cell;
use std::cell::OnceCell;
use std::cell::RefCell;

use anyhow::anyhow;
use gettextrs::gettext;
use glib::clone;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use once_cell::sync::Lazy;
use tdlib::enums::ChatAction;
use tdlib::enums::ChatMemberStatus;
use tdlib::enums::FormattedText;
use tdlib::enums::InputMessageContent;
use tdlib::enums::MessageContent;
use tdlib::enums::MessageSender as TdMessageSender;
use tdlib::enums::UserType;
use tdlib::functions;
use tdlib::types;

use crate::components::MessageEntry;
use crate::expressions;
use crate::session::content::SendMediaWindow;
use crate::strings;
use crate::tdlib::BasicGroup;
use crate::tdlib::BoxedDraftMessage;
use crate::tdlib::BoxedFormattedText;
use crate::tdlib::Chat;
use crate::tdlib::ChatType;
use crate::tdlib::SecretChatState;
use crate::tdlib::Supergroup;
use crate::utils::block_on;
use crate::utils::spawn;
use crate::utils::temp_dir;

const PHOTO_MIME_TYPES: &[&str] = &["image/png", "image/jpeg"];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum ChatActionBarState {
    #[default]
    Composing,
    Replying(i64),
    Editing(i64),
}

mod imp {
    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/app/drey/paper-plane/ui/content-chat-action-bar.ui")]
    pub(crate) struct ChatActionBar {
        pub(super) chat: RefCell<Option<Chat>>,
        pub(super) chat_action_in_cooldown: Cell<bool>,
        pub(super) state: Cell<ChatActionBarState>,
        pub(super) emoji_chooser: RefCell<Option<gtk::EmojiChooser>>,
        pub(super) chat_signal_group: OnceCell<glib::SignalGroup>,
        pub(super) basic_group_signal_group: OnceCell<glib::SignalGroup>,
        pub(super) supergroup_signal_group: OnceCell<glib::SignalGroup>,
        pub(super) bindings: RefCell<Vec<gtk::ExpressionWatch>>,
        #[template_child]
        pub(super) top_bar_revealer: TemplateChild<gtk::Revealer>,
        #[template_child]
        pub(super) top_bar_image: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) top_bar_title_label: TemplateChild<gtk::Inscription>,
        #[template_child]
        pub(super) top_bar_message_label: TemplateChild<gtk::Inscription>,
        #[template_child]
        pub(super) message_entry: TemplateChild<MessageEntry>,
        #[template_child]
        pub(super) send_message_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) select_file_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) restriction_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) mute_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) action_bar_stack: TemplateChild<gtk::Stack>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ChatActionBar {
        const NAME: &'static str = "ContentChatActionBar";
        type Type = super::ChatActionBar;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.set_layout_manager_type::<gtk::BoxLayout>();

            klass.install_action(
                "chat-action-bar.cancel-action",
                None,
                move |widget, _, _| {
                    widget.cancel_action();
                },
            );
            klass.install_action_async(
                "chat-action-bar.select-file",
                None,
                |widget, _, _| async move {
                    widget.select_file().await;
                },
            );
            klass.install_action_async(
                "chat-action-bar.send-message",
                None,
                |widget, _, _| async move {
                    if let ChatActionBarState::Editing(_) = widget.imp().state.get() {
                        widget.edit_message().await;
                    } else {
                        widget.send_text_message().await;
                    }
                },
            );
            klass.install_action_async(
                "chat-action-bar.join-chat",
                None,
                |widget, _, _| async move {
                    let chat = widget.chat().unwrap();
                    let result = functions::join_chat(chat.id(), chat.session().client_id()).await;
                    if let Err(e) = result {
                        log::warn!("Failed to join chat: {:?}", e);
                    } else {
                        // Reset the selection
                        chat.session().imp().sidebar.set_selected_chat(None);
                        // Select chat recently joined by the user
                        chat.session().imp().sidebar.set_selected_chat(Some(chat));
                    }
                },
            );
            klass.install_action_async(
                "chat-action-bar.toggle-mute",
                None,
                |widget, _, _| async move {
                    widget.toggle_mute().await;
                },
            );
            klass.install_action_async(
                "chat-action-bar.unblock-chat",
                None,
                |widget, _, _| async move {
                    let chat = widget.chat().unwrap();
                    if let ChatType::Private(user) = chat.type_() {
                        let message_sender =
                            TdMessageSender::User(types::MessageSenderUser { user_id: user.id() });
                        let result = functions::toggle_message_sender_is_blocked(
                            message_sender,
                            !chat.is_blocked(),
                            chat.session().client_id(),
                        )
                        .await;
                        if let Err(e) = result {
                            log::warn!("Failed to unblock user: {:?}", e);
                        }
                    }
                },
            )
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ChatActionBar {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: Lazy<Vec<glib::ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecObject::builder::<Chat>("chat")
                    .explicit_notify()
                    .build()]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "chat" => self.obj().set_chat(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "chat" => self.obj().chat().to_value(),
                _ => unimplemented!(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();

            let obj = self.obj();

            obj.layout_manager()
                .and_then(|l| l.downcast::<gtk::BoxLayout>().ok())
                .unwrap()
                .set_orientation(gtk::Orientation::Vertical);

            self.message_entry.connect_formatted_text_notify(
                clone!(@weak obj => move |message_entry, _| {
                    // Enable the send-message action only when the message entry contains
                    // at least one non-whitespace character
                    let should_enable = message_entry
                        .formatted_text()
                        .map(|f| f.0.text.contains(|c: char| !c.is_whitespace()))
                        .unwrap_or_default();
                    obj.action_set_enabled("chat-action-bar.send-message", should_enable);

                    // Send typing action
                    spawn(clone!(@weak obj => async move {
                        obj.send_chat_action(ChatAction::Typing).await;
                    }));
                }),
            );

            self.message_entry
                .connect_paste_clipboard(clone!(@weak obj => move |_| {
                    obj.handle_paste_action();
                }));

            self.message_entry
                .connect_emoji_button_press(clone!(@weak obj => move |_, button| {
                    obj.show_emoji_chooser(&button);
                }));

            // The message entry is always empty at this point, so disable the
            // send-message action
            obj.action_set_enabled("chat-action-bar.send-message", false);

            self.message_entry
                .connect_activate(clone!(@weak obj => move |_| {
                    obj.activate_action("chat-action-bar.send-message", None).unwrap()
                }));

            obj.create_signal_groups();
        }

        fn dispose(&self) {
            self.top_bar_revealer.unparent();
            self.action_bar_stack.unparent();
            if let Some(emoji_chooser) = self.emoji_chooser.take() {
                emoji_chooser.unparent();
            }
        }
    }

    impl WidgetImpl for ChatActionBar {}
}

glib::wrapper! {
    pub(crate) struct ChatActionBar(ObjectSubclass<imp::ChatActionBar>)
        @extends gtk::Widget;
}

impl Default for ChatActionBar {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatActionBar {
    pub(crate) fn new() -> Self {
        glib::Object::new()
    }

    fn create_signal_groups(&self) {
        let imp = self.imp();

        let chat_signal_group = glib::SignalGroup::new::<Chat>();
        chat_signal_group.connect_notify_local(
            Some("notification-settings"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_stack_page();
            }),
        );
        chat_signal_group.connect_notify_local(
            Some("is-blocked"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_stack_page();
            }),
        );
        imp.chat_signal_group.set(chat_signal_group).unwrap();

        let basic_group_signal_group = glib::SignalGroup::new::<BasicGroup>();
        basic_group_signal_group.connect_notify_local(
            Some("status"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_stack_page();
            }),
        );
        imp.basic_group_signal_group
            .set(basic_group_signal_group)
            .unwrap();

        let supergroup_signal_group = glib::SignalGroup::new::<Supergroup>();
        supergroup_signal_group.connect_notify_local(
            Some("status"),
            clone!(@weak self as obj => move |_, _| {
                obj.update_stack_page();
            }),
        );
        imp.supergroup_signal_group
            .set(supergroup_signal_group)
            .unwrap();
    }

    fn cancel_action(&self) {
        use ChatActionBarState::*;

        if let Editing(_) = self.imp().state.get() {
            // If we were editing, go back to the previous state by loading the
            // draft message if we have one, otherwise just reset everything
            // and go back to the "Composing" state.
            if let Some(draft_message) = self.chat().and_then(|c| c.draft_message()) {
                self.load_draft_message(draft_message);
            } else {
                self.reset();
            }
        } else {
            // We were probably replying, so just go back to "Composing" state
            self.set_state(Composing);
        }
    }

    fn set_state(&self, state: ChatActionBarState) {
        let imp = self.imp();
        if imp.state.get() == state {
            return;
        }

        // If we were editing, reset the message entry
        if let ChatActionBarState::Editing(_) = imp.state.get() {
            self.imp().message_entry.set_formatted_text(None);
        }

        // If the new state is "Editing", save the current
        // message composition state as draft message first
        if let ChatActionBarState::Editing(_) = state {
            block_on(async move {
                self.save_message_as_draft().await;
            });
        }

        imp.state.set(state);

        self.update_top_bar();
        self.update_send_button();

        if let ChatActionBarState::Editing(message_id) = state {
            self.load_message_to_edit(message_id);
        }
    }

    fn update_top_bar(&self) {
        use ChatActionBarState::*;
        let imp = self.imp();

        match imp.state.get() {
            Composing => {
                imp.top_bar_title_label.set_text(None);
                imp.top_bar_message_label.set_text(None);
                imp.top_bar_revealer.set_reveal_child(false);
            }
            Replying(message_id) => {
                // TODO: Use TDLib to retrieve the message if we don't have it locally
                if let Some(message) = self.chat().and_then(|c| c.message(message_id)) {
                    // TODO: Make these labels auto update
                    imp.top_bar_title_label
                        .set_text(Some(&strings::message_sender(message.sender(), true)));
                    imp.top_bar_message_label
                        .set_text(Some(&strings::message_content(&message)));
                } else {
                    imp.top_bar_title_label.set_text(Some(&gettext("Unknown")));
                    imp.top_bar_message_label
                        .set_text(Some(&gettext("Deleted Message")));
                }

                imp.top_bar_image
                    .set_icon_name(Some("mail-reply-sender-symbolic"));
                imp.top_bar_revealer.set_reveal_child(true);
            }
            Editing(message_id) => {
                // TODO: Use TDLib to retrieve the message if we don't have it locally
                if let Some(message) = self.chat().and_then(|c| c.message(message_id)) {
                    imp.top_bar_title_label
                        .set_text(Some(&gettext("Edit Message")));
                    imp.top_bar_message_label
                        .set_text(Some(&strings::message_content(&message)));
                } else {
                    imp.top_bar_title_label.set_text(Some(&gettext("Unknown")));
                    imp.top_bar_message_label
                        .set_text(Some(&gettext("Deleted Message")));
                }

                imp.top_bar_image.set_icon_name(Some("edit-symbolic"));
                imp.top_bar_revealer.set_reveal_child(true);
            }
        }
    }

    fn update_send_button(&self) {
        use ChatActionBarState::*;
        let imp = self.imp();

        match imp.state.get() {
            Editing(_) => {
                imp.send_message_button.set_icon_name("done-symbolic");
            }
            _ => {
                imp.send_message_button.set_icon_name("go-up-symbolic");
            }
        }
    }

    fn load_message_to_edit(&self, message_id: i64) {
        if let Some(chat) = self.chat() {
            let client_id = chat.session().client_id();

            if let Some(message) = chat.message(message_id) {
                match message.content().0 {
                    MessageContent::MessageText(data) => {
                        block_on(async move {
                            let FormattedText::FormattedText(markdown_text) =
                                functions::get_markdown_text(data.text, client_id)
                                    .await
                                    .unwrap();

                            self.imp()
                                .message_entry
                                .set_formatted_text(Some(BoxedFormattedText(markdown_text)));
                        });
                    }
                    _ => unimplemented!(),
                }
            }
        }
    }

    fn reset(&self) {
        self.set_state(ChatActionBarState::Composing);
        self.imp().message_entry.set_formatted_text(None);
    }

    async fn compose_text_message(&self) -> Option<InputMessageContent> {
        if let Some(formatted_text) = self.imp().message_entry.as_markdown().await {
            let content = types::InputMessageText {
                text: formatted_text,
                disable_web_page_preview: false,
                clear_draft: true,
            };

            Some(InputMessageContent::InputMessageText(content))
        } else {
            None
        }
    }

    fn show_emoji_chooser(&self, parent: &impl IsA<gtk::Widget>) {
        let imp = self.imp();
        let mut emoji_chooser = imp.emoji_chooser.borrow_mut();
        if emoji_chooser.is_none() {
            let chooser = gtk::EmojiChooser::new();
            chooser.set_parent(parent);
            chooser.connect_emoji_picked(clone!(@weak self as obj => move |_, emoji| {
                obj.imp().message_entry.insert_at_cursor(emoji);
            }));
            chooser.connect_hide(clone!(@weak self as obj => move |_| {
                obj.imp().message_entry.grab_focus();
            }));
            *emoji_chooser = Some(chooser);
        }
        emoji_chooser.as_ref().unwrap().popup();
    }

    async fn select_file(&self) {
        let dialog = gtk::FileDialog::new();
        let filter = gtk::FileFilter::new();
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let parent = self.root().and_downcast::<gtk::Window>().unwrap();

        filter.set_name(Some(&gettext("Images")));
        for mime in PHOTO_MIME_TYPES {
            filter.add_mime_type(mime);
        }

        filters.append(&filter);
        dialog.set_filters(Some(&filters));

        if let Ok(file) = dialog.open_future(Some(&parent)).await {
            let path = file.path().unwrap().to_str().unwrap().to_string();
            let chat = self.chat().unwrap();

            SendMediaWindow::new(&parent, chat, path).present();
        }
    }

    async fn edit_message(&self) {
        if let Some(chat) = self.chat() {
            if let ChatActionBarState::Editing(message_id) = self.imp().state.get() {
                if let Some(message) = self.compose_text_message().await {
                    let client_id = chat.session().client_id();
                    let chat_id = chat.id();

                    let result =
                        functions::edit_message_text(chat_id, message_id, message, client_id).await;
                    if let Err(e) = result {
                        log::warn!("Error editing a text message: {:?}", e);
                    }

                    self.cancel_action();
                }
            }
        }
    }

    async fn send_text_message(&self) {
        if let Some(chat) = self.chat() {
            if let Some(message) = self.compose_text_message().await {
                let client_id = chat.session().client_id();
                let chat_id = chat.id();
                let reply_to_message_id =
                    if let ChatActionBarState::Replying(id) = self.imp().state.get() {
                        id
                    } else {
                        0
                    };

                // Send the message
                let result = functions::send_message(
                    chat_id,
                    0,
                    reply_to_message_id,
                    None,
                    message,
                    client_id,
                )
                .await;
                if let Err(e) = result {
                    log::warn!("Error sending a message: {:?}", e);
                }

                self.reset();
            }
        }
    }

    fn is_chat_muted(&self) -> bool {
        let chat = self.chat().unwrap();
        let notifications = chat.notification_settings().0;
        if notifications.use_default_mute_for {
            chat.session()
                .channel_chats_notification_settings()
                .unwrap()
                .0
                .mute_for
                != 0
        } else {
            notifications.mute_for != 0
        }
    }

    async fn toggle_mute(&self) {
        let chat = self.chat().unwrap();
        let mut notifications = chat.clone().notification_settings().0;
        let default = chat
            .clone()
            .session()
            .channel_chats_notification_settings()
            .unwrap()
            .0;
        if default.mute_for == notifications.mute_for {
            notifications.use_default_mute_for = false;
            if notifications.mute_for == 0 {
                let now = glib::DateTime::now_utc().unwrap().to_unix() as i32;
                notifications.mute_for = std::i32::MAX - now
            } else {
                notifications.mute_for = 0
            }
        } else {
            notifications.use_default_mute_for = true;
        }

        let result = functions::set_chat_notification_settings(
            chat.id(),
            notifications.clone(),
            chat.session().client_id(),
        )
        .await;
        if let Err(e) = result {
            log::warn!("Failed to unmute/mute chat: {:?}", e);
        }
    }

    async fn save_message_as_draft(&self) {
        if let Some(chat) = self.chat() {
            let client_id = chat.session().client_id();
            let chat_id = chat.id();
            let reply_to_message_id =
                if let ChatActionBarState::Replying(id) = self.imp().state.get() {
                    id
                } else {
                    0
                };
            let draft_message =
                self.compose_text_message()
                    .await
                    .map(|message| types::DraftMessage {
                        reply_to_message_id,
                        date: glib::DateTime::now_local().unwrap().to_unix() as i32,
                        input_message_text: message,
                    });

            // Save draft message
            let result =
                functions::set_chat_draft_message(chat_id, 0, draft_message, client_id).await;
            if let Err(e) = result {
                log::warn!("Error setting a draft message: {:?}", e);
            }
        }
    }

    fn load_draft_message(&self, message: BoxedDraftMessage) {
        let imp = self.imp();

        if message.0.reply_to_message_id != 0 {
            self.set_state(ChatActionBarState::Replying(message.0.reply_to_message_id));
        } else {
            self.set_state(ChatActionBarState::Composing);
        }

        if let InputMessageContent::InputMessageText(content) = message.0.input_message_text {
            imp.message_entry
                .set_formatted_text(Some(BoxedFormattedText(content.text)));
        } else {
            log::warn!(
                "Unexpected draft message type: {:?}",
                message.0.input_message_text
            );
            imp.message_entry.set_formatted_text(None);
        }
    }

    async fn send_chat_action(&self, action: ChatAction) {
        let imp = self.imp();
        if imp.chat_action_in_cooldown.get() {
            return;
        }

        if let Some(chat) = self.chat() {
            let client_id = chat.session().client_id();
            let chat_id = chat.id();

            // Enable chat action cooldown right away
            imp.chat_action_in_cooldown.set(true);

            // Send typing action
            let result = functions::send_chat_action(chat_id, 0, Some(action), client_id).await;
            if result.is_ok() {
                glib::timeout_add_seconds_local_once(
                    5,
                    clone!(@weak self as obj =>move || {
                        obj.imp().chat_action_in_cooldown.set(false);
                    }),
                );
            } else {
                imp.chat_action_in_cooldown.set(false);
            }
        }
    }

    pub(crate) fn handle_paste_action(&self) {
        if let Some(chat) = self.chat() {
            spawn(clone!(@weak self as obj => async move {
                if let Err(e) = obj.handle_image_clipboard(chat).await {
                    log::warn!("Error on pasting an image: {:?}", e);
                }
            }));
        }
    }

    async fn handle_image_clipboard(&self, chat: Chat) -> Result<(), anyhow::Error> {
        if let Ok((stream, mime)) = self
            .clipboard()
            .read_future(PHOTO_MIME_TYPES, glib::Priority::DEFAULT)
            .await
        {
            let extension = match mime.as_str() {
                "image/png" => "png",
                "image/jpg" => "jpg",
                _ => unreachable!(),
            };

            let temp_dir =
                temp_dir().ok_or_else(|| anyhow!("The temporary directory doesn't exist"))?;
            let path = temp_dir.join("clipboard").with_extension(extension);

            save_stream_to_file(stream, &path).await?;

            let parent = self.root().and_downcast().unwrap();
            let path = path.to_str().unwrap().to_string();
            SendMediaWindow::new(&parent, chat, path).present();
        }

        Ok(())
    }

    pub(crate) fn chat(&self) -> Option<Chat> {
        self.imp().chat.borrow().clone()
    }

    fn set_chat(&self, chat: Option<Chat>) {
        if self.chat() == chat {
            return;
        }

        spawn(clone!(@weak self as obj => async move {
            obj.save_message_as_draft().await;
        }));

        let imp = self.imp();
        let mut bindings = imp.bindings.borrow_mut();
        while let Some(binding) = bindings.pop() {
            binding.unwatch();
        }

        if let Some(ref chat) = chat {
            if let Some(draft_message) = chat.draft_message() {
                self.load_draft_message(draft_message);
            } else {
                self.reset();
            }

            imp.chat_action_in_cooldown.set(false);

            // Handle restriction_label caption
            let restriction_label_binding = expressions::restriction_expression(chat).bind(
                &*imp.restriction_label,
                "label",
                Some(chat),
            );

            bindings.push(restriction_label_binding);
        }

        imp.chat.replace(chat);

        self.update_stack_page();
        self.update_signal_groups();

        self.notify("chat");
    }

    pub(crate) fn reply_to_message_id(&self, message_id: i64) {
        self.set_state(ChatActionBarState::Replying(message_id));
    }

    pub(crate) fn edit_message_id(&self, message_id: i64) {
        self.set_state(ChatActionBarState::Editing(message_id));
    }

    fn update_stack_page(&self) {
        let imp = self.imp();
        if let Some(chat) = self.chat() {
            match chat.type_() {
                ChatType::Private(user) => {
                    let is_deleted = matches!(user.type_().0, UserType::Deleted);
                    let is_blocked = chat.is_blocked();
                    if is_deleted {
                        // TODO: Add delete chat button
                        imp.action_bar_stack.set_visible_child_name("entry");
                    } else if is_blocked {
                        imp.action_bar_stack.set_visible_child_name("unblock");
                    } else {
                        imp.action_bar_stack.set_visible_child_name("entry");
                    }
                }
                ChatType::Secret(secret) => {
                    let is_closed = matches!(secret.state(), SecretChatState::Closed);
                    let is_blocked = chat.is_blocked();
                    if is_closed {
                        // TODO: Add delete chat button
                        imp.action_bar_stack.set_visible_child_name("entry");
                    } else if is_blocked {
                        imp.action_bar_stack.set_visible_child_name("unblock");
                    } else {
                        imp.action_bar_stack.set_visible_child_name("entry");
                    }
                }
                ChatType::Supergroup(data) if data.is_channel() => match data.status().0 {
                    ChatMemberStatus::Creator(_) | ChatMemberStatus::Administrator(_) => {
                        imp.action_bar_stack.set_visible_child_name("entry");
                    }
                    ChatMemberStatus::Left => {
                        imp.action_bar_stack.set_visible_child_name("join");
                    }
                    _ => {
                        imp.action_bar_stack.set_visible_child_name("mute");

                        if self.is_chat_muted() {
                            imp.mute_button.set_label(&gettext("Unmute"));
                        } else {
                            imp.mute_button.set_label(&gettext("Mute"));
                        }
                    }
                },
                ChatType::Supergroup(data) => match data.status().0 {
                    ChatMemberStatus::Restricted(data)
                        if !data.permissions.can_send_basic_messages =>
                    {
                        imp.action_bar_stack.set_visible_child_name("restricted");
                    }
                    ChatMemberStatus::Left => {
                        imp.action_bar_stack.set_visible_child_name("join");
                    }
                    _ => {
                        imp.action_bar_stack.set_visible_child_name("entry");
                    }
                },
                ChatType::BasicGroup(data) => match data.status().0 {
                    ChatMemberStatus::Restricted(_) => {
                        imp.action_bar_stack.set_visible_child_name("restricted");
                    }
                    ChatMemberStatus::Left => {
                        imp.action_bar_stack.set_visible_child_name("join");
                    }
                    _ => {
                        imp.action_bar_stack.set_visible_child_name("entry");
                    }
                },
            }
        }
    }

    fn update_signal_groups(&self) {
        let imp = self.imp();

        let chat = self.chat();
        imp.chat_signal_group
            .get()
            .unwrap()
            .set_target(chat.as_ref());

        let basic_group = chat.as_ref().and_then(|c| c.type_().basic_group());
        imp.basic_group_signal_group
            .get()
            .unwrap()
            .set_target(basic_group);

        let supergroup = chat.as_ref().and_then(|c| c.type_().supergroup());
        imp.supergroup_signal_group
            .get()
            .unwrap()
            .set_target(supergroup);
    }
}

async fn save_stream_to_file(
    stream: gio::InputStream,
    path: impl AsRef<std::path::Path>,
) -> Result<(), glib::Error> {
    let file = gio::File::for_path(path);
    let file_stream = file
        .replace_future(
            None,
            false,
            gio::FileCreateFlags::REPLACE_DESTINATION,
            glib::Priority::DEFAULT,
        )
        .await?;

    file_stream
        .splice_future(
            &stream,
            gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET,
            glib::Priority::DEFAULT,
        )
        .await?;

    Ok(())
}
