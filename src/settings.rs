use crate::editor::Editor;
use anyhow::{Context, Result, bail};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Constraint, Layout, Margin},
    style::{Color, Style},
    text::Line,
    widgets::{Block, Paragraph, Wrap},
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
pub fn setup(initial: Settings, path: &Path) -> Result<Option<Settings>> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("Setup needs an interactive terminal. Pass --name NAME to run without setup.");
    }
    let mut fields = [
        Editor::new(initial.name),
        Editor::new(initial.endpoint),
        Editor::new(initial.model),
        Editor::new(initial.workspace.to_string_lossy().into_owned()),
    ];
    let labels = [
        "Your local account ID",
        "Model server (OpenAI-compatible /v1 endpoint)",
        "Model name",
        "Directory for approved agent commands",
    ];
    let mut selected = 0usize;
    let mut error = String::new();
    let mut terminal = ratatui::init();
    let result = (|| -> Result<Option<Settings>> {
        loop {
            terminal.draw(|f| {
                let bg=Color::Rgb(14,22,33);let accent=Color::Rgb(88,177,232);
                f.render_widget(Block::default().style(Style::default().bg(bg).fg(Color::White)),f.area());
                let inner=f.area().inner(Margin::new(3,1));
                let rows=Layout::vertical([Constraint::Length(2),Constraint::Length(3),Constraint::Length(3),Constraint::Length(3),Constraint::Length(3),Constraint::Length(3),Constraint::Min(0)]).split(inner);
                f.render_widget(Paragraph::new(format!("Welcome to Term Social {}",env!("CARGO_PKG_VERSION"))).style(Style::default().fg(accent)),rows[0]);
                f.render_widget(Paragraph::new("Set up your local chat and personal agent. Chat works even when the model is offline. F7 edits your display name later. Account IDs are local, not verified identities.").wrap(Wrap{trim:true}),rows[1]);
                for i in 0..4 {
                    let area=rows[i+2];let width=area.width.saturating_sub(2) as usize;let(value,cursor)=fields[i].visible(width);
                    f.render_widget(Paragraph::new(vec![Line::styled(labels[i],Style::default().fg(if i==selected {accent}else {Color::Gray})),Line::raw(format!(" {value}"))]),area);
                    if i==selected && area.height>1 && area.width>1 {f.set_cursor_position((area.x+1+cursor.min(area.width-2),area.y+1));}
                }
                f.render_widget(Paragraph::new(format!("{error}\nTab next · Ctrl+U clear field · Enter save and open · Esc quit\nSettings: {}",path.display())).wrap(Wrap{trim:true}),rows[6]);
            })?;
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('c') => return Ok(None),
                    KeyCode::Char('u') => fields[selected] = Editor::default(),
                    _ => {}
                }
                continue;
            }
            match key.code {
                KeyCode::Esc => return Ok(None),
                KeyCode::Tab => selected = (selected + 1) % 4,
                KeyCode::BackTab => selected = (selected + 3) % 4,
                KeyCode::Char(c) => fields[selected].insert(&c.to_string()),
                KeyCode::Backspace => fields[selected].backspace(),
                KeyCode::Delete => fields[selected].delete(),
                KeyCode::Left => fields[selected].left(),
                KeyCode::Right => fields[selected].right(),
                KeyCode::Home => fields[selected].cursor = 0,
                KeyCode::End => fields[selected].cursor = fields[selected].text.len(),
                KeyCode::Enter => {
                    let mut settings = Settings {
                        name: fields[0].text.clone(),
                        endpoint: fields[1].text.clone(),
                        model: fields[2].text.clone(),
                        workspace: fields[3].text.clone().into(),
                    };
                    match settings.validate().and_then(|()| settings.save(path)) {
                        Ok(()) => return Ok(Some(settings)),
                        Err(e) => error = format!("{e:#}"),
                    }
                }
                _ => {}
            }
        }
    })();
    ratatui::restore();
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_roundtrip_validation_and_backup() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("term-social-settings-{}", std::process::id()));
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
}
