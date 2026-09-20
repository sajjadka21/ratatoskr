import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

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

const styles = {
  app: {
    minHeight: "100vh",
    background: "#0d1117",
    color: "#f0f6fc",
    fontFamily:
      'Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
    display: "flex",
  },
  sidebar: {
    width: "220px",
    borderRight: "1px solid #21262d",
    padding: "24px 18px",
    background: "#0b0f14",
  },
  logo: {
    fontSize: "17px",
    fontWeight: 700,
    marginBottom: "32px",
  },
  navItem: {
    padding: "10px 12px",
    background: "#161b22",
    borderRadius: "8px",
    fontSize: "14px",
  },
  main: {
    flex: 1,
    padding: "48px",
  },
  header: {
    maxWidth: "760px",
    marginBottom: "30px",
  },
  title: {
    fontSize: "30px",
    margin: "0 0 8px",
  },
  subtitle: {
    color: "#8b949e",
    margin: 0,
  },
  card: {
    maxWidth: "760px",
    background: "#161b22",
    border: "1px solid #30363d",
    borderRadius: "14px",
    overflow: "hidden",
  },
  cardHeader: {
    padding: "18px 20px",
    borderBottom: "1px solid #30363d",
    fontWeight: 600,
  },
  row: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "18px 20px",
    borderBottom: "1px solid #21262d",
  },
  name: {
    fontWeight: 500,
  },
  status: {
    display: "flex",
    alignItems: "center",
    gap: "8px",
    fontSize: "14px",
  },
  footer: {
    maxWidth: "760px",
    display: "flex",
    justifyContent: "space-between",
    marginTop: "18px",
    color: "#8b949e",
    fontSize: "13px",
  },
  button: {
    border: "1px solid #30363d",
    background: "#161b22",
    color: "#f0f6fc",
    borderRadius: "8px",
    padding: "8px 12px",
    cursor: "pointer",
  },
} as const;

function StatusRow({
  name,
  health,
}: {
  name: string;
  health?: ComponentHealth;
}) {
  const ready = health?.status === "ready";

  return (
    <div style={styles.row}>
      <span style={styles.name}>{name}</span>

      <span style={styles.status}>
        <span
          style={{
            width: "9px",
            height: "9px",
            borderRadius: "50%",
            background:
              health === undefined
                ? "#8b949e"
                : ready
                  ? "#3fb950"
                  : "#f85149",
          }}
        />

        {health === undefined
          ? "Checking..."
          : ready
            ? "Ready"
            : health.message ?? "Error"}
      </span>
    </div>
  );
}

function App() {
  const [health, setHealth] = useState<HealthCheckResponse | null>(null);
  const [appInfo, setAppInfo] = useState<AppInfoResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setError(null);

    try {
      const [healthResult, appInfoResult] = await Promise.all([
        invoke<HealthCheckResponse>("health_check"),
        invoke<AppInfoResponse>("get_app_info"),
      ]);

      setHealth(healthResult);
      setAppInfo(appInfoResult);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <div style={styles.app}>
      <aside style={styles.sidebar}>
        <div style={styles.logo}>Download Manager</div>

        <div style={styles.navItem}>System Status</div>
      </aside>

      <main style={styles.main}>
        <div style={styles.header}>
          <h1 style={styles.title}>
            {appInfo?.name ?? "Download Manager"}
          </h1>

          <p style={styles.subtitle}>
            Milestone 0 infrastructure validation
          </p>
        </div>

        <section style={styles.card}>
          <div style={styles.cardHeader}>
            Backend Health
          </div>

          <StatusRow
            name="Core"
            health={health?.core}
          />

          <StatusRow
            name="Storage"
            health={health?.storage}
          />

          <StatusRow
            name="Database"
            health={health?.database}
          />

          {error && (
            <div
              style={{
                padding: "18px 20px",
                color: "#f85149",
              }}
            >
              IPC Error: {error}
            </div>
          )}
        </section>

        <div style={styles.footer}>
          <span>
            Version {appInfo?.version ?? "..."}
          </span>

          <button
            style={styles.button}
            onClick={() => void refresh()}
          >
            Refresh Health
          </button>
        </div>
      </main>
    </div>
  );
}

export default App;