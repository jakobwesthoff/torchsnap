import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { SettingsPanel } from "./SettingsPanel";
import "../index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <SettingsPanel />
    </ThemeProvider>
  </StrictMode>,
);
