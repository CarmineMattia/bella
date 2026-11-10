use super::*;
use ratatui::{Terminal, backend::TestBackend};
fn app() -> Result<App> {
    app_with(Store::open(std::path::Path::new(":memory:"))?)
}
fn app_with(store: Store) -> Result<App> {
    App::new(
        "alice".into(),
        "lobby".into(),
        store,
        agent::Config {
            endpoint: "http://127.0.0.1:8080/v1".into(),
            model: "test".into(),
            workspace: PathBuf::from("."),
            author: String::new(),
            instructions: String::new(),
            tools: true,
            guest: None,
        },
    )
}
#[test]
fn switching_search_drafts_and_group_creation() -> Result<()> {
    let mut a = app()?;
    a.input = Editor::new("draft 界".into());
    a.store.add("workshop", "bob", "lets build", "human")?;
    a.refresh()?;
    assert_eq!(
        a.chats
            .iter()
            .find(|c| c.room == "workshop")
            .unwrap()
            .unread,
        1
    );
    a.switch_room("workshop".into())?;
    assert!(a.input.text.is_empty());
    assert_eq!(
        a.chats
            .iter()
            .find(|c| c.room == "workshop")
            .unwrap()
            .unread,
        0
    );
    a.switch_room("lobby".into())?;
    assert_eq!(a.input.text, "draft 界");
    a.open_dialog(Dialog::Search);
    a.query = Editor::new("build".into());
    assert_eq!(a.filtered_chats().len(), 1);
    a.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    assert_eq!(a.room, "workshop");
    a.open_dialog(Dialog::NewGroup);
    a.query = Editor::new("friends".into());
    a.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    assert_eq!(a.room, "friends");
    a.refresh()?;
    assert!(a.chats.iter().any(|c| c.room == "friends"));
    a.input = Editor::new("hello\nsecond line".into());
    a.send_input()?;
    assert_eq!(a.messages.last().unwrap().body, "hello\nsecond line");
    assert!(a.store.draft("alice", "friends")?.is_empty());
    Ok(())
}
#[test]
fn layout_survives_tiny_sizes_and_dialogs() -> Result<()> {
    let mut a = app()?;
    a.store
        .add("lobby", "bob", &"Hello 界! ".repeat(30), "human")?;
    a.store.add("lobby", "alice", "Nice to see you", "human")?;
    a.refresh()?;
    for (w, h) in [
        (140, 44),
        (100, 30),
        (80, 24),
        (70, 20),
        (30, 10),
        (10, 4),
        (1, 1),
    ] {
        let mut t = Terminal::new(TestBackend::new(w, h))?;
        t.draw(|f| ui::draw(&mut a, f))?;
        for dialog in [
            Dialog::Search,
            Dialog::FindMessages,
            Dialog::NewGroup,
            Dialog::Help,
            Dialog::ModelInfo,
            Dialog::ChatActions,
            Dialog::Tasks,
            Dialog::TaskDetails,
            Dialog::BrowserDetails,
            Dialog::Participants,
            Dialog::Network,
            Dialog::Profile,
            Dialog::Bots,
            Dialog::BotProfile,
        ] {
            a.open_dialog(dialog);
            t.draw(|f| ui::draw(&mut a, f))?;
        }
        let (tx, _) = mpsc::channel();
        a.approval = Some(Approval {
            browser: false,
            command: "pwd".into(),
            answer: tx,
            scroll: 0,
            guest: None,
        });
        t.draw(|f| ui::draw(&mut a, f))?;
        a.approval = None;
        a.dialog = None;
    }
    Ok(())
}
#[test]
fn chat_list_and_bubble_content_render() -> Result<()> {
    let mut a = app()?;
    a.store.add("lobby", "bob", "Hello Alice", "human")?;
    a.store.add("lobby", "alice", "Hello Bob", "human")?;
    a.refresh()?;
    let mut t = Terminal::new(TestBackend::new(120, 36))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    let buf = t.backend().buffer();
    let content = (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(content.contains("Hello Alice"));
    assert!(content.contains("Hello Bob"));
    assert!(content.contains("My agent"));
    assert!(content.contains("New group"));
    assert!(content.contains("saved"));
    for name in ["x".repeat(32), "界".repeat(32)] {
        a.name = name;
        let mut narrow = Terminal::new(TestBackend::new(80, 24))?;
        narrow.draw(|f| ui::draw(&mut a, f))?;
        assert!(contents(&narrow).contains("F5 Activity"));
    }
    Ok(())
}
#[test]
fn text_wrapping_and_input_validation() {
    assert_eq!(ui::wrap_text("ab界cd\nx", 4), vec!["ab界", "cd", "x"]);
    assert!(!valid_room("agent:bob"));
    assert!(!valid_room("bad\nname"));
    assert!(!valid_room("   "));
    assert!(valid_room(&"界".repeat(64)));
    assert!(!valid_room(&"界".repeat(65)));
    assert_eq!(safe("hi\x1b\x07\nthere"), "hi\nthere");
}

#[test]
#[ignore = "writes a UI review image source when BELLA_PREVIEW is set"]
fn ui_preview() -> Result<()> {
    let path = crate::paths::env_var("BELLA_PREVIEW", "TERM_SOCIAL_PREVIEW")
        .ok_or_else(|| anyhow::anyhow!("BELLA_PREVIEW or TERM_SOCIAL_PREVIEW not set"))?;
    let mut a = app()?;
    a.name = "carmine".into();
    a.store.add(
        "design studio",
        "marta",
        "The chat layout is feeling much cleaner. Can we keep the agent in this group?",
        "human",
    )?;
    a.store
        .add("weekend plans", "luca", "Coffee on Saturday?", "human")?;
    a.store.add(
        "agent:carmine",
        "carmine's bot",
        "Ready when you are. What are we building today?",
        "bot",
    )?;
    a.store.add(
        "design studio",
        "carmine",
        "Yes — just mention @bot whenever we need a hand.",
        "human",
    )?;
    a.store.add(
        "design studio",
        "marta",
        "@bot give us a short checklist for testing the new chat UI.",
        "human",
    )?;
    a.store.add("design studio","carmine's bot","Try sending from two terminals, switch chats with an unfinished draft, search for a group, and check unread messages. I will ask before running any shell commands.","bot")?;
    a.switch_room("design studio".into())?;
    a.input = Editor::new("Let's test the message editor next.".into());
    a.health = Some(true);
    a.status = "Messages saved locally · all times UTC".into();
    if crate::paths::env_var("BELLA_PREVIEW_MODE", "TERM_SOCIAL_PREVIEW_MODE").as_deref()
        == Some("options")
    {
        a.store.set_pinned(&a.name, &a.room, true)?;
        a.store.set_archived(&a.name, "weekend plans", true)?;
        a.refresh()?;
        a.open_chat_actions(a.room.clone());
    }
    if crate::paths::env_var("BELLA_PREVIEW_MODE", "TERM_SOCIAL_PREVIEW_MODE").as_deref()
        == Some("search")
    {
        a.open_dialog(Dialog::FindMessages);
        a.query = Editor::new("checklist".into());
        a.update_hits(false)?;
    }

    if let Some(mode) = crate::paths::env_var("BELLA_PREVIEW_MODE", "TERM_SOCIAL_PREVIEW_MODE")
        && (mode == "profile" || mode == "bots" || mode == "portraits")
    {
        let mut human = profiles::Profile::user("carmine");
        human.name = "Carmine".into();
        human.about = "Building a home for people and their agents.".into();
        let mut bot = profiles::Profile::bot("carmine", "bot");
        bot.name = "Orbit".into();
        bot.about = "Your personal assistant".into();
        if let Some(path) = crate::paths::env_var("BELLA_AVATAR", "TERM_SOCIAL_AVATAR") {
            human.avatar = profiles::import_avatar(&path)?;
            bot.avatar = human.avatar.clone();
        }
        profiles::save(a.store.connection(), &human)?;
        profiles::save(a.store.connection(), &bot)?;
        let mut extra = profiles::Profile::bot("carmine", "builder");
        extra.name = "Builder".into();
        extra.about = "Code, plans and careful changes".into();
        extra.instructions = "Help build and test software. Explain changes clearly.".into();
        profiles::save(a.store.connection(), &extra)?;
        a.refresh()?;
        if mode == "profile" {
            a.open_dialog(Dialog::Profile);
        }
        if mode == "bots" {
            a.open_dialog(Dialog::Bots);
        }
    }
    if crate::paths::env_var("BELLA_PREVIEW_MODE", "TERM_SOCIAL_PREVIEW_MODE").as_deref()
        == Some("recipient")
    {
        a.select_recipient(Some("bot".into()))?;
        a.input = Editor::new("Navigate to facebook".into());
    }
    if crate::paths::env_var("BELLA_PREVIEW_MODE", "TERM_SOCIAL_PREVIEW_MODE").as_deref()
        == Some("network")
    {
        network::prepare(&a.store)?;
        let mut net = network::test_connection();
        net.credentials.username = a.name.clone();
        net.state.pending = 2;
        net.state.detail = "Connected · account authenticated · sample data".into();
        a.network = Some(net);
        a.open_dialog(Dialog::Network);
    }
    if crate::paths::env_var("BELLA_PREVIEW_MODE", "TERM_SOCIAL_PREVIEW_MODE").as_deref()
        == Some("browser")
    {
        a.store
            .add(&a.room, "carmine", "@bot open https://example.com", "human")?;
        let action = browser::Action::result(
            "https://example.com".into(),
            "xdg-open 'https://example.com'".into(),
            true,
            "Exit: exit status: 0\nLauncher warning is available in Details".into(),
        );
        a.store.add(&a.room, "tools", &action.record(), "tool")?;
        a.refresh()?;
        a.input = Editor::default();
        a.status = "F11 Browser details · launch requested, window unverified".into();
    }
    if crate::paths::env_var("BELLA_PREVIEW_MODE", "TERM_SOCIAL_PREVIEW_MODE").as_deref()
        == Some("copy")
    {
        a.switch_room("agent:carmine".into())?;
        a.store.add(
            &a.room,
            "carmine",
            "Check why the app keeps crashing.",
            "human",
        )?;
        a.store.add(&a.room,"tools","Exit: exit status: 1\nAdministrator authorization required. This chat cannot receive a sudo password.\nsudo: a password is required","tool")?;
        a.store.add(&a.room,"carmine's bot","That command needs administrator authentication. I can continue with non-privileged package and crash-log checks; each command will still need your approval.","bot")?;
        a.refresh()?;
        a.input = Editor::default();
        a.status = "Click Copy on any message · text keeps its original line breaks".into();
    }
    let mut t = Terminal::new(TestBackend::new(128, 40))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    let b = t.backend().buffer();
    let mut cells = vec![];
    for y in 0..40 {
        for x in 0..128 {
            let c = &b[(x, y)];
            cells.push(serde_json::json!({"x":x,"y":y,"s":c.symbol(),"fg":format!("{:?}",c.fg),"bg":format!("{:?}",c.bg),"bold":c.modifier.contains(ratatui::style::Modifier::BOLD)}));
        }
    }
    std::fs::write(path, serde_json::to_vec(&cells)?)?;
    Ok(())
}

#[test]
fn streamed_replies_stay_in_their_origin_chat() -> Result<()> {
    let mut a = app()?;
    let (tx, rx) = mpsc::channel();
    a.job = Some(Job {
        author: String::new(),
        model: String::new(),
        task_id: None,
        saved_partial: String::new(),
        control: agent::Control::detached(),
        phase: agent::Phase::Thinking,
        stopping: false,
        ran_command: false,
        partial: String::new(),
        room: "lobby".into(),
        events: rx,
        requester: None,
    });
    a.switch_room("other".into())?;
    tx.send(agent::Event::Delta("Hello".into()))?;
    a.poll_job()?;
    assert_eq!(a.job.as_ref().unwrap().partial, "Hello");
    tx.send(agent::Event::Finished(Ok("Hello Alice".into())))?;
    a.poll_job()?;
    assert!(a.job.is_none());
    assert!(a.store.history("other")?.is_empty());
    assert_eq!(
        a.store.history("lobby")?.last().unwrap().body,
        "Hello Alice"
    );
    Ok(())
}
#[test]
#[ignore = "requires the installed local model server on port 8080"]
fn live_model_stream_and_approved_shell() -> Result<()> {
    let mut a = app()?;
    a.config.model = "Qwen3.6-35B-A3B-UD-Q4_K_XL".into();
    a.submit("@bot Use the shell tool to run exactly the command between these backticks: `printf bella-tool-ok`\nThen reply with the text printed by the tool.".into())?;
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut approved = false;
    let mut streamed = false;
    while a.job.is_some() && Instant::now() < deadline {
        a.poll_job()?;
        if let Some(job) = &a.job {
            streamed |= !job.partial.is_empty();
        }
        if let Some(approval) = &a.approval {
            assert_eq!(approval.command.trim(), "printf bella-tool-ok");
            a.key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE))?;
            approved = true;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(a.job.is_none(), "Local model timed out");
    assert!(approved, "Local model did not request the tool");
    assert!(streamed, "No streamed reply received");
    let history = a.store.history("lobby")?;
    assert!(
        history
            .iter()
            .any(|m| m.kind == "tool" && m.body.contains("bella-tool-ok"))
    );
    assert!(
        history
            .iter()
            .any(|m| m.kind == "bot" && m.body.contains("bella-tool-ok")),
        "{history:?}"
    );
    assert!(!history.iter().any(|m| m.kind == "system"), "{history:?}");
    Ok(())
}

fn contents(t: &Terminal<TestBackend>) -> String {
    let b = t.backend().buffer();
    (0..b.area.height)
        .map(|y| {
            (0..b.area.width)
                .map(|x| b[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn old_messages_load_without_duplicates_and_search_jumps_into_history() -> Result<()> {
    let mut a = app()?;
    for i in 0..450 {
        a.store.add(
            "lobby",
            "bob",
            &format!("message {i:03} literal 100%_done"),
            "human",
        )?;
    }
    a.refresh()?; // initial page ends at 200 messages
    assert_eq!(a.messages.len(), 200);
    assert!(a.has_older);
    a.load_older()?;
    assert_eq!(a.messages.len(), 400);
    a.load_older()?;
    assert_eq!(a.messages.len(), 450);
    assert!(!a.has_older);
    let ids = a
        .messages
        .iter()
        .map(|m| m.id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 450);
    a.latest()?;
    assert_eq!(a.messages.len(), 200);
    a.open_dialog(Dialog::FindMessages);
    a.query = Editor::new("message 003".into());
    a.update_hits(false)?;
    assert_eq!(a.hits.len(), 1);
    let id = a.hits[0].id;
    a.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    assert_eq!(a.highlight, Some(id));
    assert!(a.has_newer);
    assert!(!a.following);
    let mut t = Terminal::new(TestBackend::new(120, 36))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&t).contains("message 003"));
    assert!(a.scroll > 0);
    // New traffic must not move the historical reading window.
    let anchor = a.anchor;
    a.store
        .add("lobby", "bob", "brand new incoming message", "human")?;
    a.refresh()?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert_eq!(a.anchor, anchor);
    assert_eq!(
        a.chats.iter().find(|c| c.room == "lobby").unwrap().unread,
        1
    );
    let mut pages = 0;
    while a.has_newer && pages < 10 {
        a.scroll_down(usize::MAX)?;
        a.refresh()?;
        pages += 1;
    }
    assert!(!a.has_newer);
    assert!(a.following);
    assert!(a.messages.last().unwrap().body.contains("brand new"));
    a.latest()?;
    assert!(a.messages.last().unwrap().body.contains("brand new"));
    assert_eq!(a.scroll, 0);
    assert_eq!(
        a.chats.iter().find(|c| c.room == "lobby").unwrap().unread,
        0
    );
    Ok(())
}
#[test]
fn search_pagination_is_literal_and_stays_in_the_current_chat() -> Result<()> {
    let mut a = app()?;
    for i in 0..110 {
        a.store
            .add("lobby", "bob", &format!("item {i} 100%_done"), "human")?;
    }
    a.store.add("other", "bob", "100%_done secret", "human")?;
    a.store
        .add("lobby", "bob", "100XXdone should not match", "human")?;
    a.open_dialog(Dialog::FindMessages);
    a.query = Editor::new("100%_done".into());
    a.update_hits(false)?;
    assert_eq!(a.hits.len(), 50);
    assert!(a.more_hits);
    a.update_hits(true)?;
    assert_eq!(a.hits.len(), 100);
    assert!(a.more_hits);
    a.update_hits(true)?;
    assert_eq!(a.hits.len(), 110);
    assert!(!a.more_hits);
    assert!(!a.hits.iter().any(|m| m.body.contains("secret")));
    let other = a.store.history("other")?[0].id;
    assert!(a.open_message(other).is_err());
    assert_eq!(
        a.hits
            .iter()
            .map(|m| m.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        110
    );
    Ok(())
}
#[test]
fn incoming_messages_keep_the_scrolled_view_and_unread_state() -> Result<()> {
    let mut a = app()?;
    for i in 0..60 {
        a.store
            .add("lobby", "bob", &format!("earlier message {i}"), "human")?;
    }
    a.refresh()?;
    let mut t = Terminal::new(TestBackend::new(120, 36))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    a.scroll_up(18)?;
    t.draw(|f| ui::draw(&mut a, f))?;
    let anchor = a.anchor;
    let history = ui::screen(ratatui::layout::Rect::new(0, 0, 120, 36)).history;
    let visible = |t: &Terminal<TestBackend>| {
        (history.y..history.bottom())
            .map(|y| {
                (history.x..history.right())
                    .map(|x| t.backend().buffer()[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let before = visible(&t);
    a.store.add("lobby", "bob", "NEW ARRIVAL", "human")?;
    a.refresh()?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert_eq!(a.anchor, anchor);
    assert_eq!(visible(&t), before);
    assert_eq!(
        a.chats.iter().find(|c| c.room == "lobby").unwrap().unread,
        1
    );
    a.latest()?;
    assert_eq!(
        a.chats.iter().find(|c| c.room == "lobby").unwrap().unread,
        0
    );
    Ok(())
}

#[test]
fn clicking_a_message_search_result_opens_it() -> Result<()> {
    let mut a = app()?;
    a.store.add("lobby", "bob", "find this message", "human")?;
    a.open_dialog(Dialog::FindMessages);
    a.query = Editor::new("find this".into());
    a.update_hits(false)?;
    let rect = ui::dialog_rect(
        ratatui::layout::Rect::new(0, 0, 120, 36),
        &Dialog::FindMessages,
    );
    a.dialog_mouse(
        event::MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 3,
            row: rect.y + 5,
            modifiers: KeyModifiers::NONE,
        },
        rect,
    )?;
    assert!(a.dialog.is_none());
    assert_eq!(a.highlight, Some(a.hits[0].id));
    Ok(())
}

#[test]
fn stopping_preserves_partial_reply_in_origin_and_blocks_pending_approval() -> Result<()> {
    let mut a = app()?;
    let (tx, rx) = mpsc::channel();
    a.job = Some(Job {
        author: String::new(),
        model: String::new(),
        task_id: None,
        saved_partial: String::new(),
        control: agent::Control::detached(),
        phase: agent::Phase::Thinking,
        stopping: false,
        ran_command: false,
        partial: "A partially written reply".into(),
        room: "lobby".into(),
        events: rx,
        requester: None,
    });
    a.switch_room("other".into())?;
    a.stop_agent()?;
    assert!(a.job.as_ref().unwrap().stopping);
    let (answer, approval) = mpsc::channel();
    tx.send(agent::Event::Approval {
        browser: false,
        command: "must not execute".into(),
        answer,
        guest: None,
    })?;
    tx.send(agent::Event::Cancelled)?;
    a.poll_job()?;
    assert!(approval.recv().is_err());
    assert!(a.approval.is_none());
    assert!(a.job.is_none());
    let history = a.store.history("lobby")?;
    assert!(
        history
            .iter()
            .any(|m| m.kind == "interrupted" && m.body.contains("partially"))
    );
    assert!(a.store.history("other")?.is_empty());
    Ok(())
}
#[test]
fn model_status_distinguishes_missing_model_and_connection_errors() -> Result<()> {
    let mut a = app()?;
    for (result, expected, detail) in [
        (Ok(vec!["test".into()]), true, "available"),
        (Ok(vec!["other-model".into()]), false, "not listed"),
        (Err("offline fixture".into()), false, "offline fixture"),
    ] {
        let (tx, rx) = mpsc::channel();
        tx.send(result)?;
        a.health_rx = Some(rx);
        a.poll_health();
        assert_eq!(a.health, Some(expected));
        assert!(a.health_detail.contains(detail));
        assert!(a.health_rx.is_none());
    }
    Ok(())
}

#[test]
fn pin_archive_restore_and_global_search_keep_the_right_chat_context() -> Result<()> {
    let mut a = app()?;
    a.store.add("workshop", "bob", "still unread", "human")?;
    a.refresh()?;
    a.input = Editor::new("unfinished lobby draft".into());
    a.open_chat_actions("lobby".into());
    a.chat_action(0)?;
    assert_eq!(a.visible_chats()[0].room, "lobby");
    assert!(a.visible_chats()[0].pinned);
    // Archiving a different chat must not open it, consume its unread state or change this draft.
    a.open_chat_actions("workshop".into());
    a.chat_action(1)?;
    assert_eq!(a.room, "lobby");
    assert_eq!(a.input.text, "unfinished lobby draft");
    assert!(!a.visible_chats().iter().any(|c| c.room == "workshop"));
    assert_eq!(
        a.chats
            .iter()
            .find(|c| c.room == "workshop")
            .unwrap()
            .unread,
        1
    );
    a.store.add("workshop", "bob", "another message", "human")?;
    a.refresh()?;
    assert!(
        a.chats
            .iter()
            .find(|c| c.room == "workshop")
            .unwrap()
            .archived
    );
    // Global chat search includes archived rooms and opens the matching folder.
    a.open_dialog(Dialog::Search);
    a.query = Editor::new("workshop".into());
    a.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    assert_eq!(a.room, "workshop");
    assert!(a.view_archived);
    a.open_chat_actions(a.room.clone());
    a.chat_action(1)?;
    assert!(!a.view_archived);
    assert!(a.visible_chats().iter().any(|c| c.room == "workshop"));
    a.switch_room("lobby".into())?;
    assert_eq!(a.input.text, "unfinished lobby draft");
    a.toggle_folder();
    assert!(a.visible_chats().is_empty());
    a.toggle_folder();
    a.cycle_room(false)?;
    assert!(!a.room.is_empty());
    Ok(())
}
#[test]
fn archive_does_not_interrupt_the_agent_or_redirect_its_reply() -> Result<()> {
    let mut a = app()?;
    let (tx, rx) = mpsc::channel();
    a.job = Some(Job {
        author: String::new(),
        model: String::new(),
        task_id: None,
        saved_partial: String::new(),
        control: agent::Control::detached(),
        phase: agent::Phase::Thinking,
        stopping: false,
        ran_command: false,
        partial: String::new(),
        room: "lobby".into(),
        events: rx,
        requester: None,
    });
    a.open_chat_actions("lobby".into());
    a.chat_action(1)?;
    assert!(a.job.is_some());
    assert!(a.view_archived);
    a.switch_room("other".into())?;
    tx.send(agent::Event::Finished(Ok("reply to archived chat".into())))?;
    a.poll_job()?;
    a.refresh()?;
    assert!(a.store.history("other")?.is_empty());
    assert_eq!(
        a.store.history("lobby")?.last().unwrap().body,
        "reply to archived chat"
    );
    assert!(a.chats.iter().find(|c| c.room == "lobby").unwrap().archived);
    assert!(!a.view_archived);
    Ok(())
}
#[test]
fn tab_navigation_handles_a_folder_without_the_open_chat() -> Result<()> {
    let mut a = app()?;
    a.store.add("archived-one", "bob", "old", "human")?;
    a.store.set_archived("alice", "archived-one", true)?;
    a.refresh()?;
    a.toggle_folder();
    a.cycle_room(false)?;
    assert_eq!(a.room, "archived-one");
    a.toggle_folder();
    let expected = a.visible_chats()[0].room.clone();
    a.cycle_room(false)?;
    assert_eq!(a.room, expected);
    Ok(())
}

fn form_values(a: &mut App, values: &[&str]) {
    let form = a.profile_form.as_mut().unwrap();
    for (i, s) in values.iter().enumerate() {
        form.fields[i].1 = Editor::new((*s).into());
    }
}
#[test]
fn user_profile_keeps_identity_drafts_and_history_and_cancel_is_reversible() -> Result<()> {
    let mut a = app()?;
    a.input = Editor::new("unsent".into());
    a.save_draft()?;
    a.store.add("lobby", "alice", "old message", "human")?;
    a.open_dialog(Dialog::Profile);
    form_values(&mut a, &["Alice 界", "A human, building things", ""]);
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(a.dialog.is_none());
    assert_eq!(a.name, "alice");
    assert_eq!(a.store.draft("alice", "lobby")?, "unsent");
    assert_eq!(a.store.history("lobby")?[0].author, "alice");
    assert_eq!(a.display_author("alice", false), "Alice 界");
    a.open_dialog(Dialog::Profile);
    form_values(&mut a, &["discard", "discard", ""]);
    a.dialog_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
    a.refresh()?;
    assert_eq!(a.display_author("alice", false), "Alice 界");
    // Original personal-agent display names also accept a maximum-length account ID.
    profiles::Profile::bot(&"界".repeat(32), "bot").validate()?;
    Ok(())
}
#[test]
fn bots_creation_validation_room_membership_and_owner_isolation() -> Result<()> {
    let mut a = app()?;
    a.open_dialog(Dialog::Bots);
    a.dialog_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
    form_values(
        &mut a,
        &["", "Planner", "Plans tasks", "Be precise.", "", ""],
    );
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(!a.profile_form.as_ref().unwrap().error.is_empty());
    form_values(
        &mut a,
        &[
            "planner",
            "Planner",
            "Plans tasks",
            "Be precise.",
            "custom-model",
            "",
        ],
    );
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(a.dialog == Some(Dialog::Bots));
    assert!(a.enabled_bots.contains(&"planner".into()));
    assert_eq!(a.requested_bot("@planner hello").unwrap().name, "Planner");
    assert!(a.requested_bot("@planner_extra hello").is_none());
    assert!(profiles::enabled(a.store.connection(), "bob", "lobby")?.is_empty());
    assert!(profiles::enabled(a.store.connection(), "alice", "other")?.is_empty());
    a.selected = a.bots().iter().position(|p| p.handle == "planner").unwrap();
    a.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    assert!(!a.enabled_bots.contains(&"planner".into()));
    a.dialog = None;
    a.submit("@planner should stay draft".into())?;
    assert_eq!(a.input.text, "@planner should stay draft");
    assert!(a.store.history("lobby")?.is_empty());
    // Duplicate handles never overwrite an existing personality.
    a.open_dialog(Dialog::Bots);
    a.dialog_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
    form_values(&mut a, &["planner", "Wrong", "", "Overwrite", "", ""]);
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(
        a.profile_form
            .as_ref()
            .unwrap()
            .error
            .contains("already exists")
    );
    assert_eq!(
        a.requested_bot("@planner").unwrap().instructions,
        "Be precise."
    );
    Ok(())
}
#[test]
fn profile_pictures_decode_validate_survive_backup_and_source_removal() -> Result<()> {
    let dir = std::env::temp_dir().join(format!("bella-avatar-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let result = (|| -> Result<()> {
        let png = dir.join("my picture.png");
        image::RgbaImage::from_pixel(40, 32, image::Rgba([255, 40, 20, 255])).save(&png)?;
        let avatar = profiles::import_avatar(png.to_str().unwrap())?;
        assert_eq!(avatar.len(), 576);
        assert_eq!(avatar[0], [255, 40, 20]);
        let jpeg = dir.join("photo.jpg");
        image::RgbImage::from_pixel(32, 32, image::Rgb([10, 80, 200])).save(&jpeg)?;
        assert_eq!(profiles::import_avatar(jpeg.to_str().unwrap())?.len(), 576);
        let store = Store::open(&dir.join("chat.sqlite"))?;
        let mut p = profiles::Profile::user("alice");
        p.avatar = avatar;
        profiles::save(store.connection(), &p)?;
        std::fs::remove_file(png)?;
        let backup = dir.join("backup.sqlite");
        store.backup(&backup)?;
        let restored = Store::open(&backup)?;
        assert_eq!(profiles::all(restored.connection())?[0].avatar, p.avatar);
        let invalid = dir.join("bad.png");
        std::fs::write(&invalid, b"not an image")?;
        assert!(profiles::import_avatar(invalid.to_str().unwrap()).is_err());
        let big = dir.join("big.png");
        std::fs::File::create(&big)?.set_len(8 * 1024 * 1024 + 1)?;
        assert!(profiles::import_avatar(big.to_str().unwrap()).is_err());
        let wide = dir.join("wide.png");
        image::RgbImage::new(4097, 1).save(&wide)?;
        assert!(profiles::import_avatar(wide.to_str().unwrap()).is_err());
        let mut form = profiles::Form::new(p, false);
        form.fields[2].1 = Editor::new("-".into());
        assert!(form.build()?.avatar.is_empty());
        Ok(())
    })();
    std::fs::remove_dir_all(dir)?;
    result
}
#[test]
fn named_bot_request_uses_its_model_personality_and_separate_assistant_role() -> Result<()> {
    use std::{
        io::{BufRead, Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let server = std::thread::spawn(move || -> Result<serde_json::Value> {
        let (mut stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut reader = std::io::BufReader::new(stream.try_clone()?);
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if line == "\r\n" {
                break;
            }
            if let Some((k, v)) = line.split_once(':')
                && k.eq_ignore_ascii_case("content-length")
            {
                length = v.trim().parse::<usize>()?;
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body)?;
        let request = serde_json::from_slice(&body)?;
        let reply = r#"{"choices":[{"message":{"role":"assistant","content":"Planner answer"},"finish_reason":"stop"}]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            reply.len(),
            reply
        )?;
        Ok(request)
    });
    let mut a = app()?;
    a.config.endpoint = endpoint;
    let mut bot = profiles::Profile::bot("alice", "planner");
    bot.name = "Planner".into();
    bot.instructions = "Use numbered plans.".into();
    bot.model = "planning-model".into();
    profiles::save(a.store.connection(), &bot)?;
    profiles::toggle(a.store.connection(), "alice", "lobby", "planner")?;
    a.store
        .add("lobby", "alice's bot", "Other agent answer", "bot")?;
    a.store
        .add("lobby", &bot.author(), "Earlier planner answer", "bot")?;
    a.refresh()?;
    a.submit("@planner make a plan".into())?;
    let end = Instant::now() + Duration::from_secs(5);
    while a.job.is_some() && Instant::now() < end {
        a.poll_job()?;
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(a.job.is_none());
    let request = server.join().unwrap()?;
    assert_eq!(request["model"], "planning-model");
    let messages = request["messages"].as_array().unwrap();
    assert!(messages.iter().any(|m| {
        m["role"] == "system"
            && m["content"]
                .as_str()
                .unwrap()
                .contains("Use numbered plans.")
    }));
    assert!(messages.iter().any(|m| {
        m["role"] == "user"
            && m["content"]
                .as_str()
                .unwrap()
                .contains("Other agent answer")
    }));
    assert!(
        messages
            .iter()
            .any(|m| m["role"] == "assistant" && m["content"] == "Earlier planner answer")
    );
    let last = a.store.history("lobby")?.pop().unwrap();
    assert_eq!(last.author, bot.author());
    assert_eq!(last.body, "Planner answer");
    assert_eq!(
        tasks::list(a.store.connection(), "alice")?[0].author,
        bot.author()
    );
    Ok(())
}
#[test]
fn crash_recovery_retains_the_named_bot_identity() -> Result<()> {
    let a = app()?;
    let db = a.store.connection();
    db.execute(
        "INSERT INTO app_sessions VALUES('dead-bot',4294967295,'old')",
        [],
    )?;
    let id = tasks::begin(db, "alice", "lobby", "dead-bot", "@planner test")?;
    db.execute(
        "INSERT INTO task_bots VALUES(?1,?2)",
        rusqlite::params![id, "alice's @planner"],
    )?;
    tasks::update(db, id, "thinking", "Partial plan", "")?;
    tasks::session(db)?;
    let messages = a.store.history("lobby")?;
    let partial = messages.iter().find(|m| m.kind == "interrupted").unwrap();
    assert_eq!(partial.author, "alice's @planner");
    assert_eq!(partial.body, "Partial plan");
    Ok(())
}

#[test]
fn shared_bots_show_ownership_and_cannot_be_edited_or_removed_by_another_account() -> Result<()> {
    let mut a = app()?;
    network::prepare(&a.store)?;
    a.network = Some(network::test_connection());
    let mut p = profiles::Profile::bot("bob", "planner");
    p.name = "Bob's Planner".into();
    profiles::save(a.store.connection(), &p)?;
    profiles::toggle(a.store.connection(), "bob", "lobby", "planner")?;
    a.refresh()?;
    a.open_dialog(Dialog::Bots);
    a.selected = a.bots().iter().position(|p| p.owner == "bob").unwrap();
    a.dialog_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))?;
    assert!(a.dialog == Some(Dialog::Bots));
    assert!(a.status.contains("owner"));
    a.dialog_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))?;
    assert!(profiles::enabled(a.store.connection(), "bob", "lobby")?.contains(&"planner".into()));
    assert!(a.requested_bot("@planner test").is_none());
    // The guest's client never runs another account's bot; it only reports where it went.
    a.dialog = None;
    a.submit("@planner test".into())?;
    assert!(a.job.is_none());
    assert!(a.status.contains("Sent to bob's @planner"), "{}", a.status);
    a.open_dialog(Dialog::Bots);
    let mut terminal = Terminal::new(TestBackend::new(120, 36))?;
    terminal.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&terminal).contains("Shared bot"));
    a.open_dialog(Dialog::Network);
    terminal.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&terminal).contains("Network connection"));
    Ok(())
}

#[test]
#[ignore = "requires the running local model and a graphical desktop; denies the proposed launch"]
fn live_browser_request_corrects_previous_refusal_and_asks_for_approval() -> Result<()> {
    let mut a = app()?;
    a.config.model = "Qwen3.6-35B-A3B-UD-Q4_K_XL".into();
    a.switch_room("agent:alice".into())?;
    a.store.add(
        &a.room,
        "alice",
        "can you open the browse for me in my computer",
        "human",
    )?;
    a.store.add(&a.room,"alice's bot","I can't directly open a web browser on your computer, as I don't have access to your desktop environment or GUI applications. You could run xdg-open yourself.","bot")?;
    a.submit("can you open the browse for me in my computer".into())?;
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut proposed = None;
    while a.job.is_some() && Instant::now() < deadline {
        a.poll_job()?;
        if let Some(approval) = &a.approval {
            println!("Browser approval requested: {}", approval.command);
            assert!(
                approval.browser && approval.command.contains("gtk-launch "),
                "Expected the registered desktop browser launcher, got {}",
                approval.command
            );
            assert!(!approval.command.contains("about:blank"));
            proposed = Some(approval.command.clone());
            a.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    assert!(
        proposed.is_some(),
        "Bot refused or failed without requesting browser approval: {:?}",
        a.store.history(&a.room)?
    );
    assert!(a.job.is_none(), "Bot did not finish after denial");
    assert_eq!(
        tasks::list(a.store.connection(), "alice")?[0].approval,
        "denied"
    );
    assert!(
        !a.store
            .history(&a.room)?
            .iter()
            .any(|m| m.kind == "tool" && m.body.starts_with("Approved shell:"))
    );
    Ok(())
}

#[test]
fn recipient_selection_preserves_draft_and_is_scoped_to_room_and_owner() -> Result<()> {
    let mut a = app()?;
    a.input = Editor::new("draft 界\nsecond line".into());
    a.save_draft()?;
    a.select_recipient(Some("bot".into()))?;
    assert_eq!(a.input.text, "draft 界\nsecond line");
    assert_eq!(a.preview_recipient().as_deref(), Some("bot"));
    assert_eq!(a.store.recipient("alice", "lobby")?.as_deref(), Some("bot"));
    assert!(a.store.recipient("bob", "lobby")?.is_none());
    a.switch_room("workshop".into())?;
    assert!(a.recipient_handle().is_none());
    a.switch_room("lobby".into())?;
    assert_eq!(a.recipient_handle(), Some("bot"));
    assert_eq!(a.input.text, "draft 界\nsecond line");
    a.select_recipient(None)?;
    assert!(a.recipient_handle().is_none());
    assert_eq!(a.input.text, "draft 界\nsecond line");
    Ok(())
}
#[test]
fn recipient_preview_and_buttons_agree_when_typed_mentions_override_selection() -> Result<()> {
    let mut a = app()?;
    a.input = Editor::new("@bot navigate to facebook".into());
    assert_eq!(a.preview_recipient().as_deref(), Some("bot"));
    a.select_recipient(None)?;
    assert_eq!(a.input.text, "navigate to facebook");
    assert!(a.preview_recipient().is_none());
    a.submit(a.input.text.clone())?;
    assert!(a.job.is_none());
    assert_eq!(a.store.history("lobby")?[0].body, "navigate to facebook");
    a.input = Editor::new("@bot hello 界".into());
    a.select_recipient(Some("bot".into()))?;
    assert_eq!(a.input.text, "hello 界");
    assert!(a.input.text.is_char_boundary(a.input.cursor));
    Ok(())
}
#[test]
fn recipient_cycle_includes_only_owned_enabled_bots_and_personal_chat_always_targets_bot()
-> Result<()> {
    let mut a = app()?;
    for (owner, handle) in [
        ("alice", "planner"),
        ("alice", "disabled"),
        ("bob", "foreign"),
    ] {
        profiles::save(a.store.connection(), &profiles::Profile::bot(owner, handle))?;
    }
    profiles::toggle(a.store.connection(), "alice", "lobby", "planner")?;
    profiles::toggle(a.store.connection(), "bob", "lobby", "foreign")?;
    a.refresh()?;
    a.cycle_recipient(false)?;
    assert_eq!(a.recipient_handle(), Some("bot"));
    a.cycle_recipient(false)?;
    assert_eq!(a.recipient_handle(), Some("planner"));
    a.cycle_recipient(false)?;
    assert_eq!(a.recipient_handle(), None);
    a.cycle_recipient(true)?;
    assert_eq!(a.recipient_handle(), Some("bot"));
    a.cycle_recipient(true)?;
    assert_eq!(a.recipient_handle(), Some("planner"));
    profiles::toggle(a.store.connection(), "alice", "lobby", "planner")?;
    a.refresh()?;
    a.submit("keep this draft".into())?;
    assert_eq!(a.input.text, "keep this draft");
    assert!(a.job.is_none());
    assert!(a.store.history("lobby")?.is_empty());
    a.cycle_recipient(false)?;
    assert!(a.recipient_handle().is_none());
    a.switch_room("agent:alice".into())?;
    assert_eq!(a.recipient_handle(), Some("bot"));
    a.select_recipient(None)?;
    assert_eq!(a.recipient_handle(), Some("bot"));
    Ok(())
}
#[test]
fn recipient_controls_and_sharing_hint_render_in_the_composer() -> Result<()> {
    let mut a = app()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 36))?;
    terminal.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&terminal).contains("To group"));
    a.key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))?;
    terminal.draw(|f| ui::draw(&mut a, f))?;
    let content = contents(&terminal);
    assert!(content.contains("@bot · Bot"));
    assert!(content.contains("shared in this group"));
    assert!(content.contains("Ask ›"));
    a.key(KeyEvent::new(KeyCode::F(10), KeyModifiers::NONE))?;
    assert!(a.recipient_handle().is_none());
    // A received mention cannot change the local composer or start a request.
    a.store.add("lobby", "bob", "@bot do something", "human")?;
    a.refresh()?;
    assert!(a.recipient_handle().is_none());
    assert!(a.job.is_none());
    Ok(())
}
fn networked(store: Store) -> Result<App> {
    let mut a = app_with(store)?;
    network::prepare(&a.store)?;
    a.network = Some(network::test_connection());
    Ok(a)
}
fn lendable_planner(a: &mut App, lend: bool) -> Result<profiles::Profile> {
    let mut bot = profiles::Profile::bot("alice", "planner");
    bot.name = "Planner".into();
    bot.instructions = "Secret planner instructions.".into();
    bot.model = "planning-model".into();
    bot.lend_chat = lend;
    profiles::save(a.store.connection(), &bot)?;
    if profiles::enabled(a.store.connection(), "alice", "workshop")?.is_empty() {
        profiles::toggle(a.store.connection(), "alice", "workshop", "planner")?;
    }
    a.refresh()?;
    Ok(bot)
}
fn last_message_id(a: &App) -> Result<i64> {
    Ok(a.store
        .connection()
        .query_row("SELECT MAX(id) FROM messages", [], |r| r.get(0))?)
}
fn delegate_outcome(a: &App, id: i64) -> Option<String> {
    a.store
        .connection()
        .query_row(
            "SELECT outcome FROM delegate_requests WHERE message_id=?1",
            [id],
            |r| r.get(0),
        )
        .ok()
}
fn delegate_rows(a: &App) -> Result<i64> {
    Ok(a.store.connection().query_row(
        "SELECT COUNT(*) FROM delegate_requests WHERE message_id>0",
        [],
        |r| r.get(0),
    )?)
}
/// One-shot JSON model server; records each request body.
fn mock_model(
    replies: Vec<serde_json::Value>,
) -> Result<(
    String,
    std::thread::JoinHandle<Result<Vec<serde_json::Value>>>,
)> {
    use std::{
        io::{BufRead, Read, Write},
        net::TcpListener,
    };
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let endpoint = format!("http://{}/v1", listener.local_addr()?);
    let server = std::thread::spawn(move || -> Result<Vec<serde_json::Value>> {
        let mut bodies = vec![];
        for message in replies {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(5)))?;
            let mut reader = std::io::BufReader::new(stream.try_clone()?);
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                if line == "\r\n" {
                    break;
                }
                if let Some((k, v)) = line.split_once(':')
                    && k.eq_ignore_ascii_case("content-length")
                {
                    length = v.trim().parse::<usize>()?;
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body)?;
            bodies.push(serde_json::from_slice(&body)?);
            let payload = serde_json::json!({"choices":[{"message":message}]}).to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                payload.len(),
                payload
            )?;
        }
        Ok(bodies)
    });
    Ok((endpoint, server))
}
fn finish_job(a: &mut App) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(10);
    while a.job.is_some() && Instant::now() < end {
        a.poll_job()?;
        assert!(
            a.approval.is_none(),
            "A delegated request showed an approval"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(a.job.is_none(), "Delegated job did not finish");
    Ok(())
}
#[test]
fn bot_form_lend_flag_defaults_off_and_rejects_ambiguous_values() -> Result<()> {
    let mut a = app()?;
    a.open_dialog(Dialog::Bots);
    a.dialog_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
    assert_eq!(a.profile_form.as_ref().unwrap().fields[6].1.text, "5");
    assert_eq!(a.profile_form.as_ref().unwrap().fields[7].1.text, "off");
    assert_eq!(a.profile_form.as_ref().unwrap().fields[8].1.text, "no");
    form_values(
        &mut a,
        &["planner", "Planner", "", "", "", "", "5", "off", "maybe"],
    );
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(a.profile_form.as_ref().unwrap().error.contains("yes or no"));
    form_values(
        &mut a,
        &["planner", "Planner", "", "", "", "", "5", "off", "no"],
    );
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(!a.requested_bot("@planner").unwrap().lend_chat);
    a.selected = a.bots().iter().position(|p| p.handle == "planner").unwrap();
    a.dialog_key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE))?;
    a.profile_form.as_mut().unwrap().fields[8].1 = Editor::new("yes".into());
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    let planner = a.requested_bot("@planner").unwrap();
    assert!(planner.lend_chat);
    assert_eq!(planner.name, "Planner");
    Ok(())
}
#[test]
fn delegated_mentions_need_opt_in_membership_a_fresh_human_author_and_are_decided_once()
-> Result<()> {
    let store = Store::open(std::path::Path::new(":memory:"))?;
    store.add("workshop", "bob", "@planner sent before startup", "human")?;
    let mut a = networked(store)?;
    lendable_planner(&mut a, false)?;
    let before = last_message_id(&a)?;
    a.poll_delegated()?;
    assert!(a.job.is_none());
    assert_eq!(delegate_rows(&a)?, 0, "History before startup was replayed");
    assert!(before > 0);
    // Without the owner's opt-in nothing runs.
    a.store.add("workshop", "bob", "@planner hi", "human")?;
    let not_lent = last_message_id(&a)?;
    a.poll_delegated()?;
    assert!(a.job.is_none());
    assert_eq!(delegate_outcome(&a, not_lent).as_deref(), Some("not_lent"));
    assert!(a.status.contains("lending is off"));
    lendable_planner(&mut a, true)?;
    // Bot authors (no bot-to-bot chains), own messages, private agent rooms, rooms where the
    // bot is not a member, stale backlog and a requester's own same-handle bot are ignored.
    a.store
        .add("workshop", "bob's @helper", "@planner loop", "bot")?;
    a.store.add("workshop", "alice", "@planner mine", "human")?;
    a.store
        .add("agent:bob", "bob", "@planner private", "human")?;
    a.store.add("other", "bob", "@planner elsewhere", "human")?;
    a.store.add("workshop", "bob", "@bot personal", "human")?;
    a.store.connection().execute(
        "INSERT INTO messages(room,author,body,kind,created_at) VALUES('workshop','bob','@planner stale','human','2000-01-01 00:00:00')",
        [],
    )?;
    profiles::toggle(a.store.connection(), "carol", "workshop", "planner")?;
    a.store
        .add("workshop", "carol", "@planner her own bot", "human")?;
    a.poll_delegated()?;
    assert!(a.job.is_none());
    assert_eq!(delegate_rows(&a)?, 1);
    // Rescanning everything since startup never re-decides a message (dedup by message id).
    a.delegate_after = before;
    a.poll_delegated()?;
    assert!(a.job.is_none());
    assert_eq!(delegate_outcome(&a, not_lent).as_deref(), Some("not_lent"));
    assert_eq!(delegate_rows(&a)?, 1);
    // Local (non-network) mode never delegates.
    a.network = None;
    a.store
        .add("workshop", "bob", "@planner offline mode", "human")?;
    a.poll_delegated()?;
    assert!(a.job.is_none());
    Ok(())
}
#[test]
fn lent_bot_answers_a_guest_in_the_group_without_tools_or_private_context() -> Result<()> {
    let sentinel = std::env::temp_dir().join(format!("bella-guest-tool-{}", std::process::id()));
    let command = format!("printf forbidden > '{}'", sentinel.display());
    let (endpoint, server) = mock_model(vec![
        serde_json::json!({"role":"assistant","content":null,"tool_calls":[{"id":"one","type":"function","function":{"name":"shell","arguments":serde_json::json!({"command":command}).to_string()}}]}),
        serde_json::json!({"role":"assistant","content":"Guest answer"}),
    ])?;
    let mut a = networked(Store::open(std::path::Path::new(":memory:"))?)?;
    a.config.endpoint = endpoint;
    let bot = lendable_planner(&mut a, true)?;
    a.store
        .add("agent:alice", "alice", "private alice note", "human")?;
    a.store.add("lobby", "bob", "lobby-only secret", "human")?;
    a.store.add(
        "workshop",
        "bob",
        "@planner what should we do next?",
        "human",
    )?;
    let request = last_message_id(&a)?;
    a.poll_delegated()?;
    assert_eq!(a.job.as_ref().unwrap().requester.as_deref(), Some("bob"));
    assert_eq!(a.job.as_ref().unwrap().room, "workshop");
    assert!(a.status.contains("tools off"));
    // A second mention while the bot is answering is skipped, not queued.
    a.store
        .add("workshop", "bob", "@planner and also", "human")?;
    a.poll_delegated()?;
    assert_eq!(
        delegate_outcome(&a, last_message_id(&a)?).as_deref(),
        Some("busy")
    );
    finish_job(&mut a)?;
    let bodies = server.join().unwrap()?;
    assert_eq!(bodies.len(), 2);
    for body in &bodies {
        assert!(body.get("tools").is_none(), "Guest request offered tools");
        assert_eq!(body["model"], "planning-model");
        let text = body.to_string();
        assert!(!text.contains("private alice note"));
        assert!(!text.contains("lobby-only secret"));
        assert!(!text.contains("and also"), "Context went past the request");
    }
    let messages = bodies[0]["messages"].as_array().unwrap();
    assert!(messages.iter().any(|m| {
        m["role"] == "system"
            && m["content"]
                .as_str()
                .unwrap()
                .contains("Secret planner instructions.")
    }));
    assert!(messages.iter().any(|m| m["role"] == "user"
        && m["content"] == "bob: @planner what should we do next?"));
    let tool = bodies[1]["messages"]
        .as_array()
        .unwrap()
        .last()
        .unwrap()
        .clone();
    assert_eq!(tool["role"], "tool");
    assert_eq!(tool["content"], agent::TOOLS_DISABLED);
    assert!(!sentinel.exists(), "A guest's tool call executed");
    let history = a.store.history("workshop")?;
    let reply = history.iter().find(|m| m.kind == "bot").unwrap();
    assert_eq!(reply.author, bot.author());
    assert_eq!(reply.body, "Guest answer");
    assert!(!history.iter().any(|m| m.kind == "tool"));
    assert_eq!(delegate_outcome(&a, request).as_deref(), Some("started"));
    let task = &tasks::list(a.store.connection(), "alice")?[0];
    assert_eq!(task.requester, "bob");
    assert_eq!(task.state, "completed");
    assert_eq!(task.author, bot.author());
    assert_eq!(task.approval, "none");
    // The reply is queued for the hub under the owner's bot identity.
    let queued: i64 = a.store.connection().query_row(
        "SELECT COUNT(*) FROM network_outbox WHERE json_extract(payload,'$.author')=?1 AND json_extract(payload,'$.body')='Guest answer'",
        [bot.author()],
        |r| r.get(0),
    )?;
    assert_eq!(queued, 1);
    a.refresh()?;
    let mut terminal = Terminal::new(TestBackend::new(120, 36))?;
    a.open_dialog(Dialog::Tasks);
    terminal.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&terminal).contains("for bob (delegated)"));
    a.task_detail = Some(task.clone());
    a.open_dialog(Dialog::TaskDetails);
    terminal.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&terminal).contains("Requested by: bob"));
    Ok(())
}
#[test]
fn delegated_quota_and_failures_never_spam_or_leak_details() -> Result<()> {
    let mut a = networked(Store::open(std::path::Path::new(":memory:"))?)?;
    a.config.endpoint = "http://127.0.0.1:9/v1".into();
    lendable_planner(&mut a, true)?;
    for n in 1..=profiles::DELEGATE_PER_HOUR {
        a.store.connection().execute(
            "INSERT INTO delegate_requests(message_id,owner,handle,requester,room,outcome) VALUES(?1,'alice','planner','bob','workshop','started')",
            [-n],
        )?;
        a.store.connection().execute(
            "INSERT INTO delegate_requests(message_id,owner,handle,requester,room,outcome,at) VALUES(?1,'alice','planner','dave','workshop','started',datetime('now','-2 hours'))",
            [-100 - n],
        )?;
    }
    let messages_before = a.store.history("workshop")?.len();
    a.store
        .add("workshop", "bob", "@planner one more please", "human")?;
    let over = last_message_id(&a)?;
    a.poll_delegated()?;
    assert!(a.job.is_none(), "Over-quota request reached the model");
    assert_eq!(delegate_outcome(&a, over).as_deref(), Some("quota"));
    assert!(a.status.contains("limit"));
    // Nothing is posted to the group for skipped requests.
    assert_eq!(a.store.history("workshop")?.len(), messages_before + 1);
    // Older usage falls out of the rolling hour; a failure posts only a generic note.
    a.store.add("workshop", "dave", "@planner hello", "human")?;
    a.poll_delegated()?;
    assert_eq!(a.job.as_ref().unwrap().requester.as_deref(), Some("dave"));
    finish_job(&mut a)?;
    let note = a.store.history("workshop")?.pop().unwrap();
    assert_eq!(note.kind, "system");
    assert!(note.body.contains("could not answer"));
    assert!(!note.body.contains("127.0.0.1"));
    assert!(a.failed_room.is_none());
    let task = &tasks::list(a.store.connection(), "alice")?[0];
    assert_eq!(task.state, "failed");
    assert!(!task.error.is_empty());
    Ok(())
}

