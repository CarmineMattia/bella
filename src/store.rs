use anyhow::Result;
use rusqlite::{Connection, params};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Message {
    pub id: i64,
    pub author: String,
    pub body: String,
    pub kind: String,
    pub created_at: String,
}
#[derive(Clone, Debug)]
pub struct Conversation {
    pub room: String,
    pub author: String,
    pub preview: String,
    pub time: String,
    pub unread: usize,
    pub pinned: bool,
    pub archived: bool,
}
pub struct Store(Connection);
impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        db.execute_batch(
            "PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS messages (
              id INTEGER PRIMARY KEY, room TEXT NOT NULL, author TEXT NOT NULL,
              body TEXT NOT NULL, kind TEXT NOT NULL,
              created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS messages_room ON messages(room, id);
            CREATE TABLE IF NOT EXISTS conversations (room TEXT PRIMARY KEY);
            INSERT OR IGNORE INTO conversations SELECT DISTINCT room FROM messages;
            CREATE TABLE IF NOT EXISTS chat_state (
              owner TEXT NOT NULL, room TEXT NOT NULL, last_read INTEGER NOT NULL DEFAULT 0,
              draft TEXT NOT NULL DEFAULT '', PRIMARY KEY(owner,room)
            );
            CREATE TABLE IF NOT EXISTS composer_recipients (
              owner TEXT NOT NULL, room TEXT NOT NULL, handle TEXT NOT NULL DEFAULT '', PRIMARY KEY(owner,room)
            );
            CREATE TABLE IF NOT EXISTS conversation_preferences (
              owner TEXT NOT NULL, room TEXT NOT NULL,
              pinned INTEGER NOT NULL DEFAULT 0 CHECK(pinned IN (0,1)),
              archived INTEGER NOT NULL DEFAULT 0 CHECK(archived IN (0,1)),
              PRIMARY KEY(owner,room)
            );",
        )?;
        crate::profiles::schema(&db)?;
        crate::tasks::schema(&db)?;
        Ok(Self(db))
    }
    pub fn connection(&self) -> &Connection {
        &self.0
    }
    pub fn integrity(&self) -> Result<String> {
        Ok(self
            .0
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))?)
    }
    pub fn backup(&self, path: &Path) -> Result<()> {
        if path.exists() {
            anyhow::bail!("Backup destination already exists; choose a new filename");
        }
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        self.0
            .execute("VACUUM INTO ?1", [path.to_string_lossy().as_ref()])?;
        Ok(())
    }
    pub fn ensure_room(&self, room: &str) -> Result<()> {
        self.0.execute(
            "INSERT OR IGNORE INTO conversations(room) VALUES (?1)",
            [room],
        )?;
        Ok(())
    }
    pub fn add(&self, room: &str, author: &str, body: &str, kind: &str) -> Result<()> {
        self.ensure_room(room)?;
        self.0.execute(
            "INSERT INTO messages(room,author,body,kind) VALUES (?1,?2,?3,?4)",
            params![room, author, body, kind],
        )?;
        Ok(())
    }
    pub fn conversations(&self, owner: &str) -> Result<Vec<Conversation>> {
        let mut stmt = self.0.prepare(
            "SELECT c.room, COALESCE(m.author,''), COALESCE(m.body,''), COALESCE(m.created_at,''),
            (SELECT COUNT(*) FROM messages u WHERE u.room=c.room AND u.id>COALESCE(s.last_read,0) AND u.author<>?1),
            COALESCE(p.pinned,0), COALESCE(p.archived,0)
            FROM conversations c
            LEFT JOIN messages m ON m.id=(SELECT MAX(id) FROM messages WHERE room=c.room)
            LEFT JOIN chat_state s ON s.room=c.room AND s.owner=?1
            LEFT JOIN conversation_preferences p ON p.room=c.room AND p.owner=?1
            WHERE substr(c.room,1,6)<>'agent:' OR c.room='agent:' || ?1
            ORDER BY COALESCE(p.pinned,0) DESC, COALESCE(m.id,0) DESC, c.room")?;
        Ok(stmt
            .query_map([owner], |r| {
                Ok(Conversation {
                    room: r.get(0)?,
                    author: r.get(1)?,
                    preview: r.get(2)?,
                    time: r.get(3)?,
                    unread: r.get::<_, u32>(4)? as usize,
                    pinned: r.get(5)?,
                    archived: r.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn recipient(&self, owner: &str, room: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .0
            .query_row(
                "SELECT handle FROM composer_recipients WHERE owner=?1 AND room=?2",
                params![owner, room],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .filter(|s| !s.is_empty()))
    }
    pub fn set_recipient(&self, owner: &str, room: &str, handle: Option<&str>) -> Result<()> {
        self.0.execute("INSERT INTO composer_recipients VALUES(?1,?2,?3) ON CONFLICT(owner,room) DO UPDATE SET handle=excluded.handle",params![owner,room,handle.unwrap_or("")])?;
        Ok(())
    }
    pub fn set_pinned(&self, owner: &str, room: &str, pinned: bool) -> Result<()> {
        self.ensure_room(room)?;
        self.0.execute(
            "INSERT INTO conversation_preferences(owner,room,pinned) VALUES (?1,?2,?3)
            ON CONFLICT(owner,room) DO UPDATE SET pinned=excluded.pinned",
            params![owner, room, pinned],
        )?;
        Ok(())
    }
    pub fn set_archived(&self, owner: &str, room: &str, archived: bool) -> Result<()> {
        self.ensure_room(room)?;
        self.0.execute(
            "INSERT INTO conversation_preferences(owner,room,archived) VALUES (?1,?2,?3)
            ON CONFLICT(owner,room) DO UPDATE SET archived=excluded.archived",
            params![owner, room, archived],
        )?;
        Ok(())
    }
    pub fn mark_read(&self, owner: &str, room: &str, through: i64) -> Result<()> {
        self.0.execute(
            "INSERT INTO chat_state(owner,room,last_read) VALUES (?1,?2,?3)
          ON CONFLICT(owner,room) DO UPDATE SET last_read=excluded.last_read WHERE excluded.last_read>chat_state.last_read",
            params![owner, room, through],
        )?;
        Ok(())
    }
    pub fn save_draft(&self, owner: &str, room: &str, text: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO chat_state(owner,room,draft) VALUES (?1,?2,?3)
          ON CONFLICT(owner,room) DO UPDATE SET draft=excluded.draft",
            params![owner, room, text],
        )?;
        Ok(())
    }
    pub fn draft(&self, owner: &str, room: &str) -> Result<String> {
        use rusqlite::OptionalExtension;
        Ok(self
            .0
            .query_row(
                "SELECT draft FROM chat_state WHERE owner=?1 AND room=?2",
                params![owner, room],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default())
    }
    fn message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
        Ok(Message {
            id: row.get(0)?,
            author: row.get(1)?,
            body: row.get(2)?,
            kind: row.get(3)?,
            created_at: row.get(4)?,
        })
    }
    pub fn history(&self, room: &str) -> Result<Vec<Message>> {
        self.page_before(room, i64::MAX, 200)
    }
    pub fn page_before(&self, room: &str, before: i64, limit: usize) -> Result<Vec<Message>> {
        let mut stmt = self.0.prepare("SELECT id,author,body,kind,created_at FROM
            (SELECT id,author,body,kind,created_at FROM messages WHERE room=?1 AND id<?2 ORDER BY id DESC LIMIT ?3) ORDER BY id")?;
        Ok(stmt
            .query_map(params![room, before, limit as i64], Self::message)?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn page_after(&self, room: &str, after: i64) -> Result<Vec<Message>> {
        let mut stmt = self.0.prepare("SELECT id,author,body,kind,created_at FROM messages WHERE room=?1 AND id>?2 ORDER BY id LIMIT 200")?;
        Ok(stmt
            .query_map(params![room, after], Self::message)?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn has_before(&self, room: &str, before: i64) -> Result<bool> {
        Ok(self.0.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE room=?1 AND id<?2)",
            params![room, before],
            |r| r.get(0),
        )?)
    }
    pub fn has_after(&self, room: &str, after: i64) -> Result<bool> {
        Ok(self.0.query_row(
            "SELECT EXISTS(SELECT 1 FROM messages WHERE room=?1 AND id>?2)",
            params![room, after],
            |r| r.get(0),
        )?)
    }
    pub fn message_room(&self, id: i64) -> Result<String> {
        Ok(self
            .0
            .query_row("SELECT room FROM messages WHERE id=?1", [id], |r| r.get(0))?)
    }
    pub fn search(&self, room: &str, query: &str, before: i64) -> Result<Vec<Message>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        // instr treats %, _ and backslashes literally, unlike LIKE. Search is scoped to the open chat.
        let mut stmt = self.0.prepare(
            "SELECT id,author,body,kind,created_at FROM messages
            WHERE room=?1 AND id<?3 AND instr(lower(body),lower(?2))>0 ORDER BY id DESC LIMIT 50",
        )?;
        Ok(stmt
            .query_map(params![room, query, before], Self::message)?
            .collect::<rusqlite::Result<_>>()?)
    }
    pub fn around(&self, room: &str, id: i64) -> Result<Vec<Message>> {
        let mut page = self.page_before(room, id, 75)?;
        let mut stmt = self.0.prepare("SELECT id,author,body,kind,created_at FROM messages WHERE room=?1 AND id>=?2 ORDER BY id LIMIT 125")?;
        page.extend(
            stmt.query_map(params![room, id], Self::message)?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        );
        Ok(page)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn backup_includes_wal_messages_and_preferences_and_refuses_overwrite() -> Result<()> {
        let root = std::env::temp_dir().join(format!("bella-backup-{}", std::process::id()));
        std::fs::create_dir_all(&root)?;
        let original = Store::open(&root.join("original.sqlite"))?;
        original.add("room", "alice", "saved in WAL", "human")?;
        original.set_pinned("alice", "room", true)?;
        original.backup(&root.join("backup.sqlite"))?;
        assert!(original.backup(&root.join("backup.sqlite")).is_err());
        let backup = Store::open(&root.join("backup.sqlite"))?;
        assert_eq!(backup.integrity()?, "ok");
        assert_eq!(backup.history("room")?[0].body, "saved in WAL");
        assert!(backup.conversations("alice")?[0].pinned);
        drop((original, backup));
        std::fs::remove_dir_all(root)?;
        Ok(())
    }
    #[test]
    fn pin_and_archive_preferences_are_per_user_and_persist_without_touching_chat_state()
    -> Result<()> {
        let path =
            std::env::temp_dir().join(format!("bella-preferences-{}.sqlite", std::process::id()));
        let store = Store::open(&path)?;
        store.add("older", "bob", "keep this old message", "human")?;
        store.add("newer", "bob", "new message", "human")?;
        store.save_draft("alice", "older", "keep my draft")?;
        store.set_pinned("alice", "older", true)?;
        store.set_archived("alice", "older", true)?;
        let alice = store.conversations("alice")?;
        assert_eq!(alice[0].room, "older");
        assert!(alice[0].pinned && alice[0].archived);
        assert_eq!(alice[0].unread, 1);
        assert!(
            store
                .conversations("bob")?
                .iter()
                .all(|c| !c.pinned && !c.archived)
        );
        drop(store);
        let reopened = Store::open(&path)?;
        let chat = reopened
            .conversations("alice")?
            .into_iter()
            .find(|c| c.room == "older")
            .unwrap();
        assert!(chat.pinned && chat.archived);
        assert_eq!(chat.unread, 1);
        assert_eq!(reopened.draft("alice", "older")?, "keep my draft");
        reopened.set_archived("alice", "older", false)?;
        assert!(reopened.conversations("alice")?[0].pinned);
        reopened.set_pinned("alice", "older", false)?;
        assert_eq!(reopened.conversations("alice")?[0].room, "newer");
        assert_eq!(reopened.history("older")?[0].body, "keep this old message");
        drop(reopened);
        std::fs::remove_file(path)?;
        Ok(())
    }
    #[test]
    fn upgrades_the_original_database_without_losing_messages() -> Result<()> {
        let path =
            std::env::temp_dir().join(format!("bella-migration-{}.sqlite", std::process::id()));
        let old = Connection::open(&path)?;
        old.execute_batch("CREATE TABLE messages (id INTEGER PRIMARY KEY, room TEXT NOT NULL, author TEXT NOT NULL, body TEXT NOT NULL, kind TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
          INSERT INTO messages VALUES (17,'existing','bob','keep this message','human','2026-10-23 18:00:00');")?;
        drop(old);
        let store = Store::open(&path)?;
        let messages = store.history("existing")?;
        assert_eq!(messages[0].id, 17);
        assert_eq!(messages[0].body, "keep this message");
        assert_eq!(messages[0].created_at, "2026-10-23 18:00:00");
        assert_eq!(store.conversations("alice")?[0].room, "existing");
        store.save_draft("alice", "existing", "new draft")?;
        drop(store);
        std::fs::remove_file(path)?;
        Ok(())
    }
    #[test]
    fn shared_messages_unread_drafts_and_empty_groups_survive_reopen() -> Result<()> {
        let path = std::env::temp_dir().join(format!("bella-{}.sqlite", std::process::id()));
        let a = Store::open(&path)?;
        let b = Store::open(&path)?;
        a.ensure_room("empty")?;
        a.add("group", "alice", "hello bob", "human")?;
        b.add("group", "bob", "hello alice", "human")?;
        a.add("agent:alice", "alice", "private context", "human")?;
        assert_eq!(b.history("group")?.len(), 2);
        assert!(
            !b.conversations("bob")?
                .iter()
                .any(|c| c.room == "agent:alice")
        );
        let chats = a.conversations("alice")?;
        assert_eq!(chats.iter().find(|c| c.room == "group").unwrap().unread, 1);
        let latest = a.history("group")?.last().unwrap().id;
        a.mark_read("alice", "group", latest)?;
        a.save_draft("alice", "group", "unfinished 界")?;
        drop((a, b));
        let c = Store::open(&path)?;
        assert_eq!(c.draft("alice", "group")?, "unfinished 界");
        assert_eq!(c.draft("bob", "group")?, "");
        assert!(c.conversations("alice")?.iter().any(|c| c.room == "empty"));
        assert_eq!(
            c.conversations("alice")?
                .iter()
                .find(|c| c.room == "group")
                .unwrap()
                .unread,
            0
        );
        c.mark_read("alice", "group", 0)?;
        assert_eq!(
            c.conversations("alice")?
                .iter()
                .find(|c| c.room == "group")
                .unwrap()
                .unread,
            0
        );
        drop(c);
        std::fs::remove_file(path)?;
        Ok(())
    }
}
