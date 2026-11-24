import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/saira/wdth.css";
import "@fontsource-variable/big-shoulders-display";
import "@fontsource-variable/saira/wdth-italic.css";
import "uplot/dist/uPlot.min.css";
import "../styles.css";
import "./mobile.css";
import { MobileApp } from "./MobileApp";

if ("serviceWorker" in navigator && location.protocol === "https:") {
  void navigator.serviceWorker.register("./sw.js");
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <MobileApp />
  </StrictMode>,
);