#[test]
fn browser_cards_hide_diagnostics_and_details_browse_saved_actions() -> Result<()> {
    let mut a = app()?;
    let action = browser::Action::result(
        "https://example.com".into(),
        "fake-launch".into(),
        true,
        "WAYLAND_DIAGNOSTIC_SENTINEL".into(),
    );
    a.store.add("lobby", "tools", &action.record(), "tool")?;
    a.store.add(
        "lobby",
        "tools",
        &browser::Action::result(
            "https://failed.example".into(),
            "fake-fail".into(),
            false,
            "FAILED_DIAGNOSTIC_SENTINEL".into(),
        )
        .record(),
        "tool",
    )?;
    a.refresh()?;
    let mut t = Terminal::new(TestBackend::new(120, 40))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    let screen = |t: &Terminal<TestBackend>| -> String {
        t.backend()
            .buffer()
            .content
            .iter()
            .map(|c| c.symbol())
            .collect()
    };
    let chat = screen(&t);
    assert!(chat.contains("Launch requested"));
    assert!(chat.contains("Browser · Failed"));
    assert!(!chat.contains("DIAGNOSTIC_SENTINEL"));
    assert!(!chat.contains("Browser action v1"));
    a.key(KeyEvent::new(KeyCode::F(11), KeyModifiers::NONE))?;
    assert_eq!(a.selected, 1);
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(screen(&t).contains("FAILED_DIAGNOSTIC_SENTINEL"));
    a.dialog_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(screen(&t).contains("WAYLAND_DIAGNOSTIC_SENTINEL"));
    a.dialog_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
    assert!(a.dialog.is_none());
    a.submit("/browser".into())?;
    assert!(a.dialog == Some(Dialog::BrowserDetails));
    assert_eq!(a.selected, 1);
    a.dialog_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
    a.switch_room("other".into())?;
    assert!(a.browser_actions().is_empty());
    Ok(())
}

