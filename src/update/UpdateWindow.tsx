// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Update window
//
// One view per update phase. The backend drives the phase
// (`useUpdatePhase`); the buttons call back into it through the
// `actions` passed in, so the view itself has no side effects.
//
// Closing the window is how "Later" and "Close" work: the backend
// notices the window is gone and keeps an offered update for the
// tray, or returns to idle after "up to date" and errors.
// =========================================================

import type { ReactNode } from "react";
import { TitleBar } from "../components/TitleBar";
import { cn } from "../lib/cn";
import { formatMegabytes } from "./format";
import { ReleaseNotesView } from "./ReleaseNotesView";
import type { AvailableUpdate, LocationProblem, UpdatePhase } from "./types";

export interface UpdateActions {
  check: () => void;
  install: () => void;
  skip: () => void;
  close: () => void;
  openDownload: (url: string) => void;
}

export function UpdateWindow({
  phase,
  actions,
}: {
  phase: UpdatePhase | null;
  actions: UpdateActions;
}) {
  return (
    <div className="relative flex h-screen flex-col bg-surface font-sans text-text-primary antialiased">
      <TitleBar variant="titlebar" title="Software Update" />
      <main className="flex min-h-0 flex-1 flex-col px-6 pt-12 pb-5">
        <PhaseView phase={phase} actions={actions} />
      </main>
    </div>
  );
}

function PhaseView({ phase, actions }: { phase: UpdatePhase | null; actions: UpdateActions }) {
  if (phase === null) {
    return null;
  }
  switch (phase.phase) {
    case "idle":
      return (
        <Message title="Check for Updates" body="Torchsnap has not looked for a new version yet.">
          <Button onClick={actions.close}>Close</Button>
          <Button primary onClick={actions.check}>
            Check Now
          </Button>
        </Message>
      );
    case "checking":
      return (
        <Message title="Checking for updates…" body="Asking torchsnap.app for the newest version.">
          <Button onClick={actions.close}>Close</Button>
        </Message>
      );
    case "upToDate":
      return (
        <Message
          title="You're up to date"
          body={`Torchsnap ${phase.installed} is the newest version.`}
        >
          <Button primary onClick={actions.close}>
            OK
          </Button>
        </Message>
      );
    case "failed":
      return (
        <Message title="Updating failed" body={phase.message}>
          <Button onClick={actions.close}>Close</Button>
          <Button primary onClick={actions.check}>
            Try Again
          </Button>
        </Message>
      );
    case "available":
      return <AvailableView update={phase} actions={actions} />;
    case "downloading":
      return (
        <DownloadingView
          version={phase.version}
          downloaded={phase.downloaded}
          total={phase.total}
        />
      );
    case "installing":
      return (
        <Message
          title={`Installing Torchsnap ${phase.version}…`}
          body="Torchsnap restarts as soon as the update is in place."
        />
      );
  }
}

// =========================================================
// Available
// =========================================================

const LOCATION_HINTS: Record<LocationProblem, string> = {
  diskImage:
    "Torchsnap is running straight from the disk image and cannot update itself there. Move it to the Applications folder, or download the new version.",
  translocated:
    "macOS runs this copy of Torchsnap from a temporary location, so it cannot update itself. Move Torchsnap to the Applications folder and open it from there, or download the new version.",
};

function AvailableView({ update, actions }: { update: AvailableUpdate; actions: UpdateActions }) {
  const pending = update.pendingGadgetChanges;
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <h2 className="text-base font-semibold">A new version of Torchsnap is available</h2>
      <p className="mt-1 text-[13px] text-text-secondary">
        Torchsnap {update.version} is available. You have {update.installed}.
      </p>

      <div
        role="region"
        aria-label="Release notes"
        className="mt-4 min-h-0 flex-1 overflow-y-auto rounded-lg border border-border bg-surface-inset px-4 py-3"
      >
        <ReleaseNotesView releases={update.releases} />
      </div>

      {update.locationProblem ? (
        <p className="mt-3 text-[13px] text-text-secondary">
          {LOCATION_HINTS[update.locationProblem]}
        </p>
      ) : (
        pending > 0 && (
          <p className="mt-3 text-[13px] text-text-secondary">
            Restarting also applies{" "}
            {pending === 1 ? "1 pending gadget change" : `${pending} pending gadget changes`}.
          </p>
        )
      )}

      <div className="mt-4 flex items-center gap-2">
        <Button onClick={actions.skip}>Skip This Version</Button>
        <div className="flex-1" />
        <Button onClick={actions.close}>Later</Button>
        {update.locationProblem ? (
          <Button primary onClick={() => actions.openDownload(update.downloadUrl)}>
            Download
          </Button>
        ) : (
          <Button primary onClick={actions.install}>
            Install and Restart
          </Button>
        )}
      </div>
    </div>
  );
}

// =========================================================
// Downloading
// =========================================================

function DownloadingView({
  version,
  downloaded,
  total,
}: {
  version: string;
  downloaded: number;
  total: number | null;
}) {
  const percent = total ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
  return (
    <Message
      title={`Downloading Torchsnap ${version}…`}
      body={percent === null ? formatMegabytes(downloaded) : `${percent} %`}
    >
      <div
        role="progressbar"
        aria-label="Download progress"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
        className="h-1.5 w-full overflow-hidden rounded-full bg-surface-inset"
      >
        <div
          className={cn("h-full bg-accent", percent === null && "w-1/3 animate-pulse")}
          style={percent === null ? undefined : { width: `${percent}%` }}
        />
      </div>
    </Message>
  );
}

// =========================================================
// Building blocks
// =========================================================

function Message({ title, body, children }: { title: string; body: string; children?: ReactNode }) {
  return (
    <div className="flex flex-1 flex-col">
      <h2 className="text-base font-semibold">{title}</h2>
      <p className="mt-2 text-[13px] text-text-secondary select-text">{body}</p>
      <div className="flex-1" />
      {children && <div className="mt-4 flex items-center justify-end gap-2">{children}</div>}
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
