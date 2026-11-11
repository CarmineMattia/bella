use crate::editor::Editor;
use anyhow::{Context, Result, bail};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
};
use ratatui::{
    layout::{Margin, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
};
use serde::{Deserialize, Serialize};
use std::{
    io::{IsTerminal, Write},
    path::{Path, PathBuf},
    time::Duration,
};

pub const DEFAULT_ENDPOINT: &str = "http://127.0.0.1:8080/v1";
pub const DEFAULT_MODEL: &str = "Qwen3.6-35B-A3B-UD-Q4_K_XL";
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub name: String,
    pub endpoint: String,
    pub model: String,
    pub workspace: PathBuf,
    /// Skill names switched off for this device (see docs/SKILLS.md). Optional in
    /// settings.json so older files load unchanged; omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_skills: Vec<String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            name: std::env::var("USER")
                .ok()
                .filter(|s| valid_name(s))
                .unwrap_or("tester".into()),
            endpoint: DEFAULT_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            workspace: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            disabled_skills: Vec::new(),
        }
    }
}
pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.chars().count() <= 32 && !name.chars().any(char::is_control)
}
impl Settings {
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(Some(serde_json::from_slice(&bytes).with_context(|| {
                format!(
                    "Invalid settings in {}. Use --setup to reconfigure.",
                    path.display()
                )
            })?)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("Cannot read {}", path.display())),
        }
    }
    pub fn validate(&mut self) -> Result<()> {
        self.name = self.name.trim().to_owned();
        self.model = self.model.trim().to_owned();
        self.endpoint = self.endpoint.trim().trim_end_matches('/').to_owned();
        if !valid_name(&self.name) {
            bail!("Choose a name of 1–32 printable characters");
        }
        if self.model.is_empty() || self.model.chars().any(char::is_control) {
            bail!("Enter a model name");
        }
        let url = reqwest::Url::parse(&self.endpoint)
            .context("Enter a complete HTTP model endpoint, including /v1")?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            bail!("Model endpoint must be an HTTP(S) address without query or fragment");
        }
        self.workspace = self
            .workspace
            .canonicalize()
            .context("Agent workspace must be an existing directory")?;
        if !self.workspace.is_dir() {
            bail!("Agent workspace must be a directory");
        }
        Ok(())
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let temp = path.with_extension(format!("json.tmp-{}", std::process::id()));
        use std::os::unix::fs::OpenOptionsExt;
        let result = (|| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temp)?;
            let mut bytes = serde_json::to_vec_pretty(self)?;
            bytes.push(b'\n');
            file.write_all(&bytes)?;
            file.sync_all()?;
            if path.exists() {
                std::fs::copy(path, path.with_extension("json.previous"))?;
            }
            std::fs::rename(&temp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temp);
        }
        result.with_context(|| format!("Cannot save settings to {}", path.display()))
    }
}
const BG: Color = Color::Rgb(14, 22, 33);
const PANEL: Color = Color::Rgb(22, 35, 48);
const INPUT: Color = Color::Rgb(30, 46, 62);
const ACCENT: Color = Color::Rgb(88, 177, 232);
const TEXT: Color = Color::Rgb(227, 236, 244);
const MUTED: Color = Color::Rgb(132, 157, 176);
const LABELS: [&str; 4] = [
    "Account name",
    "Model server",
    "Model name",
    "Agent workspace",
];
const HINTS: [&str; 4] = [
    "Identifies your local account. Changing it opens a different local profile.",
    "Use an HTTP model server address ending in /v1. Chat also works offline.",
    "Use the model name served by your connection. Keep the default for the configured local model.",
    "Choose an existing folder where approved agent commands will run.",
];
struct SetupForm {
    fields: [Editor; 4],
    /// Not editable in setup; carried through so re-running setup keeps it.
    disabled_skills: Vec<String>,
    selected: usize,
    error: String,
}
#[derive(Default)]
struct SetupHits {
    fields: [Rect; 4],
    open: Rect,
    exit: Rect,
}
#[derive(PartialEq, Debug)]
enum SetupAction {
    Stay,
    Save,
    Exit,
}
impl SetupForm {
    fn new(initial: Settings) -> Self {
        Self {
            fields: [
                Editor::new(initial.name),
                Editor::new(initial.endpoint),
                Editor::new(initial.model),
                Editor::new(initial.workspace.to_string_lossy().into_owned()),
            ],
            disabled_skills: initial.disabled_skills,
            selected: 0,
            error: String::new(),
        }
    }
    fn event(&mut self, event: Event, hits: &SetupHits) -> SetupAction {
        match event {
            Event::Paste(text) => {
                self.fields[self.selected].insert(
                    &text
                        .trim_end_matches(['\r', '\n'])
                        .replace(['\r', '\n'], " "),
                );
                self.error.clear();
            }
            Event::Mouse(mouse) => match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    let point = (mouse.column, mouse.row).into();
                    if !hits.open.is_empty() && hits.open.contains(point) {
                        return SetupAction::Save;
                    }
                    if !hits.exit.is_empty() && hits.exit.contains(point) {
                        return SetupAction::Exit;
                    }
                    if let Some(index) = hits
                        .fields
                        .iter()
                        .position(|rect| !rect.is_empty() && rect.contains(point))
                    {
                        self.selected = index;
                    }
                }
                MouseEventKind::ScrollDown => self.selected = (self.selected + 1).min(3),
                MouseEventKind::ScrollUp => self.selected = self.selected.saturating_sub(1),
                _ => {}
            },
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match key.code {
                        KeyCode::Char('c') => return SetupAction::Exit,
                        KeyCode::Char('u') => {
                            self.fields[self.selected] = Editor::default();
                            self.error.clear();
                        }
                        _ => {}
                    }
                    return SetupAction::Stay;
                }
                let field = &mut self.fields[self.selected];
                match key.code {
                    KeyCode::Esc => return SetupAction::Exit,
                    KeyCode::Enter => return SetupAction::Save,
                    KeyCode::Tab | KeyCode::Down => self.selected = (self.selected + 1) % 4,
                    KeyCode::BackTab | KeyCode::Up => self.selected = (self.selected + 3) % 4,
                    KeyCode::Char(c) => {
                        field.insert(&c.to_string());
                        self.error.clear();
                    }
                    KeyCode::Backspace => {
                        field.backspace();
                        self.error.clear();
                    }
                    KeyCode::Delete => {
                        field.delete();
                        self.error.clear();
                    }
                    KeyCode::Left => field.left(),
                    KeyCode::Right => field.right(),
                    KeyCode::Home => field.cursor = 0,
                    KeyCode::End => field.cursor = field.text.len(),
                    _ => {}
                }
            }
            _ => {}
        }
        SetupAction::Stay
    }
    fn save(&mut self, path: &Path) -> Option<Settings> {
        let mut settings = Settings {
            name: self.fields[0].text.clone(),
            endpoint: self.fields[1].text.clone(),
            model: self.fields[2].text.clone(),
            workspace: self.fields[3].text.clone().into(),
            disabled_skills: self.disabled_skills.clone(),
        };
        match settings.validate().and_then(|()| settings.save(path)) {
            Ok(()) => Some(settings),
            Err(error) => {
                self.selected = if !valid_name(self.fields[0].text.trim()) {
                    0
                } else if self.fields[2].text.trim().is_empty() {
                    2
                } else if reqwest::Url::parse(self.fields[1].text.trim()).map_or(true, |url| {
                    !matches!(url.scheme(), "http" | "https")
                        || url.host_str().is_none()
                        || url.query().is_some()
                        || url.fragment().is_some()
                }) {
                    1
                } else {
                    self.selected
                };
                self.error = format!("{error:#}");
                None
            }
        }
    }
}
fn setup_text(frame: &mut ratatui::Frame, rect: Rect, text: &str, color: Color, bold: bool) {
    frame.render_widget(
        Paragraph::new(text).style(Style::default().fg(color).add_modifier(if bold {
            Modifier::BOLD
        } else {
            Modifier::empty()
        })),
        rect,
    );
}
fn draw_setup(frame: &mut ratatui::Frame, form: &SetupForm) -> SetupHits {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(BG).fg(TEXT)),
        area,
    );
    let mut hits = SetupHits::default();
    if area.width < 22 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("Bella\nResize to continue\nEsc exits")
                .style(Style::default().fg(ACCENT)),
            area,
        );
        return hits;
    }
    let compact = area.height < 24 || area.width < 48;
    let width = area.width.min(108);
    let height = area.height.min(30);
    let card = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Block::default().style(Style::default().bg(PANEL)), card);
    let inner = card.inner(if compact {
        Margin::new(1, 0)
    } else {
        Margin::new(3, 1)
    });
    let wide = inner.width >= 96 && inner.height >= 24;
    let form_area = if wide {
        let hero = Rect::new(inner.x, inner.y, 28, inner.height);
        setup_text(
            frame,
            Rect::new(hero.x, hero.y + 1, hero.width, 1),
            "B E L L A",
            ACCENT,
            true,
        );
        setup_text(
            frame,
            Rect::new(hero.x, hero.y + 5, hero.width, 2),
            "Your conversations.\nYour agents.",
            TEXT,
            true,
        );
        frame.render_widget(
            Paragraph::new("A shared space for people and their personal agents.")
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(MUTED)),
            Rect::new(hero.x, hero.y + 9, hero.width, 4),
        );
        frame.render_widget(
            Paragraph::new(
                "01  Make it yours\n\n02  Connect your model\n\n03  Start a conversation",
            )
            .style(Style::default().fg(MUTED)),
            Rect::new(hero.x, hero.y + 16, hero.width, 7),
        );
        Rect::new(inner.x + 32, inner.y, inner.width - 32, inner.height)
    } else {
        inner
    };
    let header = if compact { 1 } else { 3 };
    let footer = if compact { 3 } else { 4 };
    setup_text(
        frame,
        Rect::new(form_area.x, form_area.y, form_area.width, 1),
        if form_area.width >= 30 {
            "Welcome to Bella"
        } else {
            "Bella setup"
        },
        TEXT,
        true,
    );
    if !compact {
        setup_text(
            frame,
            Rect::new(form_area.x, form_area.y + 1, form_area.width, 1),
            "A few details, then you're in.",
            MUTED,
            false,
        );
    }
    let fields_area = Rect::new(
        form_area.x,
        form_area.y + header,
        form_area.width,
        form_area.height.saturating_sub(header + footer),
    );
    let step = if fields_area.height >= 16 { 4 } else { 3 };
    let count = (fields_area.height / step).clamp(1, 4) as usize;
    let first = form.selected.saturating_sub(count - 1).min(4 - count);
    for (index, label) in LABELS.iter().enumerate().skip(first).take(count) {
        let y = fields_area.y + (index - first) as u16 * step;
        let rect = Rect::new(fields_area.x, y, fields_area.width, 3).intersection(fields_area);
        if rect.height < 2 {
            continue;
        }
        hits.fields[index] = rect;
        let focused = index == form.selected;
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .title(format!(" {}  {} ", index + 1, label))
            .border_style(Style::default().fg(if focused { ACCENT } else { MUTED }))
            .style(
                Style::default()
                    .bg(if focused { INPUT } else { PANEL })
                    .fg(TEXT),
            );
        let inside = block.inner(rect);
        let (value, cursor) = form.fields[index].visible(inside.width as usize);
        frame.render_widget(Paragraph::new(value).block(block), rect);
        if focused && inside.width > 0 && inside.height > 0 {
            frame.set_cursor_position((inside.x + cursor.min(inside.width - 1), inside.y));
        }
    }
    let hint_height = if compact { 1 } else { 2 };
    let hint = Rect::new(
        form_area.x,
        form_area.bottom().saturating_sub(footer),
        form_area.width,
        hint_height,
    );
    frame.render_widget(
        Paragraph::new(if form.error.is_empty() {
            HINTS[form.selected]
        } else {
            &form.error
        })
        .wrap(Wrap { trim: true })
        .style(Style::default().fg(if form.error.is_empty() {
            MUTED
        } else {
            Color::Rgb(245, 153, 153)
        })),
        hint,
    );
    let buttons_y = form_area.bottom().saturating_sub(2);
    hits.open = Rect::new(
        form_area.x,
        buttons_y,
        18.min(form_area.width.saturating_sub(9)),
        1,
    );
    hits.exit = Rect::new(
        form_area.right().saturating_sub(8),
        buttons_y,
        8.min(form_area.width),
        1,
    );
    frame.render_widget(
        Paragraph::new("  Open chat  →  ").style(
            Style::default()
                .bg(ACCENT)
                .fg(BG)
                .add_modifier(Modifier::BOLD),
        ),
        hits.open,
    );
    setup_text(frame, hits.exit, "  Exit  ", MUTED, false);
    setup_text(
        frame,
        Rect::new(
            form_area.x,
            form_area.bottom().saturating_sub(1),
            form_area.width,
            1,
        ),
        if form_area.width >= 50 {
            "Tab Next   Shift+Tab Back   Enter Open   Esc Exit"
        } else {
            "Tab Next · Enter Open · Esc Exit"
        },
        MUTED,
        false,
    );
    hits
}
pub fn setup(initial: Settings, path: &Path) -> Result<Option<Settings>> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("Setup needs an interactive terminal. Pass --name NAME to run without setup.");
    }
    let mut form = SetupForm::new(initial);
    let mut terminal = ratatui::init();
    let result = (|| -> Result<Option<Settings>> {
        execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
        loop {
            let mut hits = SetupHits::default();
            terminal.draw(|frame| {
                hits = draw_setup(frame, &form);
            })?;
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            match form.event(event::read()?, &hits) {
                SetupAction::Exit => return Ok(None),
                SetupAction::Save => {
                    if let Some(settings) = form.save(path) {
                        return Ok(Some(settings));
                    }
                }
                SetupAction::Stay => {}
            }
        }
    })();
    let _ = execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
    ratatui::restore();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Settings {
        Settings {
            name: "carmine".into(),
            endpoint: DEFAULT_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            workspace: std::env::temp_dir(),
            disabled_skills: Vec::new(),
        }
    }
    #[test]
    fn setup_keeps_focus_visible_and_button_targets_separate_at_all_sizes() -> Result<()> {
        use ratatui::{
            Terminal,
            backend::{Backend, TestBackend},
        };
        for (width, height) in [
            (128, 40),
            (100, 30),
            (80, 24),
            (50, 16),
            (30, 10),
            (22, 8),
            (10, 4),
            (1, 1),
        ] {
            for selected in 0..4 {
                let mut form = SetupForm::new(fixture());
                form.selected = selected;
                let mut terminal = Terminal::new(TestBackend::new(width, height))?;
                let mut hits = SetupHits::default();
                terminal.draw(|frame| {
                    hits = draw_setup(frame, &form);
                })?;
                if width >= 22 && height >= 8 {
                    assert!(
                        !hits.fields[selected].is_empty(),
                        "Focused field missing at {width}x{height}"
                    );
                    assert!(
                        hits.open.intersection(hits.exit).is_empty(),
                        "Save and Exit overlap"
                    );
                    let cursor = terminal.backend_mut().get_cursor_position()?;
                    assert!(cursor.x < width && cursor.y < height);
                }
            }
        }
        Ok(())
    }
    #[test]
    fn setup_mouse_paste_navigation_and_validation_preserve_user_input() -> Result<()> {
        use crossterm::event::{KeyEvent, MouseEvent};
        use ratatui::{Terminal, backend::TestBackend};
        let mut form = SetupForm::new(fixture());
        let mut terminal = Terminal::new(TestBackend::new(100, 30))?;
        let mut hits = SetupHits::default();
        terminal.draw(|frame| {
            hits = draw_setup(frame, &form);
        })?;
        let rect = hits.fields[1];
        form.event(
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x + 1,
                row: rect.y + 1,
                modifiers: KeyModifiers::NONE,
            }),
            &hits,
        );
        assert_eq!(form.selected, 1);
        form.event(
            Event::Key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            &hits,
        );
        form.event(Event::Paste("http://127.0.0.1:9000/v1\r\n".into()), &hits);
        assert_eq!(form.fields[1].text, "http://127.0.0.1:9000/v1");
        form.event(
            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            &hits,
        );
        assert_eq!(form.selected, 2);
        let root =
            std::env::temp_dir().join(format!("bella-welcome-validation-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let path = root.join("settings.json");
        form.fields[0] = Editor::default();
        assert!(form.save(&path).is_none());
        assert_eq!(form.selected, 0);
        assert!(!path.exists());
        assert_eq!(form.fields[1].text, "http://127.0.0.1:9000/v1");
        form.event(Event::Paste("Alice 界".into()), &hits);
        assert!(form.error.is_empty());
        let open = hits.open;
        assert_eq!(
            form.event(
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: open.x,
                    row: open.y,
                    modifiers: KeyModifiers::NONE
                }),
                &hits
            ),
            SetupAction::Save
        );
        assert!(form.save(&path).is_some());
        assert_eq!(Settings::load(&path)?.unwrap().name, "Alice 界");
        assert_eq!(
            form.event(
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                &hits
            ),
            SetupAction::Exit
        );
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    #[ignore = "writes a welcome screen preview only when BELLA_PREVIEW is provided"]
    fn welcome_preview() -> Result<()> {
        use ratatui::{Terminal, backend::TestBackend};
        let path = crate::paths::env_var("BELLA_PREVIEW", "TERM_SOCIAL_PREVIEW")
            .ok_or_else(|| anyhow::anyhow!("BELLA_PREVIEW or TERM_SOCIAL_PREVIEW not set"))?;
        let form = SetupForm::new(fixture());
        let mut terminal = Terminal::new(TestBackend::new(128, 40))?;
        terminal.draw(|frame| {
            draw_setup(frame, &form);
        })?;
        let buffer = terminal.backend().buffer();
        let mut cells = vec![];
        for y in 0..40 {
            for x in 0..128 {
                let cell = &buffer[(x, y)];
                cells.push(serde_json::json!({"x":x,"y":y,"s":cell.symbol(),"fg":format!("{:?}",cell.fg),"bg":format!("{:?}",cell.bg),"bold":cell.modifier.contains(Modifier::BOLD)}));
            }
        }
        std::fs::write(path, serde_json::to_vec(&cells)?)?;
        Ok(())
    }
    #[test]
    fn settings_roundtrip_validation_and_backup() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("bella-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("settings.json");
        let mut s = Settings {
            name: " alice ".into(),
            workspace: dir.clone(),
            ..Settings::default()
        };
        s.validate()?;
        s.save(&path)?;
        assert_eq!(Settings::load(&path)?.unwrap().name, "alice");
        s.name = "bob".into();
        s.save(&path)?;
        assert_eq!(
            Settings::load(&dir.join("settings.json.previous"))?
                .unwrap()
                .name,
            "alice"
        );
        assert!(valid_name(&"界".repeat(32)));
        assert!(!valid_name(&"界".repeat(33)));
        s.endpoint = "file:///tmp/model".into();
        assert!(s.validate().is_err());
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn disabled_skills_are_optional_and_survive_setup() -> Result<()> {
        let dir =
            std::env::temp_dir().join(format!("bella-settings-skills-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("settings.json");
        // A settings.json written before skills existed still loads.
        std::fs::write(
            &path,
            serde_json::json!({"name":"alice","endpoint":DEFAULT_ENDPOINT,"model":"m","workspace":dir}).to_string(),
        )?;
        let old = Settings::load(&path)?.unwrap();
        assert!(old.disabled_skills.is_empty());
        // Empty lists are not written, so older binaries can still read the file.
        old.save(&path)?;
        assert!(!std::fs::read_to_string(&path)?.contains("disabled_skills"));
        let mut s = old;
        s.disabled_skills = vec!["browser-check".into()];
        s.save(&path)?;
        assert_eq!(
            Settings::load(&path)?.unwrap().disabled_skills,
            ["browser-check"]
        );
        let mut form = SetupForm::new(s);
        assert_eq!(form.save(&path).unwrap().disabled_skills, ["browser-check"]);
        assert_eq!(
            Settings::load(&path)?.unwrap().disabled_skills,
            ["browser-check"]
        );
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
}