#[test]
fn browser_approval_and_denial_are_explicit_in_the_interface() -> Result<()> {
    let mut a = app()?;
    let (answer, decision) = mpsc::channel();
    a.approval = Some(Approval {
        browser: true,
        command: "fake-browser 'https://example.com'".into(),
        answer,
        scroll: 0,
        guest: None,
    });
    let mut t = Terminal::new(TestBackend::new(100, 30))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&t).contains("Browser · Needs approval"));
    a.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
    assert!(!decision.recv()?);
    assert!(a.approval.is_none());
    let denied = browser::Action {
        state: browser::State::Denied,
        target: "Default browser".into(),
        command: "fake-browser".into(),
        diagnostics: "Nothing executed".into(),
    };
    a.store.add(&a.room, "tools", &denied.record(), "tool")?;
    a.refresh()?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&t).contains("Browser · Denied"));
    Ok(())
}

fn add_browser_fixture(a: &mut App, destination: &str) -> Result<i64> {
    let action = browser::Action::result(
        destination.into(),
        "fake-launch-only".into(),
        true,
        format!("Diagnostics for {destination}"),
    );
    a.store.add(&a.room, "tools", &action.record(), "tool")?;
    Ok(a.store.connection().last_insert_rowid())
}

#[test]
fn clicking_browser_details_targets_the_card_and_keeps_its_message_identity() -> Result<()> {
    let mut a = app()?;
    let first = add_browser_fixture(&mut a, "FIRST_TARGET")?;
    add_browser_fixture(&mut a, "SECOND_TARGET")?;
    a.refresh()?;
    let mut t = Terminal::new(TestBackend::new(120, 40))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    let (rect, _) = a
        .browser_links
        .iter()
        .find(|(_, id)| *id == first)
        .copied()
        .expect("first card button visible");
    assert_eq!(t.backend().buffer()[(rect.x, rect.y)].symbol(), "[");
    assert!(!a.open_browser_details_at(rect.x.saturating_sub(1), rect.y));
    assert!(!a.open_browser_details_at(rect.right(), rect.y));
    assert!(a.open_browser_details_at(rect.x, rect.y));
    assert_eq!(a.browser_detail_id, Some(first));
    assert_eq!(a.browser_detail_index(), Some(0));
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&t).contains("Diagnostics for FIRST_TARGET"));
    assert!(!contents(&t).contains("Diagnostics for SECOND_TARGET"));
    // Simulate loading an earlier action while Details is open. Its array position
    // moves, but the selected message and diagnostic content must stay the same.
    let mut earlier = a.messages[0].clone();
    earlier.id = -1;
    a.messages.insert(0, earlier);
    add_browser_fixture(&mut a, "NEW_TARGET")?;
    let latest = a.store.history(&a.room)?.pop().unwrap();
    a.messages.push(latest);
    assert_eq!(a.browser_detail_index(), Some(1));
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&t).contains("Diagnostics for FIRST_TARGET"));
    a.dialog_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE))?;
    assert_eq!(a.browser_detail_index(), Some(2));
    a.dialog_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE))?;
    assert_eq!(a.browser_detail_id, Some(first));
    a.messages.retain(|message| message.id != first);
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(contents(&t).contains("no longer in the loaded chat history"));
    assert!(!contents(&t).contains("Diagnostics for SECOND_TARGET"));
    Ok(())
}

