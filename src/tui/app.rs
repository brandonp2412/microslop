//! TUI Application state and main event loop

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use tokio_stream::StreamExt;

use super::backend::{Backend, BackendCommand, BackendResponse};
use super::compose::ComposeState;
use super::debug_log::DebugLogState;
use super::log_capture::LogBuffer;
use super::messages::MessagesState;
use super::search::SearchState;
use super::sidebar::SidebarState;
use super::ui;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    #[default]
    Sidebar,
    Messages,
    Compose,
}

impl Pane {
    pub fn as_str(&self) -> &'static str {
        match self {
            Pane::Sidebar => "sidebar",
            Pane::Messages => "messages",
            Pane::Compose => "compose",
        }
    }
}

pub struct App {
    pub should_exit: bool,
    pub is_online: bool,
    pub user_name: String,
    pub channel_name: String,
    pub connection_state: String,
    pub active_pane: Pane,
    pub sidebar: SidebarState,
    pub messages: MessagesState,
    pub compose: ComposeState,
    /// Whether the help popup is visible
    pub show_help: bool,
    pub search: SearchState,
    pub current_chat_id: Option<String>,
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub debug_log: DebugLogState,
}

impl App {
    fn prefetch_sidebar_messages(&self, backend: &Backend) {
        let channel_ids = self
            .sidebar
            .teams
            .iter()
            .flat_map(|team| team.channels.iter().map(|channel| channel.id.as_str()));
        let chat_ids = self.sidebar.chats.iter().map(|chat| chat.id.as_str());
        let ids = super::backend::prefetch_ids(channel_ids, chat_ids);
        if !ids.is_empty() {
            backend.send(BackendCommand::PrefetchMessages {
                chat_ids: ids,
                limit: 50,
            });
        }
    }

    /// Create a new App with the given log buffer for debug log capture.
    pub fn new(log_buffer: LogBuffer) -> Self {
        Self {
            should_exit: false,
            is_online: false,
            user_name: "Loading...".to_string(),
            channel_name: "".to_string(),
            connection_state: "Connecting...".to_string(),
            active_pane: Pane::default(),
            sidebar: SidebarState::default(),
            messages: MessagesState::default(),
            compose: ComposeState::default(),
            show_help: false,
            search: SearchState::default(),
            current_chat_id: None,
            status_message: None,
            status_is_error: false,
            debug_log: DebugLogState::new(log_buffer),
        }
    }
}

