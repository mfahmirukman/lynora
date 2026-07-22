use crate::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Environment {
    pub name: String,
    pub values: HashMap<String, String>,
    /// Keys listed here are secrets (UI masks; not written to git-friendly exports later).
    #[serde(default)]
    pub secrets: Vec<String>,
}

impl Environment {
    pub fn load(path: &Path) -> Result<Self> {
        let data = fs::read_to_string(path)?;
        Ok(serde_json::from_str(&data)?)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)?;
        fs::write(path, data)?;
        Ok(())
    }

    pub fn dir_load_all(dir: &Path) -> Result<Vec<Self>> {
        let mut envs = Vec::new();
        if !dir.exists() {
            return Ok(envs);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("json") {
                envs.push(Self::load(&path)?);
            }
        }
        envs.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(envs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trip_save_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("local.json");
        let mut values = HashMap::new();
        values.insert("baseUrl".into(), "http://localhost:3000".into());
        let env = Environment {
            name: "local".into(),
            values,
            secrets: vec!["token".into()],
        };
        env.save(&path).unwrap();
        let loaded = Environment::load(&path).unwrap();
        assert_eq!(loaded, env);
    }

    #[test]
    fn dir_load_all_reads_json_files() {
        let dir = tempdir().unwrap();
        let a = Environment {
            name: "a".into(),
            values: HashMap::new(),
            secrets: vec![],
        };
        let b = Environment {
            name: "b".into(),
            values: HashMap::new(),
            secrets: vec![],
        };
        a.save(&dir.path().join("a.json")).unwrap();
        b.save(&dir.path().join("b.json")).unwrap();
        let all = Environment::dir_load_all(dir.path()).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "a");
        assert_eq!(all[1].name, "b");
    }
}
