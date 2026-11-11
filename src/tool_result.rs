//! Facts reported by an executor, never authority to execute or approve an action.
use serde::{Deserialize, Serialize};
use std::fmt;

pub const SCHEMA_VERSION: u8 = 1;
pub const STREAM_LIMIT: usize = 16 * 1024;
// Includes JSON escaping of both streams and bounded metadata.
pub const RECORD_LIMIT: usize = 384 * 1024;
const METADATA_LIMIT: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    pub request_id: String,
    pub action_id: String,
    pub requester: String,
    pub tool: String,
    pub workspace: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
    Denied,
    Blocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Execution {
    NotStarted,
    Started,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum Verification {
    Unverified,
    NotApplicable,
    Verified { evidence: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticKind {
    AuthenticationRequired,
    Unavailable,
    InvalidArguments,
    Policy,
    ExecutionError,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub kind: DiagnosticKind,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedStream {
    text: String,
    truncated: bool,
}

impl CapturedStream {
    /// Convert captured bytes without splitting a UTF-8 codepoint or exceeding the cap.
    /// `truncated` also accounts for data discarded upstream by the executor.
    pub fn capture(bytes: &[u8], truncated: bool) -> Self {
        // Decode a small lookahead so the capture boundary does not turn a valid
        // multibyte character into a replacement character.
        let decoded = String::from_utf8_lossy(&bytes[..bytes.len().min(STREAM_LIMIT + 3)]);
        let mut end = decoded.len().min(STREAM_LIMIT);
        while !decoded.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            text: decoded[..end].to_owned(),
            truncated: truncated || bytes.len() > STREAM_LIMIT || decoded.len() > end,
        }
    }

    fn relimit(&self, limit: usize) -> Self {
        let mut end = self.text.len().min(limit);
        while !self.text.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            text: self.text[..end].into(),
            truncated: self.truncated || end < self.text.len(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub outcome: Outcome,
    pub execution: Execution,
    pub exit_code: Option<i32>,
    pub verification: Verification,
    pub stdout: CapturedStream,
    pub stderr: CapturedStream,
    pub diagnostic: Option<Diagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WireRecord", into = "WireRecord")]
pub struct ToolResult {
    scope: Scope,
    observation: Observation,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WireRecord {
    schema_version: u8,
    scope: Scope,
    observation: Observation,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidResult(String);

impl fmt::Display for InvalidResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for InvalidResult {}

fn invalid(message: &str) -> InvalidResult {
    InvalidResult(message.into())
}

fn metadata(value: &str) -> Result<(), InvalidResult> {
    if value.trim().is_empty()
        || value.len() > METADATA_LIMIT
        || value.chars().any(char::is_control)
    {
        return Err(invalid(
            "Metadata must be nonempty, bounded and free of control characters",
        ));
    }
    Ok(())
}

impl ToolResult {
    pub fn new(scope: Scope, observation: Observation) -> Result<Self, InvalidResult> {
        for value in [
            &scope.request_id,
            &scope.action_id,
            &scope.requester,
            &scope.tool,
            &scope.workspace,
        ] {
            metadata(value)?;
        }
        for stream in [&observation.stdout, &observation.stderr] {
            if stream.text.len() > STREAM_LIMIT {
                return Err(invalid("Captured stream exceeds the output limit"));
            }
        }
        if let Some(diagnostic) = &observation.diagnostic
            && (diagnostic.message.trim().is_empty() || diagnostic.message.len() > METADATA_LIMIT)
        {
            return Err(invalid("Diagnostic must be nonempty and bounded"));
        }
        if observation.execution == Execution::NotStarted
            && (observation.exit_code.is_some()
                || !observation.stdout.text.is_empty()
                || !observation.stderr.text.is_empty()
                || observation.stdout.truncated
                || observation.stderr.truncated)
        {
            return Err(invalid(
                "An unstarted action cannot have an exit code or captured output",
            ));
        }
        match observation.outcome {
            Outcome::Succeeded => {
                if observation.execution != Execution::Started || observation.exit_code != Some(0) {
                    return Err(invalid(
                        "Success requires a started process and exit code zero",
                    ));
                }
            }
            Outcome::Denied | Outcome::Blocked => {
                if observation.execution != Execution::NotStarted
                    || observation.verification != Verification::NotApplicable
                {
                    return Err(invalid(
                        "Denied/blocked actions must be unstarted and not applicable to verification",
                    ));
                }
            }
            Outcome::TimedOut => {
                if observation.execution != Execution::Started {
                    return Err(invalid("Execution timeout requires a started process"));
                }
            }
            Outcome::Failed | Outcome::Cancelled => {}
        }
        if observation.outcome == Outcome::Failed && observation.exit_code == Some(0) {
            return Err(invalid("Failed process outcome cannot have exit code zero"));
        }
        if let Verification::Verified { evidence } = &observation.verification {
            metadata(evidence)?;
            if observation.outcome != Outcome::Succeeded {
                return Err(invalid(
                    "Only successful actions can claim a verified intended effect",
                ));
            }
        }
        Ok(Self { scope, observation })
    }

    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    pub fn observation(&self) -> &Observation {
        &self.observation
    }

    /// Started actions may have changed external state even after failure or timeout.
    pub fn effects_may_have_occurred(&self) -> bool {
        self.observation.execution == Execution::Started
    }

    /// Process success alone never proves a browser window/page or other intended effect.
    pub fn effect_verified(&self) -> bool {
        matches!(self.observation.verification, Verification::Verified { .. })
    }

    pub fn with_output_limit(&self, limit: usize) -> Self {
        let mut copy = self.clone();
        copy.observation.stdout = copy.observation.stdout.relimit(limit);
        copy.observation.stderr = copy.observation.stderr.relimit(limit);
        copy
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Bound the whole record before allocating decoded fields from external history.
    pub fn from_json(record: &str) -> Result<Self, InvalidResult> {
        if record.len() > RECORD_LIMIT {
            return Err(invalid("Tool result record exceeds the input limit"));
        }
        serde_json::from_str(record).map_err(|error| InvalidResult(error.to_string()))
    }
}

impl TryFrom<WireRecord> for ToolResult {
    type Error = InvalidResult;

    fn try_from(record: WireRecord) -> Result<Self, Self::Error> {
        if record.schema_version != SCHEMA_VERSION {
            return Err(invalid("Unsupported tool result schema version"));
        }
        Self::new(record.scope, record.observation)
    }
}

impl From<ToolResult> for WireRecord {
    fn from(result: ToolResult) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            scope: result.scope,
            observation: result.observation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope {
            request_id: "request-1".into(),
            action_id: "action-1".into(),
            requester: "alice".into(),
            tool: "shell".into(),
            workspace: "/tmp/workspace".into(),
        }
    }

    fn observation(outcome: Outcome) -> Observation {
        let unstarted = matches!(outcome, Outcome::Denied | Outcome::Blocked);
        Observation {
            outcome,
            execution: if unstarted {
                Execution::NotStarted
            } else {
                Execution::Started
            },
            exit_code: match outcome {
                Outcome::Succeeded => Some(0),
                Outcome::Failed => Some(1),
                Outcome::TimedOut => Some(124),
                _ => None,
            },
            verification: if unstarted {
                Verification::NotApplicable
            } else {
                Verification::Unverified
            },
            stdout: CapturedStream::default(),
            stderr: CapturedStream::default(),
            diagnostic: None,
        }
    }

    #[test]
    fn interrupted_process_can_exit_zero_without_claiming_completion() {
        for outcome in [Outcome::Cancelled, Outcome::TimedOut] {
            let mut obs = observation(outcome);
            obs.exit_code = Some(0);
            let result = ToolResult::new(scope(), obs).unwrap();
            assert!(!result.effect_verified());
            assert!(result.effects_may_have_occurred());
            assert_eq!(result.observation().outcome, outcome);
        }
    }
    #[test]
    fn all_outcomes_round_trip_with_scope_and_diagnostics() {
        for outcome in [
            Outcome::Succeeded,
            Outcome::Failed,
            Outcome::TimedOut,
            Outcome::Cancelled,
            Outcome::Denied,
            Outcome::Blocked,
        ] {
            let mut obs = observation(outcome);
            obs.diagnostic = Some(Diagnostic {
                kind: DiagnosticKind::Other,
                message: "Observed detail".into(),
            });
            let result = ToolResult::new(scope(), obs).unwrap();
            assert_eq!(
                ToolResult::from_json(&result.to_json().unwrap()).unwrap(),
                result
            );
            assert_eq!(result.scope().requester, "alice");
        }
    }

    #[test]
    fn launcher_success_is_unverified_until_evidence_is_supplied() {
        let mut target = scope();
        target.tool = "open_browser".into();
        let mut obs = observation(Outcome::Succeeded);
        let launch = ToolResult::new(target.clone(), obs.clone()).unwrap();
        assert!(!launch.effect_verified());
        assert!(launch.effects_may_have_occurred());
        obs.verification = Verification::Verified {
            evidence: "Independent page check: target URL matched".into(),
        };
        assert!(ToolResult::new(target, obs).unwrap().effect_verified());
    }

    #[test]
    fn failures_and_timeouts_preserve_uncertain_effects_without_implying_denial() {
        for outcome in [Outcome::Failed, Outcome::TimedOut, Outcome::Cancelled] {
            let result = ToolResult::new(scope(), observation(outcome)).unwrap();
            assert!(result.effects_may_have_occurred());
            assert!(!result.effect_verified());
            assert_eq!(result.observation().outcome, outcome);
        }
        let mut auth = observation(Outcome::Failed);
        auth.diagnostic = Some(Diagnostic {
            kind: DiagnosticKind::AuthenticationRequired,
            message: "sudo requires authentication".into(),
        });
        assert_eq!(
            ToolResult::new(scope(), auth)
                .unwrap()
                .observation()
                .outcome,
            Outcome::Failed
        );
    }

    #[test]
    fn denied_and_blocked_results_cannot_describe_execution() {
        for outcome in [Outcome::Denied, Outcome::Blocked] {
            let base = observation(outcome);
            let result = ToolResult::new(scope(), base.clone()).unwrap();
            assert!(!result.effects_may_have_occurred());
            let mut invalid = base.clone();
            invalid.execution = Execution::Started;
            assert!(ToolResult::new(scope(), invalid).is_err());
            let mut invalid = base.clone();
            invalid.stdout = CapturedStream::capture(b"executed", false);
            assert!(ToolResult::new(scope(), invalid).is_err());
            let mut invalid = base.clone();
            invalid.exit_code = Some(1);
            assert!(ToolResult::new(scope(), invalid).is_err());
            let mut invalid = base;
            invalid.verification = Verification::Unverified;
            assert!(ToolResult::new(scope(), invalid).is_err());
        }
    }

    #[test]
    fn rejects_inconsistent_exit_state_and_false_verification() {
        let mut obs = observation(Outcome::Succeeded);
        obs.exit_code = Some(2);
        assert!(ToolResult::new(scope(), obs).is_err());
        let mut obs = observation(Outcome::Succeeded);
        obs.execution = Execution::NotStarted;
        assert!(ToolResult::new(scope(), obs).is_err());
        let mut obs = observation(Outcome::TimedOut);
        obs.execution = Execution::NotStarted;
        obs.exit_code = None;
        assert!(ToolResult::new(scope(), obs).is_err());
        let mut obs = observation(Outcome::Failed);
        obs.exit_code = Some(0);
        assert!(ToolResult::new(scope(), obs).is_err());
        let mut obs = observation(Outcome::Failed);
        obs.verification = Verification::Verified {
            evidence: "claimed success".into(),
        };
        assert!(ToolResult::new(scope(), obs).is_err());
        let mut obs = observation(Outcome::Succeeded);
        obs.verification = Verification::Verified {
            evidence: " ".into(),
        };
        assert!(ToolResult::new(scope(), obs).is_err());
    }

    #[test]
    fn spawn_failure_and_cancel_before_start_do_not_claim_execution() {
        for outcome in [Outcome::Failed, Outcome::Cancelled] {
            let mut obs = observation(outcome);
            obs.execution = Execution::NotStarted;
            obs.exit_code = None;
            let result = ToolResult::new(scope(), obs).unwrap();
            assert!(!result.effects_may_have_occurred());
        }
    }

    #[test]
    fn output_caps_preserve_utf8_and_actual_truncation() {
        let exact = vec![b'a'; STREAM_LIMIT];
        assert!(!CapturedStream::capture(&exact, false).truncated());
        let large = "界".repeat(STREAM_LIMIT);
        let stream = CapturedStream::capture(large.as_bytes(), false);
        assert!(stream.truncated());
        assert!(stream.text().len() <= STREAM_LIMIT);
        assert!(stream.text().starts_with('界'));
        assert!(!stream.text().contains('�'));
        assert!(stream.text().ends_with('界'));
        assert!(CapturedStream::capture(b"small", true).truncated());
        assert!(!CapturedStream::capture("hello\n世界".as_bytes(), false).truncated());
        let lossy = CapturedStream::capture(&vec![0xff; STREAM_LIMIT], false);
        assert!(lossy.truncated());
        assert!(lossy.text().len() <= STREAM_LIMIT);
    }

    #[test]
    fn external_records_are_validated_as_strictly_as_local_construction() {
        let result = ToolResult::new(scope(), observation(Outcome::Succeeded)).unwrap();
        let original = serde_json::to_value(result).unwrap();
        for (path, replacement) in [
            ("/schema_version", serde_json::json!(2)),
            ("/scope/request_id", serde_json::json!("")),
            ("/scope/requester", serde_json::json!("alice\nowner")),
            ("/observation/exit_code", serde_json::json!(1)),
            (
                "/observation/stdout/text",
                serde_json::json!("a".repeat(STREAM_LIMIT + 1)),
            ),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(path).unwrap() = replacement;
            assert!(ToolResult::from_json(&value.to_string()).is_err(), "{path}");
            assert!(
                serde_json::from_value::<ToolResult>(value).is_err(),
                "{path}"
            );
        }
        let mut unknown = original;
        unknown["approval"] = serde_json::json!("granted");
        assert!(ToolResult::from_json(&unknown.to_string()).is_err());
        assert!(ToolResult::from_json(&" ".repeat(RECORD_LIMIT + 1)).is_err());
        assert!(ToolResult::from_json("not a record").is_err());
    }

    #[test]
    fn maximum_escaped_record_round_trips_within_the_input_limit() {
        let field = "\"".repeat(METADATA_LIMIT);
        let scope = Scope {
            request_id: field.clone(),
            action_id: field.clone(),
            requester: field.clone(),
            tool: field.clone(),
            workspace: field.clone(),
        };
        let mut obs = observation(Outcome::Succeeded);
        obs.stdout = CapturedStream::capture(&vec![0; STREAM_LIMIT], false);
        obs.stderr = obs.stdout.clone();
        obs.diagnostic = Some(Diagnostic {
            kind: DiagnosticKind::Other,
            message: "\0".repeat(METADATA_LIMIT),
        });
        obs.verification = Verification::Verified { evidence: field };
        let result = ToolResult::new(scope, obs).unwrap();
        let json = result.to_json().unwrap();
        assert!(json.len() <= RECORD_LIMIT);
        assert_eq!(ToolResult::from_json(&json).unwrap(), result);
    }

    #[test]
    fn forged_denial_and_unknown_nested_fields_are_rejected_on_read() {
        let result = ToolResult::new(scope(), observation(Outcome::Succeeded)).unwrap();
        let original = serde_json::to_value(result).unwrap();
        let mut denial = original.clone();
        denial["observation"]["outcome"] = serde_json::json!("denied");
        assert!(ToolResult::from_json(&denial.to_string()).is_err());
        let mut unknown = original;
        unknown["scope"]["approved"] = serde_json::json!(true);
        assert!(ToolResult::from_json(&unknown.to_string()).is_err());
    }
}
