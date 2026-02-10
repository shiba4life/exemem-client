use crate::config::AppConfig;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What we return to the frontend for run_query (ai_native_index endpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunQueryResponse {
    pub session_id: String,
    pub ai_interpretation: String,
    pub raw_results: Vec<Value>,
}

/// What we return to the frontend for chat_followup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub answer: String,
    pub context_used: bool,
}

/// What we return to the frontend for search_index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<Value>,
    pub count: usize,
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

    /// Parse API response, check ok field, return raw JSON value for further extraction
    fn parse_api_response(body: Value) -> Result<Value, String> {
        let ok = body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
        if !ok {
            let error = body.get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown server error");
            return Err(error.to_string());
        }
        Ok(body)
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
        // Use ai_native_index endpoint: LLM searches word index, hydrates, interprets
        let url = format!("{}/api/llm-query/native-index", api_url);
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

        let json: Value = resp.json().await
            .map_err(|e| format!("Failed to read query response: {}", e))?;
        let data = Self::parse_api_response(json)?;

        Ok(RunQueryResponse {
            session_id: data.get("session_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            ai_interpretation: data.get("ai_interpretation")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            raw_results: data.get("raw_results")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default(),
        })
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

        let json: Value = resp.json().await
            .map_err(|e| format!("Failed to read chat response: {}", e))?;
        let data = Self::parse_api_response(json)?;

        Ok(ChatResponse {
            answer: data.get("answer")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            context_used: data.get("context_used")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    async fn search_index_internal(
        &self,
        api_url: &str,
        headers: &reqwest::header::HeaderMap,
        term: &str,
    ) -> Result<SearchResponse, String> {
        // Native index search is GET with query param
        let url = format!("{}/api/native-index/search", api_url);

        let resp = self
            .client
            .get(&url)
            .query(&[("term", term)])
            .headers(headers.clone())
            .send()
            .await
            .map_err(|e| format!("Search request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Search failed ({}): {}", status, text));
        }

        let json: Value = resp.json().await
            .map_err(|e| format!("Failed to read search response: {}", e))?;
        let data = Self::parse_api_response(json)?;

        let results = data.get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let count = results.len();

        Ok(SearchResponse { results, count })
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

        let json: Value = resp.json().await
            .map_err(|e| format!("Failed to read mutate response: {}", e))?;
        let data = Self::parse_api_response(json)?;

        Ok(MutateResponse {
            success: data.get("ok")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            message: data.get("message")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            data: data.get("data").cloned(),
        })
    }
}
