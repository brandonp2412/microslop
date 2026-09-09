//! Sidebar widget: Teams hierarchy with collapsible teams/channels and Chats list.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
};

use crate::api;

#[derive(Clone)]
pub struct Channel {
    pub name: String,
    pub id: String,
    pub unread: u32,
}

#[derive(Clone)]
pub struct Team {
    pub name: String,
    pub expanded: bool,
    pub channels: Vec<Channel>,
}

#[derive(Clone)]
pub struct Chat {
    pub name: String,
    pub id: String,
    pub is_group: bool,
    /// Number of unread messages (0 = no badge, use dot for "some")
    pub unread: u32,
}

pub struct SidebarState {
    pub teams: Vec<Team>,
    pub chats: Vec<Chat>,
    pub selected: usize,
    pub loading: bool,
    items: Vec<SidebarItem>,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            teams: Vec::new(),
            chats: Vec::new(),
            selected: 0,
            loading: true,
            items: vec![SidebarItem::TeamsHeader, SidebarItem::ChatsHeader],
        }
    }
}

impl SidebarState {
    pub fn update_teams(&mut self, teams: Vec<api::TeamInfo>) {
        self.teams = teams
            .into_iter()
            .map(|t| Team {
                name: t.name,
                expanded: true,
                channels: t
                    .channels
                    .into_iter()
                    .map(|c| Channel {
                        name: c.name,
                        id: c.id,
                        unread: 0,
                    })
                    .collect(),
            })
            .collect();
        self.rebuild_items();
        self.clamp_selection();
    }

    pub fn update_chats(&mut self, chats: Vec<api::ChatInfo>) {
        self.chats = chats
            .into_iter()
            .map(|c| Chat {
                name: c.name,
                id: c.id,
                is_group: c.is_group,
                unread: 0,
            })
            .collect();
        self.rebuild_items();
        self.clamp_selection();
    }

    /// Get the chat/channel ID of the currently selected item.
    pub fn selected_item_id(&self) -> Option<String> {
        let items = self.flat_items();
        match items.get(self.selected)? {
            SidebarItem::Channel(ti, ci) => Some(self.teams[*ti].channels[*ci].id.clone()),
            SidebarItem::Chat(ci) => Some(self.chats[*ci].id.clone()),
            _ => None,
        }
    }