#[test]
fn browser_mouse_targets_follow_scroll_resize_overlays_and_room_changes() -> Result<()> {
    let mut a = app()?;
    for i in 0..12 {
        add_browser_fixture(&mut a, &format!("https://example.com/{i}"))?;
    }
    a.refresh()?;
    let latest = a.messages.last().unwrap().id;
    let mut t = Terminal::new(TestBackend::new(100, 30))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(a.browser_links.iter().any(|(_, id)| *id == latest));
    a.scroll = 14;
    t.draw(|f| ui::draw(&mut a, f))?;
    let history = ui::screen(t.size()?.into()).history;
    let links = a.browser_links.clone();
    assert!(!links.is_empty());
    for (rect, _) in &links {
        assert!(history.contains((rect.x, rect.y).into()));
        assert!(
            rect.y < history.bottom() - 1,
            "Latest messages overlay consumes clicks"
        );
        assert_eq!(t.backend().buffer()[(rect.x, rect.y)].symbol(), "[");
    }
    let (rect, id) = links[0];
    assert!(a.open_browser_details_at(rect.x, rect.y));
    assert_eq!(a.browser_detail_id, Some(id));
    a.dialog_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))?;
    for (width, height) in [(70, 20), (30, 10), (10, 4), (1, 1)] {
        t = Terminal::new(TestBackend::new(width, height))?;
        t.draw(|f| ui::draw(&mut a, f))?;
        let history = ui::screen(t.size()?.into()).history;
        assert!(
            a.browser_links
                .iter()
                .all(|(rect, _)| rect.intersection(history) == *rect)
        );
    }
    a.switch_room("other".into())?;
    assert!(a.browser_links.is_empty());
    assert!(!a.open_browser_details_at(rect.x, rect.y));
    Ok(())
}

