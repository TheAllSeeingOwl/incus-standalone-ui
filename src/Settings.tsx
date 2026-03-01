import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

interface AppConfig {
  host: string;
  port: number;
  acceptInvalidCerts: boolean;
  caCertPath: string | null;
  clientCertPath: string | null;
  clientKeyPath: string | null;
  socketPath: string | null;
}

type ConnectionMode = "unix" | "https";

interface Props {
  onClose: () => void;
  onSaved: () => void;
}

const defaults: AppConfig = {
  host: "localhost",
  port: 8443,
  acceptInvalidCerts: false,
  caCertPath: null,
  clientCertPath: null,
  clientKeyPath: null,
  socketPath: null,
};

const DEFAULT_SOCKET = "/var/lib/incus/unix.socket";

const PEM_FILTER = [{ name: "PEM / Certificate / Key", extensions: ["pem", "crt", "key", "cer"] }];

function getMode(config: AppConfig): ConnectionMode {
  return config.socketPath ? "unix" : "https";
}

export default function Settings({ onClose, onSaved }: Props) {
  const [config, setConfig] = useState<AppConfig>(defaults);
  const [mode, setMode] = useState<ConnectionMode>("https");
  const [status, setStatus] = useState<{ kind: "idle" | "ok" | "err"; msg: string }>({
    kind: "idle",
    msg: "",
  });

  useEffect(() => {
    invoke<AppConfig>("get_settings").then((c) => {
      setConfig(c);
      setMode(getMode(c));
    }).catch(console.error);
  }, []);

  const set = <K extends keyof AppConfig>(key: K, value: AppConfig[K]) =>
    setConfig((c) => ({ ...c, [key]: value }));

  const switchMode = (newMode: ConnectionMode) => {
    setMode(newMode);
    if (newMode === "unix") {
      setConfig((c) => ({ ...c, socketPath: c.socketPath || DEFAULT_SOCKET }));
    } else {
      setConfig((c) => ({ ...c, socketPath: null }));
    }
  };

  const pickFile = async (field: "caCertPath" | "clientCertPath" | "clientKeyPath") => {
    const path = await openDialog({ multiple: false, filters: PEM_FILTER });
    if (typeof path === "string") set(field, path);
  };

  const save = async () => {
    try {
      await invoke("save_settings", { config });
      setStatus({ kind: "ok", msg: "Saved." });
      onSaved();
    } catch (e) {
      setStatus({ kind: "err", msg: String(e) });
    }
  };

  return (
    <div style={styles.panel}>
      <div style={styles.header}>
        <span style={styles.headerTitle}>Connection</span>
        <button style={styles.closeBtn} onClick={onClose} title="Close">&#x2715;</button>
      </div>

      <div style={styles.body}>
        <Section title="Mode">
          <div style={styles.modeRow}>
            <ModeButton
              label="Unix socket"
              active={mode === "unix"}
              onClick={() => switchMode("unix")}
            />
            <ModeButton
              label="HTTPS"
              active={mode === "https"}
              onClick={() => switchMode("https")}
            />
          </div>
        </Section>

        {mode === "unix" ? (
          <Section title="Socket">
            <Row label="Path">
              <input
                style={styles.input}
                value={config.socketPath || ""}
                onChange={(e) => set("socketPath", e.target.value || null)}
                placeholder={DEFAULT_SOCKET}
              />
            </Row>
          </Section>
        ) : (
          <>
            <Section title="Server">
              <Row label="Host">
                <input
                  style={styles.input}
                  value={config.host}
                  onChange={(e) => set("host", e.target.value)}
                  placeholder="localhost"
                />
              </Row>
              <Row label="Port">
                <input
                  style={{ ...styles.input, width: 90 }}
                  type="number"
                  value={config.port}
                  onChange={(e) => set("port", Number(e.target.value))}
                  min={1}
                  max={65535}
                />
              </Row>
            </Section>

            <Section title="TLS">
              <Row label="Skip cert check">
                <label style={styles.checkLabel}>
                  <input
                    type="checkbox"
                    checked={config.acceptInvalidCerts}
                    onChange={(e) => set("acceptInvalidCerts", e.target.checked)}
                  />
                  <span style={{ color: config.acceptInvalidCerts ? "#f5a623" : "#888", fontSize: 12 }}>
                    {config.acceptInvalidCerts ? " Insecure" : " Off"}
                  </span>
                </label>
              </Row>
              <Row label="CA cert">
                <FilePicker
                  path={config.caCertPath}
                  onPick={() => pickFile("caCertPath")}
                  onClear={() => set("caCertPath", null)}
                />
              </Row>
            </Section>

            <Section title="Client Certificate">
              <Row label="Certificate">
                <FilePicker
                  path={config.clientCertPath}
                  onPick={() => pickFile("clientCertPath")}
                  onClear={() => set("clientCertPath", null)}
                />
              </Row>
              <Row label="Private key">
                <FilePicker
                  path={config.clientKeyPath}
                  onPick={() => pickFile("clientKeyPath")}
                  onClear={() => set("clientKeyPath", null)}
                />
              </Row>
            </Section>
          </>
        )}
      </div>

      <div style={styles.footer}>
        {status.kind !== "idle" && (
          <span style={{ color: status.kind === "ok" ? "#6fcf97" : "#eb5757", fontSize: 12, marginRight: 8 }}>
            {status.msg}
          </span>
        )}
        <button style={styles.btn} onClick={save}>Apply</button>
      </div>
    </div>
  );
}

