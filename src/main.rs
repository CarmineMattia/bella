mod agent;
mod browser;
mod clipboard;
mod editor;
mod network;
mod paths;
mod profile_ui;
mod profiles;
mod recipient;
mod settings;
mod store;
mod tasks;
mod ui;

use anyhow::{Result, bail};
use bella::skills;
use clap::{CommandFactory, FromArgMatches, Parser};
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
    },
    execute,
};
use editor::Editor;
use std::{
    io::IsTerminal,
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
use store::{Conversation, Message, Store};

/// Characters of skill metadata in the owner's system prompt. Conservative so a
/// small local model keeps most of its context for the conversation.
const SKILLS_PROMPT_BUDGET: usize = 2500;

#[derive(Parser)]
#[command(
    version,
    about = "A terminal messenger for people and their personal agents."
)]
struct Args {
    /// Revoke this saved hub session and remove its token (keeps cached chats).
    #[arg(long,conflicts_with_all=["network","network_setup","send","history","doctor","backup","setup_only"])]
    network_logout: bool,
    /// Use the shared hub, with an isolated account cache and saved session.
    #[arg(long,conflicts_with_all=["send","history","doctor","backup","setup_only"])]
    network: bool,
    /// Reopen hub sign-in / registration (requires --network).
    #[arg(long, requires = "network")]
    network_setup: bool,
    /// Local account ID; edit its display name with F7 inside chat.
    #[arg(long)]
    name: Option<String>,
    #[arg(long, default_value = "lobby")]
    room: String,
    /// SQLite database path [env: BELLA_DATA or TERM_SOCIAL_DATA]
    #[arg(long, default_value = ".data/chat.sqlite")]
    data: PathBuf,
    #[arg(long)]
    endpoint: Option<String>,
    #[arg(long)]
    model: Option<String>,
    /// Agent workspace [env: BELLA_WORKSPACE or TERM_SOCIAL_WORKSPACE]
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Saved profile settings; CLI options override this file [env: BELLA_CONFIG or TERM_SOCIAL_CONFIG]
    #[arg(long)]
    config: Option<PathBuf>,
    /// Edit the saved profile before opening chat.
    #[arg(long)]
    setup: bool,
    /// Save profile settings and exit (used by the launcher).
    #[arg(long)]
    setup_only: bool,
    /// Print diagnostic JSON; exits 1 if the model or chat dependencies are unavailable.
    #[arg(long,conflicts_with_all=["send","history","backup","setup_only"])]
    doctor: bool,
    /// Write a consistent SQLite backup to a new file, then exit.
    #[arg(long,conflicts_with_all=["send","history","doctor","setup_only"])]
    backup: Option<PathBuf>,
    /// Post a message without opening the terminal interface.
    #[arg(long)]
    send: Option<String>,
    /// Print the selected conversation without opening the terminal interface.
    #[arg(long)]
    history: bool,
}
struct Approval {
    browser: bool,
    command: String,
    answer: mpsc::Sender<bool>,
    scroll: u16,
    guest: Option<agent::GuestContext>,
}
struct Job {
    author: String,
    model: String,
    task_id: Option<i64>,
    saved_partial: String,
    control: agent::Control,
    phase: agent::Phase,
    stopping: bool,
    ran_command: bool,
    partial: String,
    room: String,
    events: mpsc::Receiver<agent::Event>,
    /// Set for a delegated (guest) request: the account that mentioned the bot.
    requester: Option<String>,
    /// Skill selected with `$name` for this owner request, shown while it runs.
    skill: Option<String>,
}
#[derive(PartialEq)]
enum Dialog {
    Search,
    FindMessages,
    NewGroup,
    Help,
    ModelInfo,
    ChatActions,
    Tasks,
    TaskDetails,
    BrowserDetails,
    Participants,
    Network,
    Profile,
    Bots,
    BotProfile,
    Skills,
}
struct App {
    recipient: Option<String>,
    network: Option<network::Connection>,
    profiles: Vec<profiles::Profile>,
    profile_index: std::collections::HashMap<(String, bool), profiles::Profile>,
    enabled_bots: Vec<String>,
    profile_form: Option<profiles::Form>,
    name: String,
    room: String,
    input: Editor,
    chats: Vec<Conversation>,
    view_archived: bool,
    sidebar_offset: Option<usize>,
    action_room: Option<String>,
    messages: Vec<Message>,
    status: String,
    scroll: usize,
    max_scroll: usize,
    anchor: Option<(i64, isize)>,
    jump_to: Option<i64>,
    highlight: Option<i64>,
    following: bool,
    has_older: bool,
    has_newer: bool,
    hits: Vec<Message>,
    more_hits: bool,
    approval: Option<Approval>,
    job: Option<Job>,
    store: Store,
    config: agent::Config,
    /// Directory containing the database; guest folders are created under `guests/`.
    data_dir: PathBuf,
    skills: skills::Catalog,
    skill_roots: Vec<skills::Root>,
    disabled_skills: Vec<String>,
    /// settings.json used to persist `/skills disable|enable`; None in tests.
    settings_path: Option<PathBuf>,
    skills_error: String,
    dialog: Option<Dialog>,
    query: Editor,
    selected: usize,
    dialog_scroll: u16,
    health: Option<bool>,
    health_detail: String,
    health_rx: Option<mpsc::Receiver<Result<Vec<String>, String>>>,
    health_checked: Option<Instant>,
    failed_room: Option<String>,
    session: String,
    task_records: Vec<tasks::TaskRecord>,
    task_detail: Option<tasks::TaskRecord>,
    browser_detail_id: Option<i64>,
    browser_links: Vec<(ratatui::layout::Rect, i64)>,
    copy_links: Vec<(ratatui::layout::Rect, i64)>,
    copy_job: Option<(i64, mpsc::Receiver<Result<clipboard::Outcome, String>>)>,
    copy_feedback: Option<(i64, Instant, bool)>,
    quitting: bool,
    return_to_welcome: bool,
    participants: Vec<tasks::Participant>,
    /// Highest message id already examined for delegated mentions (never replays history).
    delegate_after: i64,
    /// UTC timestamp; older messages (e.g. offline backlog synced later) are never delegated.
    delegate_since: String,
}
fn valid_room(room: &str) -> bool {
    !room.trim().is_empty()
        && room.chars().count() <= 64
        && !room.starts_with("agent:")
        && !room.chars().any(char::is_control)
}
fn main() -> Result<()> {
    let matches = Args::command().get_matches();
    let mut args = Args::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    paths::apply_data_env(
        matches.value_source("data") == Some(clap::parser::ValueSource::DefaultValue),
        &mut args.data,
    );
    paths::apply_optional_env(
        &mut args.workspace,
        "BELLA_WORKSPACE",
        "TERM_SOCIAL_WORKSPACE",
    );
    paths::apply_optional_env(&mut args.config, "BELLA_CONFIG", "TERM_SOCIAL_CONFIG");
    args.room = args.room.trim().to_owned();
    if args.network_logout {
        network::logout(&args.data.with_extension("network.json"))?;
        println!("Signed out of the hub. Cached chats are kept.");
        return Ok(());
    }
    if let Some(destination) = &args.backup {
        if !args.data.is_file() {
            bail!("Source database does not exist: {}", args.data.display());
        }
        Store::open(&args.data)?.backup(destination)?;
        println!("Backup saved: {}", destination.display());
        return Ok(());
    }
    let config_path = args.config.clone().unwrap_or_else(|| {
        args.data
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("settings.json")
    });
    let absolute = |path: &std::path::Path| {
        path.canonicalize()
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join(path))
    };
    if absolute(&config_path) == absolute(&args.data) {
        bail!("Settings and database must use different files");
    }
    let loaded = match settings::Settings::load(&config_path) {
        Ok(value) => value,
        Err(_) if args.setup || args.setup_only => None,
        Err(error) => return Err(error),
    };
    let first_run = loaded.is_none()
        && args.name.is_none()
        && args.send.is_none()
        && !args.history
        && !args.doctor
        && args.backup.is_none();
    let mut profile = loaded.unwrap_or_default();
    if let Some(name) = args.name {
        profile.name = name;
    }
    if let Some(endpoint) = args.endpoint {
        profile.endpoint = endpoint;
    }
    if let Some(model) = args.model {
        profile.model = model;
    }
    if let Some(workspace) = args.workspace {
        profile.workspace = workspace;
    }
    profile.name = profile.name.trim().to_owned();
    profile.model = profile.model.trim().to_owned();
    profile.endpoint = profile.endpoint.trim().trim_end_matches('/').to_owned();
    if args.setup || args.setup_only || first_run {
        let Some(configured) = settings::setup(profile, &config_path)? else {
            if args.setup_only {
                std::process::exit(130);
            }
            return Ok(());
        };
        profile = configured;
    }
    if args.setup_only {
        return Ok(());
    }
    if !settings::valid_name(&profile.name) {
        bail!("Choose a name of 1–32 printable characters");
    }
    let welcome_data = args.data.clone();
    let welcome_network = args.network;
    let credentials = if args.network {
        let session_path = args.data.with_extension("network.json");
        let Some(c) = network::login(&session_path, args.network_setup)? else {
            return Ok(());
        };
        profile.name = c.username.clone();
        args.data = c.cache_path(&args.data);
        Some(c)
    } else {
        None
    };
    if !valid_room(&args.room) && args.room != format!("agent:{}", profile.name) {
        bail!("Choose a group name of 1–64 printable characters; agent: is reserved");
    }
    let store = Store::open(&args.data)?;
    if let Some(c) = &credentials {
        network::prepare(&store)?;
        // Pull existing public profiles before creating this device's defaults.
        let _ = network::initial_sync(&store, c);
    }
    if args.doctor {
        return doctor(&store, &args.data, &profile);
    }
    if let Some(message) = args.send {
        store.add(&args.room, &profile.name, &message, "human")?;
        return Ok(());
    }
    if args.history {
        for m in store.history(&args.room)? {
            println!(
                "{}: {}",
                safe(&m.author),
                safe(
                    &bella::tool_action::Action::decode(&m.body)
                        .filter(|_| m.kind == "tool")
                        .map(|a| a.details())
                        .unwrap_or(m.body)
                )
            );
        }
        return Ok(());
    }
    profile.validate()?;
    let disabled_skills = std::mem::take(&mut profile.disabled_skills);
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!(
            "Chat needs an interactive terminal. Use --send, --history, --doctor or --backup for headless access."
        );
    }
    let mut app = App::new(
        profile.name,
        args.room,
        store,
        agent::Config {
            endpoint: profile.endpoint,
            model: profile.model,
            workspace: profile.workspace,
            author: String::new(),
            instructions: String::new(),
            tools: true,
            guest: None,
            request_id: String::new(),
            requester: String::new(),
            skills_prompt: String::new(),
            skill: None,
        },
    )?;
    if let Some(parent) = args.data.parent().filter(|p| !p.as_os_str().is_empty()) {
        app.data_dir = parent.to_path_buf();
    }
    app.init_skills(disabled_skills, Some(config_path.clone()));
    if let Some(c) = credentials {
        app.network = Some(network::Connection::start(c, args.data));
    }
    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        execute!(std::io::stdout(), EnableMouseCapture, EnableBracketedPaste)?;
        app.run(&mut terminal)
    })();
    let _ = execute!(
        std::io::stdout(),
        DisableMouseCapture,
        DisableBracketedPaste
    );
    ratatui::restore();
    result?;
    if app.return_to_welcome {
        // Re-enter setup only after the terminal and task/session state are clean.
        // Use the original network base path, not the per-account cache path.
        let name = app.name.clone();
        let config = app.config.clone();
        drop(app);
        use std::os::unix::process::CommandExt;
        let mut command = std::process::Command::new(std::env::current_exe()?);
        command
            .env_remove("BELLA_WORKSPACE")
            .env_remove("TERM_SOCIAL_WORKSPACE")
            .arg("--setup")
            .arg("--data")
            .arg(welcome_data)
            .arg("--config")
            .arg(config_path)
            .arg("--name")
            .arg(name)
            .arg("--endpoint")
            .arg(config.endpoint)
            .arg("--model")
            .arg(config.model)
            .arg("--workspace")
            .arg(config.workspace);
        if welcome_network {
            command.arg("--network");
        }
        return Err(command.exec().into());
    }
    Ok(())
}
fn doctor(store: &Store, data: &std::path::Path, profile: &settings::Settings) -> Result<()> {
    let integrity = store.integrity()?;
    let helpers = ["sh", "timeout"].iter().all(|name| {
        std::env::var_os("PATH")
            .is_some_and(|paths| std::env::split_paths(&paths).any(|p| p.join(name).is_file()))
    });
    let result = (|| -> Result<serde_json::Value> {
        Ok(reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()?
            .get(format!("{}/models", profile.endpoint.trim_end_matches('/')))
            .send()?
            .error_for_status()?
            .json()?)
    })();
    let model_ready = result.as_ref().is_ok_and(|v| {
        v.get("data")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|models| {
                models.iter().any(|m| {
                    m.get("id").and_then(serde_json::Value::as_str) == Some(&profile.model)
                })
            })
    });
    let workspace_ready = profile.workspace.is_dir();
    let chat_ready = integrity == "ok" && helpers && workspace_ready;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"data":data,"workspace":profile.workspace,"endpoint":profile.endpoint,"model":profile.model,"database_integrity":integrity,"shell_helpers":helpers,"workspace_ready":workspace_ready,"chat_ready":chat_ready,"model_ready":model_ready,"model_check":match result {Ok(value)=>value,Err(error)=>serde_json::json!({"error":format!("{error:#}")})}})
        )?
    );
    if !chat_ready || !model_ready {
        bail!(
            "Diagnostic checks incomplete; see JSON above. Chat can still work with an offline model."
        );
    }
    Ok(())
}

