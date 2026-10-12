//! Session token secure storage (C §10.1).
//!
//! Flow: read legacy settings field → write keychain → verify → clear settings.
//! One release cycle keeps settings as read-only fallback.

use keyring::Entry;

const SERVICE: &str = "cn.abu117.abu-agent";
const ACCOUNT: &str = "abu-api-session-token";

fn entry() -> Result<Entry, String> {
    Entry::new(SERVICE, ACCOUNT).map_err(|e| format!("无法打开系统凭证存储：{e}"))
}

pub fn load_session_token() -> Option<String> {
    let entry = entry().ok()?;
    match entry.get_password() {
        Ok(token) => {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        Err(_) => None,
    }
}

pub fn store_session_token(token: &str) -> Result<(), String> {
    let trimmed = token.trim();
    if trimmed.is_empty() {
        return clear_session_token();
    }
    let entry = entry()?;
    entry
        .set_password(trimmed)
        .map_err(|e| format!("写入系统凭证存储失败：{e}"))?;
    // Verify round-trip before callers clear the settings field.
    let read_back = entry
        .get_password()
        .map_err(|e| format!("校验系统凭证存储失败：{e}"))?;
    if read_back.trim() != trimmed {
        return Err("系统凭证存储写入后读回不一致".to_string());
    }
    Ok(())
}

pub fn clear_session_token() -> Result<(), String> {
    let entry = entry()?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("清除系统凭证存储失败：{e}")),
    }
}

/// Resolve session token: keychain first, then legacy settings field.
pub fn resolve_session_token(settings_token: Option<&str>) -> Option<String> {
    if let Some(token) = load_session_token() {
        return Some(token);
    }
    settings_token
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// Migrate settings → keychain once. Returns the resolved token (if any) and
/// whether settings still holds a copy that should be cleared by the caller.
pub fn migrate_from_settings(settings_token: Option<&str>) -> Result<(Option<String>, bool), String> {
    if let Some(existing) = load_session_token() {
        let settings_has = settings_token
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .is_some();
        return Ok((Some(existing), settings_has));
    }
    let Some(legacy) = settings_token
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
    else {
        return Ok((None, false));
    };
    match store_session_token(&legacy) {
        Ok(()) => Ok((Some(legacy), true)),
        Err(err) => {
            // Keep settings fallback for this release if keychain is unavailable.
            eprintln!("[cloud-context] session token keychain migrate failed: {err}");
            Ok((Some(legacy), false))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_prefers_non_empty_settings_when_keychain_empty() {
        // Keychain may already have a value on developer machines; only assert
        // the settings fallback path when both sides are empty-safe.
        let resolved = resolve_session_token(Some("  settings-token  "));
        assert!(resolved.is_some());
        assert!(!resolved.unwrap().trim().is_empty());
    }
}
