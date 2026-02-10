use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEV_API_URL: &str = "https://ygyu7ritx8.execute-api.us-west-2.amazonaws.com";
const PROD_API_URL: &str = "https://jdsx4ixk2i.execute-api.us-east-1.amazonaws.com";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Environment {
    Dev,
    Prod,
    Custom,
}

impl Default for Environment {
    fn default() -> Self {
        Self::Dev
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub api_base_url: String,
    pub api_key: String,
    pub watched_folder: Option<PathBuf>,
    pub auto_ingest: bool,
    #[serde(default)]
    pub environment: Environment,
    #[serde(default)]
    pub session_token: Option<String>,
    #[serde(default)]
    pub user_hash: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            api_base_url: String::new(),
            api_key: String::new(),
            watched_folder: None,
            auto_ingest: true,
            environment: Environment::default(),
            session_token: None,
            user_hash: None,
        }
    }
}

impl AppConfig {
    fn config_path() -> Result<PathBuf, String> {
        let dirs = ProjectDirs::from("ai", "exemem", "exemem-client")
            .ok_or_else(|| "Could not determine config directory".to_string())?;
        Ok(dirs.config_dir().join("config.json"))
    }

    pub fn load() -> Result<Self, String> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse config: {}", e))
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        let data = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        std::fs::write(&path, data)
            .map_err(|e| format!("Failed to write config: {}", e))
    }

    pub fn api_url(&self) -> &str {
        match self.environment {
            Environment::Dev => DEV_API_URL,
            Environment::Prod => PROD_API_URL,
            Environment::Custom => &self.api_base_url,
        }
    }

    pub fn is_configured(&self) -> bool {
        !self.api_url().is_empty()
            && !self.api_key.is_empty()
            && self.watched_folder.is_some()
    }
}