impl App {
    fn next_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Sidebar => Pane::Messages,
            Pane::Messages => Pane::Compose,
            Pane::Compose => Pane::Sidebar,
        };
    }

    fn prev_pane(&mut self) {
        self.active_pane = match self.active_pane {
            Pane::Sidebar => Pane::Compose,
            Pane::Messages => Pane::Sidebar,
            Pane::Compose => Pane::Messages,
        };
    }

    pub fn handle_event(&mut self, event: Event, backend: &Backend) {
        if let Event::Key(key_event) = event {
            if key_event.kind != KeyEventKind::Press {
                return;
            }

            if self.show_help {
                self.show_help = false;
                return;
            }

            self.status_message = None;

            if self.search.active {
                self.handle_search_key(key_event);
                return;
            }

            if key_event.code == KeyCode::Char('k')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
            {
                self.search.activate();
                return;
            }

            if key_event.code == KeyCode::Char('d')
                && key_event.modifiers.contains(KeyModifiers::CONTROL)
            {
                self.debug_log.toggle();
                return;
            }

            if self.debug_log.visible {
                match key_event.code {
                    KeyCode::Char('c') => {
                        self.status_is_error = self.debug_log.copy_to_clipboard().is_err();
                        self.status_message = Some(if self.status_is_error {
                            "Could not copy debug log to clipboard".to_string()
                        } else {
                            "Debug log copied to clipboard".to_string()
                        });
                        return;
                    }
                    KeyCode::PageUp => {
                        self.debug_log.scroll_up(10);
                        return;
                    }
                    KeyCode::PageDown => {
                        self.debug_log.scroll_down(10);
                        return;
                    }
                    _ => {}
                }
            }

            if self.active_pane == Pane::Compose {
                self.handle_compose_key(key_event, backend);
            } else {
                self.handle_navigation_key(key_event, backend);
            }
        }
    }

    /// Handle key events when a non-compose pane is focused.
    fn handle_navigation_key(&mut self, key_event: crossterm::event::KeyEvent, backend: &Backend) {
        match key_event.code {
            KeyCode::Char('q') => {
                self.should_exit = true;
            }
            KeyCode::Tab => {
                self.next_pane();
            }
            KeyCode::BackTab => {
                self.prev_pane();
            }
            KeyCode::Right => {
                self.next_pane();
            }
            KeyCode::Left => {
                self.prev_pane();
            }
            KeyCode::Char('1') => {
                self.active_pane = Pane::Sidebar;
            }
            KeyCode::Char('2') => {
                self.active_pane = Pane::Messages;
            }
            KeyCode::Char('3') => {
                self.active_pane = Pane::Compose;
            }
            KeyCode::Up | KeyCode::Char('k') if self.active_pane == Pane::Sidebar => {
                self.sidebar.move_up();
            }
            KeyCode::Down | KeyCode::Char('j') if self.active_pane == Pane::Sidebar => {
                self.sidebar.move_down();
            }
            KeyCode::Enter if self.active_pane == Pane::Sidebar => {
                self.handle_sidebar_enter(backend);
            }
            KeyCode::Up | KeyCode::Char('k') if self.active_pane == Pane::Messages => {
                self.messages.select_previous();
            }
            KeyCode::Down | KeyCode::Char('j') if self.active_pane == Pane::Messages => {
                self.messages.select_next();
            }
            KeyCode::Enter if self.active_pane == Pane::Messages => {
                self.messages.toggle_thread();
            }
            KeyCode::Char('?') => {
                self.show_help = !self.show_help;
            }
            _ => {}
        }
    }

    ///
    /// If the selected item is a team, toggle expand/collapse.
    /// If it's a channel or chat, load its messages.
    fn handle_sidebar_enter(&mut self, backend: &Backend) {
        let items = self.sidebar.flat_items();
        let item = match items.get(self.sidebar.selected) {
            Some(item) => *item,
            None => return,
        };

        match item {
            super::sidebar::SidebarItem::Team(_) => {
                self.sidebar.toggle_expand();
                self.sidebar.clamp_selection();
            }
            super::sidebar::SidebarItem::Channel(_, _) | super::sidebar::SidebarItem::Chat(_) => {
                if let Some(id) = self.sidebar.selected_item_id() {
                    let name = self.sidebar.selected_item_name().unwrap_or_default();
                    self.current_chat_id = Some(id.clone());
                    self.channel_name = name.clone();
                    self.messages.loading = true;
                    self.messages.channel_header = name;
                    self.messages.messages.clear();
                    backend.send(BackendCommand::LoadMessages {
                        chat_id: id,
                        limit: 50,
                    });
                }
            }
            _ => {}
        }
    }

    /// Handle key events when the compose pane is focused.
    fn handle_compose_key(&mut self, key_event: crossterm::event::KeyEvent, backend: &Backend) {
        let modifiers = key_event.modifiers;
        let code = key_event.code;

        match (code, modifiers) {
            (KeyCode::Tab, _) => {
                self.next_pane();
            }
            (KeyCode::BackTab, _) => {
                self.prev_pane();
            }
            (KeyCode::Esc, _) => {
                self.active_pane = Pane::Messages;
            }
            (KeyCode::Enter, m) if m.contains(KeyModifiers::CONTROL) => {
                self.compose.insert_newline();
            }
            (KeyCode::Enter, _) => {
                if let Some(text) = self.compose.send() {
                    if let Some(ref chat_id) = self.current_chat_id {
                        backend.send(BackendCommand::SendMessage {
                            chat_id: chat_id.clone(),
                            message: text,
                        });
                    } else {
                        self.status_message =
                            Some("No chat selected. Select a channel or chat first.".to_string());
                        self.status_is_error = true;
                    }
                }
            }
            (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => {
                self.compose.clear();
            }
            (KeyCode::Backspace, _) => {
                self.compose.backspace();
            }
            (KeyCode::Delete, _) => {
                self.compose.delete();
            }
            (KeyCode::Left, _) => {
                self.compose.move_left();
            }
            (KeyCode::Right, _) => {
                self.compose.move_right();
            }
            (KeyCode::Home, _) => {
                self.compose.move_home();
            }
            (KeyCode::End, _) => {
                self.compose.move_end();
            }
            (KeyCode::Char(c), m) if (m.is_empty() || m == KeyModifiers::SHIFT) => {
                self.compose.insert_char(c);
            }
            _ => {}
        }
    }

    /// Handle key events when the search overlay is active.
    fn handle_search_key(&mut self, key_event: crossterm::event::KeyEvent) {
        let code = key_event.code;
        let modifiers = key_event.modifiers;

        match (code, modifiers) {
            (KeyCode::Esc, _) => {
                self.search.deactivate();
            }
            (KeyCode::Up, _) => {
                self.search.select_previous();
            }
            (KeyCode::Down, _) => {
                self.search.select_next();
            }
            (KeyCode::Enter, _) => {
                self.apply_search_selection();
            }
            (KeyCode::Backspace, _) => {
                self.search.backspace();
                self.search.update_results(&self.sidebar, &self.messages);
            }
            (KeyCode::Delete, _) => {
                self.search.delete_at_cursor();
                self.search.update_results(&self.sidebar, &self.messages);
            }
            (KeyCode::Left, _) => {
                self.search.move_left();
            }
            (KeyCode::Right, _) => {
                self.search.move_right();
            }
            (KeyCode::Home, _) => {
                self.search.move_home();
            }
            (KeyCode::End, _) => {
                self.search.move_end();
            }
            (KeyCode::Char(c), m) if (m.is_empty() || m == KeyModifiers::SHIFT) => {
                self.search.insert_char(c);
                self.search.update_results(&self.sidebar, &self.messages);
            }
            _ => {}
        }
    }

    /// Apply the currently selected search result: navigate to the matching item.
    fn apply_search_selection(&mut self) {
        use super::search::SearchResultKind;

        let result = match self.search.selected_result() {
            Some(r) => r.kind.clone(),
            None => {
                self.search.deactivate();
                return;
            }
        };

        match result {
            SearchResultKind::Channel(team_idx, channel_idx) => {
                self.sidebar.expand_team(team_idx);
                let items = self.sidebar.flat_items();
                for (idx, item) in items.iter().enumerate() {
                    if let super::sidebar::SidebarItem::Channel(ti, ci) = item {
                        if *ti == team_idx && *ci == channel_idx {
                            self.sidebar.selected = idx;
                            break;
                        }
                    }
                }
                self.active_pane = Pane::Sidebar;
            }
            SearchResultKind::Chat(chat_idx) => {
                let items = self.sidebar.flat_items();
                for (idx, item) in items.iter().enumerate() {
                    if let super::sidebar::SidebarItem::Chat(ci) = item {
                        if *ci == chat_idx {
                            self.sidebar.selected = idx;
                            break;
                        }
                    }
                }
                self.active_pane = Pane::Sidebar;
            }
            SearchResultKind::Message(msg_idx) => {
                if msg_idx < self.messages.messages.len() {
                    self.messages.selected = msg_idx;
                }
                self.active_pane = Pane::Messages;
            }
        }

        self.search.deactivate();
    }

    fn handle_backend_response(&mut self, response: BackendResponse, backend: &Backend) {
        match response {
            BackendResponse::Teams(Ok(teams)) => {
                self.sidebar.update_teams(teams);
                if !self.sidebar.chats.is_empty() {
                    self.prefetch_sidebar_messages(backend);
                }
                self.sidebar.loading = false;
                self.close_stale_search();
                // If this is the first data load and we have teams, select the first
                // selectable item (skip TeamsHeader).
                if self.sidebar.selected == 0 {
                    self.sidebar.clamp_selection();
                }
            }
            BackendResponse::Teams(Err(e)) => {
                self.set_error(format!("Failed to load teams: {:#}", e));
                self.sidebar.loading = false;
            }
            BackendResponse::Chats(Ok(chats)) => {
                self.sidebar.update_chats(chats);
                if !self.sidebar.teams.is_empty() {
                    self.prefetch_sidebar_messages(backend);
                }
                self.sidebar.loading = false;
                self.close_stale_search();
            }
            BackendResponse::Chats(Err(e)) => {
                self.set_error(format!("Failed to load chats: {:#}", e));
                self.sidebar.loading = false;
            }
            BackendResponse::Messages { chat_id, result } => {
                if self.current_chat_id.as_deref() == Some(&chat_id) {
                    match result {
                        Ok(msgs) => {
                            let header = self.messages.channel_header.clone();
                            self.messages.update_messages(&header, msgs);
                            self.close_stale_search();
                        }
                        Err(e) => {
                            self.messages.loading = false;
                            self.set_error(format!("Failed to load messages: {:#}", e));
                        }
                    }
                }
            }
            BackendResponse::MessageSent(Ok(())) => {
                self.status_message = Some("Message sent".to_string());
                self.status_is_error = false;
                if let Some(ref chat_id) = self.current_chat_id {
                    backend.send(BackendCommand::InvalidateMessages {
                        chat_id: chat_id.clone(),
                    });
                    backend.send(BackendCommand::LoadMessages {
                        chat_id: chat_id.clone(),
                        limit: 50,
                    });
                }
            }
            BackendResponse::MessageSent(Err(e)) => {
                self.set_error(format!("Failed to send message: {:#}", e));
            }
            BackendResponse::UserInfo(Ok(info)) => {
                self.user_name = info.display_name;
                self.connection_state = "Connected".to_string();
                self.is_online = true;
            }
            BackendResponse::UserInfo(Err(e)) => {
                self.set_error(format!("Failed to load user info: {:#}", e));
            }
            BackendResponse::ClientError(msg) => {
                self.connection_state = "Not authenticated".to_string();
                self.is_online = false;
                self.sidebar.loading = false;
                self.set_error(format!("Auth: {}", msg));
            }
        }
    }

    /// Close the search overlay if it's open.
    ///
    /// Called when backend data arrives to prevent stale search result indices
    fn close_stale_search(&mut self) {
        if self.search.active {
            self.search.deactivate();
        }
    }

    fn set_error(&mut self, msg: String) {
        self.status_message = Some(msg);
        self.status_is_error = true;
    }

    pub fn render(&self, frame: &mut ratatui::Frame) {
        ui::render(frame, self);
    }
}

