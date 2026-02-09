use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;

use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResult {
    pub filename: String,
    pub s3_key: String,
    pub progress_id: Option<String>,
    pub status: UploadStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UploadStatus {
    Uploading,
    Uploaded,
    Ingesting,
    Done,
    Error,
}

#[derive(Debug, Deserialize)]
struct PresignedUrlResponse {
    upload_url: String,
    s3_key: String,
}

#[derive(Debug, Deserialize)]
struct IngestResponse {
    progress_id: String,
}

pub struct Uploader {
    client: Client,
}

impl Uploader {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    pub async fn upload_and_ingest(
        &self,
        file_path: &Path,
        config: &AppConfig,
    ) -> UploadResult {
        let filename = file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let result = self.try_upload_and_ingest(file_path, config, &filename).await;

        match result {
            Ok(upload_result) => upload_result,
            Err(err) => UploadResult {
                filename,
                s3_key: String::new(),
                progress_id: None,
                status: UploadStatus::Error,
                error: Some(err),
            },
        }
    }

    async fn try_upload_and_ingest(
        &self,
        file_path: &Path,
        config: &AppConfig,
        filename: &str,
    ) -> Result<UploadResult, String> {
        // Step 1: Get presigned URL
        let presigned = self
            .with_retry(|| self.get_presigned_url(config, filename))
            .await?;

        // Step 2: Upload file to S3
        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let content_type = mime_guess::from_path(file_path)
            .first_or_octet_stream()
            .to_string();

        self.with_retry(|| {
            self.upload_to_s3(&presigned.upload_url, file_bytes.clone(), &content_type)
        })
        .await?;

        // Step 3: Trigger ingestion if auto_ingest is enabled
        if config.auto_ingest {
            let ingest_resp = self
                .with_retry(|| self.trigger_ingest(config, &presigned.s3_key))
                .await?;

            Ok(UploadResult {
                filename: filename.to_string(),
                s3_key: presigned.s3_key,
                progress_id: Some(ingest_resp.progress_id),
                status: UploadStatus::Ingesting,
                error: None,
            })
        } else {
            Ok(UploadResult {
                filename: filename.to_string(),
                s3_key: presigned.s3_key,
                progress_id: None,
                status: UploadStatus::Uploaded,
                error: None,
            })
        }
    }

    async fn get_presigned_url(
        &self,
        config: &AppConfig,
        filename: &str,
    ) -> Result<PresignedUrlResponse, String> {
        let url = format!("{}/ingestion/upload-url", config.api_base_url);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &config.api_key)
            .json(&serde_json::json!({ "filename": filename }))
            .send()
            .await
            .map_err(|e| format!("Failed to request presigned URL: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Presigned URL request failed ({}): {}", status, body));
        }

        resp.json::<PresignedUrlResponse>()
            .await
            .map_err(|e| format!("Failed to parse presigned URL response: {}", e))
    }

    async fn upload_to_s3(
        &self,
        upload_url: &str,
        file_bytes: Vec<u8>,
        content_type: &str,
    ) -> Result<(), String> {
        let resp = self
            .client
            .put(upload_url)
            .header("Content-Type", content_type)
            .body(file_bytes)
            .send()
            .await
            .map_err(|e| format!("Failed to upload to S3: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("S3 upload failed ({}): {}", status, body));
        }

        Ok(())
    }

    async fn trigger_ingest(
        &self,
        config: &AppConfig,
        s3_key: &str,
    ) -> Result<IngestResponse, String> {
        let url = format!("{}/ingestion/ingest-s3", config.api_base_url);
        let resp = self
            .client
            .post(&url)
            .header("X-API-Key", &config.api_key)
            .json(&serde_json::json!({
                "s3_key": s3_key,
            }))
            .send()
            .await
            .map_err(|e| format!("Failed to trigger ingestion: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Ingestion trigger failed ({}): {}", status, body));
        }

        resp.json::<IngestResponse>()
            .await
            .map_err(|e| format!("Failed to parse ingestion response: {}", e))
    }

    async fn with_retry<F, Fut, T>(&self, f: F) -> Result<T, String>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let max_attempts = 3;
        let mut last_err = String::new();

        for attempt in 0..max_attempts {
            match f().await {
                Ok(val) => return Ok(val),
                Err(err) => {
                    last_err = err;
                    if attempt < max_attempts - 1 {
                        let delay = Duration::from_millis(500 * 2u64.pow(attempt as u32));
                        log::warn!(
                            "Attempt {} failed, retrying in {:?}: {}",
                            attempt + 1,
                            delay,
                            last_err
                        );
                        sleep(delay).await;
                    }
                }
            }
        }

        Err(format!(
            "Failed after {} attempts: {}",
            max_attempts, last_err
        ))
    }
}
