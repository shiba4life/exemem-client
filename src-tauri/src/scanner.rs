use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 10;
const MAX_FILES: usize = 5000;

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    "__pycache__",
    ".git",
    ".svn",
    "target",
    "build",
    "dist",
    ".cache",
    "venv",
    ".venv",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecommendation {
    pub path: String,
    pub absolute_path: PathBuf,
    pub should_ingest: bool,
    pub category: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    pub personal_data_count: usize,
    pub media_count: usize,
    pub config_count: usize,
    pub website_scaffolding_count: usize,
    pub work_count: usize,
    pub unknown_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub total_files: usize,
    pub recommended_files: Vec<FileRecommendation>,
    pub skipped_files: Vec<FileRecommendation>,
    pub summary: ScanSummary,
}

/// Scan a directory tree and classify all files using heuristics.
pub fn scan_and_classify(root: &Path) -> Result<ScanResult, String> {
    let files = scan_directory_tree(root, MAX_DEPTH, MAX_FILES)?;
    let recommendations = classify_files(root, &files);

    let mut recommended = Vec::new();
    let mut skipped = Vec::new();

    for rec in &recommendations {
        if rec.should_ingest {
            recommended.push(rec.clone());
        } else {
            skipped.push(rec.clone());
        }
    }

    let summary = build_summary(&recommendations);

    Ok(ScanResult {
        total_files: files.len(),
        recommended_files: recommended,
        skipped_files: skipped,
        summary,
    })
}

fn scan_directory_tree(
    root: &Path,
    max_depth: usize,
    max_files: usize,
) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    scan_recursive(root, root, 0, max_depth, max_files, &mut files)?;
    Ok(files)
}

fn scan_recursive(
    root: &Path,
    current: &Path,
    depth: usize,
    max_depth: usize,
    max_files: usize,
    files: &mut Vec<String>,
) -> Result<(), String> {
    if depth > max_depth || files.len() >= max_files {
        return Ok(());
    }

    let entries = std::fs::read_dir(current)
        .map_err(|e| format!("Failed to read directory {}: {}", current.display(), e))?;

    for entry in entries.flatten() {
        if files.len() >= max_files {
            break;
        }

        let path = entry.path();
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip hidden files and directories
        if file_name.starts_with('.') {
            continue;
        }

        // Skip common non-data directories
        if path.is_dir() && SKIP_DIRS.contains(&file_name) {
            continue;
        }

        if path.is_dir() {
            scan_recursive(root, &path, depth + 1, max_depth, max_files, files)?;
        } else if path.is_file() {
            if let Ok(relative) = path.strip_prefix(root) {
                files.push(relative.to_string_lossy().to_string());
            }
        }
    }

    Ok(())
}

fn classify_files(root: &Path, file_tree: &[String]) -> Vec<FileRecommendation> {
    file_tree
        .iter()
        .map(|path| {
            let lower = path.to_lowercase();
            let ext = Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            // Website scaffolding patterns
            let is_scaffolding = lower.contains("node_modules")
                || lower.contains("twemoji")
                || lower.contains("/assets/")
                || lower.contains("runtime.")
                || lower.contains("modules.")
                || ext == "woff"
                || ext == "woff2"
                || ext == "eot"
                || ext == "ttf"
                || (ext == "svg" && lower.contains("emoji"));

            // Config patterns
            let is_config = lower.starts_with('.')
                || lower.contains(".config")
                || lower.contains("config/")
                || ext == "env"
                || ext == "ini"
                || ext == "yaml"
                || ext == "yml";

            // Personal data patterns
            let is_personal = ext == "json"
                || ext == "csv"
                || ext == "txt"
                || ext == "md"
                || ext == "doc"
                || ext == "docx"
                || ext == "pdf"
                || ext == "js"
                || lower.contains("data/")
                || lower.contains("export")
                || lower.contains("backup");

            // Media patterns
            let is_media = ext == "jpg"
                || ext == "jpeg"
                || ext == "png"
                || ext == "gif"
                || ext == "mp4"
                || ext == "mp3"
                || ext == "wav";

            let (should_ingest, category, reason) = if is_scaffolding {
                (
                    false,
                    "website_scaffolding",
                    "Appears to be website/app scaffolding",
                )
            } else if is_config {
                (false, "config", "Appears to be configuration file")
            } else if is_media && !lower.contains("twemoji") && !lower.contains("/assets/") {
                (true, "media", "User media file")
            } else if is_personal {
                (true, "personal_data", "Potential personal data file")
            } else {
                (false, "unknown", "Unknown file type")
            };

            FileRecommendation {
                path: path.clone(),
                absolute_path: root.join(path),
                should_ingest,
                category: category.to_string(),
                reason: reason.to_string(),
            }
        })
        .collect()
}

