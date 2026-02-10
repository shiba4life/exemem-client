import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import CategoryBadge from "./shared/CategoryBadge";
import ProgressBar from "./shared/ProgressBar";

function StatusIcon({ status }) {
  switch (status) {
    case "Uploading": return <span className="text-blue-500">...</span>;
    case "Uploaded": return <span className="text-blue-600">^</span>;
    case "Ingesting": return <span className="text-yellow-500">~</span>;
    case "Done": return <span className="text-green-600">ok</span>;
    case "Error": return <span className="text-red-500">!</span>;
    default: return <span className="text-gray-400">?</span>;
  }
}

export default function SyncPanel({ config, saveConfig, setError, setSuccess, syncStatus, setSyncStatus }) {
  const [subPhase, setSubPhase] = useState("idle"); // idle, scanning, review, ingesting, watching
  const [scanResult, setScanResult] = useState(null);
  const [selectedFiles, setSelectedFiles] = useState(new Set());
  const [showSkipped, setShowSkipped] = useState(false);
  const [ingestionProgress, setIngestionProgress] = useState([]);

  // Auto-detect if already watching
  useEffect(() => {
    if (syncStatus.watching) {
      setSubPhase("watching");
    }
  }, [syncStatus.watching]);

  useEffect(() => {
    const unlistenProgress = listen("ingestion-progress", (event) => {
      setIngestionProgress(event.payload);
    });

    const unlistenComplete = listen("ingestion-complete", () => {
      setSubPhase("watching");
      handleStartWatching();
    });

    return () => {
      unlistenProgress.then((f) => f());
      unlistenComplete.then((f) => f());
    };
  }, []);

  const handleScanAndWatch = async () => {
    setError(null);
    try {
      await saveConfig(config);
      setSubPhase("scanning");
      const result = await invoke("scan_folder");
      setScanResult(result);
      const recommended = new Set(result.recommended_files.map((f) => f.path));
      setSelectedFiles(recommended);
      setSubPhase("review");
    } catch (err) {
      setError(String(err));
      setSubPhase("idle");
    }
  };

  const handleApproveAndIngest = async () => {
    setError(null);
    try {
      const approvedPaths = Array.from(selectedFiles);
      if (approvedPaths.length === 0) {
        setSubPhase("watching");
        await handleStartWatching();
        return;
      }
      setSubPhase("ingesting");
      await invoke("approve_and_ingest", { approvedPaths });
    } catch (err) {
      setError(String(err));
      setSubPhase("review");
    }
  };

  const handleStartWatching = async () => {
    try {
      await invoke("start_watching");
      const status = await invoke("get_sync_status");
      setSyncStatus(status);
    } catch (err) {
      console.error("Failed to start watching:", err);
    }
  };

  const toggleWatching = async () => {
    setError(null);
    try {
      if (syncStatus.watching) {
        await invoke("stop_watching");
        setSubPhase("idle");
      } else {
        await invoke("start_watching");
        setSubPhase("watching");
      }
      const status = await invoke("get_sync_status");
      setSyncStatus(status);
    } catch (err) {
      setError(String(err));
    }
  };

  const toggleFileSelection = (path) => {
    setSelectedFiles((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const formatTime = (timestamp) => {
    if (!timestamp) return "";
    const date = new Date(Number(timestamp) * 1000);
    return date.toLocaleTimeString();
  };

  const progressSummary = ingestionProgress.reduce(
    (acc, p) => {
      if (p.status === "done" || p.status === "completed") acc.done++;
      else if (p.status === "error" || p.status === "failed") acc.error++;
      else if (p.status === "pending") acc.pending++;
      else acc.inProgress++;
      return acc;
    },
    { done: 0, error: 0, pending: 0, inProgress: 0 },
  );

  // Idle state - prompt to scan
  if (subPhase === "idle") {
    return (
      <div className="space-y-4">
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-8 text-center space-y-4">
          <div className="inline-flex items-center justify-center w-12 h-12 bg-blue-100 rounded-full">
            <svg className="w-6 h-6 text-blue-600" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={1.5}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M2.25 12.75V12A2.25 2.25 0 0 1 4.5 9.75h15A2.25 2.25 0 0 1 21.75 12v.75m-8.69-6.44-2.12-2.12a1.5 1.5 0 0 0-1.061-.44H4.5A2.25 2.25 0 0 0 2.25 6v12a2.25 2.25 0 0 0 2.25 2.25h15A2.25 2.25 0 0 0 21.75 18V9a2.25 2.25 0 0 0-2.25-2.25h-5.379a1.5 1.5 0 0 1-1.06-.44Z" />
            </svg>
          </div>
          <div>
            <h2 className="text-lg font-semibold text-gray-900">Folder Sync</h2>
            <p className="text-sm text-gray-500 mt-1">
              {config.watched_folder
                ? `Ready to scan: ${config.watched_folder}`
                : "Configure a watched folder in Settings first"}
            </p>
          </div>
          <button
            onClick={handleScanAndWatch}
            disabled={!config.watched_folder}
            className="px-6 py-2 bg-primary text-white rounded-lg text-sm font-medium hover:bg-secondary transition-colors disabled:opacity-50"
          >
            Scan & Watch
          </button>
        </div>
      </div>
    );
  }

  // Scanning
  if (subPhase === "scanning") {
    return (
      <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-8 text-center space-y-4">
        <div className="inline-flex items-center justify-center w-12 h-12 bg-yellow-100 rounded-full">
          <svg className="w-6 h-6 text-yellow-600 animate-spin" fill="none" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z" />
          </svg>
        </div>
        <div>
          <h2 className="text-lg font-semibold text-gray-900">Scanning folder...</h2>
          <p className="text-sm text-gray-500 mt-1">Classifying files by category</p>
        </div>
      </div>
    );
  }

  // Review
  if (subPhase === "review" && scanResult) {
    return (
      <div className="space-y-4">
        {/* Summary */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-4">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">Scan Results</h2>
            <button onClick={() => { setSubPhase("idle"); setScanResult(null); }} className="text-xs text-gray-500 hover:text-gray-700">Back</button>
          </div>

          <div className="text-sm text-gray-700">
            <span className="font-semibold">{scanResult.total_files}</span> files found:
            <span className="text-green-700 font-medium ml-1">{scanResult.recommended_files.length} to ingest</span>,
            <span className="text-gray-500 ml-1">{scanResult.skipped_files.length} skipped</span>
          </div>

          <div className="flex flex-wrap gap-2">
            {scanResult.summary.personal_data_count > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-1 rounded bg-blue-100 text-blue-700 text-xs font-medium">
                Personal Data: {scanResult.summary.personal_data_count}
              </span>
            )}
            {scanResult.summary.media_count > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-1 rounded bg-purple-100 text-purple-700 text-xs font-medium">
                Media: {scanResult.summary.media_count}
              </span>
            )}
            {scanResult.summary.config_count > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-1 rounded bg-gray-100 text-gray-600 text-xs font-medium">
                Config: {scanResult.summary.config_count}
              </span>
            )}
            {scanResult.summary.website_scaffolding_count > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-1 rounded bg-orange-100 text-orange-700 text-xs font-medium">
                Scaffolding: {scanResult.summary.website_scaffolding_count}
              </span>
            )}
            {scanResult.summary.unknown_count > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-1 rounded bg-gray-100 text-gray-500 text-xs font-medium">
                Unknown: {scanResult.summary.unknown_count}
              </span>
            )}
          </div>
        </div>

        {/* Recommended files */}
        <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-3">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-semibold text-gray-700">Recommended ({scanResult.recommended_files.length})</h3>
            <div className="flex gap-2">
              <button
                onClick={() => setSelectedFiles(new Set(scanResult.recommended_files.map((f) => f.path)))}
                className="text-xs text-blue-600 hover:text-blue-800"
              >Select All</button>
              <button
                onClick={() => setSelectedFiles(new Set())}
                className="text-xs text-gray-500 hover:text-gray-700"
              >Deselect All</button>
            </div>
          </div>

          <div className="space-y-1 max-h-64 overflow-y-auto">
            {scanResult.recommended_files.map((file) => (
              <label key={file.path} className="flex items-center gap-2 px-2 py-1.5 hover:bg-gray-50 rounded cursor-pointer">
                <input
                  type="checkbox"
                  checked={selectedFiles.has(file.path)}
                  onChange={() => toggleFileSelection(file.path)}
                  className="rounded border-gray-300 text-primary focus:ring-primary"
                />
                <CategoryBadge category={file.category} />
                <span className="text-sm text-gray-700 truncate flex-1" title={file.path}>{file.path}</span>
              </label>
            ))}
          </div>
        </div>

        {/* Skipped files */}
        {scanResult.skipped_files.length > 0 && (
          <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-3">
            <button
              onClick={() => setShowSkipped(!showSkipped)}
              className="flex items-center justify-between w-full text-left"
            >
              <h3 className="text-sm font-semibold text-gray-500">Skipped ({scanResult.skipped_files.length})</h3>
              <span className="text-xs text-gray-400">{showSkipped ? "Hide" : "Show"}</span>
            </button>

            {showSkipped && (
              <div className="space-y-1 max-h-48 overflow-y-auto">
                {scanResult.skipped_files.map((file) => (
                  <label key={file.path} className="flex items-center gap-2 px-2 py-1.5 hover:bg-gray-50 rounded cursor-pointer">
                    <input
                      type="checkbox"
                      checked={selectedFiles.has(file.path)}
                      onChange={() => toggleFileSelection(file.path)}
                      className="rounded border-gray-300 text-primary focus:ring-primary"
                    />
                    <CategoryBadge category={file.category} />
                    <span className="text-sm text-gray-400 truncate flex-1" title={file.path}>{file.path}</span>
                  </label>
                ))}
              </div>
            )}
          </div>
        )}

        {/* Approve button */}
        <button
          onClick={handleApproveAndIngest}
          className="w-full px-4 py-3 bg-primary text-white rounded-xl text-sm font-medium hover:bg-secondary transition-colors shadow-sm"
        >
          {selectedFiles.size > 0
            ? `Approve & Ingest (${selectedFiles.size})`
            : "Skip & Start Watching"}
        </button>
      </div>
    );
  }

  // Ingesting
  if (subPhase === "ingesting") {
    return (
      <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-4">
        <div className="flex items-center justify-between">
          <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">Ingesting</h2>
          <span className="text-xs text-gray-500">
            {progressSummary.done}/{ingestionProgress.length} complete
            {progressSummary.inProgress > 0 && `, ${progressSummary.inProgress} in progress`}
            {progressSummary.error > 0 && `, ${progressSummary.error} errors`}
          </span>
        </div>

        <ProgressBar
          percent={ingestionProgress.length > 0 ? (progressSummary.done / ingestionProgress.length) * 100 : 0}
          status={progressSummary.error > 0 ? "error" : progressSummary.done === ingestionProgress.length ? "done" : "ingesting"}
        />

        <div className="space-y-2 max-h-80 overflow-y-auto">
          {ingestionProgress.map((fp) => (
            <div key={fp.filename} className="px-3 py-2 bg-gray-50 rounded-lg space-y-1">
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-gray-700 truncate flex-1">{fp.filename}</span>
                <span className={`text-xs font-medium ml-2 ${
                  fp.status === "done" || fp.status === "completed" ? "text-green-600" :
                  fp.status === "error" || fp.status === "failed" ? "text-red-600" :
                  fp.status === "pending" ? "text-gray-400" :
                  "text-yellow-600"
                }`}>
                  {fp.status}
                </span>
              </div>
              <ProgressBar percent={fp.percent} status={fp.status} />
              {fp.message && <p className="text-xs text-gray-500">{fp.message}</p>}
            </div>
          ))}
        </div>
      </div>
    );
  }

  // Watching
  return (
    <div className="bg-white rounded-xl shadow-sm border border-gray-200 p-5 space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-gray-700 uppercase tracking-wide">Activity</h2>
        <div className="flex items-center gap-3">
          {syncStatus.folder && (
            <span className="text-xs text-gray-500">{syncStatus.file_count} files</span>
          )}
          <button
            onClick={toggleWatching}
            className={`px-3 py-1 rounded-lg text-xs font-medium transition-colors ${
              syncStatus.watching
                ? "bg-red-50 text-red-700 hover:bg-red-100"
                : "bg-green-50 text-green-700 hover:bg-green-100"
            }`}
          >
            {syncStatus.watching ? "Stop" : "Resume"}
          </button>
        </div>
      </div>

      {syncStatus.recent_activity.length === 0 ? (
        <p className="text-sm text-gray-400 text-center py-6">
          Watching for changes. New files will appear here.
        </p>
      ) : (
        <div className="space-y-2 max-h-80 overflow-y-auto">
          {syncStatus.recent_activity.map((entry, i) => (
            <div key={`${entry.filename}-${entry.timestamp}-${i}`} className="flex items-center gap-3 px-3 py-2 bg-gray-50 rounded-lg">
              <StatusIcon status={entry.status} />
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <p className="text-sm font-medium text-gray-800 truncate">{entry.filename}</p>
                  {entry.category && <CategoryBadge category={entry.category} />}
                </div>
                {entry.error && <p className="text-xs text-red-500 truncate">{entry.error}</p>}
              </div>
              <span className="text-xs text-gray-400 whitespace-nowrap">{formatTime(entry.timestamp)}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
