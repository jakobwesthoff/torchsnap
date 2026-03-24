import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { Launcher } from "./Launcher";
import "../index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <Launcher />
    </ThemeProvider>
  </StrictMode>,
);
