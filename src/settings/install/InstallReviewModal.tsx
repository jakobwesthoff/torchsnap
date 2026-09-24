// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Install review
//
// Shown for every install request before anything is installed,
// whether the file came from the picker, a drop, the command line
// or a double-click in Finder. One request at a time, in arrival
// order; the caller passes the first one.
// =========================================================

import { useEffect, useId, useRef, type KeyboardEvent } from "react";
import { cn } from "../../lib/cn";
import { PermissionGroups } from "./PermissionGroups";
import { currentPlatform } from "./platform";
import type { InstallRequestView, InstallReview, Provenance } from "./types";

export function InstallReviewModal({
  request,
  onInstall,
  onCancel,
}: {
  request: InstallRequestView;
  onInstall: (requestId: string) => void;
  onCancel: (requestId: string) => void;
}) {
  const titleId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const cancel = () => onCancel(request.id);

  // Move focus into the dialog whenever a new request is shown, so
  // the keyboard lands on the primary action.
  useEffect(() => {
    focusables(dialogRef.current)[0]?.focus();
  }, [request.id, request.state.kind]);

  // Escape cancels; Tab and Shift+Tab cycle through the dialog's own
  // buttons instead of escaping to the settings behind it.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      cancel();
      return;
    }
    if (event.key !== "Tab") {
      return;
    }
    const elements = focusables(dialogRef.current);
    if (elements.length === 0) {
      return;
    }
    const index = elements.indexOf(document.activeElement as HTMLElement);
    const next = event.shiftKey
      ? (index - 1 + elements.length) % elements.length
      : (index + 1) % elements.length;
    event.preventDefault();
    elements[next].focus();
  };

  const fileName = request.sourcePath.split("/").pop() ?? request.sourcePath;

  let title: string;
  let body: React.ReactNode;
  let actions: React.ReactNode;
  switch (request.state.kind) {
    case "staging":
      title = "Reading gadget";
      body = <p className="text-sm text-text-secondary">Reading {fileName}…</p>;
      actions = <DialogButton onClick={cancel}>Cancel</DialogButton>;
      break;
    case "failed":
      title = "This file can't be installed";
      body = (
        <>
          <p className="text-sm text-text-secondary">{fileName}</p>
          <p className="text-sm text-red-500">{request.state.message}</p>
        </>
      );
      actions = <DialogButton onClick={cancel}>Dismiss</DialogButton>;
      break;
    case "ready": {
      const review = request.state.review;
      const { gadget, action } = review;
      if (action.kind === "reject") {
        title = `${gadget.name} can't be installed`;
        body = (
          <>
            <GadgetIdentity review={review} />
            <p className="text-sm text-red-500">{action.reason}</p>
          </>
        );
        actions = <DialogButton onClick={cancel}>Dismiss</DialogButton>;
      } else {
        const isReplace = action.kind === "replace";
        title = isReplace ? `Replace ${gadget.name}?` : `Install ${gadget.name}?`;
        body = (
          <>
            <GadgetIdentity review={review} />
            {isReplace && <ReplaceNotice review={review} />}
            <PermissionGroups
              items={review.permissions}
              removed={review.removedPermissions}
              platform={currentPlatform()}
              showChanges={isReplace}
            />
            {review.permissions.length === 0 && (
              <p className="text-xs text-text-tertiary">This gadget asks for no permissions.</p>
            )}
          </>
        );
        actions = (
          <>
            <DialogButton primary onClick={() => onInstall(request.id)}>
              {isReplace ? "Replace" : "Install"}
            </DialogButton>
            <DialogButton onClick={cancel}>Cancel</DialogButton>
          </>
        );
      }
      break;
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-6">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        onKeyDown={onKeyDown}
        className="flex max-h-full w-full max-w-md flex-col gap-3 overflow-y-auto rounded-xl bg-surface-group p-4 shadow-xl"
      >
        <h2 id={titleId} className="text-base font-semibold text-text-primary">
          {title}
        </h2>
        {body}
        <div className="flex flex-row-reverse gap-2 pt-1">{actions}</div>
      </div>
    </div>
  );
}

function GadgetIdentity({ review }: { review: InstallReview }) {
  const { gadget, sourcePath, provenance } = review;
  const origin = describeProvenance(provenance);
  return (
    <div className="flex flex-col gap-0.5 text-sm">
      <span className="font-medium text-text-primary">
        {gadget.name} {gadget.version}
      </span>
      <span className="text-xs text-text-tertiary">{gadget.id}</span>
      {gadget.description && <span className="text-text-secondary">{gadget.description}</span>}
      <span className="break-all text-xs text-text-tertiary">{sourcePath}</span>
      {origin && <span className="text-xs text-text-secondary">{origin}</span>}
    </div>
  );
}

function ReplaceNotice({ review }: { review: InstallReview }) {
  const { action, gadget } = review;
  if (action.kind !== "replace") {
    return null;
  }
  const { previousVersion, relation } = action;
  if (relation === "downgrade") {
    return (
      <p role="alert" className="rounded-md bg-amber-500/15 px-2 py-1.5 text-sm text-amber-500">
        This is an older version than the installed {previousVersion}. Its data and settings are
        kept, but may have been changed by the newer version.
      </p>
    );
  }
  const text =
    relation === "upgrade"
      ? `Updates version ${previousVersion} to ${gadget.version}.`
      : relation === "same"
        ? `Reinstalls version ${gadget.version}.`
        : `Replaces version ${previousVersion} with ${gadget.version}.`;
  return <p className="text-sm text-text-secondary">{text} Its data and settings are kept.</p>;
}

function describeProvenance(provenance: Provenance | null): string | null {
  if (!provenance) {
    return null;
  }
  const url = provenance.downloadUrl ?? provenance.referrerUrl;
  let host: string | null = null;
  if (url) {
    try {
      host = new URL(url).host;
    } catch {
      host = null;
    }
  }
  if (host && provenance.downloadedBy) {
    return `Downloaded from ${host} with ${provenance.downloadedBy}`;
  }
  if (host) {
    return `Downloaded from ${host}`;
  }
  if (provenance.downloadedBy) {
    return `Downloaded with ${provenance.downloadedBy}`;
  }
  return null;
}

function DialogButton({
  primary = false,
  onClick,
  children,
}: {
  primary?: boolean;
  onClick: () => void;
  children: React.ReactNode;
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

function focusables(root: HTMLElement | null): HTMLElement[] {
  if (!root) {
    return [];
  }
  return Array.from(root.querySelectorAll<HTMLElement>("button:not([disabled])"));
}
