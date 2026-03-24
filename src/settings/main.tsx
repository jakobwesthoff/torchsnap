import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { SettingsPanel } from "./SettingsPanel";
import "../index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <SettingsPanel />
  </StrictMode>,
);
