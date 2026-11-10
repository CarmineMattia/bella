//! Authenticated hub sync. All local writes enter a durable SQLite outbox.
use crate::{editor::Editor, profiles, store::Store};
use anyhow::{Context, Result, bail};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub url: String,
    pub server_id: String,
    pub user_id: String,
    pub username: String,
    pub token: String,
}
fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .build()?)
}
fn response(response: reqwest::blocking::Response) -> Result<Value> {
    let status = response.status();
    let mut bytes = vec![];
    response.take(8 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        bail!("Hub response is too large");
    }
    let body: Value = serde_json::from_slice(&bytes).context("Invalid hub response")?;
    if !status.is_success() {
        bail!(
            "Hub {}: {}",
            status.as_u16(),
            body["error"].as_str().unwrap_or("Request rejected")
        );
    }
    Ok(body)
}
fn valid_id(s: &str) -> bool {
    s.len() == 32 && s.bytes().all(|c| c.is_ascii_hexdigit())
}
pub fn validate_url(value: &str) -> Result<String> {
    let url = reqwest::Url::parse(value).context("Enter an http:// or https:// hub address")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        bail!("Use a hub origin such as http://192.168.1.20:8787 (no path or credentials)");
    }
    Ok(value.trim_end_matches('/').into())
}
impl Credentials {
    fn validate(&self) -> Result<()> {
        validate_url(&self.url)?;
        if !valid_id(&self.server_id)
            || !valid_id(&self.user_id)
            || self.username.is_empty()
            || self.username.len() > 32
            || !self
                .username
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
            || self.token.len() < 32
            || self.token.len() > 128
        {
            bail!("Invalid saved hub session; use --network-setup");
        }
        Ok(())
    }
    pub fn cache_path(&self, base: &Path) -> PathBuf {
        base.with_file_name(format!(
            "network-{}-{}.sqlite",
            self.server_id, self.user_id
        ))
    }
    pub fn load(path: &Path) -> Result<Option<Self>> {
        match std::fs::read(path) {
            Ok(data) => {
                let c: Self = serde_json::from_slice(&data).context("Invalid hub session file")?;
                c.validate()?;
                Ok(Some(c))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        let result = (|| -> Result<()> {
            file.write_all(&serde_json::to_vec(self)?)?;
            file.sync_all()?;
            std::fs::rename(&tmp, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(tmp);
        }
        result
    }
}
/// Login precedes chat. Passwords and registration invites are never saved.
pub fn login(path: &Path, force: bool) -> Result<Option<Credentials>> {
    use crossterm::event::{self, Event, KeyCode, KeyModifiers};
    use ratatui::{
        layout::{Margin, Rect},
        style::{Color, Style},
        widgets::{Paragraph, Wrap},
    };
    let saved = match Credentials::load(path) {
        Ok(saved) => saved,
        Err(_) if force => None,
        Err(e) => return Err(e),
    };
    if !force && saved.is_some() {
        return Ok(saved);
    }
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        bail!("Hub sign-in needs an interactive terminal");
    }
    let mut fields = [
        Editor::new(
            saved
                .as_ref()
                .map(|c| c.url.clone())
                .unwrap_or_else(|| "http://127.0.0.1:8787".into()),
        ),
        Editor::new(
            saved
                .as_ref()
                .map(|c| c.username.clone())
                .unwrap_or_default(),
        ),
        Editor::default(),
        Editor::default(),
    ];
    let mut selected = 0;
    let mut error = String::new();
    let mut terminal = ratatui::init();
    crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste)?;
    let result = (|| -> Result<Option<Credentials>> {
        loop {
            terminal.draw(|f|{
            let r=f.area().inner(Margin::new(2,1));
            f.render_widget(Paragraph::new("Connect to Term Social hub\nEnter signs in · Ctrl+R creates an account · Esc cancels").style(Style::default().fg(Color::Cyan)),Rect::new(r.x,r.y,r.width,2).intersection(r));
            for (i,title) in ["Hub URL","Account ID (lowercase letters, digits, underscores)","Password (at least 8 bytes)","Registration invite (only for a new account)"].iter().enumerate(){
                let y=r.y+3+i as u16*3;
                f.render_widget(Paragraph::new(*title).style(Style::default().fg(if i==selected{Color::Cyan}else{Color::Gray})),Rect::new(r.x,y,r.width,1).intersection(r));
                let input=Rect::new(r.x,y+1,r.width,1).intersection(r);
                let display=if i>=2{Editor::new("•".repeat(fields[i].text.chars().count()))}else{fields[i].clone()};
                let (text,cursor)=display.visible(input.width.saturating_sub(1) as usize);
                f.render_widget(Paragraph::new(text).style(Style::default().bg(Color::DarkGray)),input);
                if i==selected && input.width>0 && input.height>0{f.set_cursor_position((input.x+cursor.min(input.width-1),input.y));}
            }
            f.render_widget(Paragraph::new(format!("{error}\nHTTP sends credentials and messages in plain text. Use HTTPS or a trusted local network.\nLocal chats are kept separate. Passwords and invites are not saved.")).wrap(Wrap{trim:false}),Rect::new(r.x,r.y+16,r.width,r.height.saturating_sub(16)).intersection(r));
        })?;
            match event::read()? {
                Event::Paste(text) => fields[selected].insert(&text.replace(['\r', '\n'], "")),
                Event::Key(k) if k.kind == event::KeyEventKind::Press => {
                    if k.code == KeyCode::Esc
                        || (k.code == KeyCode::Char('c')
                            && k.modifiers.contains(KeyModifiers::CONTROL))
                    {
                        return Ok(None);
                    }
                    if k.code == KeyCode::Enter
                        || (k.code == KeyCode::Char('r')
                            && k.modifiers.contains(KeyModifiers::CONTROL))
                    {
                        let register = k.code != KeyCode::Enter;
                        let attempt = (|| -> Result<Credentials> {
                            let url = validate_url(fields[0].text.trim())?;
                            let data=response(client()?.post(format!("{url}/{}",if register{"register"}else{"login"})).json(&json!({"username":fields[1].text.trim(),"password":fields[2].text,"invite":fields[3].text})).send()?)?;
                            let mut c: Credentials = serde_json::from_value(
                                json!({"url":url,"server_id":data["server_id"],"user_id":data["user_id"],"username":data["username"],"token":data["token"]}),
                            )?;
                            c.url = validate_url(&c.url)?;
                            c.validate()?;
                            c.save(path)?;
                            Ok(c)
                        })();
                        match attempt {
                            Ok(c) => return Ok(Some(c)),
                            Err(e) => error = format!("{e:#}"),
                        };
                        continue;
                    }
                    match k.code {
                        KeyCode::Tab | KeyCode::Down => selected = (selected + 1) % 4,
                        KeyCode::BackTab | KeyCode::Up => selected = (selected + 3) % 4,
                        KeyCode::Char('u') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                            fields[selected] = Editor::default()
                        }
                        KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                            fields[selected].insert(&c.to_string())
                        }
                        KeyCode::Backspace => fields[selected].backspace(),
                        KeyCode::Delete => fields[selected].delete(),
                        KeyCode::Left => fields[selected].left(),
                        KeyCode::Right => fields[selected].right(),
                        KeyCode::Home => fields[selected].cursor = 0,
                        KeyCode::End => fields[selected].cursor = fields[selected].text.len(),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    })();
    let _ = crossterm::execute!(std::io::stdout(), crossterm::event::DisableBracketedPaste);
    ratatui::restore();
    result
}

pub fn prepare(store: &Store) -> Result<()> {
    store.connection().execute_batch(r#"
    CREATE TABLE IF NOT EXISTS network_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
    INSERT OR IGNORE INTO network_meta VALUES('applying','0');
    INSERT OR IGNORE INTO network_meta VALUES('cursor','0');
    CREATE TABLE IF NOT EXISTS network_outbox(id INTEGER PRIMARY KEY,event_id TEXT UNIQUE NOT NULL DEFAULT (lower(hex(randomblob(16)))),payload TEXT NOT NULL,local_message INTEGER,error TEXT NOT NULL DEFAULT '');
    CREATE TABLE IF NOT EXISTS network_sent(event_id TEXT PRIMARY KEY,seq INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS network_import(seq INTEGER PRIMARY KEY);
    CREATE TABLE IF NOT EXISTS network_entity_versions(key TEXT PRIMARY KEY,seq INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS network_receipts(local_message INTEGER PRIMARY KEY,seq INTEGER NOT NULL);
    CREATE TRIGGER IF NOT EXISTS network_messages AFTER INSERT ON messages WHEN (SELECT value FROM network_meta WHERE key='applying')='0' BEGIN
      INSERT INTO network_outbox(payload,local_message) VALUES(json_object('type','message','room',NEW.room,'author',NEW.author,'body',NEW.body,'kind',NEW.kind),NEW.id);
    END;
    CREATE TRIGGER IF NOT EXISTS network_rooms AFTER INSERT ON conversations WHEN (SELECT value FROM network_meta WHERE key='applying')='0' BEGIN
      INSERT INTO network_outbox(payload) VALUES(json_object('type','room','room',NEW.room));
    END;
    CREATE TRIGGER IF NOT EXISTS network_profile_insert AFTER INSERT ON profiles WHEN (SELECT value FROM network_meta WHERE key='applying')='0' BEGIN
      INSERT INTO network_outbox(payload) VALUES(json_object('type','profile','profile',json(NEW.data)));
    END;
    CREATE TRIGGER IF NOT EXISTS network_profile_update AFTER UPDATE ON profiles WHEN (SELECT value FROM network_meta WHERE key='applying')='0' BEGIN
      INSERT INTO network_outbox(payload) VALUES(json_object('type','profile','profile',json(NEW.data)));
    END;
    CREATE TRIGGER IF NOT EXISTS network_bot_add AFTER INSERT ON room_bots WHEN (SELECT value FROM network_meta WHERE key='applying')='0' BEGIN
      INSERT INTO network_outbox(payload) VALUES(json_object('type','bot_membership','owner',NEW.owner,'room',NEW.room,'handle',NEW.handle,'enabled',json('true')));
    END;
    CREATE TRIGGER IF NOT EXISTS network_bot_remove AFTER DELETE ON room_bots WHEN (SELECT value FROM network_meta WHERE key='applying')='0' BEGIN
      INSERT INTO network_outbox(payload) VALUES(json_object('type','bot_membership','owner',OLD.owner,'room',OLD.room,'handle',OLD.handle,'enabled',json('false')));
    END;
    "#)?;
    Ok(())
}
#[derive(Default, Clone)]
pub struct State {
    pub online: bool,
    pub pending: u32,
    pub rejected: u32,
    pub detail: String,
}
pub struct Connection {
    pub credentials: Credentials,
    pub state: State,
    rx: mpsc::Receiver<State>,
    stop: Arc<AtomicBool>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
impl Connection {
    pub fn start(credentials: Credentials, path: PathBuf) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let c = credentials.clone();
        std::thread::spawn(move || {
            let run = (|| -> Result<()> {
                let store = Store::open(&path)?;
                let client = client()?;
                while !flag.load(Ordering::SeqCst) {
                    let outcome = locked_cycle(&path, &store, &client, &c, &flag);
                    let pending = store
                        .connection()
                        .query_row(
                            "SELECT COUNT(*) FROM network_outbox WHERE error=''",
                            [],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let rejected = store
                        .connection()
                        .query_row(
                            "SELECT COUNT(*) FROM network_outbox WHERE error<>''",
                            [],
                            |r| r.get(0),
                        )
                        .unwrap_or(0);
                    let detail = match &outcome {
                        Ok(()) => "Connected; queued messages sync automatically".into(),
                        Err(e) => format!("{e:#}"),
                    };
                    let _ = tx.try_send(State {
                        online: outcome.is_ok(),
                        pending,
                        rejected,
                        detail,
                    });
                    for _ in 0..10 {
                        if flag.load(Ordering::SeqCst) {
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                }
                Ok(())
            })();
            if let Err(e) = run {
                let _ = tx.try_send(State {
                    detail: format!("Sync stopped: {e:#}"),
                    ..State::default()
                });
            }
        });
        Self {
            credentials,
            state: State {
                detail: "Connecting…".into(),
                ..State::default()
            },
            rx,
            stop,
        }
    }
    pub fn poll(&mut self) {
        while let Ok(state) = self.rx.try_recv() {
            self.state = state;
        }
    }
    pub fn label(&self) -> String {
        format!(
            "Hub {} · {} queued{}",
            if self.state.online {
                "online"
            } else {
                "reconnecting"
            },
            self.state.pending,
            if self.state.rejected > 0 {
                format!(" · {} rejected (F9)", self.state.rejected)
            } else {
                String::new()
            }
        )
    }
}
fn cycle(
    store: &Store,
    client: &reqwest::blocking::Client,
    c: &Credentials,
    stop: &AtomicBool,
) -> Result<()> {
    let me = response(
        client
            .get(format!("{}/me", c.url))
            .bearer_auth(&c.token)
            .send()?,
    )?;
    if me["server_id"] != c.server_id || me["user_id"] != c.user_id || me["username"] != c.username
    {
        bail!("Hub/account identity changed; use --network-setup");
    }
    let queued = {
        let mut q=store.connection().prepare("SELECT id,event_id,payload,local_message FROM network_outbox WHERE error='' ORDER BY id LIMIT 30")?;
        q.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, event_id, payload, local_message) in queued {
        if stop.load(Ordering::SeqCst) {
            return Ok(());
        }
        let mut payload: Value = serde_json::from_str(&payload)?;
        public_payload(&mut payload);
        let result = client
            .post(format!("{}/events", c.url))
            .bearer_auth(&c.token)
            .json(&json!({"event_id":event_id,"payload":payload}))
            .send()?;
        let status = result.status();
        if matches!(status.as_u16(), 400 | 403 | 409 | 413) {
            let detail = response(result).unwrap_err().to_string();
            store.connection().execute(
                "UPDATE network_outbox SET error=?2 WHERE id=?1",
                params![id, detail],
            )?;
            continue;
        }
        let ack = response(result)?;
        let seq = ack["seq"].as_i64().context("Invalid hub receipt")?;
        let tx = store.connection().unchecked_transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO network_sent VALUES(?1,?2)",
            params![event_id, seq],
        )?;
        if let Some(local) = local_message {
            tx.execute(
                "INSERT OR REPLACE INTO network_receipts VALUES(?1,?2)",
                params![local, seq],
            )?;
        }
        if let Some(key) = entity_key(&payload) {
            tx.execute("INSERT INTO network_entity_versions VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET seq=MAX(seq,excluded.seq)",params![key,seq])?;
        }
        tx.execute("DELETE FROM network_outbox WHERE id=?1", [id])?;
        tx.commit()?;
    }
    for _ in 0..20 {
        if stop.load(Ordering::SeqCst) {
            break;
        }
        let cursor: String = store.connection().query_row(
            "SELECT value FROM network_meta WHERE key='cursor'",
            [],
            |r| r.get(0),
        )?;
        let data = response(
            client
                .get(format!("{}/events?after={cursor}", c.url))
                .bearer_auth(&c.token)
                .send()?,
        )?;
        apply(store, &data, &c.user_id)?;
        if data["more"] != true {
            break;
        }
    }
    Ok(())
}
/// Device-local bot settings (instructions, model, lending opt-in) never leave this device.
fn public_payload(payload: &mut Value) {
    if payload["type"] == "profile" {
        payload["profile"]["instructions"] = json!("");
        payload["profile"]["model"] = json!("");
        if let Some(profile) = payload["profile"].as_object_mut() {
            profile.remove("lend_chat");
            profile.remove("lend_per_hour");
        }
    }
}
fn apply(store: &Store, data: &Value, user_id: &str) -> Result<()> {
    let cursor = data["cursor"]
        .as_i64()
        .filter(|v| *v >= 0)
        .context("Invalid hub cursor")?;
    let events = data["events"].as_array().context("Invalid hub events")?;
    let tx = store.connection().unchecked_transaction()?;
    tx.execute("UPDATE network_meta SET value='1' WHERE key='applying'", [])?;
    for event in events {
        let seq = event["seq"].as_i64().context("Invalid event sequence")?;
        if tx.execute("INSERT OR IGNORE INTO network_import VALUES(?1)", [seq])? == 0 {
            continue;
        }
        let event_id = event["event_id"].as_str().context("Invalid event ID")?;
        if event["account_id"] == user_id
            && tx.query_row(
                "SELECT COUNT(*) FROM network_sent WHERE event_id=?1",
                [event_id],
                |r| r.get::<_, i64>(0),
            )? > 0
        {
            continue;
        }
        let p = &event["payload"];
        let entity = entity_key(p);
        if let Some(key) = &entity {
            let version = tx
                .query_row(
                    "SELECT seq FROM network_entity_versions WHERE key=?1",
                    [key],
                    |r| r.get::<_, i64>(0),
                )
                .unwrap_or(0);
            let pending:i64=tx.query_row("SELECT COUNT(*) FROM network_outbox WHERE error='' AND CASE json_extract(payload,'$.type') WHEN 'profile' THEN json_array('profile',json_extract(payload,'$.profile.owner'),json_extract(payload,'$.profile.handle')) WHEN 'bot_membership' THEN json_array('bot_membership',json_extract(payload,'$.owner'),json_extract(payload,'$.room'),json_extract(payload,'$.handle')) END=?1",[key],|r|r.get(0))?;
            if seq <= version || pending > 0 {
                continue;
            }
        }
        let string = |key: &str| -> Result<&str> {
            p[key].as_str().with_context(|| format!("Missing {key}"))
        };
        match string("type")? {
            "room" => {
                tx.execute(
                    "INSERT OR IGNORE INTO conversations VALUES(?1)",
                    [string("room")?],
                )?;
            }
            "profile" => {
                let mut p: profiles::Profile = serde_json::from_value(p["profile"].clone())?;
                // Model instructions and the lending opt-in remain on the device. Only public
                // presentation is shared; a remote event can never enable lending.
                p.lend_chat = false;
                p.lend_per_hour = profiles::DELEGATE_PER_HOUR;
                if let Ok(old) = tx.query_row(
                    "SELECT data FROM profiles WHERE owner=?1 AND handle=?2",
                    params![p.owner, p.handle],
                    |r| r.get::<_, String>(0),
                ) {
                    let old: profiles::Profile = serde_json::from_str(&old)?;
                    p.instructions = old.instructions;
                    p.model = old.model;
                    p.lend_chat = old.lend_chat;
                    p.lend_per_hour = old.lend_per_hour;
                }
                profiles::save(&tx, &p)?;
            }
            "bot_membership" => {
                if p["enabled"] == true {
                    tx.execute(
                        "INSERT OR IGNORE INTO room_bots VALUES(?1,?2,?3)",
                        params![string("owner")?, string("room")?, string("handle")?],
                    )?;
                } else {
                    tx.execute(
                        "DELETE FROM room_bots WHERE owner=?1 AND room=?2 AND handle=?3",
                        params![string("owner")?, string("room")?, string("handle")?],
                    )?;
                }
            }
            "message" => {
                let room = string("room")?;
                tx.execute("INSERT OR IGNORE INTO conversations VALUES(?1)", [room])?;
                tx.execute(
                    "INSERT INTO messages(room,author,body,kind,created_at) VALUES(?1,?2,?3,?4,?5)",
                    params![
                        room,
                        string("author")?,
                        string("body")?,
                        string("kind")?,
                        event["created"]
                            .as_str()
                            .context("Missing event timestamp")?
                    ],
                )?;
                if event["account_id"] == user_id {
                    tx.execute(
                        "INSERT OR REPLACE INTO network_receipts VALUES(?1,?2)",
                        params![tx.last_insert_rowid(), seq],
                    )?;
                }
            }
            _ => bail!("Unsupported hub event"),
        }
        if let Some(key) = entity {
            tx.execute("INSERT INTO network_entity_versions VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET seq=MAX(seq,excluded.seq)",params![key,seq])?;
        }
    }
    tx.execute(
        "UPDATE network_meta SET value=?1 WHERE key='cursor'",
        [cursor.to_string()],
    )?;
    tx.execute("UPDATE network_meta SET value='0' WHERE key='applying'", [])?;
    tx.commit()?;
    Ok(())
}

pub fn initial_sync(store: &Store, credentials: &Credentials) -> Result<()> {
    locked_cycle(
        Path::new(
            store
                .connection()
                .path()
                .context("Cache path unavailable")?,
        ),
        store,
        &client()?,
        credentials,
        &AtomicBool::new(false),
    )
}
pub fn delivery(store: &Store, id: i64) -> Result<&'static str> {
    let rejected: i64 = store.connection().query_row(
        "SELECT COUNT(*) FROM network_outbox WHERE local_message=?1 AND error<>''",
        [id],
        |r| r.get(0),
    )?;
    if rejected > 0 {
        return Ok("rejected");
    }
    let sent: i64 = store.connection().query_row(
        "SELECT COUNT(*) FROM network_receipts WHERE local_message=?1",
        [id],
        |r| r.get(0),
    )?;
    Ok(if sent > 0 { "on server" } else { "queued" })
}

fn locked_cycle(
    path: &Path,
    store: &Store,
    client: &reqwest::blocking::Client,
    c: &Credentials,
    stop: &AtomicBool,
) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path.with_extension("sync.lock"))?;
    lock.try_lock()
        .context("Another local session is syncing this account")?;
    cycle(store, client, c, stop)
}

pub fn logout(path: &Path) -> Result<()> {
    if let Some(c) = Credentials::load(path)? {
        response(
            client()?
                .post(format!("{}/logout", c.url))
                .bearer_auth(&c.token)
                .json(&json!({}))
                .send()?,
        )?;
        std::fs::remove_file(path)?;
    }
    Ok(())
}

fn entity_key(payload: &Value) -> Option<String> {
    match payload["type"].as_str()? {
        "profile" => Some(
            json!([
                "profile",
                payload["profile"]["owner"],
                payload["profile"]["handle"]
            ])
            .to_string(),
        ),
        "bot_membership" => Some(
            json!([
                "bot_membership",
                payload["owner"],
                payload["room"],
                payload["handle"]
            ])
            .to_string(),
        ),
        _ => None,
    }
}

#[cfg(test)]
pub fn test_connection() -> Connection {
    let (_, rx) = mpsc::channel();
    Connection {
        credentials: Credentials {
            url: "http://127.0.0.1:8787".into(),
            server_id: "a".repeat(32),
            user_id: "b".repeat(32),
            username: "alice".into(),
            token: "c".repeat(40),
        },
        state: State {
            online: true,
            ..State::default()
        },
        rx,
        stop: Arc::new(AtomicBool::new(false)),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn event(seq: i64, actor: &str, event_id: &str, payload: Value) -> Value {
        json!({"seq":seq,"account_id":actor,"event_id":event_id,"payload":payload,"created":"2026-11-04 12:00:00"})
    }
    fn message(body: &str) -> Value {
        json!({"type":"message","room":"lobby","author":"alice","kind":"human","body":body})
    }
    #[test]
    fn remote_apply_is_atomic_idempotent_and_never_requeues() -> Result<()> {
        let store = Store::open(Path::new(":memory:"))?;
        prepare(&store)?;
        store.add("lobby", "alice", "local", "human")?;
        let before: i64 =
            store
                .connection()
                .query_row("SELECT COUNT(*) FROM network_outbox", [], |r| r.get(0))?;
        let page = json!({"cursor":1,"events":[event(1,"other","a",message("remote"))]});
        apply(&store, &page, "self")?;
        apply(&store, &page, "self")?;
        assert_eq!(store.history("lobby")?.len(), 2);
        assert_eq!(
            store
                .connection()
                .query_row("SELECT COUNT(*) FROM network_outbox", [], |r| r
                    .get::<_, i64>(0))?,
            before
        );
        let invalid = json!({"cursor":3,"events":[event(2,"other","b",message("must roll back")),event(3,"other","c",json!({"type":"profile","profile":{}}))]});
        assert!(apply(&store, &invalid, "self").is_err());
        assert_eq!(store.history("lobby")?.len(), 2);
        let cursor: String = store.connection().query_row(
            "SELECT value FROM network_meta WHERE key='cursor'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(cursor, "1");
        let flag: String = store.connection().query_row(
            "SELECT value FROM network_meta WHERE key='applying'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(flag, "0");
        Ok(())
    }
    #[test]
    fn echoes_are_bound_to_account_and_other_devices_have_server_receipts() -> Result<()> {
        let store = Store::open(Path::new(":memory:"))?;
        prepare(&store)?;
        store
            .connection()
            .execute("INSERT INTO network_sent VALUES('same-id',1)", [])?;
        let page = json!({"cursor":3,"events":[event(1,"self","same-id",message("echo")),event(2,"other","same-id",message("not an echo")),event(3,"self","another-device",message("my other device"))]});
        apply(&store, &page, "self")?;
        let messages = store.history("lobby")?;
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].body, "not an echo");
        assert_eq!(delivery(&store, messages[1].id)?, "on server");
        Ok(())
    }
    #[test]
    fn public_profile_sync_preserves_device_instructions_and_bot_model() -> Result<()> {
        let store = Store::open(Path::new(":memory:"))?;
        prepare(&store)?;
        let mut p = profiles::Profile::bot("alice", "planner");
        p.instructions = "Local private instructions".into();
        p.model = "local-model".into();
        p.lend_chat = true;
        profiles::save(store.connection(), &p)?;
        let mut outgoing: Value = serde_json::from_str(&store.connection().query_row(
            "SELECT payload FROM network_outbox ORDER BY id DESC LIMIT 1",
            [],
            |r| r.get::<_, String>(0),
        )?)?;
        public_payload(&mut outgoing);
        assert!(outgoing["profile"].get("lend_chat").is_none());
        assert!(outgoing["profile"].get("lend_per_hour").is_none());
        assert_eq!(outgoing["profile"]["instructions"], "");
        store
            .connection()
            .execute("DELETE FROM network_outbox", [])?;
        let mut remote = p.clone();
        remote.name = "New public name".into();
        remote.instructions = String::new();
        remote.model = String::new();
        remote.lend_chat = false;
        apply(
            &store,
            &json!({"cursor":1,"events":[event(1,"self","from-other-device",json!({"type":"profile","profile":remote}))]}),
            "self",
        )?;
        let after = profiles::all(store.connection())?.pop().unwrap();
        assert_eq!(after.name, "New public name");
        assert_eq!(after.instructions, p.instructions);
        assert_eq!(after.model, p.model);
        assert!(
            after.lend_chat,
            "Local lending opt-in was overwritten by sync"
        );
        // A remote profile cannot switch lending on for a bot on this device.
        let mut forged = profiles::Profile::bot("bob", "helper");
        forged.lend_chat = true;
        apply(
            &store,
            &json!({"cursor":2,"events":[event(2,"bob","forged-lend",json!({"type":"profile","profile":forged}))]}),
            "self",
        )?;
        let helper = profiles::all(store.connection())?
            .into_iter()
            .find(|p| p.handle == "helper")
            .unwrap();
        assert!(!helper.lend_chat);
        Ok(())
    }
    #[test]
    fn catch_up_cannot_overwrite_a_pending_or_newly_acknowledged_profile() -> Result<()> {
        let store = Store::open(Path::new(":memory:"))?;
        prepare(&store)?;
        let mut local = profiles::Profile::user("alice");
        local.name = "My newest edit".into();
        profiles::save(store.connection(), &local)?;
        let mut old = local.clone();
        old.name = "Old remote name".into();
        apply(
            &store,
            &json!({"cursor":1,"events":[event(1,"self","remote-old",json!({"type":"profile","profile":old}))]}),
            "self",
        )?;
        assert_eq!(profiles::all(store.connection())?[0].name, local.name);
        store
            .connection()
            .execute("DELETE FROM network_outbox", [])?;
        let key = entity_key(&json!({"type":"profile","profile":local})).unwrap();
        store
            .connection()
            .execute("INSERT INTO network_entity_versions VALUES(?1,5)", [key])?;
        apply(
            &store,
            &json!({"cursor":2,"events":[event(2,"self","older-than-receipt",json!({"type":"profile","profile":old}))]}),
            "self",
        )?;
        assert_eq!(profiles::all(store.connection())?[0].name, local.name);
        old.name = "Later edit on another device".into();
        apply(
            &store,
            &json!({"cursor":6,"events":[event(6,"self","later",json!({"type":"profile","profile":old}))]}),
            "self",
        )?;
        assert_eq!(
            profiles::all(store.connection())?[0].name,
            "Later edit on another device"
        );
        Ok(())
    }
    #[test]
    fn hub_origins_cannot_embed_credentials_paths_or_queries() {
        assert!(validate_url("https://example.com").is_ok());
        for bad in [
            "ftp://host",
            "http://user:secret@host",
            "https://host/path",
            "https://host?token=secret",
            "https://host#fragment",
        ] {
            assert!(validate_url(bad).is_err(), "{bad}");
        }
    }
}
