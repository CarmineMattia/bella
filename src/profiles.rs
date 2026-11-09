//! Local identities keep stable handles; presentation and pictures are editable.
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub owner: String,
    /// Empty for the human, `bot` for the original personal agent.
    pub handle: String,
    pub name: String,
    pub about: String,
    pub instructions: String,
    pub model: String,
    pub avatar: Vec<[u8; 3]>,
    /// Owner opt-in: other group members may mention this bot for chat-only answers.
    /// Device-local like instructions; never synchronised or accepted from the hub.
    #[serde(default)]
    pub lend_chat: bool,
}
/// Delegated (guest) requests allowed per requester, per bot, in a rolling hour.
pub const DELEGATE_PER_HOUR: i64 = 5;
impl Profile {
    pub fn user(owner: &str) -> Self {
        Self {
            owner: owner.into(),
            handle: String::new(),
            name: owner.into(),
            about: String::new(),
            instructions: String::new(),
            model: String::new(),
            avatar: vec![],
            lend_chat: false,
        }
    }
    pub fn bot(owner: &str, handle: &str) -> Self {
        Self {
            handle: handle.into(),
            name: if handle == "bot" {
                format!("{owner}'s bot").chars().take(32).collect()
            } else {
                handle.into()
            },
            ..Self::user(owner)
        }
    }
    pub fn author(&self) -> String {
        match self.handle.as_str() {
            "" => self.owner.clone(),
            "bot" => format!("{}'s bot", self.owner),
            _ => format!("{}'s @{}", self.owner, self.handle),
        }
    }
    pub fn validate(&self) -> Result<()> {
        if !crate::settings::valid_name(&self.name) {
            bail!("Display name must be 1–32 printable characters");
        }
        if !self.handle.is_empty() && !valid_handle(&self.handle) {
            bail!("Handle: 1–24 lowercase letters, digits or underscores; start with a letter");
        }
        if self.about.chars().count() > 160 || self.about.chars().any(char::is_control) {
            bail!("About: at most 160 printable characters");
        }
        if self.instructions.chars().count() > 2000
            || self.instructions.chars().any(char::is_control)
        {
            bail!("Instructions: at most 2000 printable characters");
        }
        if self.model.chars().count() > 200 || self.model.chars().any(char::is_control) {
            bail!("Model: at most 200 printable characters");
        }
        if !self.avatar.is_empty() && self.avatar.len() != 24 * 24 {
            bail!("Invalid picture data");
        }
        Ok(())
    }
}
pub fn valid_handle(s: &str) -> bool {
    (1..=24).contains(&s.len())
        && s.as_bytes()[0].is_ascii_lowercase()
        && s.bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
}
pub fn schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS profiles(owner TEXT NOT NULL,handle TEXT NOT NULL,data TEXT NOT NULL,PRIMARY KEY(owner,handle));
    CREATE TABLE IF NOT EXISTS room_bots(owner TEXT NOT NULL,room TEXT NOT NULL,handle TEXT NOT NULL,PRIMARY KEY(owner,room,handle));
    CREATE TABLE IF NOT EXISTS task_bots(task_id INTEGER PRIMARY KEY,author TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS delegate_requests(message_id INTEGER PRIMARY KEY,owner TEXT NOT NULL,handle TEXT NOT NULL,requester TEXT NOT NULL,room TEXT NOT NULL,outcome TEXT NOT NULL,at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
    CREATE INDEX IF NOT EXISTS delegate_requests_quota ON delegate_requests(owner,handle,requester,at);")?;
    Ok(())
}
pub fn save(db: &Connection, p: &Profile) -> Result<()> {
    p.validate()?;
    db.execute("INSERT INTO profiles VALUES(?1,?2,?3) ON CONFLICT(owner,handle) DO UPDATE SET data=excluded.data", params![p.owner,p.handle,serde_json::to_string(p)?])?;
    Ok(())
}
pub fn all(db: &Connection) -> Result<Vec<Profile>> {
    let mut q = db.prepare("SELECT data FROM profiles ORDER BY owner,handle")?;
    let data = q
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    data.into_iter()
        .map(|s| {
            let p: Profile = serde_json::from_str(&s)?;
            p.validate()?;
            Ok(p)
        })
        .collect()
}
pub fn enabled(db: &Connection, owner: &str, room: &str) -> Result<Vec<String>> {
    let mut q =
        db.prepare("SELECT handle FROM room_bots WHERE owner=?1 AND room=?2 ORDER BY handle")?;
    Ok(q.query_map(params![owner, room], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?)
}
pub fn toggle(db: &Connection, owner: &str, room: &str, handle: &str) -> Result<()> {
    if db.execute(
        "DELETE FROM room_bots WHERE owner=?1 AND room=?2 AND handle=?3",
        params![owner, room, handle],
    )? == 0
    {
        db.execute(
            "INSERT INTO room_bots VALUES(?1,?2,?3)",
            params![owner, room, handle],
        )?;
    }
    Ok(())
}
pub fn index(profiles: &[Profile]) -> HashMap<(String, bool), Profile> {
    profiles
        .iter()
        .map(|p| ((p.author(), !p.handle.is_empty()), p.clone()))
        .collect()
}
/// Import a bounded thumbnail into SQLite, so backups don't depend on the source file.
pub fn import_avatar(path: &str) -> Result<Vec<[u8; 3]>> {
    let path = Path::new(path);
    let file = std::fs::File::open(path).context("Cannot open picture")?;
    if file.metadata()?.len() > 8 * 1024 * 1024 {
        bail!("Picture must be smaller than 8 MiB");
    }
    let mut reader =
        image::ImageReader::new(std::io::BufReader::new(file)).with_guessed_format()?;
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        bail!("Choose a PNG or JPEG picture");
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let img = reader
        .decode()
        .context("Cannot decode picture (maximum 4096 × 4096)")?
        .resize_to_fill(24, 24, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    Ok(img
        .pixels()
        .map(|p| {
            let a = p[3] as u32;
            [0, 1, 2].map(|i| ((p[i] as u32 * a + [23, 33, 43][i] * (255 - a)) / 255) as u8)
        })
        .collect())
}

pub struct Form {
    pub original: Profile,
    pub creating: bool,
    pub fields: Vec<(&'static str, crate::editor::Editor)>,
    pub selected: usize,
    pub error: String,
}
impl Form {
    pub fn new(p: Profile, creating: bool) -> Self {
        let fields = if p.handle.is_empty() {
            vec![
                ("Display name", p.name.clone()),
                ("About", p.about.clone()),
                ("Picture: PNG/JPEG path", String::new()),
            ]
        } else {
            vec![
                (
                    "Handle (fixed after creation)",
                    if creating {
                        String::new()
                    } else {
                        p.handle.clone()
                    },
                ),
                ("Display name", p.name.clone()),
                ("About", p.about.clone()),
                ("Instructions / personality", p.instructions.clone()),
                ("Model (blank inherits default)", p.model.clone()),
                ("Picture: PNG/JPEG path", String::new()),
                (
                    "Lend for chat to group members: yes/no (guests never get tools)",
                    if p.lend_chat { "yes" } else { "no" }.into(),
                ),
            ]
        };
        Self {
            original: p,
            creating,
            fields: fields
                .into_iter()
                .map(|(l, v)| (l, crate::editor::Editor::new(v)))
                .collect(),
            selected: 0,
            error: String::new(),
        }
    }
    pub fn build(&self) -> Result<Profile> {
        let mut p = self.original.clone();
        let value = |i: usize| self.fields[i].1.text.trim().to_owned();
        let image = if p.handle.is_empty() {
            p.name = value(0);
            p.about = value(1);
            value(2)
        } else {
            if self.creating {
                p.handle = value(0);
                if !valid_handle(&p.handle) {
                    bail!(
                        "Handle: 1–24 lowercase letters, digits or underscores; start with a letter"
                    );
                }
            }
            p.name = value(1);
            p.about = value(2);
            p.instructions = value(3);
            p.model = value(4);
            p.lend_chat = match value(6).to_lowercase().as_str() {
                "yes" | "y" => true,
                "no" | "n" | "" => false,
                _ => bail!("Lend for chat: type yes or no"),
            };
            value(5)
        };
        p.validate()?;
        if image == "-" {
            p.avatar.clear();
        } else if !image.is_empty() {
            p.avatar = import_avatar(&image)?;
        }
        p.validate()?;
        Ok(p)
    }
}
