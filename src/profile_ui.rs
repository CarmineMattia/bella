use crate::{App, Dialog, editor::Editor, profiles};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    widgets::Paragraph,
};
const TEXT: Color = Color::Rgb(227, 236, 244);
const ACCENT: Color = Color::Rgb(88, 177, 232);
const MUTED: Color = Color::Rgb(130, 151, 171);
const SELECTED: Color = Color::Rgb(43, 82, 120);
const PANEL: Color = Color::Rgb(30, 44, 59);

impl App {
    pub(crate) fn personal_bot(&self) -> profiles::Profile {
        self.profiles
            .iter()
            .find(|p| p.owner == self.name && p.handle == "bot")
            .cloned()
            .unwrap_or_else(|| profiles::Profile::bot(&self.name, "bot"))
    }
    pub(crate) fn requested_bot(&self, text: &str) -> Option<profiles::Profile> {
        let handle = text.split_whitespace().next()?.strip_prefix('@')?;
        self.profiles
            .iter()
            .find(|p| p.owner == self.name && p.handle == handle && !handle.is_empty())
            .cloned()
    }
    /// Guest side: a leading `@handle` naming another account's bot in this group. Nothing
    /// runs here; the owner's client decides whether to answer (opt-in, chat only).
    pub(crate) fn shared_bot_mention(&self, text: &str) -> Option<profiles::Profile> {
        self.network.as_ref()?;
        let handle = text.split_whitespace().next()?.strip_prefix('@')?;
        self.bots()
            .into_iter()
            .find(|p| p.owner != self.name && p.handle == handle && !handle.is_empty())
            .cloned()
    }
    pub(crate) fn bots(&self) -> Vec<&profiles::Profile> {
        let mut bots: Vec<_> = self
            .profiles
            .iter()
            .filter(|p| p.owner == self.name && !p.handle.is_empty())
            .collect();
        if self.network.is_some() {
            let members = self
                .store
                .connection()
                .prepare("SELECT owner,handle FROM room_bots WHERE room=?1")
                .and_then(|mut q| {
                    q.query_map([&self.room], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap_or_default();
            bots.extend(self.profiles.iter().filter(|p| {
                p.owner != self.name
                    && members
                        .iter()
                        .any(|(owner, handle)| owner == &p.owner && handle == &p.handle)
            }));
        }
        bots
    }
    pub(crate) fn author_profile(&self, author: &str, bot: bool) -> Option<&profiles::Profile> {
        self.profile_index.get(&(author.into(), bot))
    }
    pub(crate) fn display_author(&self, author: &str, bot: bool) -> String {
        self.author_profile(author, bot)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| author.into())
    }
    fn edit_bot(&mut self, creating: bool) {
        let p = if creating {
            profiles::Profile::bot(&self.name, "new_bot")
        } else {
            let Some(p) = self.bots().get(self.selected).cloned().cloned() else {
                return;
            };
            if p.owner != self.name {
                self.status = "Only the bot's owner can edit its profile".into();
                return;
            }
            p
        };
        self.profile_form = Some(profiles::Form::new(p, creating));
        self.dialog = Some(Dialog::BotProfile);
    }
    fn save_profile(&mut self) -> Result<()> {
        let form = self.profile_form.as_ref().unwrap();
        let result = (|| -> Result<()> {
            let p = form.build()?;
            if form.creating
                && self
                    .profiles
                    .iter()
                    .any(|old| old.owner == p.owner && old.handle == p.handle)
            {
                anyhow::bail!("That handle already exists; choose another");
            }
            let tx = self.store.connection().unchecked_transaction()?;
            if form.creating {
                tx.execute(
                    "INSERT INTO profiles VALUES(?1,?2,?3)",
                    rusqlite::params![p.owner, p.handle, serde_json::to_string(&p)?],
                )?;
            } else {
                profiles::save(&tx, &p)?;
            }
            if form.creating {
                tx.execute(
                    "INSERT OR IGNORE INTO room_bots VALUES(?1,?2,?3)",
                    rusqlite::params![self.name, self.room, p.handle],
                )?;
            }
            tx.commit()?;
            Ok(())
        })();
        if let Err(e) = result {
            self.profile_form.as_mut().unwrap().error = format!("{e:#}");
            return Ok(());
        }
        let bot = self.dialog == Some(Dialog::BotProfile);
        self.profile_form = None;
        self.dialog = if bot { Some(Dialog::Bots) } else { None };
        self.refresh()?;
        self.selected = self.selected.min(self.bots().len().saturating_sub(1));
        self.status = "Profile saved · pictures are included in database backups".into();
        Ok(())
    }
    pub(crate) fn profile_paste(&mut self, text: &str) {
        if let Some(form) = &mut self.profile_form {
            if form.selected == 0 && !form.creating && !form.original.handle.is_empty() {
                return;
            }
            form.fields[form.selected]
                .1
                .insert(&text.replace(['\r', '\n', '\t'], " "));
        }
    }
    pub(crate) fn profile_key(&mut self, k: KeyEvent) -> Result<()> {
        if self.dialog == Some(Dialog::Bots) {
            let count = self.bots().len();
            self.selected = self.selected.min(count.saturating_sub(1));
            match k.code {
                KeyCode::Esc => self.dialog = None,
                KeyCode::Char('n') => self.edit_bot(true),
                KeyCode::Char('e') if count > 0 => self.edit_bot(false),
                KeyCode::Up if count > 0 => self.selected = (self.selected + count - 1) % count,
                KeyCode::Down if count > 0 => self.selected = (self.selected + 1) % count,
                KeyCode::Enter | KeyCode::Char(' ') if count > 0 => {
                    if self.bots()[self.selected].owner != self.name {
                        self.status = "Only the bot's owner can change its membership".into();
                        return Ok(());
                    }
                    let handle = self.bots()[self.selected].handle.clone();
                    if handle == "bot" {
                        self.status =
                            "Your personal @bot is always available · E edits its profile".into();
                    } else {
                        profiles::toggle(self.store.connection(), &self.name, &self.room, &handle)?;
                        self.refresh()?;
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        if k.code == KeyCode::Esc {
            self.dialog = if self.dialog == Some(Dialog::BotProfile) {
                Some(Dialog::Bots)
            } else {
                None
            };
            self.profile_form = None;
            return Ok(());
        }
        if k.code == KeyCode::Char('s') && k.modifiers.contains(KeyModifiers::CONTROL) {
            return self.save_profile();
        }
        let Some(form) = &mut self.profile_form else {
            return Ok(());
        };
        let n = form.fields.len();
        let fixed = form.selected == 0 && !form.creating && !form.original.handle.is_empty();
        if k.code == KeyCode::Enter && form.selected == n - 1 {
            return self.save_profile();
        }
        match k.code {
            KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                form.selected = (form.selected + 1) % n
            }
            KeyCode::BackTab | KeyCode::Up => form.selected = (form.selected + n - 1) % n,
            KeyCode::Char('u') if !fixed && k.modifiers.contains(KeyModifiers::CONTROL) => {
                form.fields[form.selected].1 = Editor::default()
            }
            KeyCode::Char(c) if !fixed && !k.modifiers.contains(KeyModifiers::CONTROL) => {
                form.fields[form.selected].1.insert(&c.to_string())
            }
            KeyCode::Backspace if !fixed => form.fields[form.selected].1.backspace(),
            KeyCode::Delete if !fixed => form.fields[form.selected].1.delete(),
            KeyCode::Left => form.fields[form.selected].1.left(),
            KeyCode::Right => form.fields[form.selected].1.right(),
            KeyCode::Home => form.fields[form.selected].1.cursor = 0,
            KeyCode::End => {
                form.fields[form.selected].1.cursor = form.fields[form.selected].1.text.len()
            }
            _ => {}
        }
        Ok(())
    }
    pub(crate) fn profile_mouse(&mut self, m: MouseEvent, rect: Rect) -> Result<()> {
        let inner = rect.inner(Margin::new(2, 1));
        if !inner.contains((m.column, m.row).into()) {
            return Ok(());
        }
        if self.dialog == Some(Dialog::Bots) {
            match m.kind {
                MouseEventKind::ScrollUp => {
                    return self.profile_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
                }
                MouseEventKind::ScrollDown => {
                    return self.profile_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    if m.row == inner.bottom().saturating_sub(1) {
                        let part = (m.column - inner.x) * 3 / inner.width.max(1);
                        return self.profile_key(KeyEvent::new(
                            match part {
                                0 => KeyCode::Char('n'),
                                1 => KeyCode::Char('e'),
                                _ => KeyCode::Enter,
                            },
                            KeyModifiers::NONE,
                        ));
                    }
                    let count = bot_count(inner);
                    let start = self.selected.saturating_sub(count.saturating_sub(1));
                    if m.row >= inner.y + 2 && m.row < inner.y + 2 + count as u16 * 3 {
                        self.selected = (start + (m.row - inner.y - 2) as usize / 3)
                            .min(self.bots().len().saturating_sub(1));
                    }
                }
                _ => {}
            }
        } else if m.kind == MouseEventKind::Down(MouseButton::Left) {
            if m.row == inner.bottom().saturating_sub(1) {
                return self.save_profile();
            }
            if let Some(form) = &mut self.profile_form {
                let (fields, count) = field_area(inner);
                let start = form.selected.saturating_sub(count.saturating_sub(1));
                if fields.contains((m.column, m.row).into())
                    && m.row >= fields.y
                    && m.row < fields.y + count as u16 * 3
                {
                    form.selected =
                        (start + (m.row - fields.y) as usize / 3).min(form.fields.len() - 1);
                }
            }
        }
        Ok(())
    }
}
fn row(r: Rect, y: u16) -> Rect {
    Rect::new(r.x, r.y.saturating_add(y), r.width, 1).intersection(r)
}
fn label(f: &mut Frame, r: Rect, s: impl Into<String>, color: Color) {
    f.render_widget(
        Paragraph::new(s.into()).style(Style::default().fg(color)),
        r,
    );
}
fn bot_count(inner: Rect) -> usize {
    (inner.height.saturating_sub(5) as usize / 3).max(1)
}
fn field_area(inner: Rect) -> (Rect, usize) {
    let left = if inner.width >= 46 { 16 } else { 0 };
    (
        Rect::new(
            inner.x + left,
            inner.y + 2,
            inner.width.saturating_sub(left),
            inner.height.saturating_sub(6),
        )
        .intersection(inner),
        (inner.height.saturating_sub(6) as usize / 3).max(1),
    )
}
/// Truecolor half-blocks work in ordinary UTF-8 terminals, without an image protocol.
pub(crate) fn avatar_lines(
    p: &profiles::Profile,
    width: usize,
    height: usize,
) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::text::{Line, Span};
    (0..height)
        .map(|y| {
            if p.avatar.len() != 576 {
                let initials = p
                    .name
                    .split_whitespace()
                    .filter_map(|s| s.chars().next())
                    .take(2)
                    .collect::<String>()
                    .to_uppercase();
                let content = if y == height / 2 {
                    format!(
                        "{:^width$}",
                        crate::editor::clip(&initials, width),
                        width = width
                    )
                } else {
                    " ".repeat(width)
                };
                return Line::styled(
                    content,
                    Style::default()
                        .bg(SELECTED)
                        .fg(TEXT)
                        .add_modifier(Modifier::BOLD),
                );
            }
            Line::from(
                (0..width)
                    .map(|x| {
                        let top = p.avatar[(y * 2 * 24 / (height * 2)) * 24 + x * 24 / width];
                        let bottom =
                            p.avatar[((y * 2 + 1) * 24 / (height * 2)) * 24 + x * 24 / width];
                        Span::styled(
                            "▀",
                            Style::default()
                                .fg(Color::Rgb(top[0], top[1], top[2]))
                                .bg(Color::Rgb(bottom[0], bottom[1], bottom[2])),
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}
pub(crate) fn draw_avatar(f: &mut Frame, area: Rect, p: &profiles::Profile) {
    if area.width > 0 && area.height > 0 {
        f.render_widget(
            Paragraph::new(avatar_lines(p, area.width as usize, area.height as usize)),
            area,
        );
    }
}
pub(crate) fn draw(app: &App, f: &mut Frame, inner: Rect) {
    if app.dialog == Some(Dialog::Bots) {
        label(
            f,
            row(inner, 0),
            format!("Bots · {}", crate::chat_name(&app.room)),
            ACCENT,
        );
        let count = bot_count(inner);
        let start = app.selected.saturating_sub(count.saturating_sub(1));
        for (i, p) in app.bots().iter().skip(start).take(count).enumerate() {
            let card =
                Rect::new(inner.x, inner.y + 2 + i as u16 * 3, inner.width, 3).intersection(inner);
            if start + i == app.selected {
                f.render_widget(
                    Paragraph::new(" ".repeat(card.width as usize))
                        .style(Style::default().bg(SELECTED)),
                    card,
                );
            }
            draw_avatar(
                f,
                Rect::new(card.x, card.y, 6.min(card.width), 3).intersection(card),
                p,
            );
            let txt =
                Rect::new(card.x + 8, card.y, card.width.saturating_sub(8), 3).intersection(card);
            let lent_label = format!(
                "Added here · lent to members ({}, {}/h per person) · Enter removes",
                if p.guest_tools.shell() {
                    "chat + guest shell"
                } else {
                    "chat only"
                },
                p.lend_per_hour
            );
            label(
                f,
                row(txt, 0),
                format!(
                    "{}  @{}{}",
                    p.name,
                    p.handle,
                    if p.owner != app.name {
                        format!(" · {}", p.owner)
                    } else {
                        String::new()
                    }
                ),
                TEXT,
            );
            label(
                f,
                row(txt, 1),
                if p.owner != app.name {
                    "Shared bot · answers only if its owner lends it · owner edits"
                } else if p.handle == "bot" {
                    "Always available"
                } else if app.enabled_bots.contains(&p.handle) {
                    if p.lend_chat {
                        &lent_label
                    } else {
                        "Added here · Enter removes"
                    }
                } else {
                    "Not in this chat · Enter adds"
                },
                ACCENT,
            );
            label(f, row(txt, 2), &p.about, MUTED);
        }
        label(
            f,
            row(inner, inner.height.saturating_sub(3)),
            "Mention @handle to ask a bot. Commands still need approval.",
            MUTED,
        );
        label(
            f,
            row(inner, inner.height.saturating_sub(2)),
            "↑ ↓ Select · Esc Close · edits apply to future requests",
            MUTED,
        );
        for (i, s) in ["N New bot", "E Edit profile", "Enter Add/remove"]
            .iter()
            .enumerate()
        {
            let x = inner.x + inner.width * i as u16 / 3;
            let r = Rect::new(x, inner.bottom().saturating_sub(1), inner.width / 3, 1)
                .intersection(inner);
            f.render_widget(
                Paragraph::new(*s).style(Style::default().bg(SELECTED).fg(TEXT)),
                r,
            );
        }
        return;
    }
    let Some(form) = &app.profile_form else {
        return;
    };
    label(
        f,
        row(inner, 0),
        if form.original.handle.is_empty() {
            "Your profile"
        } else if form.creating {
            "Create a bot"
        } else {
            "Bot profile"
        },
        ACCENT,
    );
    let (fields, count) = field_area(inner);
    if inner.width >= 46 {
        draw_avatar(
            f,
            Rect::new(inner.x, inner.y + 2, 12, 6).intersection(inner),
            &form.original,
        );
        label(
            f,
            row(inner, 9),
            if form.original.handle.is_empty() {
                format!("ID: {}", app.name)
            } else {
                format!("@{}", form.original.handle)
            },
            MUTED,
        );
    }
    let start = form.selected.saturating_sub(count.saturating_sub(1));
    for (i, (title, editor)) in form.fields.iter().enumerate().skip(start).take(count) {
        let y = ((i - start) * 3) as u16;
        label(
            f,
            row(fields, y),
            *title,
            if i == form.selected { ACCENT } else { MUTED },
        );
        let r = row(fields, y + 1);
        f.render_widget(
            Paragraph::new(" ".repeat(r.width as usize))
                .style(Style::default().bg(if i == form.selected { SELECTED } else { PANEL })),
            r,
        );
        let (value, cursor) = if i == form.selected {
            editor.visible(r.width.saturating_sub(1) as usize)
        } else {
            (
                crate::editor::clip(&editor.text, r.width.saturating_sub(1) as usize),
                0,
            )
        };
        label(
            f,
            r,
            if value.is_empty() {
                "…".into()
            } else {
                value
            },
            TEXT,
        );
        if i == form.selected && r.width > 0 && r.height > 0 {
            f.set_cursor_position((r.x + cursor.min(r.width - 1), r.y));
        }
    }
    label(
        f,
        row(inner, inner.height.saturating_sub(4)),
        "Picture: blank keeps current · - removes · max 8 MiB",
        MUTED,
    );
    label(
        f,
        row(inner, inner.height.saturating_sub(3)),
        &form.error,
        Color::Rgb(245, 153, 153),
    );
    label(
        f,
        row(inner, inner.height.saturating_sub(2)),
        "Tab Next · Ctrl+U Clear · Esc Cancel",
        MUTED,
    );
    f.render_widget(
        Paragraph::new("Save profile · Ctrl+S (or Enter on last field)")
            .style(Style::default().bg(SELECTED).fg(TEXT)),
        row(inner, inner.height.saturating_sub(1)),
    );
}
