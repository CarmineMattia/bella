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

/// Guest command folders live beside the database: `<data-dir>/guests/<sanitized-handle>/`.
/// For the packaged app that is `$XDG_DATA_HOME/bella/guests/...`.
pub fn guest_root(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("guests")
}

/// Turn an account handle into a single, non-absolute path component.
/// `../`, `/`, NUL and other separators never appear in the result.
pub fn sanitize_guest_handle(raw: &str) -> anyhow::Result<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        anyhow::bail!("Missing guest account name");
    }
    if raw.contains('\0') {
        anyhow::bail!("Guest account name contains invalid characters");
    }
    let mut out = String::new();
    for c in raw.chars() {
        if c.is_ascii_alphanumeric()
            || matches!(c, '_' | '-')
            || (c == '.' && !out.is_empty() && !out.ends_with('.'))
        {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    while out.starts_with('.') || out.starts_with('-') {
        out.remove(0);
    }
    while out.ends_with('.') {
        out.pop();
    }
    if out.is_empty() || out == ".." {
        out = "guest".into();
    }
    if out.len() > 48 {
        out.truncate(48);
        while out.ends_with('.') {
            out.pop();
        }
        if out.is_empty() {
            out = "guest".into();
        }
    }
    if out != raw {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in raw.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        out = format!("{out}-{:08x}", hash as u32);
    }
    let path = std::path::Path::new(&out);
    if out.contains('/')
        || out.contains('\\')
        || out == ".."
        || out == "."
        || path.is_absolute()
        || path.components().count() != 1
    {
        anyhow::bail!("Refusing unsafe guest folder name");
    }
    Ok(out)
}

pub fn guest_home(data_dir: &std::path::Path, requester: &str) -> anyhow::Result<PathBuf> {
    let name = sanitize_guest_handle(requester)?;
    let root = guest_root(data_dir);
    let home = root.join(&name);
    if !home.starts_with(&root) {
        anyhow::bail!("Refusing guest folder outside the guests directory");
    }
    Ok(home)
}

/// Create the per-guest folder (mode 0700) and a `tmp` subdirectory.
pub fn prepare_guest_home(data_dir: &std::path::Path, requester: &str) -> anyhow::Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    let home = guest_home(data_dir, requester)?;
    let tmp = home.join("tmp");
    for dir in [&home, &tmp] {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if let (Ok(home_c), Ok(root_c)) = (home.canonicalize(), guest_root(data_dir).canonicalize())
        && !home_c.starts_with(&root_c)
    {
        anyhow::bail!("Guest folder escaped the guests directory");
    }
    Ok(home)
}

/// Environment for a guest command: cleared, then a small whitelist. Owner secrets are not inherited.
pub fn guest_command_env(home: &std::path::Path) -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    use std::ffi::OsString;
    let tmp = home.join("tmp");
    let path = std::env::var_os("PATH")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| OsString::from("/usr/local/bin:/usr/bin:/bin"));
    let mut env = vec![
        (OsString::from("HOME"), home.into()),
        (OsString::from("PATH"), path),
        (OsString::from("TMPDIR"), tmp.clone().into()),
        (OsString::from("TMP"), tmp.clone().into()),
        (OsString::from("TEMP"), tmp.into()),
        (OsString::from("SHELL"), OsString::from("/bin/sh")),
    ];
    for key in ["LANG", "LC_ALL", "LC_CTYPE", "LANGUAGE", "TERM", "TZ"] {
        if let Some(value) = std::env::var_os(key).filter(|v| !v.is_empty()) {
            env.push((OsString::from(key), value));
        }
    }
    env
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

    #[test]
    fn sanitize_guest_handle_rejects_traversal_and_absolute_paths() -> anyhow::Result<()> {
        assert_eq!(sanitize_guest_handle("bob")?, "bob");
        assert_eq!(sanitize_guest_handle("Alice_1")?, "Alice_1");
        for raw in [
            "../etc",
            "/tmp/x",
            "alice/../bob",
            "..",
            ".",
            "...",
            "foo/bar",
            r"foo\bar",
        ] {
            let name = sanitize_guest_handle(raw)?;
            let path = std::path::Path::new(&name);
            assert_eq!(path.components().count(), 1, "{raw:?} -> {name}");
            assert!(!path.is_absolute(), "{raw:?} -> {name}");
            assert!(!name.contains('/'), "{raw:?} -> {name}");
            assert!(!name.contains('\\'), "{raw:?} -> {name}");
            assert_ne!(name, "..");
            assert_ne!(name, ".");
            assert!(!name.contains(".."), "{raw:?} -> {name}");
        }
        assert!(sanitize_guest_handle("").is_err());
        assert!(sanitize_guest_handle("   ").is_err());
        assert!(
            sanitize_guest_handle("a\0b").is_err(),
            "NUL must be rejected"
        );
        let data = std::env::temp_dir().join(format!("bella-guest-path-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data)?;
        let home = prepare_guest_home(&data, "bob")?;
        assert_eq!(home, data.join("guests").join("bob"));
        assert!(home.is_dir());
        assert!(home.join("tmp").is_dir());
        let meta = std::fs::metadata(&home)?;
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        let escaped = prepare_guest_home(&data, "../etc")?;
        assert!(escaped.starts_with(data.join("guests")));
        assert!(
            !escaped.ends_with("etc")
                || escaped.file_name().unwrap() != std::ffi::OsStr::new("etc")
        );
        std::fs::remove_dir_all(&data)?;
        Ok(())
    }

    #[test]
    fn guest_command_env_sets_home_and_omits_unlisted_variables() {
        let home = std::path::Path::new("/tmp/bella-guest-home");
        let env = guest_command_env(home);
        let keys: Vec<String> = env
            .iter()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(keys.contains(&"HOME".into()));
        assert!(keys.contains(&"PATH".into()));
        assert!(keys.contains(&"TMPDIR".into()));
        assert!(!keys.contains(&"SECRET_TOKEN".into()));
        assert!(!keys.contains(&"SSH_AUTH_SOCK".into()));
        assert!(!keys.contains(&"SUDO_ASKPASS".into()));
        assert!(!keys.contains(&"AWS_SECRET_ACCESS_KEY".into()));
        let home_val = env
            .iter()
            .find(|(k, _)| k == "HOME")
            .unwrap()
            .1
            .to_string_lossy();
        assert_eq!(home_val, home.to_string_lossy());
    }
}
