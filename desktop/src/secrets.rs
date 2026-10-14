//! Connection passwords in the system credential store, keyed by profile id.

const SERVICE: &str = "dev.kafkalens.desktop";

pub fn password(profile_id: &str) -> Option<String> {
    keyring::Entry::new(SERVICE, profile_id)
        .ok()?
        .get_password()
        .ok()
}

/// Stores the password, or removes it when `None` or empty.
pub fn set_password(profile_id: &str, password: Option<&str>) {
    let Ok(entry) = keyring::Entry::new(SERVICE, profile_id) else {
        return;
    };
    let result = match password {
        Some(password) if !password.is_empty() => entry.set_password(password),
        // Nothing stored is fine too.
        _ => entry.delete_credential().or(Ok(())),
    };
    if let Err(err) = result {
        eprintln!("kafkalens: could not update the stored password: {err}");
    }
}
