import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowUpCircle, Fan, HelpCircle, LayoutDashboard, Lightbulb, MonitorSmartphone, Settings } from "lucide-react";
import { useEffect, useState } from "react";

import { ToastProvider } from "./components/ui";
import { api } from "./lib/api";
import { useStatus } from "./lib/hooks";
import { checkForUpdate, type UpdateCheck } from "./lib/updates";
import { Dashboard } from "./pages/Dashboard";
import { FansPage } from "./pages/FansPage";
import { HelpPage } from "./pages/HelpPage";
import { RgbPage } from "./pages/RgbPage";
import { ScreenPage } from "./pages/ScreenPage";
import { SettingsPage } from "./pages/SettingsPage";

export type Page = "dashboard" | "screen" | "rgb" | "fans" | "settings" | "help";

const NAV: { id: Page; label: string; icon: typeof LayoutDashboard; soon?: boolean }[] = [
  { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
  { id: "screen", label: "Screen", icon: MonitorSmartphone },
  { id: "rgb", label: "RGB", icon: Lightbulb },
  { id: "fans", label: "Fans", icon: Fan },
  { id: "settings", label: "Settings", icon: Settings },
  { id: "help", label: "Help", icon: HelpCircle },
];

export default function App() {
  const [page, setPage] = useState<Page>("dashboard");
  const status = useStatus();
  const [update, setUpdate] = useState<UpdateCheck | null>(null);

  // One quiet check per start, well after the UI is up; failures are silent
  // (Settings → About has the button with the real error).
  useEffect(() => {
    const ctl = new AbortController();
    const t = window.setTimeout(() => {
      api.appInfo()
        .then((i) => checkForUpdate(i.version, ctl.signal))
        .then((u) => u.available && setUpdate(u))
        .catch(() => {});
    }, 15_000);
    return () => {
      window.clearTimeout(t);
      ctl.abort();
    };
  }, []);

  const pill = status.connected
    ? { cls: "on", title: status.screen?.name ?? "Screen", sub: `${status.fps_actual.toFixed(0)} fps · ${status.theme_name}` }
    : status.needs_udev
      ? { cls: "warn", title: "Screen found", sub: "needs device access" }
      : { cls: "off", title: "No screen", sub: "waiting for a device" };

  return (
    <ToastProvider>
      <div className="shell">
        <aside className="sidebar">
          <div className="brand">
            <img src="/icon.png" alt="" />
            <div>
              <div className="brand-name">NERU</div>
              <div className="brand-sub">Screen Control</div>
            </div>
          </div>
          {NAV.map((n) => (
            <button key={n.id} className={`nav-item ${page === n.id ? "active" : ""}`} onClick={() => setPage(n.id)}>
              <n.icon size={17} />
              {n.label}
              {n.soon && <span className="badge">soon</span>}
            </button>
          ))}
          <div className="sidebar-foot">
            <div className="screen-pill" title={status.last_error ?? undefined}>
              <span className={`dot ${pill.cls}`} />
              <div style={{ minWidth: 0 }}>
                <strong>{pill.title}</strong>
                <span className="dim">{pill.sub}</span>
              </div>
            </div>
            {update && (
              <button className="update-pill" title="Open the release page" onClick={() => openUrl(update.url).catch(() => setPage("settings"))}>
                <ArrowUpCircle size={14} /> Neru {update.latest} is out
              </button>
            )}
          </div>
        </aside>
        <main className="main" id="main">
          {page === "dashboard" && <Dashboard status={status} goTo={setPage} />}
          {page === "screen" && <ScreenPage status={status} />}
          {page === "rgb" && <RgbPage />}
          {page === "fans" && <FansPage />}
          {page === "settings" && <SettingsPage status={status} />}
          {page === "help" && <HelpPage goTo={setPage} />}
        </main>
      </div>
    </ToastProvider>
  );
}
