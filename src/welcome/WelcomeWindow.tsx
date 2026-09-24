// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Welcome window
//
// Four pages, each prefilled, so going forward through all of them is
// a valid way through:
//
// 1. Welcome, with a still picture of the launcher
// 2. The launcher shortcut (the recorder from Settings)
// 3. Launch at login and automatic update checks
// 4. "Press <shortcut> now", which finishes the welcome
//
// Each page's forward button sits centered below its content and names
// the page it leads to; a back arrow sits top left from page 2 on. Every
// page fades in when it appears.
//
// The window cannot be closed before page 4 is finished. Reaching page 4
// hands the update answer to the backend, which stores it only when the
// welcome is finished, by the shortcut or the "Open the launcher" link.
// =========================================================

import { useEffect, useState, type ReactNode } from "react";
import { Icon } from "../components/Icon";
import { Mascot } from "../components/Mascot";
import { ShortcutKeys } from "../components/ShortcutKeys";
import { Switch } from "../components/Switch";
import { TitleBar } from "../components/TitleBar";
import { cn } from "../lib/cn";
import { ShortcutSection } from "../settings/ShortcutSection";
import { LauncherPreview } from "./LauncherPreview";

export interface WelcomeProps {
  globalShortcut: string;
  setGlobalShortcut: (shortcut: string) => Promise<void>;
  /** `null` while the current state is being read. */
  launchAtLogin: boolean | null;
  setLaunchAtLogin: (enabled: boolean) => void;
  /** The stored answer, or `null` when the user was never asked. */
  storedAutomaticChecks: boolean | null;
  readyToFinish: (automaticChecks: boolean) => void;
  notReady: () => void;
  finish: () => void;
}

const PAGE_COUNT = 4;

/** Forward button label per page; the last page has none. */
const FORWARD_LABELS: Record<number, string> = {
  1: "Choose your shortcut",
  2: "Next: Startup and updates",
  3: "Next: Try it",
};

export function WelcomeWindow(props: WelcomeProps) {
  const [page, setPage] = useState(1);
  // Prefilled with on for someone who was never asked.
  const [automaticChecks, setAutomaticChecks] = useState(props.storedAutomaticChecks ?? true);
  const last = page === PAGE_COUNT;

  const { readyToFinish, notReady } = props;
  useEffect(() => {
    if (last) {
      readyToFinish(automaticChecks);
    }
  }, [last, automaticChecks, readyToFinish]);

  const back = () => {
    if (last) {
      notReady();
    }
    setPage((p) => Math.max(1, p - 1));
  };
  const forward = () => setPage((p) => Math.min(PAGE_COUNT, p + 1));
  const forwardLabel = FORWARD_LABELS[page];

  return (
    <div className="relative flex h-screen flex-col bg-surface font-sans text-text-primary antialiased">
      <TitleBar variant="titlebar" title="Welcome to Torchsnap" closable={false} />
      <main className="relative flex min-h-0 flex-1 flex-col px-10 pt-12 pb-5">
        {page > 1 && (
          <button
            type="button"
            aria-label="Back"
            onClick={back}
            className="absolute top-11 left-4 rounded-md p-1.5 text-text-secondary transition-colors hover:bg-surface-hover hover:text-text-primary"
          >
            <Icon icon="heroicons:chevron-left" className="h-5 w-5" />
          </button>
        )}

        {/* Keyed by page, so every page mounts fresh and fades in. */}
        <div
          key={page}
          data-testid="welcome-page"
          className="welcome-fade-in flex min-h-0 flex-1 flex-col items-center justify-center overflow-y-auto text-center"
        >
          {page === 1 && <WelcomePage />}
          {page === 2 && (
            <ShortcutStep shortcut={props.globalShortcut} setShortcut={props.setGlobalShortcut} />
          )}
          {page === 3 && (
            <ChoicesStep
              launchAtLogin={props.launchAtLogin}
              setLaunchAtLogin={props.setLaunchAtLogin}
              automaticChecks={automaticChecks}
              setAutomaticChecks={setAutomaticChecks}
            />
          )}
          {page === 4 && <DoneStep shortcut={props.globalShortcut} finish={props.finish} />}

          {forwardLabel && (
            <button
              type="button"
              onClick={forward}
              className="mt-6 rounded-md bg-accent px-4 py-1.5 text-sm font-medium text-white transition-colors hover:bg-accent/90"
            >
              {forwardLabel}
            </button>
          )}
        </div>

        <PageDots page={page} />
      </main>
    </div>
  );
}

