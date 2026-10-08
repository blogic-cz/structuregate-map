//! `--facts-pull`: EACH DOCUMENT SAVED AS A SNAPSHOT, and nothing else. A map run never touches the network - it
//! reads what this saved - so a refresh stays as fast and as repeatable as the files it is given.
//!
//! A source's version is asked first (`files/{id}?fields=version,...`) and the document downloaded only when it
//! moved. A `save` ending in `.json` gets the document AS GOOGLE KEEPS IT (`documents.get` of the Docs API, every
//! tab): headings, bold runs and table cells as structure, read by `reader.rs` without a conversion in between -
//! the form to prefer. Any other `save` gets Google's Markdown export (`files/{id}/export?mimeType=text/markdown`).
//! The snapshot and its `<snapshot>.meta.json` are each written beside the target and
//! renamed over it, so a pull that fails half way leaves the last good snapshot whole.
//!
//! The token is the consumer's: an environment variable or a command that prints one (`gcloud auth
//! print-access-token`). Nothing is stored.

use super::config::{Config, Source};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Where the Drive API and the Docs API are. `STRUCTUREGATE_DRIVE_URL` points the tests at one fake for both.
fn base() -> (String, String) {
    match std::env::var("STRUCTUREGATE_DRIVE_URL").ok().filter(|u| !u.is_empty()) {
        Some(fake) => (fake.clone(), fake),
        None => ("https://www.googleapis.com".into(), "https://docs.googleapis.com".into()),
    }
}

pub fn meta_path(snapshot: &Path) -> PathBuf {
    let mut name = snapshot.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(".meta.json");
    snapshot.with_file_name(name)
}

/// Every source brought up to date. 0 when each is, 1 when one failed (the rest are still pulled).
pub fn run(config: &Config, out: &mut Vec<String>) -> i64 {
    let mut token: Option<Result<String, String>> = None;
    let mut failed = false;
    let (drive, docs) = base();
    // WHOSE TOKEN, WITH WHAT SCOPE - asked once, when there is one: a refused token has two usual causes, and only
    // this says which.
    let mut holder: Option<Option<Holder>> = None;
    for source in &config.sources {
        if source.kind != "google-doc" {
            out.push(format!("  {}: a `{}` source, kept by hand - nothing to pull ({})", source.name, source.kind, source.shown));
            continue;
        }
        let bearer = match token.get_or_insert_with(|| bearer(config)) {
            Ok(t) => t.clone(),
            Err(why) => {
                out.push(format!("  {}: FAILED - {why}", source.name));
                failed = true;
                continue;
            }
        };
        let who = holder.get_or_insert_with(|| {
            let who = holder_of(&drive, &bearer);
            if let Some(h) = &who
                && h.scopes.iter().any(|s| s == FULL_DRIVE)
            {
                out.push(format!(
                    "  note: the token ({}) may WRITE every file of its Drive (`{FULL_DRIVE}`); a pull only reads - see skills/map-sqlite/facts.md for a read-only one",
                    h.email
                ));
            }
            who
        });
        match pull(source, &bearer, &drive, &docs) {
            Ok(said) => out.push(format!("  {}: {said}", source.name)),
            Err(why) => {
                let why = if why.starts_with(REFUSED) { refused(&why, who.as_ref()) } else { why };
                out.push(format!("  {}: FAILED - {why}", source.name));
                failed = true;
            }
        }
    }
    i64::from(failed)
}

fn pull(source: &Source, bearer: &str, drive: &str, docs: &str) -> Result<String, String> {
    let about = get(&format!("{drive}/drive/v3/files/{}?fields=version,modifiedTime,name&supportsAllDrives=true", source.id), bearer)?;
    let about: Value = serde_json::from_slice(&about).map_err(|e| format!("the document's version could not be read - {e}"))?;
    let version = match &about["version"] {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let meta = meta_path(&source.path);
    let kept: Value = std::fs::read_to_string(&meta).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if source.path.is_file() && kept["version"].as_str() == Some(version.as_str()) {
        return Ok(format!("unchanged (version {version}) - {}", source.shown));
    }
    let structured = source.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("json"));
    let body = if structured {
        get(&format!("{docs}/v1/documents/{}?includeTabsContent=true", source.id), bearer)?
    } else {
        get(&format!("{drive}/drive/v3/files/{}/export?mimeType=text/markdown", source.id), bearer)?
    };
    let pulled = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let record = json!({
        "id": source.id, "name": about["name"], "version": version, "modifiedTime": about["modifiedTime"], "pulled": pulled,
    });
    replace(&source.path, &body)?;
    replace(&meta, serde_json::to_string_pretty(&record).unwrap_or_default().as_bytes())?;
    let was = kept["version"].as_str().map(|v| format!(" (was {v})")).unwrap_or_default();
    Ok(format!("saved version {version}{was}, {} byte(s) -> {}", body.len(), source.shown))
}