    /// Get the display name of the currently selected item.
    pub fn selected_item_name(&self) -> Option<String> {
        let items = self.flat_items();
        match items.get(self.selected)? {
            SidebarItem::Channel(ti, ci) => {
                let team = &self.teams[*ti];
                let channel = &team.channels[*ci];
                Some(format!("{} > #{}", team.name, channel.name))
            }
            SidebarItem::Chat(ci) => Some(self.chats[*ci].name.clone()),
            SidebarItem::Team(ti) => Some(self.teams[*ti].name.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarItem {
    TeamsHeader,
    Team(usize),
    Channel(usize, usize),
    ChatsHeader,
    Chat(usize),
}

impl SidebarState {
    pub fn flat_items(&self) -> &[SidebarItem] {
        &self.items
    }

    fn rebuild_items(&mut self) {
        let mut items = vec![SidebarItem::TeamsHeader];
        for (ti, team) in self.teams.iter().enumerate() {
            items.push(SidebarItem::Team(ti));
            if team.expanded {
                items.extend(
                    team.channels
                        .iter()
                        .enumerate()
                        .map(|(ci, _)| SidebarItem::Channel(ti, ci)),
                );
            }
        }
        items.push(SidebarItem::ChatsHeader);
        items.extend((0..self.chats.len()).map(SidebarItem::Chat));
        self.items = items;
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.skip_headers_up();
        }
    }

    pub fn move_down(&mut self) {
        let count = self.item_count();
        if count == 0 {
            return;
        }
        if self.selected < count - 1 {
            self.selected += 1;
            self.skip_headers_down();
        }
    }

    /// Toggle expand/collapse if current selection is a Team row.
    pub fn toggle_expand(&mut self) {
        if let Some(SidebarItem::Team(ti)) = self.items.get(self.selected).copied() {
            self.teams[ti].expanded = !self.teams[ti].expanded;
            self.rebuild_items();
        }
    }

    pub fn expand_team(&mut self, team: usize) {
        if !self.teams[team].expanded {
            self.teams[team].expanded = true;
            self.rebuild_items();
        }
    }

    /// Skip non-selectable headers when moving up.
    fn skip_headers_up(&mut self) {
        while self.selected > 0 {
            match self.items.get(self.selected) {
                Some(SidebarItem::TeamsHeader | SidebarItem::ChatsHeader) => {
                    self.selected -= 1;
                }
                _ => break,
            }
        }
        if let Some(SidebarItem::TeamsHeader | SidebarItem::ChatsHeader) =
            self.items.get(self.selected)
        {
            self.skip_headers_down();
        }
    }

    /// Skip non-selectable headers when moving down.
    fn skip_headers_down(&mut self) {
        let count = self.items.len();
        while self.selected < count - 1 {
            match self.items.get(self.selected) {
                Some(SidebarItem::TeamsHeader | SidebarItem::ChatsHeader) => {
                    self.selected += 1;
                }
                _ => break,
            }
        }
    }

    /// Clamp selected index to valid range after structural changes.
    pub fn clamp_selection(&mut self) {
        let count = self.item_count();
        if count == 0 {
            self.selected = 0;
            return;
        }
        if self.selected >= count {
            self.selected = count - 1;
        }
        // After clamping, skip headers
        self.skip_headers_down();
    }
}

pub fn render(area: Rect, buf: &mut Buffer, state: &SidebarState, focused: bool) {
    let border_style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let border_type = if focused {
        BorderType::Double
    } else {
        BorderType::Plain
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(border_type)
        .border_style(border_style);

    let inner = block.inner(area);
    block.render(area, buf);

    if state.loading && state.teams.is_empty() && state.chats.is_empty() {
        if inner.height > 0 && inner.width > 0 {
            let loading_area = Rect::new(inner.x, inner.y, inner.width, 1);
            let line = Line::from(Span::styled(
                " Loading...",
                Style::default().fg(Color::DarkGray),
            ));
            Paragraph::new(line).render(loading_area, buf);
        }
        return;
    }

    let items = state.flat_items();
    let available_height = inner.height as usize;

    if available_height == 0 || items.is_empty() {
        return;
    }

    let scroll_offset = compute_scroll_offset(state.selected, available_height, items.len());

    for (row_idx, item_idx) in (scroll_offset..items.len())
        .take(available_height)
        .enumerate()
    {
        let item = &items[item_idx];
        let ctx = RowCtx {
            area: Rect::new(inner.x, inner.y + row_idx as u16, inner.width, 1),
            selected: item_idx == state.selected,
            pane_focused: focused,
        };

        render_item(buf, &ctx, item, state);
    }
}

/// Simple scroll offset: keep selected item visible.
fn compute_scroll_offset(selected: usize, height: usize, total: usize) -> usize {
    if total <= height {
        return 0;
    }
    if selected < height {
        return 0;
    }
    let max_offset = total.saturating_sub(height);
    let offset = selected.saturating_sub(height - 1);
    offset.min(max_offset)
}

struct RowCtx {
    area: Rect,
    selected: bool,
    pane_focused: bool,
}

/// Style for a list item (channel or chat) based on selection and unread state.
fn item_style(selected: bool, has_unread: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else if has_unread {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn badge_style(selected: bool) -> Style {
    if selected {
        Style::default()
            .fg(Color::Yellow)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    }
}

fn render_item(buf: &mut Buffer, ctx: &RowCtx, item: &SidebarItem, state: &SidebarState) {
    let w = ctx.area.width as usize;
    match item {
        SidebarItem::TeamsHeader => {
            let label = if ctx.pane_focused {
                ">> TEAMS"
            } else {
                "   TEAMS"
            };
            let style = Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD);
            render_row(buf, ctx.area, label, "", style, style);
        }

        SidebarItem::Team(ti) => {
            let team = &state.teams[*ti];
            let indicator = if team.expanded {
                "\u{25BC}"
            } else {
                "\u{25B6}"
            };
            let cursor = if ctx.selected { "\u{25BA}" } else { " " };
            let label = format!("{}{} {}", cursor, indicator, team.name);

            let style = if ctx.selected {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            render_row(buf, ctx.area, &label, "", style, style);
        }

        SidebarItem::Channel(ti, ci) => {
            let channel = &state.teams[*ti].channels[*ci];
            let cursor = if ctx.selected { "\u{25BA}" } else { " " };
            let label = format!("  {}# {}", cursor, channel.name);
            let badge = if channel.unread > 0 {
                format!("{}", channel.unread)
            } else {
                String::new()
            };

            let style = item_style(ctx.selected, channel.unread > 0);
            let bstyle = if channel.unread > 0 {
                badge_style(ctx.selected)
            } else {
                style
            };

            render_row(buf, ctx.area, &label, &badge, style, bstyle);
        }

        SidebarItem::ChatsHeader => {
            let prefix = " -- CHATS ";
            let dashes = w.saturating_sub(prefix.len());
            let label = format!("{}{}", prefix, "-".repeat(dashes));
            let style = Style::default().fg(Color::DarkGray);
            render_row(buf, ctx.area, &label, "", style, style);
        }

        SidebarItem::Chat(ci) => {
            let chat = &state.chats[*ci];
            let icon = if chat.is_group {
                "\u{1F465}"
            } else {
                "\u{1F464}"
            };
            let cursor = if ctx.selected { "\u{25BA}" } else { " " };
            let label = format!("{}{} {}", cursor, icon, chat.name);
            let badge = if chat.unread > 0 {
                format!("{}", chat.unread)
            } else {
                String::new()
            };

            let style = item_style(ctx.selected, chat.unread > 0);
            let bstyle = if chat.unread > 0 {
                badge_style(ctx.selected)
            } else {
                style
            };

            render_row(buf, ctx.area, &label, &badge, style, bstyle);
        }
    }
}

/// Render a row with left-aligned text and an optional right-aligned badge.
fn render_row(
    buf: &mut Buffer,
    area: Rect,
    left: &str,
    badge: &str,
    text_style: Style,
    badge_style: Style,
) {
    let width = area.width as usize;
    if width == 0 {
        return;
    }

    let badge_w = unicode_width::UnicodeWidthStr::width(badge);
    let max_left = if badge_w > 0 {
        width.saturating_sub(badge_w + 1)
    } else {
        width
    };

    let mut left_truncated = String::new();
    let mut left_w = 0;
    for ch in left.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if left_w + cw > max_left {
            break;
        }
        left_truncated.push(ch);
        left_w += cw;
    }

    let pad = if badge_w > 0 {
        width.saturating_sub(left_w + badge_w)
    } else {
        width.saturating_sub(left_w)
    };

    let line = Line::from(vec![
        Span::styled(left_truncated, text_style),
        Span::styled(" ".repeat(pad), text_style),
        Span::styled(badge.to_string(), badge_style),
    ]);

    let row_area = Rect::new(area.x, area.y, area.width, 1);
    Paragraph::new(line).render(row_area, buf);
}