// =========================================================
// Pages
// =========================================================

function WelcomePage() {
  return (
    <>
      <LauncherPreview />
      <h1 className="mt-8 text-xl font-semibold">Welcome to Torchsnap</h1>
      <p className="mt-2 max-w-md text-sm text-text-secondary">
        Torchsnap is a launcher that lives in your menu bar. It opens on a keyboard shortcut, you
        type what you are looking for, and it gets out of the way again. Three short steps set it
        up.
      </p>
    </>
  );
}

function ShortcutStep({
  shortcut,
  setShortcut,
}: {
  shortcut: string;
  setShortcut: (shortcut: string) => Promise<void>;
}) {
  return (
    <div className="w-full max-w-lg">
      <StepTitle>Your shortcut</StepTitle>
      <p className="mt-2 mb-4 text-sm text-text-secondary">
        Torchsnap opens with this shortcut. Click it to record a different one, for example when
        another app already uses it.
      </p>
      <div className="text-left">
        <ShortcutSection globalShortcut={shortcut} setGlobalShortcut={setShortcut} />
      </div>
    </div>
  );
}

function ChoicesStep({
  launchAtLogin,
  setLaunchAtLogin,
  automaticChecks,
  setAutomaticChecks,
}: {
  launchAtLogin: boolean | null;
  setLaunchAtLogin: (enabled: boolean) => void;
  automaticChecks: boolean;
  setAutomaticChecks: (enabled: boolean) => void;
}) {
  return (
    <div>
      <StepTitle>Startup and updates</StepTitle>
      <div className="mt-4 flex flex-col gap-4 rounded-xl bg-surface-group p-4 text-left">
        <Choice
          label="Launch at login"
          description="Start Torchsnap when you log in, so the shortcut always works."
        >
          <Switch
            aria-label="Launch at login"
            checked={launchAtLogin === true}
            onChange={setLaunchAtLogin}
            disabled={launchAtLogin === null}
          />
        </Choice>
        <Choice
          label="Check for updates automatically"
          description="Once a day, Torchsnap asks torchsnap.app for the newest version. The request carries your IP address and nothing else about you. You can check by hand from the menu bar at any time."
        >
          <Switch
            aria-label="Check for updates automatically"
            checked={automaticChecks}
            onChange={setAutomaticChecks}
          />
        </Choice>
      </div>
      <p className="mt-3 text-xs text-text-tertiary">Both can be changed later in Settings.</p>
    </div>
  );
}

function Choice({
  label,
  description,
  children,
}: {
  label: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-6">
      <div className="flex flex-col">
        <span className="text-sm text-text-primary">{label}</span>
        <span className="mt-0.5 text-xs text-text-tertiary">{description}</span>
      </div>
      {children}
    </div>
  );
}

function DoneStep({ shortcut, finish }: { shortcut: string; finish: () => void }) {
  return (
    <div className="flex flex-col items-center text-center">
      <Mascot size={96} />
      <StepTitle>You're set</StepTitle>
      <p className="mt-3 text-sm text-text-secondary">Press</p>
      <div className="mt-2 scale-150">
        <ShortcutKeys shortcut={shortcut} />
      </div>
      <p className="mt-4 text-sm text-text-secondary">now to open the launcher.</p>
      <button
        type="button"
        onClick={finish}
        className="mt-6 text-xs text-text-tertiary underline underline-offset-2 hover:text-text-secondary"
      >
        Open the launcher
      </button>
    </div>
  );
}

// =========================================================
// Building blocks
// =========================================================

function StepTitle({ children }: { children: ReactNode }) {
  return <h2 className="mt-2 text-lg font-semibold">{children}</h2>;
}

function PageDots({ page }: { page: number }) {
  return (
    <div
      role="group"
      aria-label={`Page ${page} of ${PAGE_COUNT}`}
      className="mt-4 flex justify-center gap-1.5"
    >
      {Array.from({ length: PAGE_COUNT }, (_, i) => (
        <span
          key={i}
          className={cn(
            "h-1.5 w-1.5 rounded-full",
            i + 1 === page ? "bg-accent" : "bg-text-muted/30",
          )}
        />
      ))}
    </div>
  );
}
