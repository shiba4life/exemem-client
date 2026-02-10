use clap::{Parser, Subcommand};
use exemem_client_lib::query::QueryClient;
use serde_json::Value;

// Re-use config from the library crate
// Note: config is private in lib, so we replicate the load path here
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const DEV_API_URL: &str = "https://ygyu7ritx8.execute-api.us-west-2.amazonaws.com";
const PROD_API_URL: &str = "https://jdsx4ixk2i.execute-api.us-east-1.amazonaws.com";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
enum Environment {
    Dev,
    Prod,
    Custom,
}

impl Default for Environment {
    fn default() -> Self {
        Self::Dev
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CliConfig {
    api_base_url: String,
    api_key: String,
    watched_folder: Option<PathBuf>,
    auto_ingest: bool,
    #[serde(default = "default_true")]
    auto_approve_watched: bool,
    #[serde(default)]
    environment: Environment,
    #[serde(default)]
    session_token: Option<String>,
    #[serde(default)]
    user_hash: Option<String>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            api_base_url: String::new(),
            api_key: String::new(),
            watched_folder: None,
            auto_ingest: true,
            auto_approve_watched: true,
            environment: Environment::default(),
            session_token: None,
            user_hash: None,
        }
    }
}

impl CliConfig {
    fn config_path() -> Result<PathBuf, String> {
        let dirs = ProjectDirs::from("ai", "exemem", "exemem-client")
            .ok_or_else(|| "Could not determine config directory".to_string())?;
        Ok(dirs.config_dir().join("config.json"))
    }

    fn load() -> Result<Self, String> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(Self::default());
        }
        let data = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&data)
            .map_err(|e| format!("Failed to parse config: {}", e))
    }

    fn save(&self) -> Result<(), String> {
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

    fn api_url(&self) -> &str {
        match self.environment {
            Environment::Dev => DEV_API_URL,
            Environment::Prod => PROD_API_URL,
            Environment::Custom => &self.api_base_url,
        }
    }
}

/// Adapter to convert CliConfig into the library's AppConfig-compatible struct
/// for QueryClient methods
struct ConfigAdapter<'a> {
    config: &'a CliConfig,
}

impl<'a> ConfigAdapter<'a> {
    fn to_app_config(&self) -> exemem_client_lib::query::AdapterConfig {
        exemem_client_lib::query::AdapterConfig {
            api_url: self.config.api_url().to_string(),
            api_key: self.config.api_key.clone(),
            user_hash: self.config.user_hash.clone(),
        }
    }
}

#[derive(Parser)]
#[command(name = "exemem-cli")]
#[command(about = "Exemem CLI — Query, search, and mutate your Exemem data")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a natural language query against your data
    Query {
        /// The query string
        query: String,
        /// Session ID for follow-up queries
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Search the native word index
    Search {
        /// The search term
        term: String,
    },
    /// Execute a mutation against a schema
    Mutate {
        /// Target schema name
        #[arg(long)]
        schema: String,
        /// Operation type (insert, update, delete)
        #[arg(long)]
        operation: String,
        /// JSON data for the mutation
        #[arg(long)]
        data: String,
    },
    /// Ask a follow-up question in an existing session
    Chat {
        /// Session ID from a previous query
        #[arg(long)]
        session_id: String,
        /// The follow-up question
        question: String,
    },
    /// View or update configuration
    Config {
        /// Show current configuration
        #[arg(long)]
        show: bool,
        /// Set environment (Dev, Prod, Custom)
        #[arg(long)]
        env: Option<String>,
        /// Set API key
        #[arg(long)]
        api_key: Option<String>,
        /// Set custom API URL (only used with Custom env)
        #[arg(long)]
        api_url: Option<String>,
    },
}

fn error_json(msg: &str) -> ! {
    let err = serde_json::json!({ "error": msg });
    eprintln!("{}", serde_json::to_string_pretty(&err).unwrap());
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Query { query, session_id } => {
            let config = CliConfig::load().unwrap_or_else(|e| error_json(&e));
            let adapter = ConfigAdapter { config: &config };
            let app_cfg = adapter.to_app_config();
            let client = QueryClient::new();

            match client
                .run_query_with_adapter(&app_cfg, &query, session_id.as_deref())
                .await
            {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                }
                Err(e) => error_json(&e),
            }
        }
        Commands::Search { term } => {
            let config = CliConfig::load().unwrap_or_else(|e| error_json(&e));
            let adapter = ConfigAdapter { config: &config };
            let app_cfg = adapter.to_app_config();
            let client = QueryClient::new();

            match client.search_index_with_adapter(&app_cfg, &term).await {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                }
                Err(e) => error_json(&e),
            }
        }
        Commands::Mutate {
            schema,
            operation,
            data,
        } => {
            let config = CliConfig::load().unwrap_or_else(|e| error_json(&e));
            let adapter = ConfigAdapter { config: &config };
            let app_cfg = adapter.to_app_config();
            let client = QueryClient::new();

            let data_value: Value = serde_json::from_str(&data)
                .unwrap_or_else(|e| error_json(&format!("Invalid JSON data: {}", e)));

            match client
                .mutate_with_adapter(&app_cfg, &schema, &operation, data_value)
                .await
            {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                }
                Err(e) => error_json(&e),
            }
        }
        Commands::Chat {
            session_id,
            question,
        } => {
            let config = CliConfig::load().unwrap_or_else(|e| error_json(&e));
            let adapter = ConfigAdapter { config: &config };
            let app_cfg = adapter.to_app_config();
            let client = QueryClient::new();

            match client
                .chat_followup_with_adapter(&app_cfg, &session_id, &question)
                .await
            {
                Ok(resp) => {
                    println!("{}", serde_json::to_string_pretty(&resp).unwrap());
                }
                Err(e) => error_json(&e),
            }
        }
        Commands::Config {
            show,
            env,
            api_key,
            api_url,
        } => {
            let mut config = CliConfig::load().unwrap_or_else(|e| error_json(&e));

            if show && env.is_none() && api_key.is_none() && api_url.is_none() {
                let output = serde_json::json!({
                    "environment": format!("{:?}", config.environment),
                    "api_url": config.api_url(),
                    "api_key_set": !config.api_key.is_empty(),
                    "user_hash": config.user_hash,
                    "watched_folder": config.watched_folder,
                    "auto_ingest": config.auto_ingest,
                    "auto_approve_watched": config.auto_approve_watched,
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
                return;
            }

            let mut changed = false;

            if let Some(env_str) = env {
                config.environment = match env_str.as_str() {
                    "Dev" | "dev" => Environment::Dev,
                    "Prod" | "prod" => Environment::Prod,
                    "Custom" | "custom" => Environment::Custom,
                    _ => error_json(&format!("Invalid environment: {}. Use Dev, Prod, or Custom", env_str)),
                };
                changed = true;
            }

            if let Some(key) = api_key {
                config.api_key = key;
                changed = true;
            }

            if let Some(url) = api_url {
                config.api_base_url = url;
                config.environment = Environment::Custom;
                changed = true;
            }

            if changed {
                config.save().unwrap_or_else(|e| error_json(&e));
                let output = serde_json::json!({
                    "status": "saved",
                    "environment": format!("{:?}", config.environment),
                    "api_url": config.api_url(),
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                error_json("No config changes specified. Use --show, --env, --api-key, or --api-url");
            }
        }
    }
}
