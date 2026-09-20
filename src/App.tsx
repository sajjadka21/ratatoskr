import { useCallback, useEffect, useMemo, useState } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";

type HealthState = "ready" | "error";

type ComponentHealth = {
  status: HealthState;
  message: string | null;
};

type HealthCheckResponse = {
  core: ComponentHealth;
  storage: ComponentHealth;
  database: ComponentHealth;
};

type AppInfoResponse = {
  name: string;
  version: string;
};

type DownloadProgressEvent = {
  downloadId: string;
  downloadedBytes: number;
  totalBytes: number | null;
};

type StartDownloadResponse = {
  id: string;
  filename: string | null;
  destinationPath: string | null;
  downloadedBytes: number;
  totalBytes: number | null;
  status: string;
};

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";

  const units = ["B", "KB", "MB", "GB", "TB"];
  const index = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    units.length - 1,
  );

  const value = bytes / 1024 ** index;
  return `${value.toFixed(index === 0 ? 0 : 2)} ${units[index]}`;
}

function App() {
  const [health, setHealth] = useState<HealthCheckResponse | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfoResponse | null>(null);
  const [url, setUrl] = useState("");
  const [progress, setProgress] = useState<DownloadProgressEvent | null>(null);
  const [result, setResult] = useState<StartDownloadResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [downloading, setDownloading] = useState(false);

  const refreshHealth = useCallback(async () => {
    const [healthResult, appInfoResult] = await Promise.all([
      invoke<HealthCheckResponse>("health_check"),
      invoke<AppInfoResponse>("get_app_info"),
    ]);

    setHealth(healthResult);
    setAppInfo(appInfoResult);
  }, []);

  useEffect(() => {
    void refreshHealth().catch((reason) => {
      setError(String(reason));
    });
  }, [refreshHealth]);

  const percent = useMemo(() => {
    if (!progress?.totalBytes || progress.totalBytes <= 0) {
      return null;
    }

    return Math.min(
      100,
      (progress.downloadedBytes / progress.totalBytes) * 100,
    );
  }, [progress]);

  async function startDownload() {
    const trimmedUrl = url.trim();

    if (!trimmedUrl) {
      setError("Enter a download URL.");
      return;
    }

    setDownloading(true);
    setProgress(null);
    setResult(null);
    setError(null);

    const onProgress = new Channel<DownloadProgressEvent>();

    onProgress.onmessage = (message) => {
      setProgress(message);
    };

    try {
      const response = await invoke<StartDownloadResponse>(
        "start_download",
        {
          url: trimmedUrl,
          onProgress,
        },
      );

      setResult(response);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setDownloading(false);
    }
  }

  const allReady =
    health?.core.status === "ready" &&
    health?.storage.status === "ready" &&
    health?.database.status === "ready";

  return (
    <main
      style={{
        minHeight: "100vh",
        boxSizing: "border-box",
        padding: 40,
        background: "#0d1117",
        color: "#f0f6fc",
        fontFamily: "Segoe UI, sans-serif",
      }}
    >
      <div style={{ maxWidth: 760, margin: "0 auto" }}>
        <h1 style={{ marginBottom: 6 }}>
          {appInfo?.name ?? "Download Manager"}
        </h1>

        <div style={{ color: "#8b949e", marginBottom: 32 }}>
          Version {appInfo?.version ?? "..."} · 
          {allReady ? "Backend Ready" : "Checking backend..."}
        </div>

        <section
          style={{
            padding: 24,
            border: "1px solid #30363d",
            borderRadius: 14,
            background: "#161b22",
          }}
        >
          <h2 style={{ marginTop: 0 }}>New Download</h2>

          <input
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !downloading) {
                void startDownload();
              }
            }}
            placeholder="https://example.com/file.zip"
            disabled={downloading}
            style={{
              boxSizing: "border-box",
              width: "100%",
              padding: "13px 14px",
              borderRadius: 8,
              border: "1px solid #30363d",
              background: "#0d1117",
              color: "#f0f6fc",
              fontSize: 14,
              outline: "none",
            }}
          />

          <button
            onClick={() => void startDownload()}
            disabled={downloading || !allReady}
            style={{
              marginTop: 14,
              padding: "11px 18px",
              border: 0,
              borderRadius: 8,
              background: downloading ? "#30363d" : "#238636",
              color: "#fff",
              fontWeight: 600,
              cursor: downloading ? "default" : "pointer",
            }}
          >
            {downloading ? "Downloading..." : "Download"}
          </button>

          {(downloading || progress) && (
            <div style={{ marginTop: 24 }}>
              <div
                style={{
                  height: 10,
                  overflow: "hidden",
                  borderRadius: 999,
                  background: "#30363d",
                }}
              >
                <div
                  style={{
                    width: `${percent ?? 100}%`,
                    height: "100%",
                    background: "#3fb950",
                    transition: "width 100ms linear",
                    opacity: percent === null ? 0.5 : 1,
                  }}
                />
              </div>

              <div
                style={{
                  marginTop: 10,
                  display: "flex",
                  justifyContent: "space-between",
                  color: "#8b949e",
                  fontSize: 13,
                }}
              >
                <span>
                  {formatBytes(progress?.downloadedBytes ?? 0)}
                </span>

                <span>
                  {progress?.totalBytes
                    ? `${formatBytes(progress.totalBytes)} · ${percent?.toFixed(1)}%`
                    : "Unknown size"}
                </span>
              </div>
            </div>
          )}

          {result && (
            <div
              style={{
                marginTop: 24,
                padding: 16,
                borderRadius: 8,
                background: "#0d2818",
                border: "1px solid #238636",
              }}
            >
              <strong>Completed</strong>
              <div style={{ marginTop: 8 }}>
                {result.filename ?? "Downloaded file"}
              </div>
              <div
                style={{
                  marginTop: 4,
                  color: "#8b949e",
                  wordBreak: "break-all",
                }}
              >
                {result.destinationPath}
              </div>
            </div>
          )}

          {error && (
            <div
              style={{
                marginTop: 20,
                color: "#ff7b72",
                whiteSpace: "pre-wrap",
              }}
            >
              {error}
            </div>
          )}
        </section>

        <div
          style={{
            marginTop: 18,
            color: "#8b949e",
            fontSize: 13,
          }}
        >
          Files are currently saved to your system Downloads folder.
        </div>
      </div>
    </main>
  );
}

export default App;
