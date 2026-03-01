import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import Settings from "./Settings";

interface ProxyInfo {
  port: number;
  first_run: boolean;
}

interface BuildInfo {
  incus_version: string;
  incus_commit: string;
  ui_commit: string;
}

export default function App() {
  const [proxyPort, setProxyPort] = useState<number | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [buildInfo, setBuildInfo] = useState<BuildInfo | null>(null);
  const iframeRef = useRef<HTMLIFrameElement>(null);

  useEffect(() => {
    invoke<ProxyInfo>("get_proxy_port").then((info) => {
      setProxyPort(info.port);
      if (info.first_run) setSettingsOpen(true);
    });

    invoke<BuildInfo>("get_build_info").then(setBuildInfo);

    // Called by Rust (tray → Settings) to open the panel
    (window as any).__openSettings = () => setSettingsOpen(true);

    // Called by reload_main_window command
    (window as any).__reloadIncus = () => {
      const f = iframeRef.current;
      if (f) f.src = f.src;
    };

    // Handle messages from the iframe (link interceptor script)
    const handleMessage = (e: MessageEvent) => {
      const msg = e.data;
      if (!msg || msg.__incus === undefined) return;
      if (msg.__incus === "open-external" && typeof msg.url === "string") {
        invoke("open_external_url", { url: msg.url }).catch(console.error);
      }
      if (msg.__incus === "open-docs" && typeof msg.url === "string") {
        invoke("open_docs_window", { url: msg.url }).catch(console.error);
      }
    };
    window.addEventListener("message", handleMessage);

    return () => {
      delete (window as any).__openSettings;
      delete (window as any).__reloadIncus;
      window.removeEventListener("message", handleMessage);
    };
  }, []);

  const incusUrl = proxyPort ? `http://127.0.0.1:${proxyPort}/ui/` : null;

  return (
    <div style={styles.root}>
      {/* Settings sidebar */}
      <div style={{ ...styles.sidebar, width: settingsOpen ? 360 : 0 }}>
        {settingsOpen && (
          <Settings
            onClose={() => setSettingsOpen(false)}
            onSaved={() => {
              const f = iframeRef.current;
              if (f) f.src = f.src;
            }}
          />
        )}
      </div>

      {/* Incus UI iframe */}
      <div style={styles.main}>
        {/* Thin top bar with settings toggle */}
        <div style={styles.topbar} data-tauri-drag-region>
          <div style={styles.topbarLeft}>
            <span style={styles.topbarTitle}>Incus</span>
            {buildInfo && (
              <span style={styles.topbarMeta} title={`Incus ${buildInfo.incus_version} (${buildInfo.incus_commit}) · UI (${buildInfo.ui_commit})`}>
                v{buildInfo.incus_version} (<span style={styles.topbarCommit}>{buildInfo.ui_commit.split(" ")[0]}</span>)
                <span style={styles.topbarSep}>·</span>
                docs: <span style={styles.topbarCommit}>{buildInfo.incus_commit.split(" ")[0]}</span>
              </span>
            )}
          </div>
          <button
            style={{ ...styles.iconBtn, background: settingsOpen ? "#2a2a2a" : "transparent" }}
            title="Connection settings"
            onClick={() => setSettingsOpen((o) => !o)}
          >
            ⚙
          </button>
        </div>

        {incusUrl ? (
          <iframe
            ref={iframeRef}
            src={incusUrl}
            style={styles.iframe}
            title="Incus UI"
          />
        ) : (
          <div style={styles.loading}>Starting proxy…</div>
        )}
      </div>
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  root: {
    display: "flex",
    height: "100vh",
    width: "100vw",
    overflow: "hidden",
    background: "#0f0f0f",
  },
  sidebar: {
    overflow: "hidden",
    transition: "width 0.2s ease",
    borderRight: "1px solid #2a2a2a",
    flexShrink: 0,
  },
  main: {
    flex: 1,
    display: "flex",
    flexDirection: "column",
    overflow: "hidden",
  },
  topbar: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "0 12px",
    height: 36,
    background: "#111",
    borderBottom: "1px solid #222",
    flexShrink: 0,
  },
  topbarLeft: {
    display: "flex",
    alignItems: "baseline",
    gap: 8,
    overflow: "hidden",
  },
  topbarTitle: {
    fontSize: 13,
    fontWeight: 600,
    color: "#888",
    fontFamily: "system-ui, sans-serif",
    flexShrink: 0,
  },
  topbarMeta: {
    fontSize: 11,
    color: "#555",
    fontFamily: "system-ui, sans-serif",
    display: "flex",
    alignItems: "baseline",
    gap: 4,
    overflow: "hidden",
  },
  topbarSep: {
    color: "#3a3a3a",
  },
  topbarCommit: {
    fontFamily: "monospace",
    fontSize: 10,
    color: "#4a4a4a",
  },
  iconBtn: {
    border: "none",
    color: "#aaa",
    fontSize: 16,
    cursor: "pointer",
    borderRadius: 5,
    padding: "2px 8px",
    lineHeight: 1,
  },
  iframe: {
    flex: 1,
    border: "none",
    width: "100%",
    height: "100%",
  },
  loading: {
    flex: 1,
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    color: "#555",
    fontFamily: "system-ui, sans-serif",
    fontSize: 14,
  },
};
