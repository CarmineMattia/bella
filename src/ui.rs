use crate::{App, Dialog, chat_name, editor::clip, safe};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
const BG: Color = Color::Rgb(14, 22, 33);
const PANEL: Color = Color::Rgb(23, 33, 43);
const SELECTED: Color = Color::Rgb(43, 82, 120);
const MUTED: Color = Color::Rgb(130, 151, 171);
const TEXT: Color = Color::Rgb(227, 236, 244);
const ACCENT: Color = Color::Rgb(88, 177, 232);
const OUTGOING: Color = Color::Rgb(43, 82, 90);
const INCOMING: Color = Color::Rgb(30, 44, 59);
const GREEN: Color = Color::Rgb(105, 206, 168);

pub struct Screen {
    pub sidebar: Rect,
    pub header: Rect,
    pub history: Rect,
    pub composer: Rect,
    pub send: Rect,
    pub group_target: Rect,
    pub bot_target: Rect,
    pub footer: Rect,
    pub stop: Rect,
    pub menu: Rect,
    pub welcome: Rect,
}
pub fn screen(area: Rect) -> Screen {
    let side = if area.width >= 76 {
        (area.width / 3).clamp(28, 36)
    } else {
        0
    };
    let cols = Layout::horizontal([Constraint::Length(side), Constraint::Min(1)]).split(area);
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Min(0),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(cols[1]);
    let composer = Rect::new(
        rows[2].x,
        rows[2].y,
        rows[2].width.saturating_sub(9),
        rows[2].height,
    );
    let send = Rect::new(
        composer.right(),
        rows[2].y,
        rows[2].width.saturating_sub(composer.width),
        rows[2].height,
    );
    Screen {
        sidebar: cols[0],
        header: rows[0],
        history: rows[1],
        composer,
        group_target: Rect::new(composer.x + 1, composer.y, 9, 1).intersection(composer),
        bot_target: Rect::new(
            composer.x + 11,
            composer.y,
            composer.width.saturating_sub(12),
            1,
        )
        .intersection(composer),
        send,
        footer: rows[3],
        welcome: if rows[3].width >= 30 && rows[3].height > 1 {
            Rect::new(rows[3].right() - 19, rows[3].y + 1, 19, 1)
        } else {
            Rect::default()
        },
        menu: if rows[0].width > 48 {
            Rect::new(rows[0].right() - 36, rows[0].y + 1, 10, 1).intersection(rows[0])
        } else {
            Rect::default()
        },
        stop: if rows[0].width > 36 {
            Rect::new(rows[0].right() - 24, rows[0].y + 1, 10, 1).intersection(rows[0])
        } else {
            Rect::default()
        },
    }
}
pub fn chat_start(app: &App, sidebar: Rect) -> usize {
    let count = sidebar.height.saturating_sub(7) as usize / 4;
    if let Some(offset) = app.sidebar_offset {
        return offset.min(app.visible_chats().len().saturating_sub(count.max(1)));
    }
    let selected = app
        .visible_chats()
        .iter()
        .position(|c| c.room == app.room)
        .unwrap_or(0);
    selected.saturating_sub(count.max(1) - 1)
}
fn fill(f: &mut Frame, area: Rect, color: Color) {
    f.render_widget(
        Block::default().style(Style::default().bg(color).fg(TEXT)),
        area,
    );
}
fn text(f: &mut Frame, area: Rect, value: impl Into<String>, color: Color, bold: bool) {
    f.render_widget(
        Paragraph::new(value.into()).style(Style::default().fg(color).add_modifier(if bold {
            Modifier::BOLD
        } else {
            Modifier::empty()
        })),
        area,
    );
}
fn row(area: Rect, y: u16) -> Rect {
    Rect::new(area.x, area.y.saturating_add(y), area.width, 1).intersection(area)
}
fn inset(area: Rect, x: u16) -> Rect {
    area.inner(Margin::new(x, 0))
}
fn time(s: &str) -> &str {
    s.get(11..16).unwrap_or("")
}
pub fn draw(app: &mut App, f: &mut Frame) {
    app.browser_links.clear();
    app.copy_links.clear();
    let s = screen(f.area());
    fill(f, f.area(), BG);
    if s.sidebar.width > 0 {
        draw_sidebar(app, f, s.sidebar);
    }
    fill(f, s.header, PANEL);
    text(
        f,
        inset(row(s.header, 1), 2),
        chat_name(&app.room),
        TEXT,
        true,
    );
    let status = if app.job.as_ref().is_some_and(|j| j.room == app.room) {
        let job = app.job.as_ref().unwrap();
        if job.stopping {
            if job.ran_command {
                "Stopping · waiting for approved command…"
            } else {
                "Stopping request…"
            }
        } else {
            match job.phase {
                crate::agent::Phase::Thinking => "◌  Generating a reply…",
                crate::agent::Phase::AwaitingApproval => "Waiting for your command approval",
                crate::agent::Phase::RunningCommand => "Running an approved command…",
            }
        }
        .to_owned()
            + &job
                .skill
                .as_ref()
                .map(|name| format!(" · skill {name}"))
                .unwrap_or_default()
    } else if app.room.starts_with("agent:") {
        match app.health {
            Some(true) => "●  Local model connected".into(),
            Some(false) => "○  Model unavailable · chats still work".into(),
            None => "◌  Checking local model…".into(),
        }
    } else {
        let mut authors: Vec<_> = app
            .messages
            .iter()
            .filter(|m| m.kind == "human")
            .map(|m| &m.author)
            .collect();
        authors.sort();
        authors.dedup();
        format!(
            "{} · {} participant{} in recent history",
            if app.network.is_some() {
                "Shared group"
            } else {
                "Local group"
            },
            authors.len(),
            if authors.len() == 1 { "" } else { "s" }
        )
    };
    let status = if app.chats.iter().any(|c| c.room == app.room && c.archived) {
        format!("Archived · {status}")
    } else {
        status
    };
    let status = if let Some(net) = &app.network {
        format!("{} · {}", net.label(), status)
    } else {
        status
    };
    text(f, inset(row(s.header, 2), 2), status, MUTED, false);
    fill(f, s.menu, INCOMING);
    text(f, s.menu, "Options F3", ACCENT, false);
    if s.header.width > 24 {
        let find = Rect::new(s.header.right() - 12, s.header.y + 1, 11, 1).intersection(s.header);
        fill(f, find, SELECTED);
        text(f, find, "Find Ctrl+F", TEXT, false);
    }
    if let Some(job) = &app.job {
        fill(
            f,
            s.stop,
            if job.stopping {
                INCOMING
            } else {
                Color::Rgb(110, 51, 65)
            },
        );
        text(
            f,
            s.stop,
            if job.stopping {
                "Stopping…"
            } else {
                "Stop ^X"
            },
            TEXT,
            true,
        );
    }
    if s.history.width > 0 && s.history.height > 0 {
        if app.messages.is_empty() {
            draw_empty(app, f, s.history);
        } else {
            draw_messages(app, f, s.history);
        }
    }
    fill(f, s.composer, PANEL);
    fill(f, s.send, SELECTED);
    let recipient = app.preview_recipient();
    let bot_selected = recipient.is_some();
    let personal = app.room.starts_with("agent:");
    fill(
        f,
        s.group_target,
        if !bot_selected { SELECTED } else { INCOMING },
    );
    text(
        f,
        s.group_target,
        if personal { "Personal" } else { " Group " },
        if personal { MUTED } else { TEXT },
        !bot_selected,
    );
    fill(
        f,
        s.bot_target,
        if bot_selected { SELECTED } else { INCOMING },
    );
    let recipient_label = if let Some(handle) = &recipient {
        format!(" @{handle} · Bot · F10 ›")
    } else {
        " Bot @bot · F10 ›".into()
    };
    text(
        f,
        s.bot_target,
        recipient_label,
        if bot_selected { GREEN } else { ACCENT },
        bot_selected,
    );
    let route_hint = if let Some(handle) = &recipient {
        if !app.available_recipient(handle) {
            format!("@{handle} unavailable · choose with F10 or add with F8")
        } else if personal {
            format!("To @{handle} · no tag needed")
        } else {
            format!("To @{handle} · replies shared in this group · no tag needed")
        }
    } else {
        "To group · click Bot to ask your agent".into()
    };
    text(f, inset(row(s.composer, 2), 2), route_hint, MUTED, false);
    let input = inset(row(s.composer, 1), 2);
    let (visible, cursor) = app.input.visible(input.width as usize);
    let placeholder = if let Some(handle) = &recipient {
        format!("Ask @{handle}…")
    } else {
        "Write a message to group…".into()
    };
    text(
        f,
        input,
        if app.input.text.is_empty() {
            placeholder
        } else {
            visible
        },
        if app.input.text.is_empty() {
            MUTED
        } else {
            TEXT
        },
        false,
    );
    text(
        f,
        inset(row(s.send, 1), 1),
        if bot_selected { "Ask ›" } else { "Send ›" },
        TEXT,
        true,
    );
    text(
        f,
        inset(row(s.footer, 0), 1),
        safe(&app.status),
        MUTED,
        false,
    );
    text(
        f,
        Rect::new(
            s.footer.x + 1,
            s.footer.y + 1,
            s.footer
                .width
                .saturating_sub(if s.welcome.is_empty() { 1 } else { 21 }),
            1,
        )
        .intersection(s.footer),
        if s.sidebar.width > 0 {
            "F7 Profile   F8 Bots   F9 Network   F1 Help"
        } else {
            "F7 Profile · F8 Bots · F9 Network"
        },
        MUTED,
        false,
    );
    text(f, s.welcome, "[ Exit to welcome ]", ACCENT, true);
    if app.approval.is_some() {
        draw_approval(app, f);
    } else if app.dialog.is_some() {
        draw_dialog(app, f);
    } else if input.width > 0 && input.height > 0 {
        f.set_cursor_position((input.x + cursor.min(input.width - 1), input.y));
    }
}
fn draw_sidebar(app: &App, f: &mut Frame, area: Rect) {
    fill(f, area, PANEL);
    let left = Rect::new(area.x, area.y, area.width / 2, 1);
    let right = Rect::new(left.right(), area.y, area.width - left.width, 1);
    fill(f, if app.view_archived { right } else { left }, SELECTED);
    text(
        f,
        inset(left, 2),
        "Chats",
        if app.view_archived { MUTED } else { TEXT },
        true,
    );
    let archived = app.chats.iter().filter(|c| c.archived).count();
    let archived_unread = app.chats.iter().any(|c| c.archived && c.unread > 0);
    text(
        f,
        inset(right, 1),
        format!(
            "Archive ({archived}){}",
            if archived_unread { " •" } else { "" }
        ),
        if archived_unread {
            GREEN
        } else if app.view_archived {
            TEXT
        } else {
            MUTED
        },
        true,
    );
    text(
        f,
        inset(row(area, 1), 2),
        "Search chats          Ctrl+K",
        MUTED,
        false,
    );
    text(
        f,
        inset(row(area, 3), 2),
        "+ New group              Ctrl+N",
        ACCENT,
        true,
    );
    let count = area.height.saturating_sub(7) as usize / 4;
    for (i, c) in app
        .visible_chats()
        .iter()
        .skip(chat_start(app, area))
        .take(count)
        .enumerate()
    {
        let y = area.y + 5 + i as u16 * 4;
        let card = Rect::new(area.x, y, area.width, 4).intersection(area);
        if c.room == app.room {
            fill(f, card, SELECTED);
        }
        let picture = if c.room.starts_with("agent:") {
            app.personal_bot()
        } else {
            crate::profiles::Profile::user(&c.room)
        };
        crate::profile_ui::draw_avatar(
            f,
            Rect::new(card.x + 1, card.y, 6, 3).intersection(card),
            &picture,
        );
        let text_card = Rect::new(
            card.x + 8,
            card.y,
            card.width.saturating_sub(8),
            card.height,
        )
        .intersection(card);
        let title = format!("{}{}", if c.pinned { "↑ " } else { "" }, chat_name(&c.room));
        let badge = if c.unread > 0 {
            format!(" {} ", c.unread)
        } else {
            String::new()
        };
        let title_width = text_card.width.saturating_sub(3 + badge.width() as u16) as usize;
        text(
            f,
            Rect::new(text_card.x, y, title_width as u16, 1),
            clip(&title, title_width),
            TEXT,
            true,
        );
        if !badge.is_empty() {
            text(
                f,
                Rect::new(
                    area.right().saturating_sub(badge.width() as u16 + 1),
                    y,
                    badge.width() as u16,
                    1,
                ),
                badge,
                GREEN,
                true,
            );
        }
        let draft = if c.room == app.room {
            app.input.text.clone()
        } else {
            app.store.draft(&app.name, &c.room).unwrap_or_default()
        };
        let preview_author = app
            .author_profile(&c.author, true)
            .or_else(|| app.author_profile(&c.author, false))
            .map(|p| p.name.as_str())
            .unwrap_or(&c.author);
        let preview = if !draft.is_empty() {
            format!("Draft: {}", single_line(&draft))
        } else if app.job.as_ref().is_some_and(|j| j.room == c.room) {
            let job = app.job.as_ref().unwrap();
            if job.stopping {
                "Stopping…"
            } else {
                match job.phase {
                    crate::agent::Phase::Thinking => "Agent is replying…",
                    crate::agent::Phase::AwaitingApproval => "Awaiting command approval",
                    crate::agent::Phase::RunningCommand => "Running approved command",
                }
            }
            .into()
        } else if c.preview.is_empty() {
            if c.room.starts_with("agent:") {
                "Your personal AI assistant".into()
            } else {
                "No messages yet".into()
            }
        } else {
            format!(
                "{}: {}",
                if c.author == app.name {
                    "You"
                } else {
                    preview_author
                },
                message_preview(&c.preview)
            )
        };
        text(
            f,
            row(text_card, 1),
            preview,
            if draft.is_empty() { MUTED } else { ACCENT },
            false,
        );
        text(
            f,
            row(text_card, 2),
            if c.time.is_empty() {
                String::new()
            } else {
                format!("{} UTC", time(&c.time))
            },
            MUTED,
            false,
        );
    }
    if app.visible_chats().is_empty() && area.height > 10 {
        text(
            f,
            Rect::new(area.x + 2, area.y + 6, area.width.saturating_sub(4), 3),
            if app.view_archived {
                "No archived chats.\nF3 opens chat options."
            } else {
                "No active chats.\nF4 opens the archive."
            },
            MUTED,
            false,
        );
    }
    if area.height >= 2 {
        let footer = Rect::new(area.x, area.bottom() - 2, area.width, 2);
        fill(f, footer, INCOMING);
        let health = match app.health {
            Some(true) => "● Model ready",
            Some(false) => "○ Model unavailable",
            None => "◌ Checking model",
        };
        let profile = inset(row(footer, 0), 2);
        let activity = Rect::new(
            profile.right().saturating_sub(11),
            profile.y,
            profile.width.min(11),
            profile.height,
        );
        if let Some(p) = app.author_profile(&app.name, false) {
            crate::profile_ui::draw_avatar(
                f,
                Rect::new(profile.x, profile.y, 2.min(profile.width), 1).intersection(profile),
                p,
            );
        }
        let name_area = Rect::new(
            profile.x + 3,
            profile.y,
            activity.x.saturating_sub(profile.x + 4),
            profile.height,
        );
        let name = safe(&app.display_author(&app.name, false));
        let shown = if name.width() > name_area.width as usize {
            format!(
                "{}…",
                clip(&name, name_area.width.saturating_sub(1) as usize)
            )
        } else {
            name
        };
        text(f, name_area, shown, TEXT, true);
        text(f, activity, "F5 Activity", ACCENT, false);
        text(
            f,
            inset(row(footer, 1), 2),
            health,
            if app.health == Some(true) {
                GREEN
            } else {
                MUTED
            },
            false,
        );
    }
}
fn single_line(s: &str) -> String {
    safe(s).replace(['\n', '\t'], " ")
}
fn draw_empty(app: &App, f: &mut Frame, area: Rect) {
    let card = centered(area, 56, 7);
    text(
        f,
        row(card, 0),
        if app.room.starts_with("agent:") {
            "◇  Your personal agent"
        } else {
            "✦  This is the beginning of your group"
        },
        ACCENT,
        true,
    );
    let description = if app.room.starts_with("agent:") {
        "Ask a question or give it a task. You review each shell command before it runs."
    } else {
        "Write your first message below. Mention @bot to bring your personal agent into the conversation."
    };
    f.render_widget(
        Paragraph::new(description)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(MUTED)),
        Rect::new(card.x, card.y + 2, card.width, 3).intersection(area),
    );
    text(
        f,
        row(card, 6),
        "Ctrl+N  New group      Ctrl+K  Find a chat",
        MUTED,
        false,
    );
}
struct HistoryLines {
    lines: Vec<Line<'static>>,
    starts: Vec<(i64, usize)>,
    action_links: Vec<(i64, usize, usize, usize)>,
    copy_links: Vec<(i64, usize, usize, usize)>,
}
fn message_preview(body: &str) -> String {
    if let Some(action) = bella::tool_action::Action::decode(body) {
        return format!("{} · {}", action.title(), single_line(&action.command));
    }
    crate::browser::Action::decode(body)
        .map(|action| format!("{} · {}", action.title(), single_line(&action.target)))
        .unwrap_or_else(|| single_line(body))
}
fn bubble_lines(app: &App, width: u16) -> HistoryLines {
    let available = width
        .saturating_sub(if width >= 32 { 12 } else { 4 })
        .max(1) as usize;
    let max_bubble = (available * 4 / 5).max(12).min(available);
    let mut lines = vec![];
    let mut date = String::new();
    let mut starts = Vec::new();
    let mut action_links = Vec::new();
    let mut copy_links = Vec::new();
    let mut messages = app.messages.clone();
    if let Some(job) = &app.job
        && job.room == app.room
        && !job.partial.is_empty()
    {
        messages.push(crate::store::Message {
            id: 0,
            author: if job.author.is_empty() {
                format!("{}'s bot", app.name)
            } else {
                job.author.clone()
            },
            body: job.partial.clone(),
            kind: "streaming".into(),
            created_at: String::new(),
        });
    }
    for m in &messages {
        let day = m.created_at.get(..10).unwrap_or("");
        if !day.is_empty() && day != date {
            date = day.into();
            lines.push(Line::styled(format!("{day} · UTC"), Style::default().fg(MUTED)).centered());
            lines.push(Line::raw(""));
        }
        let own = m.kind == "human" && m.author == app.name;
        let background = if app.highlight == Some(m.id) {
            SELECTED
        } else if own {
            OUTGOING
        } else {
            INCOMING
        };
        starts.push((m.id, lines.len()));
        let author = if own {
            "You".into()
        } else {
            safe(&app.display_author(
                &m.author,
                m.kind == "bot" || m.kind == "streaming" || m.kind == "interrupted",
            ))
        };
        let browser_action = (m.kind == "tool")
            .then(|| crate::browser::Action::decode(&m.body))
            .flatten();
        let display_body = browser_action
            .as_ref()
            .map(|action| action.summary())
            .unwrap_or_else(|| {
                if m.kind == "tool" {
                    bella::tool_action::Action::decode(&m.body)
                        .map(|a| a.details())
                        .unwrap_or_else(|| m.body.clone())
                } else {
                    m.body.clone()
                }
            });
        let body = wrap_text(
            &safe(&display_body).replace('\t', "    "),
            max_bubble.saturating_sub(4).max(1),
        );
        let content_width = body
            .iter()
            .map(|l| l.width())
            .max()
            .unwrap_or(0)
            .max(author.width().saturating_add(2))
            .max(if own && app.network.is_some() { 24 } else { 18 })
            .min(max_bubble.saturating_sub(4).max(1));
        let bubble_width = content_width + 4;
        let bot = m.kind == "bot" || m.kind == "streaming" || m.kind == "interrupted";
        let profile = app
            .author_profile(&m.author, bot)
            .cloned()
            .unwrap_or_else(|| crate::profiles::Profile::user(&m.author));
        let has_avatar = !own && width >= 32 && (bot || m.kind == "human");
        let avatar_start = lines.len();
        let offset = if own {
            width.saturating_sub(bubble_width as u16 + 2) as usize
        } else {
            if has_avatar {
                10
            } else {
                2.min(width as usize)
            }
        };
        let color = match m.kind.as_str() {
            "system" => Color::Rgb(245, 153, 153),
            "tool" => Color::Rgb(232, 195, 128),
            "bot" | "streaming" | "interrupted" => ACCENT,
            _ => GREEN,
        };
        let mut add = |content: String, fg: Color, bold: bool| {
            let content = clip(&content, content_width);
            let padding = content_width.saturating_sub(content.width());
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(offset)),
                Span::styled(
                    format!("  {content}{}  ", " ".repeat(padding)),
                    Style::default()
                        .bg(background)
                        .fg(fg)
                        .add_modifier(if bold {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]));
        };
        let details_row = avatar_start + 1 + body.len();
        let copy_label = match app.copy_feedback {
            Some((id, when, true)) if id == m.id && when.elapsed().as_secs() < 2 => "✓",
            Some((id, when, false)) if id == m.id && when.elapsed().as_secs() < 2 => "↗",
            _ => "⧉",
        };
        let copy_width = content_width.min(copy_label.width());
        let copy_x = content_width.saturating_sub(copy_width);
        let heading = if content_width >= copy_label.width() + 2 {
            let name = clip(&author, content_width - copy_label.width() - 1);
            format!(
                "{}{}{}",
                name,
                " ".repeat(copy_x.saturating_sub(name.width())),
                copy_label
            )
        } else {
            clip(copy_label, content_width)
        };
        add(heading, color, true);
        for line in body {
            add(line, TEXT, false);
        }
        if browser_action.is_some() {
            add("[ Details ]".into(), ACCENT, true);
        }
        let stamp = if m.kind == "interrupted" {
            "interrupted".to_owned()
        } else if m.kind == "streaming" {
            "typing…".to_owned()
        } else {
            format!(
                "{} UTC{}",
                time(&m.created_at),
                if own {
                    if app.network.is_some() {
                        match crate::network::delivery(&app.store, m.id).unwrap_or("queued") {
                            "on server" => " · on server",
                            "rejected" => " · rejected",
                            _ => " · queued",
                        }
                    } else {
                        " · saved"
                    }
                } else {
                    ""
                }
            )
        };
        add(
            format!("{:>width$}", stamp, width = content_width),
            MUTED,
            false,
        );
        copy_links.push((m.id, avatar_start, offset + 2 + copy_x, copy_width));
        if browser_action.is_some() {
            action_links.push((m.id, details_row, offset + 2, content_width.min(11)));
        }
        if has_avatar {
            for (i, picture) in crate::profile_ui::avatar_lines(&profile, 6, 3)
                .into_iter()
                .enumerate()
            {
                if let Some(line) = lines.get_mut(avatar_start + i) {
                    let mut prefix = vec![Span::raw("  ")];
                    prefix.extend(picture.spans);
                    prefix.push(Span::raw("  "));
                    line.spans.splice(..1, prefix);
                }
            }
        }
        lines.push(Line::raw(""));
    }
    HistoryLines {
        lines,
        starts,
        action_links,
        copy_links,
    }
}
fn draw_messages(app: &mut App, f: &mut Frame, area: Rect) {
    let history = bubble_lines(app, area.width);
    let max = history.lines.len().saturating_sub(area.height as usize);
    let mut start = max.saturating_sub(app.scroll);
    if let Some(id) = app.jump_to.take() {
        if let Some((_, line)) = history.starts.iter().find(|(m, _)| *m == id) {
            start = (*line).min(max);
        }
    } else if app.scroll > 0
        && let Some((id, offset)) = app.anchor
        && let Some((_, line)) = history.starts.iter().find(|(m, _)| *m == id)
    {
        start = line.saturating_add_signed(offset).min(max);
    }
    app.scroll = max.saturating_sub(start);
    app.max_scroll = max;
    app.anchor = if app.scroll > 0 {
        history
            .starts
            .iter()
            .rev()
            .find(|(_, line)| *line <= start)
            .or(history.starts.first())
            .map(|(id, line)| (*id, start as isize - *line as isize))
    } else {
        None
    };
    let links_for = |links: &[(i64, usize, usize, usize)]| -> Vec<(Rect, i64)> {
        links
            .iter()
            .filter_map(|(id, line, x, width)| {
                let relative = line.checked_sub(start)?;
                if relative >= area.height as usize
                    || (relative + 1 == area.height as usize && (app.scroll > 0 || app.has_newer))
                    || (relative == 0 && app.has_older && start == 0)
                {
                    return None;
                }
                let rect = Rect::new(
                    area.x + *x as u16,
                    area.y + relative as u16,
                    *width as u16,
                    1,
                )
                .intersection(area);
                (!rect.is_empty()).then_some((rect, *id))
            })
            .collect()
    };
    let browser_links = links_for(&history.action_links);
    let copy_links = links_for(&history.copy_links);
    app.browser_links = browser_links;
    app.copy_links = copy_links;
    let visible = history
        .lines
        .into_iter()
        .skip(start)
        .take(area.height as usize)
        .collect::<Vec<_>>();
    f.render_widget(Paragraph::new(visible), area);
    if app.scroll > 0 || app.has_newer {
        text(
            f,
            row(area, area.height - 1),
            "  ↓ Latest messages · click here or Ctrl+End",
            ACCENT,
            true,
        );
    } else if app.has_older && start == 0 {
        text(
            f,
            row(area, 0),
            "  ↑ PgUp loads earlier messages",
            ACCENT,
            false,
        );
    }
}