///
/// Sets up a panic hook so the terminal is always restored even on panic.
/// Requires a LogBuffer for capturing tracing output into the debug log pane.
pub async fn run(log_buffer: LogBuffer) -> Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        default_hook(info);
    }));

    let mut terminal = ratatui::init();
    let res = run_app(&mut terminal, log_buffer).await;
    ratatui::restore();
    res
}

async fn run_app(terminal: &mut DefaultTerminal, log_buffer: LogBuffer) -> Result<()> {
    let mut app = App::new(log_buffer);
    let mut backend = Backend::start();
    let mut events = EventStream::new();

    backend.send(BackendCommand::LoadTeams);
    backend.send(BackendCommand::LoadChats { limit: 50 });
    backend.send(BackendCommand::LoadUserInfo);

    while !app.should_exit {
        app.debug_log.refresh();
        terminal.draw(|frame| app.render(frame))?;

        tokio::select! {
            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        app.handle_event(event, &backend);
                    }
                    Some(Err(e)) => {
                        tracing::error!("Event stream error: {:#}", e);
                    }
                    None => {
                        break;
                    }
                }
            }
            maybe_response = backend.recv() => {
                match maybe_response {
                    Some(response) => {
                        app.handle_backend_response(response, &backend);
                    }
                    None => {
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
