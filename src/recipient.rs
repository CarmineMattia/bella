//! Explicit composer routing is a local per-owner/chat preference, never an incoming-message action.
use crate::{App, editor::Editor};
use anyhow::Result;
impl App {
    pub(crate) fn recipient_handle(&self) -> Option<&str> {
        self.recipient
            .as_deref()
            .or_else(|| self.room.starts_with("agent:").then_some("bot"))
    }
    pub(crate) fn preview_recipient(&self) -> Option<String> {
        self.requested_bot(self.input.text.trim())
            .map(|p| p.handle)
            .or_else(|| self.recipient_handle().map(str::to_owned))
    }
    pub(crate) fn available_recipient(&self, handle: &str) -> bool {
        handle == "bot"
            || (self.enabled_bots.iter().any(|h| h == handle)
                && self
                    .profiles
                    .iter()
                    .any(|p| p.owner == self.name && p.handle == handle))
    }
    fn recipient_options(&self) -> Vec<Option<String>> {
        let mut options = vec![];
        if !self.room.starts_with("agent:") {
            options.push(None);
        }
        options.push(Some("bot".into()));
        options.extend(
            self.profiles
                .iter()
                .filter(|p| {
                    p.owner == self.name
                        && p.handle != "bot"
                        && !p.handle.is_empty()
                        && self.enabled_bots.contains(&p.handle)
                })
                .map(|p| Some(p.handle.clone())),
        );
        options
    }
    pub(crate) fn remember_recipient(&mut self, handle: Option<String>) -> Result<()> {
        self.store
            .set_recipient(&self.name, &self.room, handle.as_deref())?;
        self.recipient = handle;
        Ok(())
    }
    pub(crate) fn select_recipient(&mut self, handle: Option<String>) -> Result<()> {
        if handle.is_none() && self.room.starts_with("agent:") {
            self.status = "My agent always addresses a bot · open a group for ordinary chat".into();
            return Ok(());
        }
        if let Some(handle) = &handle
            && !self.available_recipient(handle)
        {
            self.status = format!("@{handle} is unavailable here · add your bot with F8");
            return Ok(());
        }
        // A typed leading @bot is the address, not the message body. Changing the
        // recipient removes that address while keeping the draft's actual content.
        if self.requested_bot(self.input.text.trim_start()).is_some() {
            let previous_cursor = self.input.cursor;
            let body = self
                .input
                .text
                .trim_start()
                .split_once(char::is_whitespace)
                .map(|(_, body)| body.trim_start())
                .unwrap_or("")
                .to_owned();
            let removed = self.input.text.len() - body.len();
            self.input = Editor::new(body);
            self.input.cursor = previous_cursor
                .saturating_sub(removed)
                .min(self.input.text.len());
            self.save_draft()?;
        }
        self.remember_recipient(handle)?;
        self.status = if let Some(handle) = self.recipient_handle() {
            format!("To @{handle} · no tag needed · click Group to return to chat")
        } else {
            "To group · messages won't ask a bot unless you mention one".into()
        };
        Ok(())
    }
    pub(crate) fn cycle_recipient(&mut self, bots_only: bool) -> Result<()> {
        let mut options = self.recipient_options();
        if bots_only {
            options.retain(Option::is_some);
        }
        let current = self.preview_recipient();
        let next = options
            .iter()
            .position(|p| p == &current)
            .map(|i| (i + 1) % options.len())
            .unwrap_or(0);
        self.select_recipient(options[next].clone())
    }
}
