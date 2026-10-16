import { useState } from "react";
import { RecordingAlert } from "./components/RecordingAlert";
import { StatusBadge } from "./components/StatusBadge";
import { UpdateBanner } from "./components/UpdateBanner";
import { isTauri } from "./lib/api";
import { useStatus } from "./lib/hooks";
import { ComparisonPage } from "./pages/ComparisonPage";
import { DashboardPage } from "./pages/DashboardPage";
import { LapAnalysisPage } from "./pages/LapAnalysisPage";
import { LiveCoachPage } from "./pages/LiveCoachPage";
import { SessionsPage } from "./pages/SessionsPage";
import { SettingsPage } from "./pages/SettingsPage";

export type Route =
  | { page: "dashboard" }
  | { page: "sessions" }
  | { page: "analysis"; sessionId?: number; refLapId?: number; cmpLapId?: number }
  | { page: "comparison" }
  | { page: "live" }
  | { page: "settings" };

const NAV: { page: Route["page"]; label: string }[] = [
  { page: "dashboard", label: "Tableau de bord" },
  { page: "sessions", label: "Sessions" },
  { page: "analysis", label: "Analyse de tour" },
  { page: "comparison", label: "Comparaison" },
  { page: "live", label: "Live Coach" },
  { page: "settings", label: "Réglages" },
];

export function App() {
  const [route, setRoute] = useState<Route>({ page: "dashboard" });
  const status = useStatus();

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="brand">
          <svg width="22" height="22" viewBox="0 0 1024 1024" aria-hidden>
            <path d="M190 780 C 330 780, 400 700, 470 560 S 640 250, 834 244" fill="none" stroke="#a259ff" strokeWidth="110" strokeLinecap="round" />
          </svg>
          Apex Companion
        </div>
        {NAV.map((n) => (
          <button key={n.page} className={`nav-item ${route.page === n.page ? "active" : ""}`} onClick={() => setRoute({ page: n.page } as Route)}>
            {n.label}
          </button>
        ))}
        <div className="spacer" />
        <StatusBadge status={status} />
      </nav>
      <main className="main">
        <UpdateBanner />
        <RecordingAlert status={status} />
        {!isTauri() && <div className="error">Mode navigateur : lance l'application avec « npm run tauri dev » pour accéder aux données.</div>}
        {route.page === "dashboard" && <DashboardPage navigate={setRoute} />}
        {route.page === "sessions" && <SessionsPage navigate={setRoute} />}
        {route.page === "analysis" && <LapAnalysisPage route={route} navigate={setRoute} />}
        {route.page === "comparison" && <ComparisonPage navigate={setRoute} />}
        {route.page === "live" && <LiveCoachPage status={status} />}
        {route.page === "settings" && <SettingsPage />}
      </main>
    </div>
  );
}
