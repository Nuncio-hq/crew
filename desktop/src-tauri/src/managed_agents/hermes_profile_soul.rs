//! Read and atomically replace a Hermes profile's `SOUL.md`.

use std::{
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use super::{
    hermes_profile::{crew_may_mutate_hermes_profile, validate_hermes_profile_name},
    hermes_profile_lifecycle::hermes_profile_dir,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HermesProfileSoulResult {
    Ok { name: String, content: String },
    DoesNotExist { name: String, message: String },
    Missing { name: String, message: String },
    InvalidName { name: String, message: String },
    Failed { name: String, message: String },
}

fn profile_dir(name: &str) -> Result<(String, PathBuf), HermesProfileSoulResult> {
    let trimmed = name.trim();
    if let Err(message) = validate_hermes_profile_name(trimmed) {
        return Err(HermesProfileSoulResult::InvalidName {
            name: trimmed.to_string(),
            message,
        });
    }
    if !crew_may_mutate_hermes_profile(trimmed) {
        return Err(HermesProfileSoulResult::InvalidName {
            name: trimmed.to_string(),
            message: "hermes profile 'default' cannot be edited by Crew".to_string(),
        });
    }
    let Some(dir) = hermes_profile_dir(trimmed) else {
        return Err(HermesProfileSoulResult::Failed {
            name: trimmed.to_string(),
            message: "could not resolve Hermes home directory".to_string(),
        });
    };
    Ok((trimmed.to_string(), dir))
}

fn read_from_dir(name: &str, dir: &Path) -> HermesProfileSoulResult {
    if !dir.is_dir() {
        return HermesProfileSoulResult::DoesNotExist {
            name: name.to_string(),
            message: format!("Hermes profile '{name}' does not exist"),
        };
    }
    let path = dir.join("SOUL.md");
    match std::fs::read_to_string(&path) {
        Ok(content) => HermesProfileSoulResult::Ok {
            name: name.to_string(),
            content,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            HermesProfileSoulResult::Missing {
                name: name.to_string(),
                message: format!("Hermes profile '{name}' has no SOUL.md"),
            }
        }
        Err(error) => HermesProfileSoulResult::Failed {
            name: name.to_string(),
            message: format!("failed to read {}: {error}", path.display()),
        },
    }
}

/// Reads the exact UTF-8 contents of a Hermes profile's `SOUL.md`.
pub fn read_profile_soul(name: &str) -> HermesProfileSoulResult {
    match profile_dir(name) {
        Ok((name, dir)) => read_from_dir(&name, &dir),
        Err(result) => result,
    }
}

/// Atomically replaces a Hermes profile's `SOUL.md` with `content`.
pub fn write_profile_soul(name: &str, content: &str) -> HermesProfileSoulResult {
    let (name, dir) = match profile_dir(name) {
        Ok(value) => value,
        Err(result) => return result,
    };
    if !dir.is_dir() {
        return HermesProfileSoulResult::DoesNotExist {
            name,
            message: "Hermes profile does not exist".to_string(),
        };
    }
    let path = dir.join("SOUL.md");
    let mut temp = match NamedTempFile::new_in(&dir) {
        Ok(file) => file,
        Err(error) => {
            return HermesProfileSoulResult::Failed {
                name,
                message: format!("failed to create temporary SOUL.md: {error}"),
            }
        }
    };
    if let Err(error) = temp
        .write_all(content.as_bytes())
        .and_then(|_| temp.as_file().sync_all())
    {
        return HermesProfileSoulResult::Failed {
            name,
            message: format!("failed to write temporary SOUL.md: {error}"),
        };
    }
    if let Err(error) = temp.persist(&path) {
        return HermesProfileSoulResult::Failed {
            name,
            message: format!("failed to replace SOUL.md: {error}"),
        };
    }
    HermesProfileSoulResult::Ok {
        name,
        content: content.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn setup() -> (std::sync::MutexGuard<'static, ()>, TempDir, PathBuf) {
        let guard = crate::managed_agents::lock_path_mutex();
        let home = tempfile::tempdir().unwrap();
        let profile = home.path().join("profiles/scout");
        fs::create_dir_all(&profile).unwrap();
        std::env::set_var("HERMES_HOME", home.path());
        (guard, home, profile)
    }

    #[test]
    fn round_trip_preserves_bytes() {
        let (_guard, _home, profile) = setup();
        let content = "# Soul\r\n\nexact bytes\r\n";
        assert!(matches!(
            write_profile_soul("scout", content),
            HermesProfileSoulResult::Ok { .. }
        ));
        assert_eq!(
            fs::read(profile.join("SOUL.md")).unwrap(),
            content.as_bytes()
        );
        assert_eq!(
            read_profile_soul("scout"),
            HermesProfileSoulResult::Ok {
                name: "scout".to_string(),
                content: content.to_string(),
            }
        );
    }

    #[test]
    fn missing_profile_is_classified() {
        let (_guard, _home, _profile) = setup();
        assert!(matches!(
            read_profile_soul("missing"),
            HermesProfileSoulResult::DoesNotExist { .. }
        ));
    }

    #[test]
    fn missing_soul_is_distinct() {
        let (_guard, _home, _profile) = setup();
        assert!(matches!(
            read_profile_soul("scout"),
            HermesProfileSoulResult::Missing { .. }
        ));
    }

    #[test]
    fn write_does_not_modify_other_profile_files() {
        let (_guard, _home, profile) = setup();
        fs::write(profile.join("config.yaml"), "model: {}\n").unwrap();
        fs::write(profile.join("notes.txt"), "untouched").unwrap();
        let _ = write_profile_soul("scout", "new content");
        assert_eq!(
            fs::read_to_string(profile.join("config.yaml")).unwrap(),
            "model: {}\n"
        );
        assert_eq!(
            fs::read_to_string(profile.join("notes.txt")).unwrap(),
            "untouched"
        );
    }

    #[test]
    fn invalid_name_is_rejected() {
        assert!(matches!(
            read_profile_soul("default"),
            HermesProfileSoulResult::InvalidName { .. }
        ));
    }
}