pub fn wrap_text(s: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut lines = vec![];
    for source in s.split('\n') {
        let mut current = String::new();
        for word in source.split_inclusive(char::is_whitespace) {
            if !current.is_empty() && current.width() + word.trim_end().width() > width {
                lines.push(current.trim_end().to_owned());
                current.clear();
            }
            for c in word.chars() {
                let n = c.width().unwrap_or(0);
                if current.width() + n > width {
                    lines.push(current.trim_end().to_owned());
                    current.clear();
                }
                if n <= width && (!current.is_empty() || !c.is_whitespace()) {
                    current.push(c);
                }
            }
        }
        lines.push(current.trim_end().to_owned());
    }
    lines
}

fn message_excerpt(body: &str, query: &str, width: usize) -> String {
    let source = single_line(body);
    let position = source
        .to_ascii_lowercase()
        .find(&query.to_ascii_lowercase())
        .unwrap_or(0);
    let skip = source[..position].chars().count().saturating_sub(18);
    let remaining = source.chars().skip(skip).collect::<String>();
    let prefix = if skip > 0 { "…" } else { "" };
    let budget = width.saturating_sub(prefix.width() + 1);
    let excerpt = clip(&remaining, budget);
    format!(
        "{prefix}{excerpt}{}",
        if remaining.width() > budget {
            "…"
        } else {
            ""
        }
    )
}
pub fn dialog_rect(area: Rect, dialog: &Dialog) -> Rect {
    centered(
        area,
        64,
        if *dialog == Dialog::Profile {
            20
        } else if matches!(dialog, Dialog::BotProfile | Dialog::Bots) {
            28
        } else if *dialog == Dialog::TaskDetails {
            26
        } else if *dialog == Dialog::ChatActions {
            13
        } else if *dialog == Dialog::ModelInfo {
            24
        } else if matches!(dialog, Dialog::Help | Dialog::Skills) {
            28
        } else if *dialog == Dialog::NewGroup {
            12
        } else {
            22
        },
    )
}
fn draw_info(f: &mut Frame, inner: Rect, heading: &str, body: &str, footer: &str, scroll: u16) {
    text(f, row(inner, 0), heading, ACCENT, true);
    let area = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(3),
    );
    let paragraph = Paragraph::new(body)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(TEXT));
    let max = paragraph
        .line_count(area.width)
        .saturating_sub(area.height as usize)
        .min(u16::MAX as usize) as u16;
    f.render_widget(paragraph.scroll((scroll.min(max), 0)), area);
    text(
        f,
        row(inner, inner.height.saturating_sub(1)),
        footer,
        MUTED,
        false,
    );
}
fn draw_dialog(app: &App, f: &mut Frame) {
    let help = app.dialog == Some(Dialog::Help);
    let area = dialog_rect(f.area(), app.dialog.as_ref().unwrap());
    f.render_widget(Clear, area);
    fill(f, area, PANEL);
    let inner = area.inner(Margin::new(2, 1));
    if matches!(
        app.dialog,
        Some(Dialog::Profile | Dialog::BotProfile | Dialog::Bots)
    ) {
        crate::profile_ui::draw(app, f, inner);
        return;
    }
    if app.dialog == Some(Dialog::Network) {
        let body = if let Some(net) = &app.network {
            let errors = app
                .store
                .connection()
                .prepare(
                    "SELECT error FROM network_outbox WHERE error<>'' ORDER BY id DESC LIMIT 5",
                )
                .and_then(|mut q| {
                    q.query_map([], |r| r.get::<_, String>(0))?
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap_or_default()
                .join("\n");
            format!(
                "{}\n\nHub\n{}\n\nAccount: {}\nStable ID: {}\n\n{}\n\n{}\n\nQueued messages survive restarts. 'On server' confirms storage, not a recipient read receipt.\n\nTo sign in again: ./start --network --network-setup\nSign out: ./start --network-logout\nOmit --network to return to local chats.",
                net.label(),
                net.credentials.url,
                net.credentials.username,
                net.credentials.user_id,
                net.state.detail,
                errors
            )
        } else {
            "Local mode: chats are shared only with sessions using this database.\n\nStart the hub on one computer:\npython3 server/hub.py --bind 0.0.0.0\n\nConnect from either computer:\n./start --network\n\nEnter the hub address and sign in, or use Ctrl+R with the host's registration invite.\n\nUse HTTPS or a trusted LAN. Network chats use a separate account cache; existing local chats are never uploaded.".into()
        };
        draw_info(
            f,
            inner,
            "Network connection",
            &body,
            "R Retry rejected · PgUp/PgDn Scroll · Esc Close",
            app.dialog_scroll,
        );
        return;
    }
    if app.dialog == Some(Dialog::Participants) {
        text(
            f,
            row(inner, 0),
            "People in this conversation",
            ACCENT,
            true,
        );
        let mut lines = vec![];
        for person in &app.participants {
            let p = app
                .author_profile(&person.name, false)
                .cloned()
                .unwrap_or_else(|| crate::profiles::Profile::user(&person.name));
            let detail = [
                format!("{} · ID: {}", p.name, person.name),
                format!(
                    "{} · last: {}",
                    if app.network.is_some() && person.name != app.name {
                        "Remote account · presence not tracked".into()
                    } else if person.sessions == 0 {
                        "not viewing this room".into()
                    } else {
                        format!("{} local session(s) here", person.sessions)
                    },
                    if person.last_message.is_empty() {
                        "none"
                    } else {
                        &person.last_message
                    }
                ),
                p.about.clone(),
            ];
            for (i, mut line) in crate::profile_ui::avatar_lines(&p, 6, 3)
                .into_iter()
                .enumerate()
            {
                line.spans.push(Span::styled(
                    format!("  {}", detail[i]),
                    Style::default().fg(if i == 0 { TEXT } else { MUTED }),
                ));
                lines.push(line);
            }
            lines.push(Line::raw(""));
        }
        let body = Rect::new(
            inner.x,
            inner.y + 2,
            inner.width,
            inner.height.saturating_sub(4),
        )
        .intersection(inner);
        let max = lines
            .len()
            .saturating_sub(body.height as usize)
            .min(u16::MAX as usize) as u16;
        f.render_widget(
            Paragraph::new(lines).scroll((app.dialog_scroll.min(max), 0)),
            body,
        );
        text(
            f,
            row(inner, inner.height.saturating_sub(1)),
            "PgUp/PgDn Scroll · Esc Close · local profiles",
            MUTED,
            false,
        );
        return;
    }
    if app.dialog == Some(Dialog::Tasks) {
        text(
            f,
            row(inner, 0),
            "Agent activity · latest 50 tasks",
            ACCENT,
            true,
        );
        let count = inner.height.saturating_sub(4) as usize / 3;
        let start = app.selected.saturating_sub(count.max(1) - 1);
        if app.task_records.is_empty() {
            text(
                f,
                Rect::new(inner.x, inner.y + 2, inner.width, 4),
                "No agent tasks yet.\nMention @bot in a group, or open /agent.",
                MUTED,
                false,
            );
        }
        for (i, task) in app.task_records.iter().enumerate().skip(start).take(count) {
            let item = Rect::new(
                inner.x,
                inner.y + 2 + (i - start) as u16 * 3,
                inner.width,
                3,
            );
            if app.selected == i {
                fill(f, item, SELECTED);
            }
            text(
                f,
                row(item, 0),
                format!(
                    "#{} · {} · {}",
                    task.id,
                    task.state.replace('_', " "),
                    chat_name(&task.room)
                ),
                TEXT,
                true,
            );
            text(
                f,
                row(item, 1),
                if task.requester.is_empty() {
                    format!(
                        "{}: {}",
                        app.display_author(&task.author, true),
                        single_line(&task.request)
                    )
                } else {
                    format!(
                        "{} for {} (delegated): {}",
                        app.display_author(&task.author, true),
                        safe(&task.requester),
                        single_line(&task.request)
                    )
                },
                MUTED,
                false,
            );
            text(
                f,
                row(item, 2),
                format!("{} UTC", task.started),
                MUTED,
                false,
            );
        }
        text(
            f,
            row(inner, inner.height.saturating_sub(1)),
            "↑ ↓ Choose · Enter Details · Esc Close",
            MUTED,
            false,
        );
        return;
    }
    if app.dialog == Some(Dialog::BrowserDetails) {
        let actions = app.browser_actions();
        let index = app.browser_detail_index();
        let body = index.and_then(|index| actions.get(index)).map(|(_, action)| safe(&action.details()))
            .unwrap_or_else(|| if app.browser_detail_id.is_some() {
                "This action is no longer in the loaded chat history.\nClose Details and load the relevant messages to inspect it again.".into()
            } else {
                "No browser actions in the loaded history of this chat.\nLoad older messages to inspect earlier actions.".into()
            });
        draw_info(
            f,
            inner,
            &format!(
                "Browser details · {} / {}",
                index.map_or(0, |index| index + 1),
                actions.len()
            ),
            &body,
            "← / → Action · PgUp/PgDn Scroll · Esc Close",
            app.dialog_scroll,
        );
        return;
    }
    if app.dialog == Some(Dialog::TaskDetails) {
        if let Some(task) = &app.task_detail {
            let body = format!(
                "{} · {} UTC\nState: {}\nBot: {}\nRequested by: {}\n\nRequest\n{}\n\nLast shell command ({})\n{}\n\nResult / received text\n{}\n\n{}\n\nInterrupted tasks are never replayed automatically. Check command effects before retrying.",
                chat_name(&task.room),
                task.started,
                task.state.replace('_', " "),
                app.display_author(&task.author, true),
                if task.requester.is_empty() {
                    "you".to_owned()
                } else {
                    format!(
                        "{} · delegated request; execution governed by saved action policy",
                        safe(&task.requester)
                    )
                },
                safe(&task.request),
                task.approval,
                if task.command.is_empty() {
                    "None".into()
                } else {
                    safe(&task.command)
                },
                safe(&task.partial),
                safe(&task.error)
            );
            let body = format!(
                "{body}{}",
                app.task_tool_results(task)
                    .unwrap_or_else(|_| "\nTool results could not be loaded.".into())
            );
            draw_info(
                f,
                inner,
                &format!("Task #{}", task.id),
                &body,
                "Enter Open chat · PgUp/PgDn Scroll · Esc Back",
                app.dialog_scroll,
            );
        }
        return;
    }
    if app.dialog == Some(Dialog::ChatActions) {
        text(f, row(inner, 0), "Chat options", ACCENT, true);
        let target = app.action_room.as_deref().unwrap_or(&app.room);
        text(f, row(inner, 2), chat_name(target), TEXT, true);
        if let Some(chat) = app.chats.iter().find(|c| c.room == target) {
            for (index, label) in [
                if chat.pinned {
                    "Unpin chat       P"
                } else {
                    "Pin to top       P"
                },
                if chat.archived {
                    "Restore chat     A"
                } else {
                    "Archive chat     A"
                },
            ]
            .into_iter()
            .enumerate()
            {
                let item = row(inner, 4 + index as u16 * 2);
                if app.selected == index {
                    fill(f, item, SELECTED);
                }
                text(f, item, format!("  {label}"), TEXT, true);
            }
        }
        text(
            f,
            row(inner, 8),
            "Only your list changes. Messages are kept.",
            MUTED,
            false,
        );
        text(
            f,
            row(inner, inner.height.saturating_sub(1)),
            "↑ ↓ Select · Enter Apply · Esc Close",
            MUTED,
            false,
        );
        return;
    }
    if app.dialog == Some(Dialog::Skills) {
        draw_info(
            f,
            inner,
            "Skills",
            &app.skills_report(),
            "R Rescan · PgUp/PgDn Scroll · Esc Close",
            app.dialog_scroll,
        );
        return;
    }
    if app.dialog == Some(Dialog::ModelInfo) {
        let body = format!(
            "Model\n{}\n\nServer\n{}\n\nWorkspace\n{}\n\n{}\n\nR checks the connection again.\nStart the model with scripts/start-model if offline.\nEsc closes this window.",
            safe(&app.config.model),
            safe(&app.config.endpoint),
            safe(&app.config.workspace.display().to_string()),
            safe(&app.health_detail)
        );
        draw_info(
            f,
            inner,
            "Model connection",
            &body,
            "R Recheck · PgUp/PgDn Scroll · Esc Close",
            app.dialog_scroll,
        );
        return;
    }
    if help {
        draw_info(
            f,
            inner,
            "Make yourself at home",
            "Ctrl+K          Search chats and latest messages\nCtrl+F          Search all messages in this chat\nCtrl+End        Jump to latest messages\nCtrl+N          Create a group\nTab / Shift+Tab Next / previous chat\nClick a chat    Open it\nClick Copy      Copy the full message text\nMouse wheel     Scroll messages\nEnter / Send    Send message\nAlt+Enter       Add a newline\nLeft / Right    Move message cursor\nHome / End      Beginning / end of message\nCtrl+U          Clear draft\nCtrl+R          Retry a failed agent request\nCtrl+X / Stop   Stop the agent response\nF2 / Ctrl+O     Model connection details\nF3 / Right-click Chat options: pin and archive\nF4              Chats / Archive folder\nF5 / /tasks     Agent activity and crash recovery\nF6 / /people    People and local session presence\nF7 / /profile   Edit your name, bio and picture\nF8 / /bots      Create, edit and add bots to this chat\nF9 / /network   Hub connection and outgoing queue\nF10 / Group/Bot Select message recipient (remembered per chat)\nF11 / /browser Browser actions and launcher diagnostics\n/skills         Skills: roots, enabled, disabled, diagnostics\n$skill task     Ask your agent using one skill\n/to group       Return to ordinary group messages\n/to @handle     Address your enabled bot without typing a tag\n@bot task       Ask your agent in a group\n/agent          Open your agent conversation\n/welcome        Save draft and return to welcome setup\n/join name      Open or create a group\nCtrl+C / /quit  Quit\n\nMessages are shared locally. Times are UTC.\nEsc closes this window.",
            "PgUp/PgDn Scroll · Esc Close",
            app.dialog_scroll,
        );
        return;
    }
    let find = app.dialog == Some(Dialog::FindMessages);
    let new = app.dialog == Some(Dialog::NewGroup);
    text(
        f,
        row(inner, 0),
        if new {
            "Create a group"
        } else if find {
            "Search messages in this chat"
        } else {
            "Find a conversation"
        },
        ACCENT,
        true,
    );
    let input = row(inner, 2);
    fill(f, input, INCOMING);
    let (query, cursor) = app.query.visible(input.width.saturating_sub(2) as usize);
    text(
        f,
        inset(input, 1),
        if query.is_empty() {
            if new {
                "Group name…".into()
            } else if find {
                "Find text in all saved messages…".into()
            } else {
                "Search by chat or latest message…".into()
            }
        } else {
            query
        },
        TEXT,
        false,
    );
    if input.width > 2 {
        f.set_cursor_position((input.x + 1 + cursor, input.y));
    }
    if new {
        f.render_widget(Paragraph::new("Create a shared local conversation.\nOther sessions can join using the same name.\n\nEnter creates · Esc cancels").style(Style::default().fg(MUTED)),Rect::new(inner.x,inner.y+4,inner.width,inner.height.saturating_sub(4)));
        text(
            f,
            row(inner, inner.height.saturating_sub(1)),
            safe(&app.status),
            MUTED,
            false,
        );
    } else if find {
        let count = inner.height.saturating_sub(6) as usize / 2;
        let start = app.selected.saturating_sub(count.max(1) - 1);
        if app.hits.is_empty() {
            text(
                f,
                row(inner, 4),
                if app.query.text.trim().is_empty() {
                    "Type text to search this conversation's full history."
                } else {
                    "No matching messages in this conversation."
                },
                MUTED,
                false,
            );
        }
        for (i, m) in app.hits.iter().enumerate().skip(start).take(count) {
            let rect = Rect::new(
                inner.x,
                inner.y + 4 + (i - start) as u16 * 2,
                inner.width,
                2,
            );
            if i == app.selected {
                fill(f, rect, SELECTED);
            }
            text(
                f,
                row(rect, 0),
                format!("  {} · {} UTC", safe(&m.author), m.created_at),
                TEXT,
                true,
            );
            text(
                f,
                row(rect, 1),
                format!(
                    "  {}",
                    message_excerpt(
                        &if m.kind == "tool" {
                            message_preview(&m.body)
                        } else {
                            m.body.clone()
                        },
                        &app.query.text,
                        inner.width.saturating_sub(4) as usize
                    )
                ),
                MUTED,
                false,
            );
        }
        text(
            f,
            row(inner, inner.height.saturating_sub(1)),
            if app.more_hits {
                "↑ ↓ Choose · Enter Jump · PgDn More · Esc Close"
            } else {
                "↑ ↓ Choose · Enter Jump · Esc Close"
            },
            MUTED,
            false,
        );
    } else {
        let results = app.filtered_chats();
        let count = inner.height.saturating_sub(6) as usize / 2;
        let start = app.selected.saturating_sub(count.max(1) - 1);
        if results.is_empty() {
            text(
                f,
                row(inner, 4),
                "No chats match. Ctrl+N creates a group.",
                MUTED,
                false,
            );
        }
        for (i, c) in results.iter().enumerate().skip(start).take(count) {
            let rect = Rect::new(
                inner.x,
                inner.y + 4 + (i - start) as u16 * 2,
                inner.width,
                2,
            );
            if i == app.selected {
                fill(f, rect, SELECTED);
            }
            text(
                f,
                row(rect, 0),
                format!(
                    "  {}{}",
                    chat_name(&c.room),
                    if c.archived { " · archived" } else { "" }
                ),
                TEXT,
                true,
            );
            text(
                f,
                row(rect, 1),
                format!("  {}", message_preview(&c.preview)),
                MUTED,
                false,
            );
        }
        text(
            f,
            row(inner, inner.height.saturating_sub(1)),
            "↑ ↓ Choose    Enter Open    Esc Close",
            MUTED,
            false,
        );
    }
}
fn draw_approval(app: &App, f: &mut Frame) {
    let a = app.approval.as_ref().unwrap();
    let area = centered(f.area(), 92, 28);
    f.render_widget(Clear, area);
    fill(f, area, PANEL);
    let inner = area.inner(Margin::new(2, 1));
    text(
        f,
        row(inner, 0),
        if a.browser {
            "Browser · Needs approval"
        } else if a.guest.is_some() {
            "Guest shell · Needs approval"
        } else {
            "Review a shell command"
        },
        Color::Yellow,
        true,
    );
    let room = app
        .job
        .as_ref()
        .map(|j| chat_name(&j.room))
        .unwrap_or_default();
    let workdir = a
        .guest
        .as_ref()
        .map(|g| g.home.display().to_string())
        .unwrap_or_else(|| app.config.workspace.display().to_string());
    let guest_line = a
        .guest
        .as_ref()
        .map(|g| format!("Requested by guest: {}\n", safe(&g.requester)))
        .unwrap_or_default();
    let warning = if a.guest.is_some() {
        "Not a sandbox: the command still runs as your Linux user and can read files outside this folder.\nCPU and RAM caps plus the guest folder are the isolation. No time limit; Ctrl+X stops a runaway command.\nNon-interactive: no password input; sudo uses -n."
    } else {
        "Runs as your Linux user, without a sandbox.\nNon-interactive: no password input; sudo uses -n."
    };
    let body = format!(
        "{guest_line}{}\n\nWorking directory: {}\nResult shared in: {}\n\n{warning}",
        safe(&a.command),
        safe(&workdir),
        room
    );
    let content = Rect::new(
        inner.x,
        inner.y + 2,
        inner.width,
        inner.height.saturating_sub(5),
    );
    let p = Paragraph::new(body)
        .wrap(Wrap { trim: false })
        .style(Style::default().fg(TEXT));
    let max = p
        .line_count(content.width)
        .saturating_sub(content.height as usize)
        .min(u16::MAX as usize) as u16;
    f.render_widget(p.scroll((a.scroll.min(max), 0)), content);
    text(
        f,
        row(inner, inner.height.saturating_sub(2)),
        "[y] Approve once    [n / Esc] Deny",
        Color::Yellow,
        true,
    );
    text(
        f,
        row(inner, inner.height.saturating_sub(1)),
        "PgUp / PgDn reviews · Ctrl+X stops this request",
        MUTED,
        false,
    );
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}