#[test]
fn browser_clicks_cannot_act_on_human_messages_or_bypass_a_dialog() -> Result<()> {
    let mut a = app()?;
    let record = browser::Action::result(
        "looks like a browser action".into(),
        "not executed".into(),
        true,
        String::new(),
    )
    .record();
    a.store.add(&a.room, "bob", &record, "human")?;
    a.refresh()?;
    let mut t = Terminal::new(TestBackend::new(120, 40))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(a.browser_links.is_empty());
    add_browser_fixture(&mut a, "REAL_TOOL_RECORD")?;
    a.refresh()?;
    t.draw(|f| ui::draw(&mut a, f))?;
    let (rect, _) = a.browser_links[0];
    a.open_dialog(Dialog::Help);
    assert!(!a.open_browser_details_at(rect.x, rect.y));
    a.dialog = None;
    let (answer, _) = mpsc::channel();
    a.approval = Some(Approval {
        browser: true,
        command: "not executed".into(),
        answer,
        scroll: 0,
        guest: None,
    });
    assert!(!a.open_browser_details_at(rect.x, rect.y));
    assert!(a.approval.is_some());
    Ok(())
}

#[test]
fn copy_buttons_cover_message_kinds_and_preserve_exact_text() -> Result<()> {
    let mut a = app()?;
    let body = "Hello 界\n```sh\nprintf '%s' \"$HOME\"\n```";
    for (author, kind) in [
        ("alice", "human"),
        ("bob", "human"),
        ("alice's bot", "bot"),
        ("tools", "tool"),
        ("system", "system"),
        ("alice's bot", "interrupted"),
    ] {
        a.store.add(&a.room, author, body, kind)?;
    }
    a.refresh()?;
    for message in &a.messages {
        assert_eq!(a.message_copy_text(message.id).as_deref(), Some(body));
    }
    let mut t = Terminal::new(TestBackend::new(120, 50))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    assert!(a.copy_links.len() >= 5);
    for (rect, id) in &a.copy_links {
        assert!(a.message_copy_text(*id).is_some());
        assert_eq!(t.backend().buffer()[(rect.x, rect.y)].symbol(), "[");
    }
    // Test completion without touching the real desktop clipboard.
    let id = a.messages[0].id;
    let (sender, receiver) = mpsc::channel();
    a.copy_job = Some((id, receiver));
    sender.send(Ok(clipboard::Outcome::Copied))?;
    a.poll_clipboard();
    assert_eq!(a.status, "Message copied");
    assert!(a.copy_feedback.as_ref().unwrap().2);
    a.switch_room("other".into())?;
    assert!(a.copy_links.is_empty());
    assert!(a.message_copy_text(id).is_none());
    Ok(())
}
#[test]
fn copy_hitboxes_follow_resize_and_do_not_overlap_details_or_latest_overlay() -> Result<()> {
    let mut a = app()?;
    for i in 0..12 {
        add_browser_fixture(&mut a, &format!("https://example.com/{i}"))?;
    }
    a.refresh()?;
    a.scroll = 14;
    for (width, height) in [(120, 40), (70, 20), (30, 10), (10, 4), (1, 1)] {
        let mut t = Terminal::new(TestBackend::new(width, height))?;
        t.draw(|f| ui::draw(&mut a, f))?;
        let history = ui::screen(t.size()?.into()).history;
        for (rect, _) in &a.copy_links {
            assert_eq!(rect.intersection(history), *rect);
            if a.scroll > 0 {
                assert!(rect.y < history.bottom() - 1);
            }
            assert!(
                a.browser_links
                    .iter()
                    .all(|(details, _)| details.intersection(*rect).is_empty())
            );
        }
    }
    Ok(())
}

