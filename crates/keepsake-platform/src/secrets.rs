//! The Claude API key (ADR-036), kept in the OS's credential store:
//! Windows Credential Manager, the macOS Keychain. Never in a file.
//!
//! `KEEPSAKE_CLAUDE_KEY`, when set, is used instead (for the CLI and tests);
//! saving then changes nothing in the store.

use crate::devices::PlatformError;

const SERVICE: &str = "Keepsake";
const ACCOUNT: &str = "claude-api-key";
const ENV: &str = "KEEPSAKE_CLAUDE_KEY";

fn entry() -> Result<keyring::Entry, PlatformError> {
    keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| PlatformError::Os(e.to_string()))
}

fn from_env() -> Option<String> {
    std::env::var(ENV)
        .ok()
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
}

/// The saved key, if any.
pub fn claude_key() -> Result<Option<String>, PlatformError> {
    if let Some(k) = from_env() {
        return Ok(Some(k));
    }
    match entry()?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(PlatformError::Os(e.to_string())),
    }
}

/// Save the key (`None`: remove it).
pub fn set_claude_key(key: Option<&str>) -> Result<(), PlatformError> {
    if from_env().is_some() {
        return Ok(());
    }
    let e = entry()?;
    match key {
        Some(k) => e
            .set_password(k.trim())
            .map_err(|e| PlatformError::Os(e.to_string())),
        None => match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(PlatformError::Os(e.to_string())),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_environment_key_comes_first_and_is_not_saved() {
        // One test touches the variable, so tests running in parallel
        // cannot see it change.
        std::env::set_var(ENV, "  sk-ant-from-the-environment  ");
        assert_eq!(
            claude_key().unwrap().as_deref(),
            Some("sk-ant-from-the-environment")
        );
        set_claude_key(Some("sk-ant-other")).unwrap();
        assert_eq!(
            claude_key().unwrap().as_deref(),
            Some("sk-ant-from-the-environment")
        );
        std::env::remove_var(ENV);
    }
}
