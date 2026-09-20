use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub base_dir: PathBuf,
    pub configs_dir: PathBuf,
    pub db_path: PathBuf,
    pub key_path: PathBuf,
}

impl AppPaths {
    pub fn default_paths() -> Result<Self> {
        let base_dir = dirs::config_dir()
            .context("Could not determine user config directory")?
            .join("ovpn3-tui");

        let configs_dir = base_dir.join("configs");
        let db_path = base_dir.join("ovpn3_tui.db");
        let key_path = base_dir.join(".key");

        Ok(Self {
            base_dir,
            configs_dir,
            db_path,
            key_path,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        if !self.base_dir.exists() {
            fs::create_dir_all(&self.base_dir)
                .with_context(|| format!("Failed to create base dir: {:?}", self.base_dir))?;
        }
        if !self.configs_dir.exists() {
            fs::create_dir_all(&self.configs_dir)
                .with_context(|| format!("Failed to create configs dir: {:?}", self.configs_dir))?;
        }
        Ok(())
    }

    pub fn list_config_files(&self) -> Result<Vec<PathBuf>> {
        self.ensure_dirs()?;

        let mut files = Vec::new();
        let entries = fs::read_dir(&self.configs_dir)
            .with_context(|| format!("Failed to read configs dir: {:?}", self.configs_dir))?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext == "ovpn" || ext == "conf" {
                        files.push(path);
                    }
                }
            }
        }

        files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
        Ok(files)
    }
}

pub fn get_profile_name(path: &Path) -> String {
    path.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_list_config_files() {
        let dir = tempdir().unwrap();
        let base_dir = dir.path().join("ovpn3-tui");
        let configs_dir = base_dir.join("configs");
        let paths = AppPaths {
            base_dir: base_dir.clone(),
            configs_dir: configs_dir.clone(),
            db_path: base_dir.join("test.db"),
            key_path: base_dir.join(".key"),
        };

        paths.ensure_dirs().unwrap();
        assert!(configs_dir.exists());

        // Create sample files
        fs::write(configs_dir.join("office.ovpn"), "# dummy").unwrap();
        fs::write(configs_dir.join("home.conf"), "# dummy").unwrap();
        fs::write(configs_dir.join("ignore.txt"), "# dummy").unwrap();

        let files = paths.list_config_files().unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(get_profile_name(&files[0]), "home.conf");
        assert_eq!(get_profile_name(&files[1]), "office.ovpn");
    }
}
