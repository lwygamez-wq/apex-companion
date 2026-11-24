import { useState } from "react";
import { RecordingAlert } from "./components/RecordingAlert";
import { StatusBadge } from "./components/StatusBadge";
import { UpdateBanner } from "./components/UpdateBanner";
import { NewSessionBanner } from "./components/NewSessionBanner";
import { PageBoundary } from "./components/PageBoundary";
import { isTauri } from "./lib/api";
import { useCoachVoice, useStatus } from "./lib/hooks";
import { HistoryPage } from "./pages/HistoryPage";
import { ComparisonPage } from "./pages/ComparisonPage";
import { DashboardPage } from "./pages/DashboardPage";
import { LapAnalysisPage } from "./pages/LapAnalysisPage";
import { LiveCoachPage } from "./pages/LiveCoachPage";
import { SessionsPage } from "./pages/SessionsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { StrategyPage } from "./pages/StrategyPage";
import { SetupsPage } from "./pages/SetupsPage";
import { ReferencesPage } from "./pages/ReferencesPage";
import { Alert } from "./components/ui";

export type Route =
  | { page: "dashboard" }
  | { page: "sessions" }
  | { page: "analysis"; sessionId?: number; refLapId?: number; cmpLapId?: number }
  | { page: "comparison" }
  | { page: "history" }
  | { page: "live" }
  | { page: "strategy" }
  | { page: "setups" }
  | { page: "references" }
  | { page: "settings" };

/** Navigation rangée par moment du week-end : avant de rouler, en piste, après la séance. */
const NAV: { group: string; items: { page: Route["page"]; label: string; also?: Route["page"][] }[] }[] = [
  { group: "Avant de rouler", items: [{ page: "dashboard", label: "Briefing" }, { page: "strategy", label: "Stratégie" }, { page: "setups", label: "Setups" }] },
  { group: "En piste", items: [{ page: "live", label: "Au volant" }] },
  { group: "Après la séance", items: [{ page: "sessions", label: "Séances", also: ["analysis", "comparison"] }, { page: "references", label: "Références" }, { page: "history", label: "Progression" }] },
  { group: "Application", items: [{ page: "settings", label: "Réglages" }] },
];

export function App() {
  const [route, setRoute] = useState<Route>({ page: "dashboard" });
  const status = useStatus();
  useCoachVoice();

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="brand">
          <span className="brand-mark">
            APEX<span aria-hidden>/</span>
          </span>
          <span className="brand-sub">Companion, ingénieur de course</span>
        </div>
        {NAV.map((g) => (
          <div key={g.group} role="group" aria-label={g.group}>
            <div className="nav-group">{g.group}</div>
            {g.items.map((n) => (
              <button key={n.page} className={`nav-item ${route.page === n.page || n.also?.includes(route.page) ? "active" : ""}`} aria-current={route.page === n.page || n.also?.includes(route.page) ? "page" : undefined} onClick={() => setRoute({ page: n.page } as Route)}>
                {n.label}
              </button>
            ))}
          </div>
        ))}
        <div className="spacer" />
        <StatusBadge status={status} />
      </nav>
      <main className="main">
        <UpdateBanner />
        <RecordingAlert status={status} />
        <NewSessionBanner onOpen={(id) => setRoute({ page: "analysis", sessionId: id })} />
        {!isTauri() && <Alert tone="danger" title="Mode navigateur">Lance l'application avec « npm run tauri dev » pour accéder aux données.</Alert>}
<PageBoundary key={route.page} onHome={() => setRoute({ page: "dashboard" })}>
        {route.page === "dashboard" && <DashboardPage navigate={setRoute} status={status} />}
        {route.page === "sessions" && <SessionsPage navigate={setRoute} />}
        {route.page === "analysis" && <LapAnalysisPage route={route} navigate={setRoute} />}
        {route.page === "comparison" && <ComparisonPage navigate={setRoute} />}
        {route.page === "history" && <HistoryPage navigate={setRoute} />}
        {route.page === "live" && <LiveCoachPage status={status} />}
        {route.page === "strategy" && <StrategyPage />}
        {route.page === "setups" && <SetupsPage />}
        {route.page === "references" && <ReferencesPage navigate={setRoute} />}
        {route.page === "settings" && <SettingsPage />}
        </PageBoundary>
      </main>
    </div>
  );
}