impl App {
    fn new(name: String, room: String, store: Store, mut config: agent::Config) -> Result<Self> {
        config.requester = name.clone();
        let existing = profiles::all(store.connection())?;
        let networked = store.connection().query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='network_meta'",
            [],
            |r| r.get::<_, i64>(0),
        )? > 0;
        for p in [
            profiles::Profile::user(&name),
            profiles::Profile::bot(&name, "bot"),
        ] {
            if !networked
                && !existing
                    .iter()
                    .any(|old| old.owner == p.owner && old.handle == p.handle)
            {
                store.connection().execute(
                    "INSERT OR IGNORE INTO profiles VALUES(?1,?2,?3)",
                    rusqlite::params![p.owner, p.handle, serde_json::to_string(&p)?],
                )?;
            }
        }
        let session = tasks::session(store.connection())?;
        store.ensure_room("lobby")?;
        store.ensure_room(&format!("agent:{name}"))?;
        store.ensure_room(&room)?;
        tasks::presence(store.connection(), &session, &name, &room)?;
        let input = Editor::new(store.draft(&name, &room)?);
        let recipient = store.recipient(&name, &room)?;
        let delegate_after: i64 =
            store
                .connection()
                .query_row("SELECT COALESCE(MAX(id),0) FROM messages", [], |r| r.get(0))?;
        let delegate_since: String =
            store
                .connection()
                .query_row("SELECT datetime('now','-60 seconds')", [], |r| r.get(0))?;
        let mut app = Self {
            recipient,
            network: None,
            profiles: vec![],
            profile_index: std::collections::HashMap::new(),
            enabled_bots: vec![],
            profile_form: None,
            name,
            room,
            input,
            chats: vec![],
            view_archived: false,
            sidebar_offset: None,
            action_room: None,
            messages: vec![],
            status: "Welcome. Ctrl+N creates a group · Ctrl+K finds a chat".into(),
            scroll: 0,
            max_scroll: 0,
            anchor: None,
            jump_to: None,
            highlight: None,
            following: true,
            has_older: false,
            has_newer: false,
            hits: vec![],
            more_hits: false,
            approval: None,
            job: None,
            store,
            config,
            data_dir: PathBuf::from("."),
            skills: skills::Catalog::default(),
            skill_roots: Vec::new(),
            disabled_skills: Vec::new(),
            settings_path: None,
            skills_error: String::new(),
            dialog: None,
            query: Editor::default(),
            selected: 0,
            dialog_scroll: 0,
            health: None,
            health_detail: "Connection not checked yet".into(),
            health_rx: None,
            health_checked: None,
            failed_room: None,
            session,
            task_records: vec![],
            task_detail: None,
            browser_detail_id: None,
            browser_links: Vec::new(),
            copy_links: Vec::new(),
            copy_job: None,
            copy_feedback: None,
            quitting: false,
            return_to_welcome: false,
            participants: vec![],
            delegate_after,
            delegate_since,
        };
        app.refresh()?;
        app.view_archived = app.chats.iter().any(|c| c.room == app.room && c.archived);
        Ok(app)
    }
    fn refresh(&mut self) -> Result<()> {
        if let Some(net) = &mut self.network {
            net.poll();
        }
        self.profiles = profiles::all(self.store.connection())?;
        self.profile_index = profiles::index(&self.profiles);
        self.enabled_bots = profiles::enabled(self.store.connection(), &self.name, &self.room)?;
        if self.messages.is_empty() {
            self.messages = self.store.history(&self.room)?;
        } else if self.following {
            let last = self.messages.last().unwrap().id;
            self.messages
                .extend(self.store.page_after(&self.room, last)?);
        }
        self.has_older = match self.messages.first() {
            Some(m) => self.store.has_before(&self.room, m.id)?,
            None => false,
        };
        self.has_newer = match self.messages.last() {
            Some(m) => self.store.has_after(&self.room, m.id)?,
            None => false,
        };
        if !self.has_newer {
            self.following = true;
        }
        if self.scroll == 0
            && self.jump_to.is_none()
            && !self.has_newer
            && let Some(m) = self.messages.last()
        {
            self.store.mark_read(&self.name, &self.room, m.id)?;
        }
        if self.dialog == Some(Dialog::Participants) {
            self.participants = tasks::participants(self.store.connection(), &self.room)?;
        }
        self.task_records = tasks::list(self.store.connection(), &self.name)?;
        self.chats = self.store.conversations(&self.name)?;
        Ok(())
    }
    fn load_older(&mut self) -> Result<()> {
        if let Some(first) = self.messages.first() {
            let mut older = self.store.page_before(&self.room, first.id, 200)?;
            let count = older.len();
            older.append(&mut self.messages);
            self.messages = older;
            self.has_older = match self.messages.first() {
                Some(m) => self.store.has_before(&self.room, m.id)?,
                None => false,
            };
            self.status = if count > 0 {
                format!("Loaded {count} earlier messages")
            } else {
                "Beginning of this conversation".into()
            };
        }
        Ok(())
    }
    fn scroll_up(&mut self, lines: usize) -> Result<()> {
        self.anchor = None;
        if self.scroll.saturating_add(lines) >= self.max_scroll && self.has_older {
            self.load_older()?;
        }
        self.scroll = self.scroll.saturating_add(lines);
        Ok(())
    }
    fn scroll_down(&mut self, lines: usize) -> Result<()> {
        self.anchor = None;
        if self.scroll <= lines && self.has_newer {
            if let Some(last) = self.messages.last() {
                let id = last.id;
                self.messages.extend(self.store.page_after(&self.room, id)?);
                self.jump_to = Some(id);
                self.scroll = 1;
                self.refresh()?;
            }
        } else {
            self.scroll = self.scroll.saturating_sub(lines);
        }
        Ok(())
    }
    fn latest(&mut self) -> Result<()> {
        self.messages = self.store.history(&self.room)?;
        self.following = true;
        self.has_newer = false;
        self.scroll = 0;
        self.anchor = None;
        self.jump_to = None;
        self.highlight = None;
        self.status = "Latest messages · Ctrl+F searches this conversation".into();
        self.refresh()
    }
    fn update_hits(&mut self, more: bool) -> Result<()> {
        let before = if more {
            self.hits.last().map_or(i64::MAX, |m| m.id)
        } else {
            i64::MAX
        };
        let page = self.store.search(&self.room, &self.query.text, before)?;
        self.more_hits = page.len() == 50;
        if !more {
            self.hits.clear();
            self.selected = 0;
        }
        self.hits.extend(page);
        Ok(())
    }
    fn open_message(&mut self, id: i64) -> Result<()> {
        // The result must belong to the open conversation, including when ids are guessed.
        if self.store.message_room(id)? != self.room {
            bail!("Message belongs to a different conversation");
        }
        self.messages = self.store.around(&self.room, id)?;
        self.following = false;
        self.scroll = 1;
        self.anchor = None;
        self.jump_to = Some(id);
        self.highlight = Some(id);
        self.refresh()?;
        self.status = "Search result highlighted · Ctrl+End returns to latest messages".into();
        Ok(())
    }
    fn switch_room(&mut self, room: String) -> Result<()> {
        self.view_archived = self.chats.iter().any(|c| c.room == room && c.archived);
        self.sidebar_offset = None;
        if self.room != room {
            self.store
                .save_draft(&self.name, &self.room, &self.input.text)?;
            self.store.ensure_room(&room)?;
            self.input = Editor::new(self.store.draft(&self.name, &room)?);
            self.room = room;
            self.recipient = self.store.recipient(&self.name, &self.room)?;
            tasks::presence(
                self.store.connection(),
                &self.session,
                &self.name,
                &self.room,
            )?;
            self.messages.clear();
            self.browser_links.clear();
            self.copy_links.clear();
            self.browser_detail_id = None;
            self.following = true;
            self.has_newer = false;
            self.anchor = None;
            self.jump_to = None;
            self.highlight = None;
            self.scroll = 0;
            self.status = "Enter to send · @bot brings your agent into a group".into();
            self.refresh()?;
        }
        Ok(())
    }
    fn visible_chats(&self) -> Vec<&Conversation> {
        self.chats
            .iter()
            .filter(|c| c.archived == self.view_archived)
            .collect()
    }
    fn toggle_folder(&mut self) {
        self.view_archived = !self.view_archived;
        self.sidebar_offset = None;
        self.status = if self.view_archived {
            "Archived chats · F4 returns to active chats"
        } else {
            "Active chats · F4 opens the archive"
        }
        .into();
    }
    fn open_chat_actions(&mut self, room: String) {
        self.open_dialog(Dialog::ChatActions);
        self.action_room = Some(room);
    }
    fn chat_action(&mut self, index: usize) -> Result<()> {
        let room = self
            .action_room
            .clone()
            .unwrap_or_else(|| self.room.clone());
        // Refresh preferences in case another instance changed this user's settings.
        let state = self
            .store
            .conversations(&self.name)?
            .into_iter()
            .find(|c| c.room == room);
        if let Some(chat) = state {
            match index {
                0 => {
                    self.store.set_pinned(&self.name, &room, !chat.pinned)?;
                    self.status = format!(
                        "{} {}",
                        chat_name(&room),
                        if chat.pinned {
                            "unpinned"
                        } else {
                            "pinned to the top"
                        }
                    );
                }
                1 => {
                    self.store.set_archived(&self.name, &room, !chat.archived)?;
                    if room == self.room {
                        self.view_archived = !chat.archived;
                    }
                    self.status = format!(
                        "{} {} for you · messages and drafts kept",
                        chat_name(&room),
                        if chat.archived {
                            "restored"
                        } else {
                            "archived"
                        }
                    );
                }
                _ => {}
            }
            self.refresh()?;
        }
        self.dialog = None;
        self.action_room = None;
        self.sidebar_offset = None;
        Ok(())
    }
    fn cycle_room(&mut self, back: bool) -> Result<()> {
        let chats = self.visible_chats();
        if chats.is_empty() {
            return Ok(());
        }
        let current = chats.iter().position(|c| c.room == self.room);
        let next = match current {
            Some(i) if back => (i + chats.len() - 1) % chats.len(),
            Some(i) => (i + 1) % chats.len(),
            None if back => chats.len() - 1,
            None => 0,
        };
        self.switch_room(chats[next].room.clone())
    }
    fn filtered_chats(&self) -> Vec<&Conversation> {
        let q = self.query.text.to_lowercase();
        self.chats
            .iter()
            .filter(|c| {
                c.room.to_lowercase().contains(&q)
                    || c.preview.to_lowercase().contains(&q)
                    || (c.room.starts_with("agent:") && "my agent".contains(&q))
            })
            .collect()
    }
    fn task_tool_results(&self, task: &tasks::TaskRecord) -> Result<String> {
        let session: String = self.store.connection().query_row(
            "SELECT session FROM agent_tasks WHERE id=?1 AND owner=?2",
            rusqlite::params![task.id, self.name],
            |r| r.get(0),
        )?;
        let request_id = format!("{session}:{}", task.id);
        let mut query = self.store.connection().prepare(
            "SELECT body FROM messages WHERE room=?1 AND kind='tool' AND body LIKE 'Tool action v1: %' AND instr(body,?2)>0 ORDER BY id LIMIT 100")?;
        let mut text = String::new();
        for row in query.query_map(rusqlite::params![task.room, request_id], |r| {
            r.get::<_, String>(0)
        })? {
            if let Some(action) = bella::tool_action::Action::decode(&row?)
                && action.result.scope().request_id == request_id
            {
                text.push_str(&format!("\n\n{}", safe(&action.details())));
            }
        }
        Ok(text)
    }
    fn message_copy_text(&self, id: i64) -> Option<String> {
        if id == 0 {
            return self
                .job
                .as_ref()
                .filter(|job| job.room == self.room && !job.partial.is_empty())
                .map(|job| job.partial.clone());
        }
        let message = self.messages.iter().find(|message| message.id == id)?;
        Some(if message.kind == "tool" {
            bella::tool_action::Action::decode(&message.body)
                .map(|action| action.details())
                .or_else(|| browser::Action::decode(&message.body).map(|action| action.details()))
                .unwrap_or_else(|| message.body.clone())
        } else {
            message.body.clone()
        })
    }
    fn copy_message_at(&mut self, column: u16, row: u16) -> bool {
        if self.dialog.is_some() || self.approval.is_some() {
            return false;
        }
        let Some(id) = self
            .copy_links
            .iter()
            .find(|(rect, _)| rect.contains((column, row).into()))
            .map(|(_, id)| *id)
        else {
            return false;
        };
        let Some(text) = self.message_copy_text(id) else {
            return false;
        };
        if self.copy_job.is_some() {
            self.status = "Copy in progress; try again shortly".into();
            return true;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(clipboard::copy(text));
        });
        self.copy_job = Some((id, rx));
        self.status = "Copying message…".into();
        true
    }
    fn poll_clipboard(&mut self) {
        let Some((id, receiver)) = &self.copy_job else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Clipboard worker stopped".into()),
        };
        let id = *id;
        self.copy_job = None;
        match result {
            Ok(clipboard::Outcome::Copied) => {
                self.copy_feedback = Some((id, Instant::now(), true));
                self.status = "Message copied".into();
            }
            Ok(clipboard::Outcome::TerminalRequest(sequence)) => {
                use std::io::Write;
                let mut output = std::io::stdout().lock();
                match output
                    .write_all(sequence.as_bytes())
                    .and_then(|_| output.flush())
                {
                    Ok(()) => {
                        self.copy_feedback = Some((id, Instant::now(), false));
                        self.status =
                            "Copy sent to terminal · clipboard access must be enabled".into();
                    }
                    Err(error) => self.status = format!("Copy failed: {error}"),
                }
            }
            Err(error) => self.status = format!("Copy failed: {error}"),
        }
    }
    fn browser_actions(&self) -> Vec<(i64, browser::Action)> {
        self.messages
            .iter()
            .filter(|m| m.kind == "tool")
            .filter_map(|m| browser::Action::decode(&m.body).map(|action| (m.id, action)))
            .collect()
    }
    fn browser_detail_index(&self) -> Option<usize> {
        self.browser_actions()
            .iter()
            .position(|(id, _)| Some(*id) == self.browser_detail_id)
    }
    fn move_browser_detail(&mut self, forward: bool) {
        let actions = self.browser_actions();
        if let Some(index) = self.browser_detail_index() {
            self.selected = if forward {
                (index + 1).min(actions.len() - 1)
            } else {
                index.saturating_sub(1)
            };
            self.browser_detail_id = Some(actions[self.selected].0);
            self.dialog_scroll = 0;
        }
    }
    fn open_browser_details_at(&mut self, column: u16, row: u16) -> bool {
        if self.dialog.is_some() || self.approval.is_some() {
            return false;
        }
        let id = self
            .browser_links
            .iter()
            .find(|(rect, _)| rect.contains((column, row).into()))
            .map(|(_, id)| *id);
        if let Some(id) = id
            && let Some(index) = self
                .browser_actions()
                .iter()
                .position(|(action_id, _)| *action_id == id)
        {
            self.open_dialog(Dialog::BrowserDetails);
            self.selected = index;
            self.browser_detail_id = Some(id);
            return true;
        }
        false
    }
    fn open_dialog(&mut self, dialog: Dialog) {
        if dialog == Dialog::Profile {
            let p = self
                .profiles
                .iter()
                .find(|p| p.owner == self.name && p.handle.is_empty())
                .cloned()
                .unwrap_or_else(|| profiles::Profile::user(&self.name));
            self.profile_form = Some(profiles::Form::new(p, false));
        }
        self.dialog = Some(dialog);
        self.query = Editor::default();
        if self.dialog == Some(Dialog::BrowserDetails) {
            self.browser_detail_id = self.browser_actions().last().map(|(id, _)| *id);
        }
        self.selected = if self.dialog == Some(Dialog::BrowserDetails) {
            self.browser_actions().len().saturating_sub(1)
        } else {
            0
        };
        self.dialog_scroll = 0;
        self.hits.clear();
        self.more_hits = false;
        self.action_room = None;
    }
    fn poll_health(&mut self) {
        if let Some(rx) = &self.health_rx {
            let result = match rx.try_recv() {
                Ok(value) => Some(value),
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("Connection check stopped unexpectedly".into()))
                }
                Err(mpsc::TryRecvError::Empty) => None,
            };
            if let Some(result) = result {
                match result {
                    Ok(models) => {
                        let ready = models.iter().any(|m| m == &self.config.model);
                        self.health = Some(ready);
                        self.health_detail = if ready {
                            format!(
                                "Selected model is available. Server models: {}",
                                models.join(", ")
                            )
                        } else {
                            format!(
                                "Server reached, but the selected model is not listed. Available: {}",
                                if models.is_empty() {
                                    "none".into()
                                } else {
                                    models.join(", ")
                                }
                            )
                        };
                    }
                    Err(error) => {
                        self.health = Some(false);
                        self.health_detail = error;
                    }
                }
                self.health_rx = None;
                self.health_checked = Some(Instant::now());
            }
        }
        if self.health_rx.is_none()
            && self
                .health_checked
                .is_none_or(|t| t.elapsed() > Duration::from_secs(15))
        {
            let (tx, rx) = mpsc::channel();
            let endpoint = self.config.endpoint.clone();
            self.health_rx = Some(rx);
            std::thread::spawn(move || {
                let result = (|| -> Result<Vec<String>> {
                    let value: serde_json::Value = reqwest::blocking::Client::builder()
                        .timeout(Duration::from_secs(2))
                        .build()?
                        .get(format!("{}/models", endpoint.trim_end_matches('/')))
                        .send()?
                        .error_for_status()?
                        .json()?;
                    Ok(value
                        .get("data")
                        .and_then(serde_json::Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(|m| m.get("id").and_then(serde_json::Value::as_str))
                        .map(str::to_owned)
                        .collect())
                })()
                .map_err(|e| format!("Cannot connect to model server: {e:#}"));
                let _ = tx.send(result);
            });
        }
    }
    fn recheck_model(&mut self) {
        if self.health_rx.is_none() {
            self.health_checked = None;
            self.health = None;
            self.health_detail = "Checking connection…".into();
            self.poll_health();
        }
    }
    fn request_welcome(&mut self) -> Result<()> {
        self.save_draft()?;
        self.return_to_welcome = true;
        self.quitting = true;
        if self.job.is_some() {
            self.stop_agent()?;
            self.status = "Returning to welcome · finishing the active request safely".into();
        } else {
            self.status = "Returning to welcome…".into();
        }
        Ok(())
    }
    fn stop_agent(&mut self) -> Result<()> {
        if let Some(job) = &mut self.job {
            job.stopping = true;
            if let Some(id) = job.task_id {
                tasks::update(self.store.connection(), id, "stopping", &job.partial, "")?;
            }
            job.control.cancel();
            self.approval.take();
            self.status = if job.ran_command {
                "Stopping response · waiting for the approved command's result"
            } else {
                "Stopping agent request…"
            }
            .into();
        } else {
            self.status = "No active agent request".into();
        }
        Ok(())
    }
    fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        loop {
            self.poll_job()?;
            self.poll_clipboard();
            if self.quitting && self.job.is_none() {
                break;
            }
            self.refresh()?;
            self.poll_delegated()?;
            self.poll_health();
            terminal.draw(|f| ui::draw(self, f))?;
            if !event::poll(Duration::from_millis(100))? {
                continue;
            }
            match event::read()? {
                Event::Key(k)
                    if k.kind == KeyEventKind::Press || k.kind == KeyEventKind::Repeat =>
                {
                    if self.quitting
                        && !(k.code == KeyCode::Char('c')
                            && k.modifiers.contains(KeyModifiers::CONTROL))
                    {
                        continue;
                    }
                    if self.key(k)? {
                        if self.job.is_some() && !self.quitting {
                            self.quitting = true;
                            self.stop_agent()?;
                            self.status.push_str(" · Ctrl+C again forces exit");
                        } else {
                            break;
                        }
                    }
                }
                Event::Paste(text) if !self.quitting && self.approval.is_none() => {
                    if matches!(
                        self.dialog,
                        Some(Dialog::Search | Dialog::NewGroup | Dialog::FindMessages)
                    ) {
                        self.query.insert(&text.replace(['\n', '\r'], " "));
                        self.selected = 0;
                        if self.dialog == Some(Dialog::FindMessages) {
                            self.update_hits(false)?;
                        }
                    } else if matches!(self.dialog, Some(Dialog::Profile | Dialog::BotProfile)) {
                        self.profile_paste(&text);
                    } else if self.dialog.is_none() {
                        self.input.insert(&text);
                        self.save_draft()?;
                    }
                }
                Event::Mouse(m)
                    if !self.quitting && self.approval.is_none() && self.dialog.is_some() =>
                {
                    let rect =
                        ui::dialog_rect(terminal.size()?.into(), self.dialog.as_ref().unwrap());
                    self.dialog_mouse(m, rect)?;
                }
                Event::Mouse(m)
                    if !self.quitting && self.approval.is_none() && self.dialog.is_none() =>
                {
                    let layout = ui::screen(terminal.size()?.into());
                    match m.kind {
                        MouseEventKind::Down(MouseButton::Left) => {
                            if layout.sidebar.contains((m.column, m.row).into()) {
                                if m.row == layout.sidebar.bottom().saturating_sub(2) {
                                    if m.column >= layout.sidebar.right().saturating_sub(13) {
                                        self.open_dialog(Dialog::Tasks);
                                    } else {
                                        self.open_dialog(Dialog::Profile);
                                    }
                                } else if m.row == layout.sidebar.bottom().saturating_sub(1) {
                                    self.open_dialog(Dialog::ModelInfo);
                                } else if m.row == layout.sidebar.y {
                                    self.view_archived =
                                        m.column >= layout.sidebar.x + layout.sidebar.width / 2;
                                    self.sidebar_offset = None;
                                } else if m.row == layout.sidebar.y + 1 {
                                    self.open_dialog(Dialog::Search);
                                } else if m.row == layout.sidebar.y + 3 {
                                    self.open_dialog(Dialog::NewGroup);
                                } else if m.row >= layout.sidebar.y + 5
                                    && m.row < layout.sidebar.bottom().saturating_sub(2)
                                {
                                    let i = ui::chat_start(self, layout.sidebar)
                                        + (m.row - layout.sidebar.y - 5) as usize / 4;
                                    if let Some(c) = self.visible_chats().get(i) {
                                        self.switch_room(c.room.clone())?;
                                    }
                                }
                            } else if layout.welcome.contains((m.column, m.row).into()) {
                                self.request_welcome()?;
                            } else if layout.menu.contains((m.column, m.row).into()) {
                                self.open_chat_actions(self.room.clone());
                            } else if layout.stop.contains((m.column, m.row).into())
                                && self.job.is_some()
                            {
                                self.stop_agent()?;
                            } else if layout.header.contains((m.column, m.row).into())
                                && m.column >= layout.header.right().saturating_sub(12)
                            {
                                self.open_dialog(Dialog::FindMessages);
                            } else if layout.history.contains((m.column, m.row).into())
                                && m.row == layout.history.bottom().saturating_sub(1)
                                && (self.scroll > 0 || self.has_newer)
                            {
                                self.latest()?;
                            } else if layout.history.contains((m.column, m.row).into()) {
                                if !self.copy_message_at(m.column, m.row) {
                                    self.open_browser_details_at(m.column, m.row);
                                }
                            } else if layout.group_target.contains((m.column, m.row).into()) {
                                self.select_recipient(None)?;
                            } else if layout.bot_target.contains((m.column, m.row).into()) {
                                self.cycle_recipient(true)?;
                            } else if layout.send.contains((m.column, m.row).into()) {
                                self.send_input()?;
                            }
                        }
                        MouseEventKind::Down(MouseButton::Right)
                            if layout.sidebar.contains((m.column, m.row).into())
                                && m.row >= layout.sidebar.y + 5
                                && m.row < layout.sidebar.bottom().saturating_sub(2) =>
                        {
                            let i = ui::chat_start(self, layout.sidebar)
                                + (m.row - layout.sidebar.y - 5) as usize / 4;
                            if let Some(chat) = self.visible_chats().get(i) {
                                self.open_chat_actions(chat.room.clone());
                            }
                        }
                        MouseEventKind::ScrollUp
                            if layout.sidebar.contains((m.column, m.row).into()) =>
                        {
                            self.sidebar_offset =
                                Some(ui::chat_start(self, layout.sidebar).saturating_sub(1))
                        }
                        MouseEventKind::ScrollDown
                            if layout.sidebar.contains((m.column, m.row).into()) =>
                        {
                            self.sidebar_offset =
                                Some(ui::chat_start(self, layout.sidebar).saturating_add(1))
                        }
                        MouseEventKind::ScrollUp => self.scroll_up(3)?,
                        MouseEventKind::ScrollDown => self.scroll_down(3)?,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        self.save_draft()?;
        self.poll_job()?;
        if let Some(job) = &mut self.job {
            record_end(
                &self.store,
                job,
                &self.name,
                "interrupted",
                "Session closed before completion",
                "Session closed before the agent finished. No task will be replayed. An approved command or background job may still run.",
                None,
            )?;
            job.control.cancel();
            job.partial.clear();
        }
        self.approval.take();
        tasks::leave(self.store.connection(), &self.session)?;
        Ok(())
    }
    fn save_draft(&self) -> Result<()> {
        self.store
            .save_draft(&self.name, &self.room, &self.input.text)
    }
    fn key(&mut self, k: KeyEvent) -> Result<bool> {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            self.return_to_welcome = false;
            return Ok(true);
        }
        if ctrl && k.code == KeyCode::Char('x') {
            self.stop_agent()?;
            return Ok(false);
        }
        if self.approval.is_some() {
            match k.code {
                KeyCode::Char('y') if k.kind == KeyEventKind::Press => {
                    if let Some(a) = self.approval.take() {
                        if let Some(id) = self.job.as_ref().and_then(|j| j.task_id) {
                            tasks::decision(self.store.connection(), id, true)?;
                        }
                        let _ = a.answer.send(true);
                    }
                    self.status = "Running approved command…".into();
                }
                KeyCode::Char('n') | KeyCode::Esc => {
                    if let Some(a) = self.approval.take() {
                        if let Some(id) = self.job.as_ref().and_then(|j| j.task_id) {
                            tasks::decision(self.store.connection(), id, false)?;
                        }
                        let _ = a.answer.send(false);
                    }
                    self.status = "Command denied".into();
                }
                KeyCode::PageUp => {
                    if let Some(a) = &mut self.approval {
                        a.scroll = a.scroll.saturating_sub(5);
                    }
                }
                KeyCode::PageDown => {
                    if let Some(a) = &mut self.approval {
                        a.scroll = a.scroll.saturating_add(5);
                    }
                }
                _ => {}
            }
            return Ok(false);
        }
        if self.dialog.is_some() {
            self.dialog_key(k)?;
            return Ok(false);
        }
        if ctrl {
            match k.code {
                KeyCode::Char('o') => self.open_dialog(Dialog::ModelInfo),
                KeyCode::Char('f') => self.open_dialog(Dialog::FindMessages),
                KeyCode::End => self.latest()?,
                KeyCode::Char('k') => self.open_dialog(Dialog::Search),
                KeyCode::Char('n') => self.open_dialog(Dialog::NewGroup),
                KeyCode::Char('h') => self.open_dialog(Dialog::Help),
                KeyCode::Char('r') => self.retry()?,
                KeyCode::Char('a') => self.input.cursor = 0,
                KeyCode::Char('e') => self.input.cursor = self.input.text.len(),
                KeyCode::Char('u') => {
                    self.input = Editor::default();
                    self.save_draft()?;
                }
                _ => {}
            }
            return Ok(false);
        }
        match k.code {
            KeyCode::Tab => self.cycle_room(false)?,
            KeyCode::BackTab => self.cycle_room(true)?,
            KeyCode::Char(c) => {
                self.input.insert(&c.to_string());
                self.save_draft()?;
            }
            KeyCode::Backspace => {
                self.input.backspace();
                self.save_draft()?;
            }
            KeyCode::Delete => {
                self.input.delete();
                self.save_draft()?;
            }
            KeyCode::Left => self.input.left(),
            KeyCode::Right => self.input.right(),
            KeyCode::Home => self.input.cursor = 0,
            KeyCode::End => self.input.cursor = self.input.text.len(),
            KeyCode::PageUp => self.scroll_up(8)?,
            KeyCode::PageDown => self.scroll_down(8)?,
            KeyCode::Enter if k.modifiers.contains(KeyModifiers::ALT) => {
                self.input.insert("\n");
                self.save_draft()?;
            }
            KeyCode::Enter => {
                if self.input.text.trim() == "/quit" {
                    self.return_to_welcome = false;
                    return Ok(true);
                }
                self.send_input()?;
            }
            KeyCode::Esc => {
                self.highlight = None;
            }
            KeyCode::F(1) => self.open_dialog(Dialog::Help),
            KeyCode::F(2) => self.open_dialog(Dialog::ModelInfo),
            KeyCode::F(3) => self.open_chat_actions(self.room.clone()),
            KeyCode::F(4) => self.toggle_folder(),
            KeyCode::F(5) => self.open_dialog(Dialog::Tasks),
            KeyCode::F(6) => self.open_dialog(Dialog::Participants),
            KeyCode::F(7) => self.open_dialog(Dialog::Profile),
            KeyCode::F(8) => self.open_dialog(Dialog::Bots),
            KeyCode::F(9) => self.open_dialog(Dialog::Network),
            KeyCode::F(10) => self.cycle_recipient(false)?,
            KeyCode::F(11) => self.open_dialog(Dialog::BrowserDetails),
            _ => {}
        }
        Ok(false)
    }
    fn dialog_mouse(&mut self, m: event::MouseEvent, rect: ratatui::layout::Rect) -> Result<()> {
        if matches!(
            self.dialog,
            Some(Dialog::Profile | Dialog::BotProfile | Dialog::Bots)
        ) {
            return self.profile_mouse(m, rect);
        }
        if self.dialog == Some(Dialog::Tasks) {
            if m.kind == MouseEventKind::Down(MouseButton::Left) {
                let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
                let count = inner.height.saturating_sub(4) as usize / 3;
                if inner.contains((m.column, m.row).into())
                    && m.row >= inner.y + 2
                    && m.row < inner.y + 2 + count as u16 * 3
                {
                    let index = self.selected.saturating_sub(count.max(1) - 1)
                        + (m.row - inner.y - 2) as usize / 3;
                    if index < self.task_records.len() {
                        self.selected = index;
                        self.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
                    }
                }
            } else if matches!(
                m.kind,
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            ) {
                self.dialog_key(KeyEvent::new(
                    if m.kind == MouseEventKind::ScrollUp {
                        KeyCode::Up
                    } else {
                        KeyCode::Down
                    },
                    KeyModifiers::NONE,
                ))?;
            }
            return Ok(());
        }
        if self.dialog == Some(Dialog::ChatActions) {
            if m.kind == MouseEventKind::Down(MouseButton::Left)
                && rect.contains((m.column, m.row).into())
            {
                if m.row == rect.y + 5 {
                    self.chat_action(0)?;
                } else if m.row == rect.y + 7 {
                    self.chat_action(1)?;
                }
            }
            return Ok(());
        }
        if matches!(
            self.dialog,
            Some(
                Dialog::Help
                    | Dialog::ModelInfo
                    | Dialog::TaskDetails
                    | Dialog::BrowserDetails
                    | Dialog::Participants
                    | Dialog::Network
                    | Dialog::Skills
            )
        ) {
            match m.kind {
                MouseEventKind::ScrollUp => {
                    self.dialog_scroll = self.dialog_scroll.saturating_sub(3)
                }
                MouseEventKind::ScrollDown => {
                    self.dialog_scroll = self.dialog_scroll.saturating_add(3)
                }
                _ => {}
            }
            return Ok(());
        }
        if self.dialog == Some(Dialog::NewGroup) {
            return Ok(());
        }
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
                let count = inner.height.saturating_sub(6) as usize / 2;
                if inner.contains((m.column, m.row).into())
                    && m.row >= inner.y + 4
                    && m.row < inner.y + 4 + count as u16 * 2
                {
                    let start = self.selected.saturating_sub(count.max(1) - 1);
                    let index = start + (m.row - inner.y - 4) as usize / 2;
                    let total = if self.dialog == Some(Dialog::FindMessages) {
                        self.hits.len()
                    } else {
                        self.filtered_chats().len()
                    };
                    if index < total {
                        self.selected = index;
                        self.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                self.dialog_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE))?
            }
            MouseEventKind::ScrollDown => {
                self.dialog_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE))?
            }
            _ => {}
        }
        Ok(())
    }
    fn dialog_key(&mut self, k: KeyEvent) -> Result<()> {
        if matches!(
            self.dialog,
            Some(Dialog::Profile | Dialog::BotProfile | Dialog::Bots)
        ) {
            return self.profile_key(k);
        }
        if self.dialog == Some(Dialog::Tasks) {
            let count = self.task_records.len();
            match k.code {
                KeyCode::Esc => self.dialog = None,
                KeyCode::Down if count > 0 => self.selected = (self.selected + 1) % count,
                KeyCode::Up if count > 0 => self.selected = (self.selected + count - 1) % count,
                KeyCode::Enter if count > 0 => {
                    self.task_detail = self.task_records.get(self.selected).cloned();
                    self.dialog = Some(Dialog::TaskDetails);
                    self.dialog_scroll = 0;
                }
                _ => {}
            }
            return Ok(());
        }
        if self.dialog == Some(Dialog::TaskDetails) {
            match k.code {
                KeyCode::Esc => self.dialog = Some(Dialog::Tasks),
                KeyCode::Enter => {
                    if let Some(task) = &self.task_detail {
                        self.switch_room(task.room.clone())?;
                    }
                    self.dialog = None;
                }
                KeyCode::PageUp => self.dialog_scroll = self.dialog_scroll.saturating_sub(5),
                KeyCode::PageDown => self.dialog_scroll = self.dialog_scroll.saturating_add(5),
                _ => {}
            }
            return Ok(());
        }
        if self.dialog == Some(Dialog::ChatActions) {
            match k.code {
                KeyCode::Esc => {
                    self.dialog = None;
                    self.action_room = None;
                }
                KeyCode::Up | KeyCode::Down => self.selected = 1 - self.selected.min(1),
                KeyCode::Char('p') => self.chat_action(0)?,
                KeyCode::Char('a') => self.chat_action(1)?,
                KeyCode::Enter => self.chat_action(self.selected)?,
                _ => {}
            }
            return Ok(());
        }
        if matches!(
            self.dialog,
            Some(
                Dialog::ModelInfo
                    | Dialog::Help
                    | Dialog::Participants
                    | Dialog::Network
                    | Dialog::BrowserDetails
                    | Dialog::Skills
            )
        ) {
            match k.code {
                KeyCode::PageUp => self.dialog_scroll = self.dialog_scroll.saturating_sub(5),
                KeyCode::PageDown => self.dialog_scroll = self.dialog_scroll.saturating_add(5),
                KeyCode::Left if self.dialog == Some(Dialog::BrowserDetails) => {
                    self.move_browser_detail(false);
                }
                KeyCode::Right if self.dialog == Some(Dialog::BrowserDetails) => {
                    self.move_browser_detail(true);
                }
                KeyCode::Char('r')
                    if self.dialog == Some(Dialog::Network) && self.network.is_some() =>
                {
                    self.store
                        .connection()
                        .execute("UPDATE network_outbox SET error='' WHERE error<>''", [])?;
                    self.status = "Rejected events queued for another attempt".into();
                }
                KeyCode::Char('r') if self.dialog == Some(Dialog::ModelInfo) => {
                    self.recheck_model()
                }
                KeyCode::Char('r') if self.dialog == Some(Dialog::Skills) => {
                    self.rescan_skills();
                    self.status = "Skill roots scanned again".into();
                }
                KeyCode::Esc | KeyCode::Enter => self.dialog = None,
                _ => {}
            }
            return Ok(());
        }
        match k.code {
            KeyCode::Esc => self.dialog = None,
            KeyCode::Enter => match self.dialog {
                Some(Dialog::Search) => {
                    let room = self
                        .filtered_chats()
                        .get(self.selected)
                        .map(|c| c.room.clone());
                    if let Some(room) = room {
                        self.switch_room(room)?;
                        self.dialog = None;
                    }
                }
                Some(Dialog::FindMessages) => {
                    if let Some(m) = self.hits.get(self.selected) {
                        self.open_message(m.id)?;
                        self.dialog = None;
                    }
                }
                Some(Dialog::NewGroup) => {
                    let room = self.query.text.trim().to_owned();
                    if valid_room(&room) {
                        self.switch_room(room)?;
                        self.dialog = None;
                    } else {
                        self.status =
                            "Group name must be 1–64 printable characters; agent: is reserved"
                                .into();
                    }
                }
                _ => self.dialog = None,
            },
            KeyCode::Down => {
                let n = if self.dialog == Some(Dialog::FindMessages) {
                    self.hits.len()
                } else {
                    self.filtered_chats().len()
                };
                if n > 0 {
                    self.selected = (self.selected + 1) % n;
                }
            }
            KeyCode::Up => {
                let n = if self.dialog == Some(Dialog::FindMessages) {
                    self.hits.len()
                } else {
                    self.filtered_chats().len()
                };
                if n > 0 {
                    self.selected = (self.selected + n - 1) % n;
                }
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.query.insert(&c.to_string());
                self.selected = 0;
            }
            KeyCode::Backspace => {
                self.query.backspace();
                self.selected = 0;
            }
            KeyCode::Delete => {
                self.query.delete();
                self.selected = 0;
            }
            KeyCode::PageDown if self.dialog == Some(Dialog::FindMessages) && self.more_hits => {
                let old = self.hits.len();
                self.update_hits(true)?;
                if self.hits.len() > old {
                    self.selected = old;
                }
            }
            KeyCode::Left => self.query.left(),
            KeyCode::Right => self.query.right(),
            KeyCode::Home => self.query.cursor = 0,
            KeyCode::End => self.query.cursor = self.query.text.len(),
            _ => {}
        }
        if self.dialog == Some(Dialog::FindMessages)
            && matches!(
                k.code,
                KeyCode::Char(_) | KeyCode::Backspace | KeyCode::Delete
            )
        {
            self.update_hits(false)?;
        }
        Ok(())
    }
    fn send_input(&mut self) -> Result<()> {
        let text = self.input.take();
        self.submit(text)?;
        self.save_draft()
    }
    fn submit(&mut self, text: String) -> Result<()> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(());
        }
        if let Some(target) = text.strip_prefix("/to ") {
            let target = target.trim();
            if target == "group" {
                self.select_recipient(None)?;
            } else {
                self.select_recipient(Some(target.trim_start_matches('@').to_owned()))?;
            }
            return Ok(());
        }
        if text == "/chats" {
            self.sidebar_offset = None;
            self.view_archived = false;
            return Ok(());
        }
        if text == "/archived" {
            self.sidebar_offset = None;
            self.view_archived = true;
            return Ok(());
        }
        if text == "/pin" || text == "/unpin" {
            self.store
                .set_pinned(&self.name, &self.room, text == "/pin")?;
            self.refresh()?;
            self.status = "Pin preference saved for this user".into();
            return Ok(());
        }
        if text == "/archive" || text == "/restore" {
            let archived = text == "/archive";
            self.store.set_archived(&self.name, &self.room, archived)?;
            self.view_archived = archived;
            self.refresh()?;
            self.status = "Archive preference saved · messages and drafts kept".into();
            return Ok(());
        }
        if text == "/welcome" {
            self.request_welcome()?;
            return Ok(());
        }
        if text == "/browser" {
            self.open_dialog(Dialog::BrowserDetails);
            return Ok(());
        }
        if text == "/skills" {
            self.rescan_skills();
            self.open_dialog(Dialog::Skills);
            return Ok(());
        }
        if let Some(rest) = text.strip_prefix("/skills ") {
            return self.skills_command(rest.trim());
        }
        if text == "/network" {
            self.open_dialog(Dialog::Network);
            return Ok(());
        }
        if text == "/profile" || text == "/bots" {
            self.open_dialog(if text == "/profile" {
                Dialog::Profile
            } else {
                Dialog::Bots
            });
            return Ok(());
        }
        if text == "/help" {
            self.open_dialog(Dialog::Help);
            return Ok(());
        }
        if text == "/agent" {
            return self.switch_room(format!("agent:{}", self.name));
        }
        if text == "/stop" {
            return self.stop_agent();
        }
        if text == "/people" {
            self.open_dialog(Dialog::Participants);
            return Ok(());
        }
        if text == "/tasks" {
            self.open_dialog(Dialog::Tasks);
            return Ok(());
        }
        if text == "/model" {
            self.open_dialog(Dialog::ModelInfo);
            return Ok(());
        }
        if text == "/retry" {
            return self.retry();
        }
        if let Some(room) = text.strip_prefix("/join ") {
            let room = room.trim();
            if valid_room(room) {
                self.switch_room(room.into())?;
            } else {
                self.status =
                    "Use a group name of 1–64 printable characters; agent: is reserved".into();
            }
            return Ok(());
        }
        if text.starts_with('/') {
            self.status = "Unknown command · F1 opens help".into();
            return Ok(());
        }
        let explicit = self.requested_bot(text);
        let handle = explicit
            .as_ref()
            .map(|p| p.handle.clone())
            .or_else(|| self.recipient_handle().map(str::to_owned));
        if let Some(handle) = &handle
            && !self.available_recipient(handle)
        {
            self.input = Editor::new(text.into());
            self.status = if explicit.is_some() {
                "Bot is not enabled here · F8 opens Bots · F10 changes recipient".into()
            } else {
                format!("@{handle} is unavailable here · F8 manages bots; F10 changes recipient")
            };
            return Ok(());
        }
        let invoke = handle.is_some();
        if invoke && self.job.is_some() {
            self.input = Editor::new(text.into());
            self.status = "Your agent is busy; your draft is kept here".into();
            return Ok(());
        }
        // A leading `$name` addressed to your agent must name an enabled skill; never
        // send it as if the skill were active.
        if invoke && let Some((_, selection)) = skill_selection(text) {
            self.rescan_skills();
            if let Err(error) = self.skills.resolve(selection.name) {
                self.input = Editor::new(text.into());
                self.status = format!("{error} · /skills lists skills; your draft is kept");
                return Ok(());
            }
        }
        self.latest()?;
        let routed = if explicit.is_none()
            && let Some(handle) = &handle
        {
            if self.room.starts_with("agent:") && handle == "bot" {
                text.to_owned()
            } else {
                format!("@{handle} {text}")
            }
        } else {
            text.to_owned()
        };
        let tx = self.store.connection().unchecked_transaction()?;
        self.store.add(&self.room, &self.name, &routed, "human")?;
        if let Some(bot) = &explicit {
            self.store
                .set_recipient(&self.name, &self.room, Some(&bot.handle))?;
        }
        tx.commit()?;
        if let Some(bot) = explicit {
            self.recipient = Some(bot.handle);
        }
        if invoke {
            self.start_agent()?;
        } else if let Some(shared) = self.shared_bot_mention(text) {
            self.status = format!(
                "Sent to {} · it answers only if its owner lends it (chat only)",
                safe(&shared.author())
            );
        } else {
            self.status = if self.network.is_some() {
                "Message queued · F9 shows hub status".into()
            } else {
                "Message saved · shared with local sessions in this group".into()
            };
        }
        self.refresh()?;
        Ok(())
    }
    /// Installs the bundled skills into the data directory and discovers all roots.
    fn init_skills(&mut self, disabled: Vec<String>, settings_path: Option<PathBuf>) {
        self.skills_error = skills::install_bundled(&skills::bundled_dir(&self.data_dir))
            .err()
            .map(|e| format!("Bundled skills could not be installed: {e}"))
            .unwrap_or_default();
        self.skill_roots = skills::default_roots(&self.data_dir, &self.config.workspace);
        self.disabled_skills = disabled;
        self.settings_path = settings_path;
        self.rescan_skills();
    }
    /// Discovery is bounded and cheap, so it runs again before each owner request
    /// and when /skills opens; edits on disk apply without a restart.
    fn rescan_skills(&mut self) {
        self.skills = skills::discover(
            &self.skill_roots,
            &self.disabled_skills,
            &skills::Limits::default(),
        );
    }
    fn skills_command(&mut self, args: &str) -> Result<()> {
        let (verb, name) = args.split_once(' ').unwrap_or((args, ""));
        let name = name.trim().trim_start_matches('$');
        let disable = match verb {
            "disable" => true,
            "enable" => false,
            _ => {
                self.status = "Use /skills, /skills disable NAME or /skills enable NAME".into();
                return Ok(());
            }
        };
        if skills::validate_name(name, skills::Limits::default().max_name_chars).is_err() {
            self.status = "Name a skill: lowercase letters, digits and '-'".into();
            return Ok(());
        }
        let mut list = self.disabled_skills.clone();
        list.retain(|n| n != name);
        if disable {
            list.push(name.to_owned());
            list.sort();
        }
        // Persist into the existing settings.json only; CLI overrides are never written.
        let mut saved = false;
        if let Some(path) = &self.settings_path
            && let Some(mut current) = settings::Settings::load(path)?
        {
            current.disabled_skills = list.clone();
            current.save(path)?;
            saved = true;
        }
        self.disabled_skills = list;
        self.rescan_skills();
        let known =
            self.skills.get(name).is_some() || self.skills.disabled.iter().any(|m| m.name == name);
        self.status = format!(
            "Skill {name} {}{}{}",
            if disable { "disabled" } else { "enabled" },
            if known {
                ""
            } else {
                " · not found in any root yet"
            },
            if saved {
                ""
            } else {
                " · this session only (no settings.json)"
            }
        );
        Ok(())
    }
    /// Plain-text body of the /skills window.
    fn skills_report(&self) -> String {
        let mut out = String::from(
            "Skills are instructions only. They add no tools or permissions; every action still needs your approval. Start a request to your agent with $name to use one. Guests of lent bots never get skills.\n\nRoots (highest priority first)\n",
        );
        for root in &self.skills.roots {
            let status = match &root.status {
                skills::RootStatus::Missing => "not created".to_owned(),
                skills::RootStatus::Unusable => "unusable".to_owned(),
                skills::RootStatus::Scanned { entries } => format!("{entries} entries"),
            };
            out.push_str(&format!(
                "  {} · {} · {status}\n",
                root.label,
                root.path.display()
            ));
        }
        let list = |out: &mut String, title: &str, items: &[skills::SkillMeta]| {
            out.push_str(&format!("\n{title} ({})\n", items.len()));
            if items.is_empty() {
                out.push_str("  none\n");
            }
            for m in items {
                out.push_str(&format!(
                    "  ${} · {}{}\n    {}\n",
                    m.name,
                    m.root,
                    m.version().map(|v| format!(" · v{v}")).unwrap_or_default(),
                    m.description
                ));
            }
        };
        list(&mut out, "Enabled", &self.skills.skills);
        list(&mut out, "Disabled", &self.skills.disabled);
        out.push_str(&format!(
            "\nDiagnostics ({})\n",
            self.skills.diagnostics.len()
        ));
        if !self.skills_error.is_empty() {
            out.push_str(&format!("  {}\n", self.skills_error));
        }
        if self.skills.diagnostics.is_empty() && self.skills_error.is_empty() {
            out.push_str("  none\n");
        }
        for d in &self.skills.diagnostics {
            out.push_str(&format!(
                "  {}{}: {}\n",
                d.root
                    .as_deref()
                    .map(|r| format!("{r} · "))
                    .unwrap_or_default(),
                d.path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "settings".into()),
                d.kind
            ));
        }
        out.push_str("\n/skills disable NAME · /skills enable NAME (saved in settings.json)");
        safe(&out)
    }
    fn start_agent(&mut self) -> Result<()> {
        let history = self.store.history(&self.room)?;
        let request = history
            .iter()
            .rev()
            .find(|m| m.kind == "human" && m.author == self.name)
            .map(|m| m.body.as_str())
            .unwrap_or("Agent request");
        let bot = self
            .requested_bot(request)
            .unwrap_or_else(|| self.personal_bot());
        if bot.handle != "bot" && !self.enabled_bots.contains(&bot.handle) {
            self.status = "Add this bot to the chat with F8 before retrying".into();
            return Ok(());
        }
        let author = bot.author();
        let mut config = self.config.clone();
        config.author = author.clone();
        config.instructions = bot.instructions.clone();
        if !bot.model.is_empty() {
            config.model = bot.model.clone();
        }
        self.rescan_skills();
        config.skills_prompt = skills::prompt_block(&self.skills.skills, SKILLS_PROMPT_BUDGET);
        config.skill = None;
        let mut skill_note = None;
        if let Some((prefix, selection)) = skill_selection(request) {
            match self.skills.load(selection.name, &skills::Limits::default()) {
                Ok(loaded) => {
                    let rest = if selection.rest.is_empty() {
                        "Use this skill."
                    } else {
                        selection.rest
                    };
                    skill_note = Some(format!(
                        "Skill {} · {} · instructions only; tools and approvals unchanged",
                        loaded.meta.name, loaded.meta.root
                    ));
                    config.skill = Some(agent::ActiveSkill {
                        name: loaded.meta.name.clone(),
                        instructions: loaded.prompt_block(),
                        request: format!("{prefix}{rest}"),
                    });
                }
                Err(error) => {
                    self.failed_room = Some(self.room.clone());
                    self.status = format!("Skill not used: {error} · /skills shows details");
                    return Ok(());
                }
            }
        }
        let skill = config.skill.as_ref().map(|s| s.name.clone());
        if let Some(note) = skill_note {
            self.store.add(&self.room, "tools", &note, "tool")?;
        }
        let tx = self.store.connection().unchecked_transaction()?;
        let id = tasks::begin(&tx, &self.name, &self.room, &self.session, request)?;
        tx.execute(
            "INSERT INTO task_bots(task_id,author) VALUES(?1,?2)",
            rusqlite::params![id, author],
        )?;
        tx.commit()?;
        let model = config.model.clone();
        config.request_id = format!("{}:{id}", self.session);
        config.requester = self.name.clone();
        let task = agent::start(config, history);
        self.job = Some(Job {
            author,
            model,
            task_id: Some(id),
            saved_partial: String::new(),
            control: task.control,
            phase: agent::Phase::Thinking,
            stopping: false,
            ran_command: false,
            partial: String::new(),
            room: self.room.clone(),
            events: task.events,
            requester: None,
            skill: skill.clone(),
        });
        self.failed_room = None;
        self.status = match skill {
            Some(name) => format!("Your agent is thinking… · skill {name}"),
            None => "Your agent is thinking…".into(),
        };
        Ok(())
    }
    /// Owner side of bot lending: answer new mentions of this account's lent bots that other
    /// accounts post in shared groups. Tools are off unless the owner enabled guest shell.
    fn poll_delegated(&mut self) -> Result<()> {
        if self.network.is_none() || self.quitting {
            return Ok(());
        }
        let rows = {
            let mut q = self.store.connection().prepare(
                "SELECT id,room,author,body,kind,created_at FROM messages WHERE id>?1 ORDER BY id LIMIT 100",
            )?;
            q.query_map([self.delegate_after], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, room, author, body, kind, created) in rows {
            self.delegate_after = id;
            // Only fresh human messages from other accounts in shared groups. Bot, tool and
            // system messages and this account's own messages never trigger a delegated run.
            if kind != "human"
                || author == self.name
                || room.starts_with("agent:")
                || created < self.delegate_since
            {
                continue;
            }
            let Some(handle) = body
                .split_whitespace()
                .next()
                .and_then(|w| w.strip_prefix('@'))
                .filter(|h| profiles::valid_handle(h) && *h != "bot")
            else {
                continue;
            };
            let Some(bot) = self
                .profiles
                .iter()
                .find(|p| p.owner == self.name && p.handle == handle)
                .cloned()
            else {
                continue;
            };
            let member = |owner: &str| -> Result<bool> {
                Ok(self
                    .store
                    .connection()
                    .prepare("SELECT 1 FROM room_bots WHERE owner=?1 AND room=?2 AND handle=?3")?
                    .exists(rusqlite::params![owner, room, handle])?)
            };
            // The bot must be in this group, and the requester's own bot of the same handle
            // takes precedence over a lent one.
            if !member(&self.name)? || member(&author)? {
                continue;
            }
            // Dedup by message id: each mention is decided once, even across restarts.
            if self.store.connection().execute(
                "INSERT OR IGNORE INTO delegate_requests(message_id,owner,handle,requester,room,outcome) VALUES(?1,?2,?3,?4,?5,'pending')",
                rusqlite::params![id, self.name, handle, author, room],
            )? == 0
            {
                continue;
            }
            let used: i64 = self.store.connection().query_row(
                "SELECT COUNT(*) FROM delegate_requests WHERE owner=?1 AND handle=?2 AND requester=?3 AND outcome='started' AND at>=datetime('now','-1 hour')",
                rusqlite::params![self.name, handle, author],
                |r| r.get(0),
            )?;
            let who = format!("{} → @{} in {}", safe(&author), handle, chat_name(&room));
            let outcome = if !bot.lend_chat {
                self.status = format!("{who}: lending is off (F8 → E to lend, chat only)");
                "not_lent"
            } else if self.job.is_some() {
                self.status = format!("{who}: skipped, your agent is busy");
                "busy"
            } else if used >= bot.lend_per_hour {
                self.status = format!(
                    "{who}: skipped, limit of {} guest requests per hour reached (F8 → E to change)",
                    bot.lend_per_hour
                );
                "quota"
            } else {
                self.start_delegated(&bot, &room, &author, &body, id)?;
                "started"
            };
            self.store.connection().execute(
                "UPDATE delegate_requests SET outcome=?2 WHERE message_id=?1",
                rusqlite::params![id, outcome],
            )?;
        }
        Ok(())
    }
    fn start_delegated(
        &mut self,
        bot: &profiles::Profile,
        room: &str,
        requester: &str,
        request: &str,
        message_id: i64,
    ) -> Result<()> {
        // Context is this group's history up to the request only: never agent:<owner> or
        // other rooms. Instructions travel only as the system prompt, as for the owner.
        let history = self.store.page_before(room, message_id + 1, 200)?;
        let author = bot.author();
        let mut config = self.config.clone();
        config.author = author.clone();
        config.instructions = bot.instructions.clone();
        config.guest = None;
        config.tools = false;
        // Skills are owner-only in this version: guests never get metadata or instructions.
        config.skills_prompt = String::new();
        config.skill = None;
        if bot.guest_tools.shell() {
            let home = paths::prepare_guest_home(&self.data_dir, requester)?;
            config.tools = true;
            config.workspace = home.clone();
            config.guest = Some(agent::GuestContext {
                requester: requester.into(),
                home,
            });
        }
        if !bot.model.is_empty() {
            config.model = bot.model.clone();
        }
        let tx = self.store.connection().unchecked_transaction()?;
        let id = tasks::begin_for(&tx, &self.name, room, &self.session, request, requester)?;
        tx.execute(
            "INSERT INTO task_bots(task_id,author) VALUES(?1,?2)",
            rusqlite::params![id, author],
        )?;
        tx.commit()?;
        let model = config.model.clone();
        config.request_id = format!("{}:{id}", self.session);
        config.requester = requester.into();
        let task = agent::start(config, history);
        self.job = Some(Job {
            author: author.clone(),
            model,
            task_id: Some(id),
            saved_partial: String::new(),
            control: task.control,
            phase: agent::Phase::Thinking,
            stopping: false,
            ran_command: false,
            partial: String::new(),
            room: room.into(),
            events: task.events,
            requester: Some(requester.into()),
            skill: None,
        });
        self.status = if bot.guest_tools.shell() {
            format!(
                "{} is answering {} in {} · guest shell, needs your approval · Ctrl+X stops",
                safe(&author),
                safe(requester),
                chat_name(room)
            )
        } else {
            format!(
                "{} is answering {} in {} · chat only, tools off · Ctrl+X stops",
                safe(&author),
                safe(requester),
                chat_name(room)
            )
        };
        Ok(())
    }
    fn retry(&mut self) -> Result<()> {
        if self.job.is_some() {
            self.status = "Your agent is still working".into();
        } else if self.failed_room.as_ref() == Some(&self.room) {
            self.start_agent()?;
        } else {
            self.status = "Open a chat with a failed agent request to retry".into();
        }
        Ok(())
    }
    fn poll_job(&mut self) -> Result<()> {
        let mut finished = false;
        if let Some(job) = &mut self.job {
            loop {
                match job.events.try_recv() {
                    Ok(agent::Event::Phase(phase)) => {
                        job.phase = phase;
                        job.ran_command |= phase == agent::Phase::RunningCommand;
                        if let Some(id) = job.task_id {
                            tasks::update(
                                self.store.connection(),
                                id,
                                if job.stopping {
                                    "stopping"
                                } else {
                                    phase_name(phase)
                                },
                                &job.partial,
                                "",
                            )?;
                        }
                    }
                    Ok(agent::Event::Cancelled) => {
                        let note = if job.ran_command {
                            "Agent response stopped. Any already-approved command was allowed to finish; completed effects are not undone. Background jobs may continue."
                        } else {
                            "Agent request stopped. Pending command approvals were cancelled."
                        };
                        record_end(&self.store, job, &self.name, "cancelled", "", note, None)?;
                        self.status = format!("Agent stopped in {}", chat_name(&job.room));
                        self.failed_room = None;
                        finished = true;
                        break;
                    }
                    Ok(agent::Event::Turn) => job.partial.clear(),
                    Ok(agent::Event::Delta(delta)) => job.partial.push_str(&delta),
                    Ok(agent::Event::Note(note)) => {
                        self.store.add(&job.room, "tools", &note, "tool")?
                    }
                    Ok(agent::Event::Approval {
                        command,
                        answer,
                        browser,
                        guest,
                    }) => {
                        // Stopping cancels pending approval. Guest browser proposals are
                        // refused here as well (the agent should already have blocked them).
                        // Guest shell commands require the owner's approval on this device.
                        if job.stopping || browser && job.requester.is_some() {
                            drop(answer);
                            continue;
                        }
                        if let Some(id) = job.task_id {
                            tasks::proposed(self.store.connection(), id, &command)?;
                        }
                        self.status = if let Some(guest) = &guest {
                            format!(
                                "Approval needed for guest {} in {}",
                                safe(&guest.requester),
                                chat_name(&job.room)
                            )
                        } else {
                            format!("Approval needed in {}", chat_name(&job.room))
                        };
                        self.approval = Some(Approval {
                            browser,
                            command,
                            answer,
                            scroll: 0,
                            guest,
                        });
                    }
                    Ok(agent::Event::Finished(result)) => {
                        match result {
                            Ok(reply) => {
                                record_end(
                                    &self.store,
                                    job,
                                    &self.name,
                                    "completed",
                                    "",
                                    "",
                                    Some(&reply),
                                )?;
                                if job.model == self.config.model {
                                    self.health = Some(true);
                                    self.health_detail =
                                        "Selected model responded successfully.".into();
                                }
                                self.status = "Agent replied".into();
                            }
                            Err(error) => {
                                let error = format!("{error:#}");
                                if job.requester.is_some() {
                                    // Guests see a generic note; details (endpoint, model)
                                    // stay in the owner's local task record.
                                    record_end(
                                        &self.store,
                                        job,
                                        &self.name,
                                        "failed",
                                        &error,
                                        &format!("{} could not answer this request.", job.author),
                                        None,
                                    )?;
                                    self.status =
                                        "Delegated request failed · details in F5 Activity".into();
                                } else {
                                    record_end(
                                        &self.store,
                                        job,
                                        &self.name,
                                        "failed",
                                        &error,
                                        &format!("Agent error: {error}"),
                                        None,
                                    )?;
                                    self.failed_room = Some(job.room.clone());
                                    self.status =
                                        "Agent request failed · Ctrl+R retries in its chat".into();
                                }
                            }
                        }
                        finished = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let delegated = job.requester.is_some();
                        record_end(
                            &self.store,
                            job,
                            &self.name,
                            "failed",
                            "Worker disconnected",
                            if delegated {
                                "Agent worker stopped unexpectedly."
                            } else {
                                "Agent worker stopped unexpectedly. Ctrl+R retries in its chat."
                            },
                            None,
                        )?;
                        if !delegated {
                            self.failed_room = Some(job.room.clone());
                        }
                        finished = true;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                }
            }
            if !finished && job.partial != job.saved_partial {
                if let Some(id) = job.task_id {
                    tasks::update(
                        self.store.connection(),
                        id,
                        if job.stopping {
                            "stopping"
                        } else {
                            phase_name(job.phase)
                        },
                        &job.partial,
                        "",
                    )?;
                }
                job.saved_partial = job.partial.clone();
            }
        }
        if finished {
            self.job = None;
            self.approval = None;
        }
        Ok(())
    }
}
fn phase_name(phase: agent::Phase) -> &'static str {
    match phase {
        agent::Phase::Thinking => "thinking",
        agent::Phase::AwaitingApproval => "awaiting_approval",
        agent::Phase::RunningCommand => "running",
    }
}
fn record_end(
    store: &Store,
    job: &Job,
    owner: &str,
    state: &str,
    error: &str,
    note: &str,
    reply: Option<&str>,
) -> Result<()> {
    let text = reply.unwrap_or(&job.partial);
    let mut posts = vec![];
    if !text.is_empty() {
        posts.push((
            if job.author.is_empty() {
                format!("{owner}'s bot")
            } else {
                job.author.clone()
            },
            text.to_owned(),
            if state == "completed" {
                "bot"
            } else {
                "interrupted"
            }
            .into(),
        ));
    }
    if !note.is_empty() {
        posts.push(("system".into(), note.into(), "system".into()));
    }
    if let Some(id) = job.task_id {
        tasks::finish(store.connection(), id, state, text, error, &posts)?;
    } else {
        for (author, body, kind) in posts {
            store.add(&job.room, &author, &body, &kind)?;
        }
    }
    Ok(())
}

/// Explicit skill selection in an agent request: `$name …`, optionally after a
/// leading `@handle`. Returns that `@handle ` prefix and the parsed selection.
fn skill_selection(text: &str) -> Option<(String, skills::Selection<'_>)> {
    let text = text.trim_start();
    let (prefix, body) = match text.strip_prefix('@') {
        Some(after) => {
            let end = after.find(char::is_whitespace)?;
            (format!("@{} ", &after[..end]), &after[end..])
        }
        None => (String::new(), text),
    };
    Some((prefix, skills::parse_selection(body)?))
}
fn chat_name(room: &str) -> String {
    if room.starts_with("agent:") {
        "My agent".into()
    } else {
        format!("# {}", safe(room))
    }
}
fn safe(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}
#[cfg(test)]
mod tests;
