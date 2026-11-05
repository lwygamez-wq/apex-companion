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

export type Route =
  | { page: "dashboard" }
  | { page: "sessions" }
  | { page: "analysis"; sessionId?: number; refLapId?: number; cmpLapId?: number }
  | { page: "comparison" }
  | { page: "history" }
  | { page: "live" }
  | { page: "settings" };

/** Navigation rangée par moment : avant (Briefing), après (Séances, Progression), pendant (Au volant). */
const NAV: { page: Route["page"]; label: string; also?: Route["page"][]; icon: string }[] = [
  { page: "dashboard", label: "Briefing", icon: "M4 12l8-8 8 8M6 10v10h12V10" },
  { page: "sessions", label: "Séances", also: ["analysis", "comparison"], icon: "M4 6h16M4 12h16M4 18h10" },
  { page: "history", label: "Progression", icon: "M4 18l6-6 4 4 6-8" },
  { page: "live", label: "Au volant", icon: "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18M12 3v6M3.5 14h6M14.5 14h6" },
  { page: "settings", label: "Réglages", icon: "M4 7h10M18 7h2M4 17h4M12 17h8M14 4v6M8 14v6" },
];

export function App() {
  const [route, setRoute] = useState<Route>({ page: "dashboard" });
  const status = useStatus();
  useCoachVoice();

  return (
    <div className="app">
      <nav className="sidebar">
        <div className="brand">
          <svg width="22" height="22" viewBox="0 0 1024 1024" aria-hidden>
            <path d="M190 780 C 330 780, 400 700, 470 560 S 640 250, 834 244" fill="none" stroke="#e10600" strokeWidth="120" strokeLinecap="round" />
          </svg>
          Apex Companion
        </div>
        {NAV.map((n) => (
          <button key={n.page} className={`nav-item ${route.page === n.page || n.also?.includes(route.page) ? "active" : ""}`} onClick={() => setRoute({ page: n.page } as Route)}>
            <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d={n.icon} /></svg>
            {n.label}
          </button>
        ))}
        <div className="spacer" />
        <StatusBadge status={status} />
      </nav>
      <main className="main">
        <UpdateBanner />
        <RecordingAlert status={status} />
        <NewSessionBanner onOpen={(id) => setRoute({ page: "analysis", sessionId: id })} />
        {!isTauri() && <div className="error">Mode navigateur : lance l'application avec « npm run tauri dev » pour accéder aux données.</div>}
<PageBoundary key={route.page} onHome={() => setRoute({ page: "dashboard" })}>
        {route.page === "dashboard" && <DashboardPage navigate={setRoute} status={status} />}
        {route.page === "sessions" && <SessionsPage navigate={setRoute} />}
        {route.page === "analysis" && <LapAnalysisPage route={route} navigate={setRoute} />}
        {route.page === "comparison" && <ComparisonPage navigate={setRoute} />}
        {route.page === "history" && <HistoryPage navigate={setRoute} />}
        {route.page === "live" && <LiveCoachPage status={status} />}
        {route.page === "settings" && <SettingsPage />}
        </PageBoundary>
      </main>
    </div>
  );
}
