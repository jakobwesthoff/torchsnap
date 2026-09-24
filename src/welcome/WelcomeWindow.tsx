// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Welcome window
//
// Five steps, each prefilled, so continuing through all of them is a
// valid way through:
//
// 1. Welcome
// 2. How it works
// 3. The launcher shortcut (the recorder from Settings)
// 4. Launch at login and automatic update checks
// 5. "Press <shortcut> now", which finishes the welcome
//
// The window cannot be closed before step 5 is finished. Reaching
// step 5 hands the update answer to the backend, which stores it only
// when the welcome is finished, by the shortcut or the "Open the
// launcher" button.
// =========================================================

import { useEffect, useState, type ReactNode } from "react";
import { Mascot } from "../components/Mascot";
import { ShortcutKeys } from "../components/ShortcutKeys";
import { Switch } from "../components/Switch";
import { TitleBar } from "../components/TitleBar";
import { cn } from "../lib/cn";
import { ShortcutSection } from "../settings/ShortcutSection";

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

const STEP_COUNT = 5;

export function WelcomeWindow(props: WelcomeProps) {
  const [step, setStep] = useState(1);
  // Prefilled with on for someone who was never asked.
  const [automaticChecks, setAutomaticChecks] = useState(props.storedAutomaticChecks ?? true);
  const last = step === STEP_COUNT;

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
    setStep((s) => Math.max(1, s - 1));
  };
  const next = () => setStep((s) => Math.min(STEP_COUNT, s + 1));

  return (
    <div className="relative flex h-screen flex-col bg-surface font-sans text-text-primary antialiased">
      <TitleBar variant="titlebar" title="Welcome to Torchsnap" closable={false} />
      <main className="flex min-h-0 flex-1 flex-col px-10 pt-14 pb-6">
        <div className="min-h-0 flex-1 overflow-y-auto">
          {step === 1 && <WelcomeStep />}
          {step === 2 && <HowItWorksStep shortcut={props.globalShortcut} />}
          {step === 3 && (
            <ShortcutStep shortcut={props.globalShortcut} setShortcut={props.setGlobalShortcut} />
          )}
          {step === 4 && (
            <ChoicesStep
              launchAtLogin={props.launchAtLogin}
              setLaunchAtLogin={props.setLaunchAtLogin}
              automaticChecks={automaticChecks}
              setAutomaticChecks={setAutomaticChecks}
            />
          )}
          {step === 5 && <DoneStep shortcut={props.globalShortcut} finish={props.finish} />}
        </div>

        <div className="mt-6 flex items-center gap-2">
          <StepDots step={step} />
          <div className="flex-1" />
          {step > 1 && <Button onClick={back}>Back</Button>}
          {!last && (
            <Button primary onClick={next}>
              Continue
            </Button>
          )}
        </div>
      </main>
    </div>
  );
}

// =========================================================
// Steps
// =========================================================

function WelcomeStep() {
  return (
    <div className="flex flex-col items-center text-center">
      <Mascot size={192} />
      <h1 className="mt-4 text-xl font-semibold">Welcome to Torchsnap</h1>
      <p className="mt-2 max-w-md text-sm text-text-secondary">
        Torchsnap is a launcher that lives in your menu bar. It opens on a keyboard shortcut, you
        type what you are looking for, and it gets out of the way again. Four short steps set it up.
      </p>
    </div>
  );
}

// TODO: Replace the list with the "how it works" visual once its
// storyboard and medium are decided (updater plan, step 8).
function HowItWorksStep({ shortcut }: { shortcut: string }) {
  return (
    <div>
      <StepTitle>How it works</StepTitle>
      <ol className="mt-4 space-y-3 text-sm text-text-secondary">
        <HowItWorksItem n={1}>
          Press <ShortcutKeys shortcut={shortcut} /> in any app to open the launcher.
        </HowItWorksItem>
        <HowItWorksItem n={2}>Type a few letters of what you are looking for.</HowItWorksItem>
        <HowItWorksItem n={3}>
          Press Return to open the highlighted result, or Escape to close the launcher.
        </HowItWorksItem>
      </ol>
      <p className="mt-4 text-sm text-text-secondary">
        Torchsnap stays in the menu bar. Its menu there has Settings and Quit.
      </p>
    </div>
  );
}

function HowItWorksItem({ n, children }: { n: number; children: ReactNode }) {
  return (
    <li className="flex items-center gap-3">
      <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-surface-inset text-xs font-semibold text-text-primary">
        {n}
      </span>
      <span>{children}</span>
    </li>
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
    <div>
      <StepTitle>Your shortcut</StepTitle>
      <p className="mt-2 mb-4 text-sm text-text-secondary">
        Torchsnap opens with this shortcut. Click it to record a different one, for example when
        another app already uses it.
      </p>
      <ShortcutSection globalShortcut={shortcut} setGlobalShortcut={setShortcut} />
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
      <StepTitle>Two choices</StepTitle>
      <div className="mt-4 flex flex-col gap-4 rounded-xl bg-surface-group p-4">
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

function StepDots({ step }: { step: number }) {
  return (
    <div role="group" aria-label={`Step ${step} of ${STEP_COUNT}`} className="flex gap-1.5">
      {Array.from({ length: STEP_COUNT }, (_, i) => (
        <span
          key={i}
          className={cn(
            "h-1.5 w-1.5 rounded-full",
            i + 1 === step ? "bg-accent" : "bg-text-muted/30",
          )}
        />
      ))}
    </div>
  );
}

function Button({
  primary = false,
  onClick,
  children,
}: {
  primary?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "rounded-md px-3 py-1.5 text-sm font-medium transition-colors",
        primary
          ? "bg-accent text-white hover:bg-accent/90"
          : "text-text-secondary hover:bg-surface-hover hover:text-text-primary",
      )}
    >
      {children}
    </button>
  );
}
