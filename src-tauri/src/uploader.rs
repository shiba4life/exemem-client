use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use uuid::Uuid;

use crate::config::AppConfig;

/// Max concurrent uploads
const MAX_CONCURRENT_UPLOADS: usize = 3;

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
    #[allow(dead_code)]
    s3_bucket: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IngestResponse {
    progress_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressResponse {
    pub progress_id: String,
    pub status: String,
    pub percent: Option<f64>,
    pub message: Option<String>,
}

pub struct Uploader {
    client: Client,
    semaphore: Arc<Semaphore>,
}

impl Uploader {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");
        Self {
            client,
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_UPLOADS)),
        }
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

        // Acquire semaphore permit for concurrency limiting
        let _permit = self.semaphore.acquire().await;

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
        // Determine content type upfront so presigned URL is signed with the same type
        let content_type = mime_guess::from_path(file_path)
            .first_or_octet_stream()
            .to_string();

        // Step 1: Get presigned URL (signed with our content_type)
        let presigned = self
            .with_retry(|| self.get_presigned_url(config, filename, &content_type))
            .await?;

        // Step 2: Upload file to S3
        let file_bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| format!("Failed to read file: {}", e))?;

        self.with_retry(|| {
            self.upload_to_s3(&presigned.upload_url, file_bytes.clone(), &content_type)
        })
        .await?;

        // Step 3: Trigger ingestion if auto_ingest is enabled
        if config.auto_ingest {
            let progress_id = Uuid::new_v4().to_string();
            let s3_bucket = presigned
                .s3_bucket
                .clone()
                .unwrap_or_else(|| "exemem-user-data".to_string());

            let ingest_resp = self
                .with_retry(|| {
                    self.trigger_ingest(config, &presigned.s3_key, &s3_bucket, &progress_id)
                })
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
        content_type: &str,
    ) -> Result<PresignedUrlResponse, String> {
        let url = format!("{}/api/ingestion/upload-url", config.api_url());
        let mut req = self
            .client
            .post(&url)
            .header("X-API-Key", &config.api_key)
            .json(&serde_json::json!({
                "filename": filename,
                "file_type": content_type,
            }));

        if let Some(user_hash) = &config.user_hash {
            req = req.header("X-User-Hash", user_hash);
        }

        let resp = req
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
        s3_bucket: &str,
        progress_id: &str,
    ) -> Result<IngestResponse, String> {
        let url = format!("{}/api/ingestion/ingest-s3", config.api_url());
        let mut req = self
            .client
            .post(&url)
            .header("X-API-Key", &config.api_key)
            .json(&serde_json::json!({
                "s3_key": s3_key,
                "s3_bucket": s3_bucket,
                "progress_id": progress_id,
            }));

        if let Some(user_hash) = &config.user_hash {
            req = req.header("X-User-Hash", user_hash);
        }

        let resp = req
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

    pub async fn poll_progress(
        &self,
        config: &AppConfig,
        progress_id: &str,
    ) -> Result<ProgressResponse, String> {
        let url = format!(
            "{}/api/ingestion/progress/{}",
            config.api_url(),
            progress_id
        );
        let mut req = self.client.get(&url).header("X-API-Key", &config.api_key);

        if let Some(user_hash) = &config.user_hash {
            req = req.header("X-User-Hash", user_hash);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to poll progress: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Progress poll failed ({}): {}", status, body));
        }

        resp.json::<ProgressResponse>()
            .await
            .map_err(|e| format!("Failed to parse progress response: {}", e))
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