fn build_summary(recommendations: &[FileRecommendation]) -> ScanSummary {
    let mut summary = ScanSummary {
        personal_data_count: 0,
        media_count: 0,
        config_count: 0,
        website_scaffolding_count: 0,
        work_count: 0,
        unknown_count: 0,
    };

    for rec in recommendations {
        match rec.category.as_str() {
            "personal_data" => summary.personal_data_count += 1,
            "media" => summary.media_count += 1,
            "config" => summary.config_count += 1,
            "website_scaffolding" => summary.website_scaffolding_count += 1,
            "work" => summary.work_count += 1,
            _ => summary.unknown_count += 1,
        }
    }

    summary
}

/// Classify a single file path using the same heuristics.
/// Used by the watcher to classify newly detected files.
pub fn classify_single_file(root: &Path, absolute_path: &Path) -> FileRecommendation {
    let relative = absolute_path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| {
            absolute_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string())
        });

    let results = classify_files(root, &[relative]);
    results.into_iter().next().unwrap_or(FileRecommendation {
        path: absolute_path.to_string_lossy().to_string(),
        absolute_path: absolute_path.to_path_buf(),
        should_ingest: false,
        category: "unknown".to_string(),
        reason: "Could not classify".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_json_file() {
        let root = Path::new("/tmp/test");
        let files = vec!["data/export.json".to_string()];
        let results = classify_files(root, &files);
        assert_eq!(results.len(), 1);
        assert!(results[0].should_ingest);
        assert_eq!(results[0].category, "personal_data");
    }

    #[test]
    fn test_classify_node_modules() {
        let root = Path::new("/tmp/test");
        let files = vec!["node_modules/react/index.js".to_string()];
        let results = classify_files(root, &files);
        assert_eq!(results.len(), 1);
        assert!(!results[0].should_ingest);
        assert_eq!(results[0].category, "website_scaffolding");
    }

    #[test]
    fn test_classify_media() {
        let root = Path::new("/tmp/test");
        let files = vec!["photos/vacation.jpg".to_string()];
        let results = classify_files(root, &files);
        assert_eq!(results.len(), 1);
        assert!(results[0].should_ingest);
        assert_eq!(results[0].category, "media");
    }

    #[test]
    fn test_classify_config() {
        let root = Path::new("/tmp/test");
        let files = vec!["config/settings.yaml".to_string()];
        let results = classify_files(root, &files);
        assert_eq!(results.len(), 1);
        assert!(!results[0].should_ingest);
        assert_eq!(results[0].category, "config");
    }

    #[test]
    fn test_classify_media_in_assets_skipped() {
        let root = Path::new("/tmp/test");
        let files = vec!["web/assets/logo.png".to_string()];
        let results = classify_files(root, &files);
        assert_eq!(results.len(), 1);
        assert!(!results[0].should_ingest);
    }

    #[test]
    fn test_classify_unknown() {
        let root = Path::new("/tmp/test");
        let files = vec!["something.xyz".to_string()];
        let results = classify_files(root, &files);
        assert_eq!(results.len(), 1);
        assert!(!results[0].should_ingest);
        assert_eq!(results[0].category, "unknown");
    }
}