function ModeButton({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      style={{
        ...styles.modeBtn,
        background: active ? "#0e6cc4" : "#1e1e1e",
        color: active ? "#fff" : "#888",
        borderColor: active ? "#0e6cc4" : "#333",
      }}
      onClick={onClick}
    >
      {label}
    </button>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div style={styles.section}>
      <div style={styles.sectionTitle}>{title}</div>
      {children}
    </div>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div style={styles.row}>
      <label style={styles.label}>{label}</label>
      <div style={{ flex: 1 }}>{children}</div>
    </div>
  );
}

function FilePicker({ path, onPick, onClear }: { path: string | null; onPick: () => void; onClear: () => void }) {
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
      <button style={styles.btnSmall} onClick={onPick}>Browse…</button>
      {path ? (
        <>
          <span style={styles.filePath} title={path}>{path.split("/").pop()}</span>
          <button style={{ ...styles.btnSmall, color: "#eb5757" }} onClick={onClear}>&#x2715;</button>
        </>
      ) : (
        <span style={{ color: "#444", fontSize: 12 }}>Not set</span>
      )}
    </div>
  );
}

const styles: Record<string, React.CSSProperties> = {
  panel: {
    width: 360,
    height: "100%",
    display: "flex",
    flexDirection: "column",
    background: "#111",
    fontFamily: "system-ui, sans-serif",
    color: "#e0e0e0",
  },
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "0 14px",
    height: 36,
    borderBottom: "1px solid #222",
    flexShrink: 0,
  },
  headerTitle: {
    fontSize: 13,
    fontWeight: 600,
    color: "#aaa",
  },
  closeBtn: {
    background: "none",
    border: "none",
    color: "#555",
    cursor: "pointer",
    fontSize: 14,
    padding: "2px 4px",
    borderRadius: 4,
  },
  body: {
    flex: 1,
    overflowY: "auto",
    padding: "10px 14px",
  },
  section: {
    marginBottom: 18,
  },
  sectionTitle: {
    fontSize: 10,
    fontWeight: 700,
    textTransform: "uppercase",
    letterSpacing: "0.08em",
    color: "#555",
    marginBottom: 8,
  },
  row: {
    display: "flex",
    alignItems: "center",
    gap: 8,
    marginBottom: 8,
  },
  label: {
    width: 90,
    fontSize: 12,
    color: "#999",
    flexShrink: 0,
  },
  input: {
    width: "100%",
    padding: "5px 8px",
    background: "#1e1e1e",
    border: "1px solid #333",
    borderRadius: 5,
    color: "#e0e0e0",
    fontSize: 12,
    outline: "none",
  },
  checkLabel: {
    display: "flex",
    alignItems: "center",
    gap: 4,
    cursor: "pointer",
  },
  filePath: {
    fontSize: 11,
    color: "#888",
    maxWidth: 140,
    overflow: "hidden",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  footer: {
    display: "flex",
    alignItems: "center",
    justifyContent: "flex-end",
    padding: "10px 14px",
    borderTop: "1px solid #1e1e1e",
    flexShrink: 0,
  },
  btn: {
    padding: "6px 16px",
    background: "#0e6cc4",
    color: "#fff",
    border: "none",
    borderRadius: 5,
    fontSize: 12,
    fontWeight: 600,
    cursor: "pointer",
  },
  btnSmall: {
    padding: "3px 8px",
    background: "#1e1e1e",
    color: "#aaa",
    border: "1px solid #333",
    borderRadius: 4,
    fontSize: 11,
    cursor: "pointer",
  },
  modeRow: {
    display: "flex",
    gap: 6,
  },
  modeBtn: {
    flex: 1,
    padding: "5px 0",
    border: "1px solid #333",
    borderRadius: 5,
    fontSize: 12,
    fontWeight: 600,
    cursor: "pointer",
    textAlign: "center" as const,
  },
};
