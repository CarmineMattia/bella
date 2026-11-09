use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct TaskRecord {
    pub author: String,
    pub id: i64,
    pub room: String,
    pub request: String,
    pub state: String,
    pub started: String,
    pub command: String,
    pub approval: String,
    pub error: String,
    pub partial: String,
    /// Empty for the owner's own requests; otherwise the account that mentioned a lent bot.
    pub requester: String,
}
const ACTIVE: &str = "('thinking','awaiting_approval','running','stopping')";
fn identity(pid: u32) -> std::io::Result<String> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
    Ok(stat
        .rsplit_once(')')
        .and_then(|(_, tail)| tail.split_whitespace().nth(19))
        .unwrap_or("")
        .to_owned())
}
fn alive(pid: u32, saved: &str) -> bool {
    match identity(pid) {
        Ok(actual) => actual == saved,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
    }
}
pub fn schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS app_sessions(id TEXT PRIMARY KEY,pid INTEGER NOT NULL,identity TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS local_presence(session TEXT PRIMARY KEY,owner TEXT NOT NULL,room TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS agent_tasks (
          id INTEGER PRIMARY KEY,owner TEXT NOT NULL,room TEXT NOT NULL,session TEXT NOT NULL,
          request TEXT NOT NULL,state TEXT NOT NULL,partial TEXT NOT NULL DEFAULT '',command TEXT NOT NULL DEFAULT '',
          approval TEXT NOT NULL DEFAULT 'none',error TEXT NOT NULL DEFAULT '',
          started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        CREATE INDEX IF NOT EXISTS agent_tasks_owner ON agent_tasks(owner,id);")?;
    // Additive migration: older databases gain the delegated-requester column, defaulting to ''.
    let has_requester = db
        .prepare("SELECT 1 FROM pragma_table_info('agent_tasks') WHERE name='requester'")?
        .exists([])?;
    if !has_requester {
        db.execute_batch("ALTER TABLE agent_tasks ADD COLUMN requester TEXT NOT NULL DEFAULT '';")?;
    }
    Ok(())
}
pub fn session(db: &Connection) -> Result<String> {
    recover(db)?;
    let pid = std::process::id();
    let id = format!(
        "{pid}-{}",
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    db.execute(
        "INSERT INTO app_sessions(id,pid,identity) VALUES(?1,?2,?3)",
        params![
            id,
            pid,
            identity(pid).context("Linux process identity is unavailable")?
        ],
    )?;
    Ok(id)
}
fn recover(db: &Connection) -> Result<()> {
    let mut query=db.prepare(&format!("SELECT DISTINCT s.id,s.pid,s.identity FROM app_sessions s JOIN agent_tasks t ON t.session=s.id WHERE t.state IN {ACTIVE}"))?;
    let sessions = query
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(query);
    for (session, pid, saved) in sessions {
        if alive(pid, &saved) {
            continue;
        }
        let tx = db.unchecked_transaction()?;
        let rows = {
            let mut q=tx.prepare(&format!("SELECT id,owner,room,partial FROM agent_tasks WHERE session=?1 AND state IN {ACTIVE}"))?;
            q.query_map([&session], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, owner, room, partial) in rows {
            tx.execute("UPDATE agent_tasks SET state='interrupted',error='Session ended unexpectedly; task was not replayed',approval=CASE WHEN approval='pending' THEN 'cancelled' ELSE approval END,updated_at=CURRENT_TIMESTAMP WHERE id=?1",[id])?;
            if !partial.is_empty() {
                tx.execute(
                    "INSERT INTO messages(room,author,body,kind) VALUES (?1,?2,?3,'interrupted')",
                    params![
                        room,
                        tx.query_row("SELECT author FROM task_bots WHERE task_id=?1", [id], |r| r
                            .get::<_, String>(0))
                            .unwrap_or_else(|_| format!("{owner}'s bot")),
                        partial
                    ],
                )?;
            }
            tx.execute("INSERT INTO messages(room,author,body,kind) VALUES (?1,'system',?2,'system')",params![room,format!("{}'s task #{id} was interrupted by an unexpected session exit. It was not replayed. An approved command or background job may still be running; check its effects before retrying.",owner)])?;
        }
        tx.commit()?;
    }
    Ok(())
}
pub fn begin(
    db: &Connection,
    owner: &str,
    room: &str,
    session: &str,
    request: &str,
) -> Result<i64> {
    begin_for(db, owner, room, session, request, "")
}
/// Start a task; `requester` is non-empty for a delegated (guest) chat-only request.
pub fn begin_for(
    db: &Connection,
    owner: &str,
    room: &str,
    session: &str,
    request: &str,
    requester: &str,
) -> Result<i64> {
    db.execute(
        "INSERT INTO agent_tasks(owner,room,session,request,state,requester) VALUES(?1,?2,?3,?4,'thinking',?5)",
        params![owner, room, session, request, requester],
    )?;
    Ok(db.last_insert_rowid())
}
pub fn update(db: &Connection, id: i64, state: &str, partial: &str, error: &str) -> Result<()> {
    db.execute("UPDATE agent_tasks SET state=?2,partial=?3,error=?4,approval=CASE WHEN ?2 IN ('cancelled','interrupted') AND approval='pending' THEN 'cancelled' ELSE approval END,updated_at=CURRENT_TIMESTAMP WHERE id=?1",params![id,state,partial,error])?;
    Ok(())
}
pub fn finish(
    db: &Connection,
    id: i64,
    state: &str,
    partial: &str,
    error: &str,
    posts: &[(String, String, String)],
) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    let room: String = tx.query_row("SELECT room FROM agent_tasks WHERE id=?1", [id], |r| {
        r.get(0)
    })?;
    update(&tx, id, state, partial, error)?;
    for (author, body, kind) in posts {
        tx.execute(
            "INSERT INTO messages(room,author,body,kind) VALUES(?1,?2,?3,?4)",
            params![room, author, body, kind],
        )?;
    }
    tx.commit()?;
    Ok(())
}
pub fn proposed(db: &Connection, id: i64, command: &str) -> Result<()> {
    db.execute("UPDATE agent_tasks SET command=?2,approval='pending',state='awaiting_approval',updated_at=CURRENT_TIMESTAMP WHERE id=?1",params![id,command])?;
    Ok(())
}
pub fn decision(db: &Connection, id: i64, approved: bool) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    let (owner, room, command) = tx.query_row(
        "SELECT owner,room,command FROM agent_tasks WHERE id=?1",
        [id],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    )?;
    tx.execute(
        "UPDATE agent_tasks SET approval=?2,state=?3,updated_at=CURRENT_TIMESTAMP WHERE id=?1",
        params![
            id,
            if approved { "approved" } else { "denied" },
            if approved { "running" } else { "thinking" }
        ],
    )?;
    tx.execute(
        "INSERT INTO messages(room,author,body,kind) VALUES(?1,'tools',?2,'tool')",
        params![
            room,
            format!(
                "{owner} {} shell: {command}{}",
                if approved { "approved" } else { "denied" },
                if approved {
                    "\nApproved for execution; result will follow."
                } else {
                    ""
                }
            )
        ],
    )?;
    tx.commit()?;
    Ok(())
}
pub fn list(db: &Connection, owner: &str) -> Result<Vec<TaskRecord>> {
    let mut q=db.prepare("SELECT id,room,request,state,started_at,command,approval,error,partial,COALESCE((SELECT author FROM task_bots WHERE task_id=agent_tasks.id),owner || char(39) || 's bot'),requester FROM agent_tasks WHERE owner=?1 ORDER BY id DESC LIMIT 50")?;
    Ok(q.query_map([owner], |r| {
        Ok(TaskRecord {
            id: r.get(0)?,
            room: r.get(1)?,
            request: r.get(2)?,
            state: r.get(3)?,
            started: r.get(4)?,
            command: r.get(5)?,
            approval: r.get(6)?,
            error: r.get(7)?,
            partial: r.get(8)?,
            author: r.get(9)?,
            requester: r.get(10)?,
        })
    })?
    .collect::<rusqlite::Result<_>>()?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completion_and_chat_reply_commit_or_rollback_together() -> Result<()> {
        let store = crate::store::Store::open(std::path::Path::new(":memory:"))?;
        let db = store.connection();
        let session = session(db)?;
        let id = begin(db, "alice", "room", &session, "hello")?;
        let posts = vec![("alice's bot".into(), "answer".into(), "bot".into())];
        db.execute_batch("CREATE TRIGGER fail_reply BEFORE INSERT ON messages BEGIN SELECT RAISE(FAIL,'fixture'); END;")?;
        assert!(finish(db, id, "completed", "answer", "", &posts).is_err());
        assert_eq!(list(db, "alice")?[0].state, "thinking");
        db.execute_batch("DROP TRIGGER fail_reply;")?;
        finish(db, id, "completed", "answer", "", &posts)?;
        assert_eq!(list(db, "alice")?[0].state, "completed");
        assert_eq!(store.history("room")?[0].body, "answer");
        Ok(())
    }
    #[test]
    fn requester_column_is_added_to_existing_databases() -> Result<()> {
        let path = std::env::temp_dir().join(format!(
            "term-social-requester-{}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let old = Connection::open(&path)?;
            old.execute_batch("CREATE TABLE agent_tasks (
              id INTEGER PRIMARY KEY,owner TEXT NOT NULL,room TEXT NOT NULL,session TEXT NOT NULL,
              request TEXT NOT NULL,state TEXT NOT NULL,partial TEXT NOT NULL DEFAULT '',command TEXT NOT NULL DEFAULT '',
              approval TEXT NOT NULL DEFAULT 'none',error TEXT NOT NULL DEFAULT '',
              started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
              INSERT INTO agent_tasks(owner,room,session,request,state) VALUES('alice','lobby','old','old request','completed');")?;
        }
        let result = (|| -> Result<()> {
            let store = crate::store::Store::open(&path)?;
            let db = store.connection();
            let id = begin_for(db, "alice", "lobby", "s", "@planner hi", "bob")?;
            let tasks = list(db, "alice")?;
            assert_eq!(tasks.iter().find(|t| t.id == id).unwrap().requester, "bob");
            assert_eq!(
                tasks
                    .iter()
                    .find(|t| t.request == "old request")
                    .unwrap()
                    .requester,
                ""
            );
            drop(store);
            // Reopening is idempotent.
            crate::store::Store::open(&path)?;
            Ok(())
        })();
        let _ = std::fs::remove_file(&path);
        result
    }
    #[test]
    fn presence_counts_only_live_sessions_in_the_room() -> Result<()> {
        let store = crate::store::Store::open(std::path::Path::new(":memory:"))?;
        let db = store.connection();
        let first = session(db)?;
        let second = session(db)?;
        presence(db, &first, "alice", "room")?;
        presence(db, &second, "alice", "room")?;
        store.add("room", "bob", "past message", "human")?;
        db.execute(
            "INSERT INTO app_sessions VALUES('gone',4294967295,'old')",
            [],
        )?;
        presence(db, "gone", "ghost", "room")?;
        let people = participants(db, "room")?;
        assert_eq!(people[0].name, "alice");
        assert_eq!(people[0].sessions, 2);
        assert!(!people.iter().any(|p| p.name == "ghost"));
        assert_eq!(people[1].sessions, 0);
        leave(db, &first)?;
        assert_eq!(participants(db, "room")?[0].sessions, 1);
        Ok(())
    }
    #[test]
    fn crash_recovery_preserves_partial_once_and_never_replays_or_interrupts_live_sessions()
    -> Result<()> {
        let store = crate::store::Store::open(std::path::Path::new(":memory:"))?;
        let db = store.connection();
        let live = session(db)?;
        let live_id = begin(db, "alice", "lobby", &live, "live request")?;
        session(db)?;
        assert_eq!(list(db, "alice")?[0].state, "thinking");
        db.execute(
            "INSERT INTO app_sessions VALUES('dead',4294967295,'old')",
            [],
        )?;
        let dead_id = begin(db, "alice", "lobby", "dead", "interrupted request")?;
        update(db, dead_id, "running", "partial text", "")?;
        proposed(db, dead_id, "printf old")?;
        decision(db, dead_id, true)?;
        session(db)?;
        session(db)?;
        let tasks = list(db, "alice")?;
        assert_eq!(tasks[0].state, "interrupted");
        assert_eq!(tasks[0].approval, "approved");
        assert_eq!(
            tasks.iter().find(|t| t.id == live_id).unwrap().state,
            "thinking"
        );
        assert_eq!(
            store
                .history("lobby")?
                .iter()
                .filter(|m| m.kind == "interrupted")
                .count(),
            1
        );
        assert!(list(db, "bob")?.is_empty());
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Participant {
    pub name: String,
    pub sessions: usize,
    pub last_message: String,
}
pub fn presence(db: &Connection, session: &str, owner: &str, room: &str) -> Result<()> {
    db.execute("INSERT INTO local_presence(session,owner,room) VALUES(?1,?2,?3) ON CONFLICT(session) DO UPDATE SET room=excluded.room",params![session,owner,room])?;
    Ok(())
}
pub fn leave(db: &Connection, session: &str) -> Result<()> {
    db.execute("DELETE FROM local_presence WHERE session=?1", [session])?;
    Ok(())
}
pub fn participants(db: &Connection, room: &str) -> Result<Vec<Participant>> {
    let mut names = std::collections::BTreeMap::<String, Participant>::new();
    let mut q=db.prepare("SELECT author,MAX(created_at) FROM messages WHERE room=?1 AND kind='human' GROUP BY author")?;
    for value in q.query_map([room], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })? {
        let (name, last_message) = value?;
        names.insert(
            name.clone(),
            Participant {
                name,
                sessions: 0,
                last_message,
            },
        );
    }
    let mut q=db.prepare("SELECT p.owner,s.pid,s.identity FROM local_presence p JOIN app_sessions s ON s.id=p.session WHERE p.room=?1")?;
    for value in q.query_map([room], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, u32>(1)?,
            r.get::<_, String>(2)?,
        ))
    })? {
        let (name, pid, saved) = value?;
        if alive(pid, &saved) {
            names
                .entry(name.clone())
                .or_insert(Participant {
                    name,
                    sessions: 0,
                    last_message: String::new(),
                })
                .sessions += 1;
        }
    }
    let mut people = names.into_values().collect::<Vec<_>>();
    people.sort_by(|a, b| b.sessions.cmp(&a.sessions).then(a.name.cmp(&b.name)));
    Ok(people)
}
