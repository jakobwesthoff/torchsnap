// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadgets Management Panel
//
// Top-level settings section that lists every user-visible
// gadget, shows its source badge, lets users enable/disable
// it, and — for user-installed gadgets only — uninstall it.
// Also hosts the Install flow: a file-picker button plus a
// drop zone accepting `.torchsnap` archives. Both hand the files
// to the backend install queue, and every queued request is
// reviewed in `InstallReviewModal` before anything is installed.
//
// Both install and uninstall require an app restart to take
// effect because `GadgetHost::register` freezes the gadget
// set after setup. The banner at the top of the panel
// surfaces that requirement with a single-click restart
// button;
// `todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`
// tracks the follow-up work that will remove the restart.
// =========================================================

import { useCallback, useEffect, useMemo, useState } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { command, type GadgetSourceKind } from "../../lib/command";
import { getGadgetsWithSettings } from "../../gadgets/registry";
import { useSetting } from "../../hooks/useSetting";
import { Icon } from "../../components/Icon";
import { Switch } from "../../components/Switch";
import { SectionHeader } from "../SectionHeader";
import { Section } from "../Section";
import { cn } from "../../lib/cn";
import { InstallReviewModal } from "../install/InstallReviewModal";
import { BroadAccessTag, PermissionGroups } from "../install/PermissionGroups";
import { groupPermissions } from "../install/permissionModel";
import { currentPlatform } from "../install/platform";
import { useInstallQueue } from "../install/useInstallQueue";
import { usePendingChanges } from "../install/usePendingChanges";
import type { PendingGadget, PermissionItem } from "../install/types";

// =========================================================
// Types
// =========================================================

interface PluginRow {
  id: string;
  label: string;
  description?: string;
  icon?: string;
  sourceKind: GadgetSourceKind;
  permissions: PermissionItem[];
  /** Manifest version of a loaded WASM gadget; built-ins have none. */
  version?: string;
  /** False for a gadget that exists only as a pending install. */
  loaded: boolean;
  /** A change to this gadget that waits for a restart. */
  pending?: PendingGadget;
}

// =========================================================
// Component
// =========================================================

