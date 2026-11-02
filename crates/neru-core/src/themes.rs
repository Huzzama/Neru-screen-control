//! Theme files: the bundled ones (read-only) and the user's (`themes/`).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::render::{self, Theme};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct ThemeInfo {
    /// `bundled:<file>` or `user:<file>`.
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub bundled: bool,
    pub path: String,
    pub elements: usize,
}

pub struct ThemeStore {
    pub user_dir: PathBuf,
}

fn slug(name: &str) -> String {
    let mut s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while s.contains("--") {
        s = s.replace("--", "-");
    }
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "theme".into()
    } else {
        s
    }
}

impl ThemeStore {
    pub fn new(user_dir: PathBuf) -> Self {
        Self { user_dir }
    }

    fn info(id: String, path: &Path, bundled: bool) -> Option<ThemeInfo> {
        let t = render::load_theme(path).ok()?;
        Some(ThemeInfo {
            id,
            name: t.name,
            width: t.width,
            height: t.height,
            bundled,
            path: path.display().to_string(),
            elements: t.elements.len(),
        })
    }

    pub fn list(&self) -> Vec<ThemeInfo> {
        let mut out = Vec::new();
        for p in render::bundled_themes() {
            let file = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            if let Some(i) = Self::info(format!("bundled:{file}"), &p, true) {
                out.push(i);
            }
        }
        let mut user: Vec<PathBuf> = fs::read_dir(&self.user_dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().map_or(false, |e| e == "json"))
                    .collect()
            })
            .unwrap_or_default();
        user.sort();
        for p in user {
            let file = p.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default();
            if let Some(i) = Self::info(format!("user:{file}"), &p, false) {
                out.push(i);
            }
        }
        out
    }

    pub fn path_for(&self, id: &str) -> Result<(PathBuf, bool)> {
        if let Some(file) = id.strip_prefix("bundled:") {
            let p = render::bundled_themes()
                .into_iter()
                .find(|p| p.file_name().map_or(false, |f| f == file))
                .ok_or_else(|| Error::Config(format!("no bundled theme {file}")))?;
            return Ok((p, true));
        }
        if let Some(file) = id.strip_prefix("user:") {
            if file.contains('/') || file.contains("..") {
                return Err(Error::InvalidValue(format!("bad theme id {id}")));
            }
            return Ok((self.user_dir.join(file), false));
        }
        Err(Error::InvalidValue(format!("bad theme id {id}")))
    }

    pub fn load(&self, id: &str) -> Result<Theme> {
        let (p, _) = self.path_for(id)?;
        render::load_theme(&p)
    }

    /// Save a user theme (bundled ids are rejected).
    pub fn save(&self, id: &str, theme: &Theme) -> Result<()> {
        let (p, bundled) = self.path_for(id)?;
        if bundled {
            return Err(Error::InvalidValue("bundled themes are read-only; duplicate it first".into()));
        }
        fs::create_dir_all(&self.user_dir)?;
        let tmp = p.with_extension("json.tmp");
        fs::write(&tmp, theme.to_json())?;
        fs::rename(&tmp, &p)?;
        Ok(())
    }

    /// Create a new user theme file for `theme`; returns its id.
    pub fn create(&self, theme: &Theme) -> Result<String> {
        fs::create_dir_all(&self.user_dir)?;
        let base = slug(&theme.name);
        let mut file = format!("{base}.json");
        let mut n = 2;
        while self.user_dir.join(&file).exists() {
            file = format!("{base}-{n}.json");
            n += 1;
        }
        let id = format!("user:{file}");
        self.save(&id, theme)?;
        Ok(id)
    }

    pub fn duplicate(&self, id: &str, new_name: Option<&str>) -> Result<String> {
        let mut t = self.load(id)?;
        t.name = new_name
            .map(str::to_string)
            .unwrap_or_else(|| format!("{} copy", t.name));
        self.create(&t)
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        let (p, bundled) = self.path_for(id)?;
        if bundled {
            return Err(Error::InvalidValue("bundled themes cannot be deleted".into()));
        }
        fs::remove_file(p)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_save_duplicate_delete() {
        let dir = std::env::temp_dir().join(format!("neru-themes-{}", std::process::id()));
        let store = ThemeStore::new(dir.clone());
        let mut t = Theme::blank(100, 100);
        t.name = "My Theme!".into();
        let id = store.create(&t).unwrap();
        assert_eq!(id, "user:my-theme.json");
        let id2 = store.create(&t).unwrap();
        assert_eq!(id2, "user:my-theme-2.json");
        let dup = store.duplicate(&id, None).unwrap();
        assert_eq!(store.load(&dup).unwrap().name, "My Theme! copy");
        assert_eq!(store.list().iter().filter(|i| !i.bundled).count(), 3);
        store.delete(&id).unwrap();
        store.delete(&id2).unwrap();
        store.delete(&dup).unwrap();
        assert!(store.path_for("user:../x.json").is_err());
        assert!(store.save("bundled:neru-720.json", &t).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
