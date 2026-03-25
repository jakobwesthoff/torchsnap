import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { ThemeProvider } from "../contexts/ThemeProvider";
import { KeyBindingProvider } from "../keybindings";
import { Launcher } from "./Launcher";
import "../index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ThemeProvider>
      <KeyBindingProvider>
        <Launcher />
      </KeyBindingProvider>
    </ThemeProvider>
  </StrictMode>,
);