export function GadgetsManagementPanel() {
  const gadgetMetadata = useMemo(() => getGadgetsWithSettings(), []);
  const [sourceKinds, setSourceKinds] = useState<Record<string, GadgetSourceKind>>({});
  const [loadingSourceKinds, setLoadingSourceKinds] = useState(true);
  const [permissions, setPermissions] = useState<Record<string, PermissionItem[]>>({});
  const [versions, setVersions] = useState<Record<string, string>>({});
  const pending = usePendingChanges();
  const [banner, setBanner] = useState<Banner | null>(null);
  const [dragActive, setDragActive] = useState(false);
  const installQueue = useInstallQueue();
  // Requests are reviewed one at a time, in the order they arrived.
  const nextRequest = installQueue.requests[0];

  // Fetch the authoritative id→kind map from the backend.
  // Gadgets registered in the frontend registry but absent
  // from the backend snapshot (a shouldn't-happen edge case)
  // fall out of the list rather than being shown with a
  // guessed kind.
  useEffect(() => {
    let cancelled = false;
    command("gadget_sources")
      .then((kinds) => {
        if (!cancelled) {
          setSourceKinds(kinds);
          setLoadingSourceKinds(false);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setLoadingSourceKinds(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    command("wasm_gadgets")
      .then((manifests) => {
        if (!cancelled) {
          setVersions(Object.fromEntries(manifests.map((m) => [m.gadget.id, m.gadget.version])));
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // Permissions only exist for WASM gadgets; native ones are compiled
  // into the app and have no manifest, so their cards show none.
  useEffect(() => {
    let cancelled = false;
    command("gadget_permissions")
      .then((byGadget) => {
        if (!cancelled) {
          setPermissions(byGadget);
        }
      })
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, []);

  // Loaded gadgets first, then installs of this session that only
  // load after the restart.
  const rows: PluginRow[] = useMemo(() => {
    const loadedRows: PluginRow[] = gadgetMetadata
      .filter((gadget) => sourceKinds[gadget.id] != null)
      .map((gadget) => ({
        id: gadget.id,
        label: gadget.label,
        description: gadget.description,
        icon: gadget.icon,
        sourceKind: sourceKinds[gadget.id],
        permissions: permissions[gadget.id] ?? [],
        version: versions[gadget.id],
        loaded: true,
        pending: pending.changes[gadget.id],
      }));
    const loadedIds = new Set(loadedRows.map((row) => row.id));
    const pendingRows: PluginRow[] = Object.entries(pending.changes)
      .filter(([id]) => !loadedIds.has(id))
      .map(([id, change]) => ({
        id,
        label: change.name,
        description: change.description,
        icon: "heroicons:puzzle-piece",
        sourceKind: "user",
        permissions: [],
        loaded: false,
        pending: change,
      }));
    return [...loadedRows, ...pendingRows];
  }, [gadgetMetadata, sourceKinds, permissions, versions, pending.changes]);
  const pendingCount = Object.keys(pending.changes).length;

  const submitToQueue = useCallback(
    async (paths: string[], origin: "settingsPicker" | "settingsDrop") => {
      try {
        await command("install_queue_submit", { paths, origin });
      } catch (e) {
        setBanner({ kind: "error", message: formatError(e, "Failed to open the gadget file") });
      }
    },
    [],
  );

  const handleInstallClick = useCallback(async () => {
    const selected = await openFileDialog({
      multiple: false,
      filters: [{ name: "Torchsnap Gadget", extensions: ["torchsnap"] }],
    });
    if (typeof selected !== "string") {
      return;
    }
    await submitToQueue([selected], "settingsPicker");
  }, [submitToQueue]);

  // =========================================================
  // Install: drag-and-drop path
  //
  // Tauri exposes drag events through `getCurrentWebview`.
  // Dropped files arrive as absolute paths. Registration
  // resolves asynchronously; if the panel unmounts first, the
  // late unlistener is called on arrival instead of leaking a
  // listener that would submit every later drop twice.
  // =========================================================

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDragActive(true);
        } else if (event.payload.type === "leave") {
          setDragActive(false);
        } else if (event.payload.type === "drop") {
          setDragActive(false);
          const paths = event.payload.paths.filter((p) => p.endsWith(".torchsnap"));
          // Reject mixed drops with any non-torchsnap files so
          // the user sees a clear error instead of silent
          // partial success.
          if (paths.length !== event.payload.paths.length) {
            setBanner({
              kind: "error",
              message: "Only .torchsnap files can be installed as gadgets.",
            });
            return;
          }
          void submitToQueue(paths, "settingsDrop");
        }
      })
      .then((u) => {
        if (cancelled) {
          u();
        } else {
          unlisten = u;
        }
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [submitToQueue]);

  const handleUninstall = useCallback(
    async (gadgetId: string) => {
      try {
        await command("uninstall_user_gadget", { gadgetId });
      } catch (e) {
        setBanner({
          kind: "error",
          message: formatError(e, "Failed to uninstall gadget"),
        });
      }
      await pending.refresh();
    },
    [pending],
  );

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        icon="heroicons:puzzle-piece"
        title="Gadgets"
        description="Enable, disable, install, and uninstall gadgets. Built-in and bundled gadgets ship with the app and cannot be removed."
      />

      {pendingCount > 0 && <RestartBar count={pendingCount} />}

      {banner && <BannerView banner={banner} onDismiss={() => setBanner(null)} />}
      {pending.error && (
        <BannerView
          banner={{ kind: "error", message: pending.error }}
          onDismiss={pending.dismissError}
        />
      )}
      {installQueue.error && (
        <BannerView
          banner={{ kind: "error", message: installQueue.error }}
          onDismiss={installQueue.dismissError}
        />
      )}

      <Section title="Install">
        <InstallArea onInstallClick={handleInstallClick} dragActive={dragActive} />
      </Section>

      <Section title="Installed Gadgets">
        {loadingSourceKinds ? (
          <div className="text-sm text-text-tertiary">Loading gadgets…</div>
        ) : rows.length === 0 ? (
          <div className="text-sm text-text-tertiary">No gadgets registered.</div>
        ) : (
          <div className="flex flex-col divide-y divide-border-divider">
            {rows.map((row) => (
              <PluginRowView
                key={row.id}
                row={row}
                onUninstall={handleUninstall}
                onUndo={(gadgetId) => void pending.undo(gadgetId)}
              />
            ))}
          </div>
        )}
      </Section>

      {nextRequest && (
        <InstallReviewModal
          request={nextRequest}
          onInstall={(requestId) => void installQueue.confirm(requestId).then(pending.refresh)}
          onCancel={(requestId) => void installQueue.dismiss(requestId)}
        />
      )}
    </div>
  );
}

// =========================================================
// Row — one gadget with badge, enable toggle, uninstall.
// =========================================================

function PluginRowView({
  row,
  onUninstall,
  onUndo,
}: {
  row: PluginRow;
  onUninstall: (gadgetId: string) => void;
  onUndo: (gadgetId: string) => void;
}) {
  const [enabled, setEnabled] = useSetting<boolean>(`enabled.${row.id}`);

  const canUninstall = row.sourceKind === "user";
  const change = row.pending;
  // A gadget that is not running yet, or will not run after the
  // restart, is dimmed; a replaced one keeps running until then.
  const dimmed = change !== undefined && change.kind !== "replaced";

  return (
    <div data-gadget={row.id} className="flex items-start gap-3 py-2 first:pt-0 last:pb-0">
      <div className={cn("flex min-w-0 flex-1 items-start gap-3", dimmed && "opacity-60")}>
        {row.icon && (
          <Icon
            icon={row.icon}
            className="h-8 w-8 shrink-0 rounded-md bg-surface-hover p-1.5 text-text-primary"
          />
        )}
        <div className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-sm font-medium text-text-primary">
            {row.label}
            {versionText(row) && (
              <span className="ml-1.5 text-xs font-normal text-text-tertiary">
                {versionText(row)}
              </span>
            )}
          </span>
          {row.description && (
            <span className="truncate text-xs text-text-tertiary">{row.description}</span>
          )}
          {change && <span className="text-xs text-accent">{pendingText(change)}</span>}
          {row.loaded && <PermissionLine sourceKind={row.sourceKind} items={row.permissions} />}
        </div>
      </div>
      <Switch
        checked={enabled ?? true}
        onChange={setEnabled}
        disabled={!row.loaded || change?.kind === "uninstalled"}
      />
      {/* Trailing slot: Undo for a pending change, Uninstall for user
          gadgets, the source badge for every other kind. Its fixed
          width keeps every switch in the same column. */}
      <div data-slot="trailing" className="flex w-32 shrink-0 justify-end">
        {change ? (
          <button
            type="button"
            onClick={() => onUndo(row.id)}
            title="Undo this change before it takes effect"
            className="shrink-0 rounded-md border border-accent/40 px-2 py-0.5 text-xs font-medium text-accent transition-colors hover:bg-accent/15"
          >
            Undo
          </button>
        ) : canUninstall ? (
          <button
            type="button"
            onClick={() => onUninstall(row.id)}
            title="Uninstall this user gadget"
            className="rounded-md px-2 py-1 text-xs text-text-muted transition-colors hover:bg-surface-hover hover:text-text-primary"
          >
            Uninstall
          </button>
        ) : (
          <SourceBadge kind={row.sourceKind} />
        )}
      </div>
    </div>
  );
}

/** The version shown after the name, or the change of version. */
function versionText(row: PluginRow): string | undefined {
  const change = row.pending;
  if (!change) {
    return row.version;
  }
  if (change.kind === "uninstalled") {
    return change.previousVersion ?? undefined;
  }
  if (change.previousVersion && change.version) {
    return `${change.previousVersion} → ${change.version}`;
  }
  return change.version ?? undefined;
}

function pendingText(change: PendingGadget): string {
  switch (change.kind) {
    case "installed":
      return "Installs on restart";
    case "replaced":
      return `Updates to ${change.version} on restart`;
    case "reinstalled":
      return `Reinstalls ${change.version} on restart`;
    case "uninstalled":
      return "Removed on restart";
  }
}

// =========================================================
// Restart bar — present while any change waits for a restart.
// It is derived from the backend's record, so it is back after
// Settings is closed and reopened. The backend restarts the app
// and opens Settings on this section again afterwards.
// =========================================================

function RestartBar({ count }: { count: number }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border border-accent/50 bg-accent/10 px-3 py-2 text-sm text-accent">
      <span>{count === 1 ? "1 change applies" : `${count} changes apply`} after a restart</span>
      <button
        type="button"
        onClick={() => void command("restart_to_apply_gadget_changes")}
        className="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-accent/90"
      >
        Restart now
      </button>
    </div>
  );
}

// =========================================================
// Permission line — one quiet line per card that expands into
// the full list. Built-in gadgets are native code without a
// manifest, so there is nothing to list for them.
// =========================================================

function PermissionLine({
  sourceKind,
  items,
}: {
  sourceKind: GadgetSourceKind;
  items: PermissionItem[];
}) {
  const [open, setOpen] = useState(false);
  const quiet = "mt-1 self-start text-left text-xs text-text-tertiary";

  if (sourceKind === "builtin") {
    return (
      <button type="button" disabled className={quiet}>
        Permissions aren't listed for built-in gadgets
      </button>
    );
  }
  if (items.length === 0) {
    return (
      <button type="button" disabled className={quiet}>
        No permissions
      </button>
    );
  }

  const platform = currentPlatform();
  const groups = groupPermissions(items, [], platform);
  const titles = groups.map((group) => group.title);
  const broad = groups.some((group) => group.broad);
  return (
    <>
      <button
        type="button"
        aria-expanded={open}
        aria-label={`Permissions: ${titles.join(", ")}${broad ? ", with broad access" : ""}`}
        onClick={() => setOpen((value) => !value)}
        className={cn(quiet, "flex items-center gap-1 hover:text-text-secondary")}
      >
        <Icon
          icon="heroicons:chevron-right"
          className={cn("h-3 w-3 transition-transform", open && "rotate-90")}
        />
        <span>{titles.join(" · ")}</span>
        {broad && <BroadAccessTag />}
      </button>
      {open && <PermissionGroups items={items} platform={platform} className="mt-1.5 pl-4" />}
    </>
  );
}

// =========================================================
// Source badge — visual + tooltip carries the rationale for
// why a given gadget is (or is not) uninstallable.
// =========================================================

function SourceBadge({ kind }: { kind: GadgetSourceKind }) {
  const { label, tooltip, className } = badgeMetadata(kind);
  return (
    <span
      title={tooltip}
      className={cn(
        "inline-flex items-center rounded px-1.5 py-0.5 text-[10px] font-medium tracking-wide uppercase",
        className,
      )}
    >
      {label}
    </span>
  );
}

function badgeMetadata(kind: GadgetSourceKind): {
  label: string;
  tooltip: string;
  className: string;
} {
  switch (kind) {
    case "builtin":
      return {
        label: "Built-in",
        tooltip: "Native gadget compiled into the app. Always present.",
        className: "bg-surface-hover text-text-secondary",
      };
    case "system":
      return {
        label: "Bundled",
        tooltip: "Bundled with the app. Updated with Torchsnap; cannot be uninstalled.",
        className: "bg-surface-hover text-text-secondary",
      };
    case "user":
      return {
        label: "User",
        tooltip: "WASM gadget you installed. Uninstall available.",
        className: "bg-accent/15 text-accent",
      };
    case "dev":
      return {
        label: "Dev",
        tooltip:
          "WASM gadget loaded from the repository in debug builds. Release builds never include it.",
        className: "bg-amber-500/15 text-amber-500",
      };
  }
}

// =========================================================
// Install drop zone + file picker button.
// =========================================================

function InstallArea({
  onInstallClick,
  dragActive,
}: {
  onInstallClick: () => void;
  dragActive: boolean;
}) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-2 rounded-lg border border-dashed px-6 py-6 text-center transition-colors",
        dragActive
          ? "border-accent bg-accent/5 text-accent"
          : "border-border-divider text-text-tertiary",
      )}
    >
      <Icon icon="heroicons:arrow-up-tray" className="h-6 w-6" />
      <div className="text-sm">
        {dragActive ? "Drop to install" : "Drop a .torchsnap file here, or"}
      </div>
      <button
        type="button"
        onClick={onInstallClick}
        className="rounded-md bg-accent px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-accent/90"
      >
        Choose a file…
      </button>
    </div>
  );
}

// =========================================================
// Banner — errors from picking, dropping or uninstalling.
// Successful changes show up in the list instead.
// =========================================================

type Banner = { kind: "error"; message: string };

function BannerView({ banner, onDismiss }: { banner: Banner; onDismiss: () => void }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-lg border border-red-500/50 bg-red-500/10 px-3 py-2 text-sm text-red-500">
      <span className="flex-1">{banner.message}</span>
      <button
        type="button"
        onClick={onDismiss}
        className="rounded-md px-1.5 py-0.5 text-xs hover:bg-surface-hover"
      >
        Dismiss
      </button>
    </div>
  );
}

function formatError(error: unknown, fallback: string): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return fallback;
}
