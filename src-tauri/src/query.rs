use crate::config::AppConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunQueryResponse {
    pub session_id: String,
    pub results: Vec<Value>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub answer: String,
    pub context_used: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<Value>,
    pub count: usize,
    pub term: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutateResponse {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<Value>,
}

/// Lightweight config adapter for CLI usage (avoids depending on full AppConfig)
pub struct AdapterConfig {
    pub api_url: String,
    pub api_key: String,
    pub user_hash: Option<String>,
}

pub struct QueryClient {
    client: Client,
}

impl Default for QueryClient {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    fn build_headers(&self, api_key: &str, user_hash: Option<&str>) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        if !api_key.is_empty() {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(api_key) {
                headers.insert("X-API-Key", val);
            }
        }
        if let Some(uh) = user_hash {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(uh) {
                headers.insert("X-User-Hash", val);
            }
        }
        headers
    }

    fn headers_from_config(&self, config: &AppConfig) -> reqwest::header::HeaderMap {
        self.build_headers(&config.api_key, config.user_hash.as_deref())
    }

    fn headers_from_adapter(&self, config: &AdapterConfig) -> reqwest::header::HeaderMap {
        self.build_headers(&config.api_key, config.user_hash.as_deref())
    }

    // --- Tauri command methods (use AppConfig) ---

    pub async fn run_query(
        &self,
        config: &AppConfig,
        query: &str,
        session_id: Option<&str>,
    ) -> Result<RunQueryResponse, String> {
        self.run_query_internal(config.api_url(), &self.headers_from_config(config), query, session_id).await
    }

    pub async fn chat_followup(
        &self,
        config: &AppConfig,
        session_id: &str,
        question: &str,
    ) -> Result<ChatResponse, String> {
        self.chat_followup_internal(config.api_url(), &self.headers_from_config(config), session_id, question).await
    }

    pub async fn search_index(
        &self,
        config: &AppConfig,
        term: &str,
    ) -> Result<SearchResponse, String> {
        self.search_index_internal(config.api_url(), &self.headers_from_config(config), term).await
    }

    pub async fn mutate(
        &self,
        config: &AppConfig,
        schema: &str,
        operation: &str,
        data: Value,
    ) -> Result<MutateResponse, String> {
        self.mutate_internal(config.api_url(), &self.headers_from_config(config), schema, operation, data).await
    }

    // --- CLI adapter methods (use AdapterConfig) ---

    pub async fn run_query_with_adapter(
        &self,
        config: &AdapterConfig,
        query: &str,
        session_id: Option<&str>,
    ) -> Result<RunQueryResponse, String> {
        self.run_query_internal(&config.api_url, &self.headers_from_adapter(config), query, session_id).await
    }

    pub async fn chat_followup_with_adapter(
        &self,
        config: &AdapterConfig,
        session_id: &str,
        question: &str,
    ) -> Result<ChatResponse, String> {
        self.chat_followup_internal(&config.api_url, &self.headers_from_adapter(config), session_id, question).await
    }

    pub async fn search_index_with_adapter(
        &self,
        config: &AdapterConfig,
        term: &str,
    ) -> Result<SearchResponse, String> {
        self.search_index_internal(&config.api_url, &self.headers_from_adapter(config), term).await
    }

    pub async fn mutate_with_adapter(
        &self,
        config: &AdapterConfig,
        schema: &str,
        operation: &str,
        data: Value,
    ) -> Result<MutateResponse, String> {
        self.mutate_internal(&config.api_url, &self.headers_from_adapter(config), schema, operation, data).await
    }

    // --- Internal implementations ---

    async fn run_query_internal(
        &self,
        api_url: &str,
        headers: &reqwest::header::HeaderMap,
        query: &str,
        session_id: Option<&str>,
    ) -> Result<RunQueryResponse, String> {
        let url = format!("{}/api/llm-query/run", api_url);
        let mut body = serde_json::json!({ "query": query });
        if let Some(sid) = session_id {
            body["session_id"] = serde_json::json!(sid);
        }

        let resp = self
            .client
            .post(&url)
            .headers(headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Query request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Query failed ({}): {}", status, text));
        }

        resp.json::<RunQueryResponse>()
            .await
            .map_err(|e| format!("Failed to parse query response: {}", e))
    }

    async fn chat_followup_internal(
        &self,
        api_url: &str,
        headers: &reqwest::header::HeaderMap,
        session_id: &str,
        question: &str,
    ) -> Result<ChatResponse, String> {
        let url = format!("{}/api/llm-query/chat", api_url);
        let body = serde_json::json!({
            "session_id": session_id,
            "question": question,
        });

        let resp = self
            .client
            .post(&url)
            .headers(headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Chat request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Chat failed ({}): {}", status, text));
        }

        resp.json::<ChatResponse>()
            .await
            .map_err(|e| format!("Failed to parse chat response: {}", e))
    }

    async fn search_index_internal(
        &self,
        api_url: &str,
        headers: &reqwest::header::HeaderMap,
        term: &str,
    ) -> Result<SearchResponse, String> {
        let url = format!("{}/api/native-index/search", api_url);
        let body = serde_json::json!({ "term": term });

        let resp = self
            .client
            .post(&url)
            .headers(headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Search request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Search failed ({}): {}", status, text));
        }

        resp.json::<SearchResponse>()
            .await
            .map_err(|e| format!("Failed to parse search response: {}", e))
    }

    async fn mutate_internal(
        &self,
        api_url: &str,
        headers: &reqwest::header::HeaderMap,
        schema: &str,
        operation: &str,
        data: Value,
    ) -> Result<MutateResponse, String> {
        let url = format!("{}/api/mutation/execute", api_url);
        let body = serde_json::json!({
            "schema": schema,
            "operation": operation,
            "data": data,
        });

        let resp = self
            .client
            .post(&url)
            .headers(headers.clone())
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Mutate request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Mutate failed ({}): {}", status, text));
        }

        resp.json::<MutateResponse>()
            .await
            .map_err(|e| format!("Failed to parse mutate response: {}", e))
    }
}