#[test]
fn bot_form_guest_quota_is_bounded_and_per_bot() -> Result<()> {
    let mut a = app()?;
    a.open_dialog(Dialog::Bots);
    a.dialog_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
    for bad in ["0", "61", "-1", "two", "1.5", ""] {
        form_values(
            &mut a,
            &["planner", "Planner", "", "", "", "", bad, "off", "yes"],
        );
        a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
        assert!(
            a.profile_form.as_ref().unwrap().error.contains("1 to 60"),
            "accepted quota {bad:?}"
        );
    }
    form_values(
        &mut a,
        &["planner", "Planner", "", "", "", "", "2", "off", "yes"],
    );
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    let planner = a.requested_bot("@planner").unwrap();
    assert_eq!(planner.lend_per_hour, 2);
    // Profiles saved before this field existed fall back to the default.
    let mut json = serde_json::to_value(&planner)?;
    json.as_object_mut().unwrap().remove("lend_per_hour");
    let old: profiles::Profile = serde_json::from_value(json)?;
    assert_eq!(old.lend_per_hour, profiles::DELEGATE_PER_HOUR);
    Ok(())
}
#[test]
fn delegated_quota_uses_the_bots_own_limit() -> Result<()> {
    let mut a = networked(Store::open(std::path::Path::new(":memory:"))?)?;
    a.config.endpoint = "http://127.0.0.1:9/v1".into();
    let mut bot = lendable_planner(&mut a, true)?;
    bot.lend_per_hour = 1;
    profiles::save(a.store.connection(), &bot)?;
    a.refresh()?;
    a.store.connection().execute(
        "INSERT INTO delegate_requests(message_id,owner,handle,requester,room,outcome) VALUES(-1,'alice','planner','bob','workshop','started')",
        [],
    )?;
    a.store.add("workshop", "bob", "@planner again", "human")?;
    let over = last_message_id(&a)?;
    a.poll_delegated()?;
    assert!(
        a.job.is_none(),
        "Request over the bot's own limit reached the model"
    );
    assert_eq!(delegate_outcome(&a, over).as_deref(), Some("quota"));
    assert!(a.status.contains("limit of 1"));
    Ok(())
}
#[test]
fn bot_form_guest_tools_default_off_and_legacy_profiles_stay_off() -> Result<()> {
    let mut a = app()?;
    a.open_dialog(Dialog::Bots);
    a.dialog_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
    assert_eq!(a.profile_form.as_ref().unwrap().fields[7].1.text, "off");
    form_values(
        &mut a,
        &["planner", "Planner", "", "", "", "", "5", "maybe", "yes"],
    );
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    assert!(
        a.profile_form
            .as_ref()
            .unwrap()
            .error
            .contains("off or shell")
    );
    form_values(
        &mut a,
        &["planner", "Planner", "", "", "", "", "5", "shell", "yes"],
    );
    a.dialog_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL))?;
    let planner = a.requested_bot("@planner").unwrap();
    assert!(planner.lend_chat);
    assert_eq!(planner.guest_tools, profiles::GuestTools::Shell);
    let mut json = serde_json::to_value(&planner)?;
    json.as_object_mut().unwrap().remove("guest_tools");
    let old: profiles::Profile = serde_json::from_value(json)?;
    assert_eq!(old.guest_tools, profiles::GuestTools::Off);
    Ok(())
}