fn get(url: &str, bearer: &str) -> Result<Vec<u8>, String> {
    let response = ureq::get(url).header("Authorization", &format!("Bearer {bearer}")).call();
    match response {
        Ok(mut ok) => ok.body_mut().with_config().limit(64 * 1024 * 1024).read_to_vec().map_err(|e| format!("the download broke off - {e}")),
        Err(ureq::Error::StatusCode(code @ (401 | 403))) => Err(format!("{REFUSED} (HTTP {code})")),
        Err(ureq::Error::StatusCode(404)) => Err("HTTP 404: no document with that id (or it is not shared with the token's account)".into()),
        Err(ureq::Error::StatusCode(code)) => Err(format!("HTTP {code}")),
        Err(other) => Err(format!("the network - {other}")),
    }
}

const FULL_DRIVE: &str = "https://www.googleapis.com/auth/drive";
const REFUSED: &str = "the token was refused";

/// The account a token belongs to and the scopes it carries, as Google's `tokeninfo` says - None when it cannot be asked.
struct Holder {
    email: String,
    scopes: Vec<String>,
}

fn holder_of(api: &str, bearer: &str) -> Option<Holder> {
    let mut response = ureq::get(&format!("{api}/oauth2/v3/tokeninfo")).query("access_token", bearer).call().ok()?;
    let info: Value = serde_json::from_slice(&response.body_mut().read_to_vec().ok()?).ok()?;
    Some(Holder {
        email: info["email"].as_str().unwrap_or("an account tokeninfo does not name").to_string(),
        scopes: info["scope"].as_str().unwrap_or("").split_whitespace().map(String::from).collect(),
    })
}

/// A refusal with its two usual causes, and which of them the token's own account and scopes point at.
fn refused(why: &str, who: Option<&Holder>) -> String {
    let Some(who) = who else {
        return format!("{why}: either it carries no Drive scope, or its account may not read the document (tokeninfo could not be asked which)");
    };
    let drive = who.scopes.iter().any(|s| s == FULL_DRIVE || s == "https://www.googleapis.com/auth/drive.readonly");
    if drive {
        format!("{why}: it carries a Drive scope, so its account {} may not read the document - share it with that account, or use a token of one that can", who.email)
    } else {
        format!("{why}: it carries no Drive scope (it has: {}) - account {}; see skills/map-sqlite/facts.md for a read-only token", who.scopes.join(" "), who.email)
    }
}

/// The file written beside its target and renamed over it.
fn replace(target: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(folder) = target.parent() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{} could not be made - {e}", folder.display()))?;
    }
    let mut temp = target.as_os_str().to_os_string();
    temp.push(".pulling");
    let temp = PathBuf::from(temp);
    std::fs::write(&temp, bytes).map_err(|e| format!("{} could not be written - {e}", temp.display()))?;
    std::fs::rename(&temp, target).map_err(|e| {
        let _ = std::fs::remove_file(&temp);
        format!("{} could not be replaced - {e}", target.display())
    })
}

/// The token: the environment variable the config names, else what its command prints.
fn bearer(config: &Config) -> Result<String, String> {
    if !config.token_env.is_empty() {
        return match std::env::var(&config.token_env) {
            Ok(t) if !t.trim().is_empty() => Ok(t.trim().to_string()),
            _ => Err(format!("no token: the environment variable {} is empty", config.token_env)),
        };
    }
    if config.token_command.is_empty() {
        return Err("no token: the config sets neither auth.token_env nor auth.token_command".into());
    }
    let (shell, flag) = if cfg!(windows) { ("cmd", "/C") } else { ("sh", "-c") };
    let ran = std::process::Command::new(shell)
        .args([flag, &config.token_command])
        .output()
        .map_err(|e| format!("no token: `{}` did not run - {e}", config.token_command))?;
    let printed = String::from_utf8_lossy(&ran.stdout).trim().to_string();
    if !ran.status.success() || printed.is_empty() {
        let said = String::from_utf8_lossy(&ran.stderr).trim().lines().last().unwrap_or("").to_string();
        return Err(format!("no token: `{}` failed - {said}", config.token_command));
    }
    Ok(printed.lines().last().unwrap_or("").trim().to_string())
}

#[cfg(test)]
#[path = "pull_tests.rs"]
mod tests;
