use crate::collection::{Collection, CollectionMeta};
use crate::environment::Environment;
use crate::history::HistoryStore;
use crate::{LynoraError, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub config_dir: PathBuf,
    pub collections_dir: PathBuf,
}

impl Workspace {
    pub fn open_default() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| LynoraError::Message("could not resolve config directory".into()))?
            .join("lynora");
        let collections_dir = config_dir.join("collections");
        Self::open(config_dir, collections_dir)
    }

    pub fn open(config_dir: PathBuf, collections_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(config_dir.join("environments"))?;
        fs::create_dir_all(&collections_dir)?;
        Ok(Self {
            config_dir,
            collections_dir,
        })
    }

    pub fn environments_dir(&self) -> PathBuf {
        self.config_dir.join("environments")
    }

    pub fn history_path(&self) -> PathBuf {
        self.config_dir.join("history.db")
    }

    pub fn list_environments(&self) -> Result<Vec<Environment>> {
        Environment::dir_load_all(&self.environments_dir())
    }

    pub fn history(&self) -> Result<HistoryStore> {
        HistoryStore::open(&self.history_path())
    }

    pub fn list_collections(&self) -> Result<Vec<(PathBuf, CollectionMeta)>> {
        let mut out = Vec::new();
        if !self.collections_dir.exists() {
            return Ok(out);
        }
        for entry in fs::read_dir(&self.collections_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() && path.join("lynora.json").exists() {
                let col = Collection::load(&path)?;
                out.push((path, col.meta));
            }
        }
        out.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        Ok(out)
    }

    pub fn create_collection(&self, name: &str) -> Result<Collection> {
        let slug = slugify(name);
        let mut root = self.collections_dir.join(&slug);
        let mut n = 2;
        while root.exists() {
            root = self.collections_dir.join(format!("{slug}-{n}"));
            n += 1;
        }
        Collection::create(&root, name)
    }

    pub fn import_postman_file(&self, json_path: &Path) -> Result<Collection> {
        let json = fs::read_to_string(json_path)?;
        let name = json_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported");
        let slug = slugify(name);
        let mut root = self.collections_dir.join(&slug);
        let mut n = 2;
        while root.exists() {
            root = self.collections_dir.join(format!("{slug}-{n}"));
            n += 1;
        }
        crate::import::postman::import_postman_json(&json, &root)
    }
}

fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "collection".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn create_and_list_collections() {
        let dir = tempdir().unwrap();
        let ws = Workspace::open(dir.path().join("cfg"), dir.path().join("cols")).unwrap();
        let col = ws.create_collection("Demo API").unwrap();
        assert!(col.root.join("lynora.json").exists());
        let listed = ws.list_collections().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].1.name, "Demo API");
    }
}