fn finish_job_with_decision(a: &mut App, approve: bool) -> Result<()> {
    let end = Instant::now() + Duration::from_secs(10);
    while a.job.is_some() && Instant::now() < end {
        a.poll_job()?;
        if let Some(approval) = a.approval.take() {
            let _ = approval.answer.send(approve);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(a.job.is_none(), "Delegated job did not finish");
    Ok(())
}

#[test]
fn guest_shell_approval_dialog_names_the_guest_and_workdir() -> Result<()> {
    let mut a = app()?;
    let home = std::env::temp_dir().join(format!("bella-guest-ui-{}", std::process::id()));
    let (answer, decision) = mpsc::channel();
    a.approval = Some(Approval {
        browser: false,
        command: "printf hello".into(),
        answer,
        scroll: 0,
        guest: Some(agent::GuestContext {
            requester: "bob".into(),
            home: home.clone(),
        }),
    });
    let mut t = Terminal::new(TestBackend::new(100, 32))?;
    t.draw(|f| ui::draw(&mut a, f))?;
    let screen = contents(&t);
    assert!(screen.contains("Guest shell · Needs approval"));
    assert!(screen.contains("Requested by guest: bob"));
    assert!(screen.contains(&home.display().to_string()));
    assert!(screen.contains("Not a sandbox"));
    a.key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE))?;
    assert!(!decision.recv()?);
    Ok(())
}

