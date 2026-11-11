//! The API keys of the AI providers (ADR-036, ADR-038), kept in the OS's
//! credential store: Windows Credential Manager, the macOS Keychain. Never
//! in a file.
//!
//! Each provider has its own entry, `<id>-api-key` (`claude-api-key`, the
//! one Tag with Claude used, stays). `KEEPSAKE_<ID>_KEY` (such as
//! `KEEPSAKE_CLAUDE_KEY`), when set, is used instead (for the CLI and tests);
//! saving then changes nothing in the store.

use crate::devices::PlatformError;

const SERVICE: &str = "Keepsake";

/// A provider id is lowercase letters only (`claude`, `openai`, `gemini`).
fn check(provider: &str) -> Result<(), PlatformError> {
    if !provider.is_empty() && provider.bytes().all(|b| b.is_ascii_lowercase()) {
        Ok(())
    } else {
        Err(PlatformError::Os(format!(
            "not a provider id: {provider:?}"
        )))
    }
}

fn account(provider: &str) -> String {
    format!("{provider}-api-key")
}

fn env_var(provider: &str) -> String {
    format!("KEEPSAKE_{}_KEY", provider.to_ascii_uppercase())
}

fn entry(provider: &str) -> Result<keyring::Entry, PlatformError> {
    keyring::Entry::new(SERVICE, &account(provider)).map_err(|e| PlatformError::Os(e.to_string()))
}

fn from_env(provider: &str) -> Option<String> {
    std::env::var(env_var(provider))
        .ok()
        .map(|k| k.trim().to_owned())
        .filter(|k| !k.is_empty())
}

/// `provider`'s saved key, if any.
pub fn api_key(provider: &str) -> Result<Option<String>, PlatformError> {
    check(provider)?;
    if let Some(k) = from_env(provider) {
        return Ok(Some(k));
    }
    match entry(provider)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(PlatformError::Os(e.to_string())),
    }
}

/// Save `provider`'s key (`None`: remove it).
pub fn set_api_key(provider: &str, key: Option<&str>) -> Result<(), PlatformError> {
    check(provider)?;
    if from_env(provider).is_some() {
        return Ok(());
    }
    let e = entry(provider)?;
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
    fn each_provider_has_its_own_entry_and_variable() {
        assert_eq!(
            account("claude"),
            "claude-api-key",
            "the entry Claude used stays"
        );
        assert_eq!(env_var("claude"), "KEEPSAKE_CLAUDE_KEY");
        assert_eq!(env_var("openai"), "KEEPSAKE_OPENAI_KEY");
        assert!(api_key("Open AI").is_err());
        assert!(set_api_key("", None).is_err());
    }

    #[test]
    fn the_environment_key_comes_first_and_is_not_saved() {
        // Only this test touches this variable, so tests running in
        // parallel cannot see it change.
        let var = env_var("gemini");
        std::env::set_var(&var, "  AIza-from-the-environment  ");
        assert_eq!(
            api_key("gemini").unwrap().as_deref(),
            Some("AIza-from-the-environment")
        );
        set_api_key("gemini", Some("AIza-other")).unwrap();
        assert_eq!(
            api_key("gemini").unwrap().as_deref(),
            Some("AIza-from-the-environment")
        );
        std::env::remove_var(&var);
    }
}
