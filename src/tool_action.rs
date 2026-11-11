//! Stored tool events are conversation data, never execution authority.
use crate::tool_result::{Execution, Outcome, RECORD_LIMIT, ToolResult, Verification};
use serde::{Deserialize, Serialize};

const PREFIX: &str = "Tool action v1: ";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub command: String,
    pub target: Option<String>,
    pub result: ToolResult,
}
impl Action {
    /// Fit shared-message transport by reducing output only; never alter the command.
    pub fn record(&self) -> Result<String, std::io::Error> {
        let mut stored = self.clone();
        for limit in [16384, 8192, 4096, 2048, 1024, 512, 0] {
            stored.result = self.result.with_output_limit(limit);
            let record = format!(
                "{PREFIX}{}",
                serde_json::to_string(&stored).map_err(std::io::Error::other)?
            );
            if record.len() <= 64 * 1024 {
                return Ok(record);
            }
        }
        Err(std::io::Error::other(
            "Tool record metadata exceeds the 64 KiB transport limit",
        ))
    }
    pub fn decode(body: &str) -> Option<Self> {
        let body = if body.starts_with('[') {
            body.split_once("] ")?.1
        } else {
            body
        };
        let json = body.strip_prefix(PREFIX)?;
        if json.len() > RECORD_LIMIT + 128 * 1024 {
            return None;
        }
        let action: Self = serde_json::from_str(json).ok()?;
        if action.command.len() > 65536 || action.target.as_ref().is_some_and(|s| s.len() > 65536) {
            return None;
        }
        Some(action)
    }
    pub fn title(&self) -> &'static str {
        if !matches!(self.result.scope().tool.as_str(), "shell" | "open_browser") {
            return match self.result.observation().outcome {
                Outcome::Succeeded => "Tool · Succeeded",
                Outcome::Failed => "Tool · Failed",
                Outcome::TimedOut => "Tool · Timed out",
                Outcome::Cancelled => "Tool · Cancelled",
                Outcome::Denied => "Tool · Denied",
                Outcome::Blocked => "Tool · Blocked",
            };
        }
        let browser = self.result.scope().tool == "open_browser";
        match (browser, self.result.observation().outcome) {
            (true, Outcome::Succeeded) => "Browser · Launch requested",
            (true, Outcome::Failed) => "Browser · Failed",
            (true, Outcome::TimedOut) => "Browser · Timed out",
            (true, Outcome::Cancelled) => "Browser · Cancelled",
            (true, Outcome::Denied) => "Browser · Denied",
            (true, Outcome::Blocked) => "Browser · Blocked",
            (false, Outcome::Succeeded) => "Shell · Succeeded",
            (false, Outcome::Failed) => "Shell · Failed",
            (false, Outcome::TimedOut) => "Shell · Timed out",
            (false, Outcome::Cancelled) => "Shell · Cancelled",
            (false, Outcome::Denied) => "Shell · Denied",
            (false, Outcome::Blocked) => "Shell · Blocked",
        }
    }
    pub fn details(&self) -> String {
        let o = self.result.observation();
        let mut text = format!("{}\nCommand\n{}\n", self.title(), self.command);
        if let Some(target) = &self.target {
            text.push_str(&format!("Target: {target}\n"));
        }
        text.push_str(&format!(
            "Request: {}\nAction: {}\nRequester: {}\nWorkspace: {}\n",
            self.result.scope().request_id,
            self.result.scope().action_id,
            self.result.scope().requester,
            self.result.scope().workspace
        ));
        match o.execution {
            Execution::NotStarted => text.push_str("Execution: not started\n"),
            Execution::Started => {
                text.push_str("Execution: started; external effects may have occurred\n")
            }
        }
        if let Some(code) = o.exit_code {
            text.push_str(&format!("Exit: exit status: {code}\n"));
        }
        match &o.verification {
            Verification::Unverified => {
                text.push_str(if self.result.scope().tool == "open_browser" {
                    "Window/page visibility is unverified.\n"
                } else {
                    "Intended effect is unverified.\n"
                })
            }
            Verification::NotApplicable => {}
            Verification::Verified { evidence } => {
                text.push_str(&format!("Effect verified: {evidence}\n"))
            }
        }
        if let Some(diagnostic) = &o.diagnostic {
            text.push_str(&format!("{}\n", diagnostic.message));
        }
        text.push_str(o.stdout.text());
        if !o.stderr.text().is_empty() {
            text.push_str(&format!("\n{}", o.stderr.text()));
        }
        if o.stdout.truncated() || o.stderr.truncated() {
            text.push_str("\n[output truncated at 16 KiB per stream]");
        }
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tool_result::*;
    #[test]
    fn transport_fitting_preserves_commands_and_marks_only_actual_output_loss() {
        let command = "printf \"a\"".repeat(500);
        let result = ToolResult::new(
            Scope {
                request_id: "r".into(),
                action_id: "a".into(),
                requester: "alice".into(),
                tool: "shell".into(),
                workspace: "/tmp".into(),
            },
            Observation {
                outcome: Outcome::Succeeded,
                execution: Execution::Started,
                exit_code: Some(0),
                verification: Verification::Unverified,
                stdout: CapturedStream::capture(&vec![0; 16384], false),
                stderr: CapturedStream::capture(&vec![0; 16384], false),
                diagnostic: None,
            },
        )
        .unwrap();
        let action = Action {
            command: command.clone(),
            target: None,
            result,
        };
        let encoded = action.record().unwrap();
        assert!(encoded.len() <= 65536);
        let stored = Action::decode(&encoded).unwrap();
        assert_eq!(stored.command, command);
        assert!(stored.result.observation().stdout.truncated());
        assert!(stored.details().contains("output truncated"));
    }
    #[test]
    fn new_records_preserve_literal_commands_and_do_not_match_embedded_markers() {
        let action = Action {
            command: "printf '$HOME\\n'".into(),
            target: None,
            result: ToolResult::new(
                Scope {
                    request_id: "r".into(),
                    action_id: "a".into(),
                    requester: "alice".into(),
                    tool: "shell".into(),
                    workspace: "/tmp".into(),
                },
                Observation {
                    outcome: Outcome::Denied,
                    execution: Execution::NotStarted,
                    exit_code: None,
                    verification: Verification::NotApplicable,
                    stdout: CapturedStream::default(),
                    stderr: CapturedStream::default(),
                    diagnostic: None,
                },
            )
            .unwrap(),
        };
        let record = action.record().unwrap();
        assert_eq!(Action::decode(&record).unwrap().command, action.command);
        assert!(Action::decode(&format!("[alice] {record}")).is_some());
        assert!(Action::decode(&format!("arbitrary output\n{record}")).is_none());
        assert!(action.details().contains("Execution: not started"));
        assert!(!action.details().contains("exit status"));
    }
}
