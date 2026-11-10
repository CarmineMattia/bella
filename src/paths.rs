//! Environment variable names and leftover Term Social compatibility.
use std::path::PathBuf;

/// First non-empty value, treating empty strings as unset.
pub fn first_nonempty<I, S>(values: I) -> Option<String>
where
    I: IntoIterator<Item = Option<S>>,
    S: Into<String>,
{
    values
        .into_iter()
        .flatten()
        .map(Into::into)
        .find(|value| !value.is_empty())
}

/// Read `new`, then the leftover `legacy` name, ignoring empty values.
pub fn env_var(new: &str, legacy: &str) -> Option<String> {
    first_nonempty([std::env::var(new).ok(), std::env::var(legacy).ok()])
}

pub fn env_path(new: &str, legacy: &str) -> Option<PathBuf> {
    env_var(new, legacy).map(PathBuf::from)
}

/// Fill CLI defaults from BELLA_* , falling back to TERM_SOCIAL_* .
pub fn apply_data_env(data_is_default: bool, data: &mut PathBuf) {
    if data_is_default && let Some(path) = env_path("BELLA_DATA", "TERM_SOCIAL_DATA") {
        *data = path;
    }
}

pub fn apply_optional_env(value: &mut Option<PathBuf>, new: &str, legacy: &str) {
    if value.is_none() {
        *value = env_path(new, legacy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_nonempty_skips_missing_and_empty() {
        assert_eq!(
            first_nonempty([None, Some(String::new()), Some("keep".to_string())]).as_deref(),
            Some("keep")
        );
        assert_eq!(
            first_nonempty([None::<String>, Some(String::new()), Some(String::new())]),
            None
        );
        assert_eq!(
            first_nonempty([Some("new".to_string()), Some("legacy".to_string())]).as_deref(),
            Some("new")
        );
    }
}
