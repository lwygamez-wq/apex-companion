import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/archivo/wdth.css";
import "@fontsource-variable/archivo/wdth-italic.css";
import "uplot/dist/uPlot.min.css";
import "./styles.css";
import { App } from "./App";

const root = document.getElementById("root");
if (!root) throw new Error("élément #root introuvable");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