#[test]
fn lent_bot_guest_shell_needs_owner_approval_and_counts_against_quota() -> Result<()> {
    let home_root = std::env::temp_dir().join(format!("bella-guest-data-{}", std::process::id()));
    std::fs::create_dir_all(&home_root)?;
    let marker = paths::guest_home(&home_root, "bob")?.join("from-guest");
    let command = format!("printf ok > '{}'", marker.display());
    let (endpoint, server) = mock_model(vec![
        serde_json::json!({"role":"assistant","content":null,"tool_calls":[
            {"id":"one","type":"function","function":{"name":"shell","arguments":serde_json::json!({"command":command}).to_string()}},
            {"id":"two","type":"function","function":{"name":"open_browser","arguments":"{}"}}
        ]}),
        serde_json::json!({"role":"assistant","content":"Guest shell done"}),
    ])?;
    let mut a = networked(Store::open(std::path::Path::new(":memory:"))?)?;
    a.data_dir = home_root.clone();
    a.config.endpoint = endpoint;
    let mut bot = lendable_planner(&mut a, true)?;
    bot.guest_tools = profiles::GuestTools::Shell;
    bot.lend_per_hour = 1;
    profiles::save(a.store.connection(), &bot)?;
    a.refresh()?;
    a.store
        .add("workshop", "bob", "@planner please run a check", "human")?;
    let request = last_message_id(&a)?;
    a.poll_delegated()?;
    assert_eq!(a.job.as_ref().unwrap().requester.as_deref(), Some("bob"));
    assert!(a.status.contains("guest shell"));
    assert_eq!(delegate_outcome(&a, request).as_deref(), Some("started"));
    // Second mention while busy still counts as skipped, and quota is already used.
    a.store.add("workshop", "bob", "@planner again", "human")?;
    a.poll_delegated()?;
    assert_eq!(
        delegate_outcome(&a, last_message_id(&a)?).as_deref(),
        Some("busy")
    );
    finish_job_with_decision(&mut a, true)?;
    assert!(
        marker.exists(),
        "Approved guest shell did not run in the guest folder"
    );
    let history = a.store.history("workshop")?;
    assert!(
        history
            .iter()
            .any(|m| m.kind == "bot" && m.body == "Guest shell done")
    );
    assert!(
        history
            .iter()
            .any(|m| m.kind == "tool" && m.body.contains("Approved shell"))
    );
    let bodies = server.join().unwrap()?;
    let tools = bodies[0]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["function"]["name"], "shell");
    let tool_replies: Vec<_> = bodies[1]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|m| m["role"] == "tool")
        .collect();
    assert_eq!(tool_replies.len(), 2);
    assert!(
        tool_replies
            .iter()
            .any(|m| m["content"] == agent::GUEST_BROWSER_DISABLED)
    );
    // Quota already consumed by the started request; a later mention is blocked.
    a.store
        .add("workshop", "bob", "@planner after the first", "human")?;
    let later = last_message_id(&a)?;
    a.poll_delegated()?;
    assert!(a.job.is_none());
    assert_eq!(delegate_outcome(&a, later).as_deref(), Some("quota"));
    let _ = std::fs::remove_dir_all(home_root);
    Ok(())
}

#[test]
fn lent_bot_keeps_tools_off_when_guest_tools_setting_is_off() -> Result<()> {
    // Covered in more detail by lent_bot_answers_a_guest_in_the_group_without_tools_or_private_context;
    // this asserts the new setting defaults off even when lending is enabled.
    let mut a = networked(Store::open(std::path::Path::new(":memory:"))?)?;
    let bot = lendable_planner(&mut a, true)?;
    assert_eq!(bot.guest_tools, profiles::GuestTools::Off);
    Ok(())
}
