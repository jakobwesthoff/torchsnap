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
// drop zone accepting `.torchsnap` archives.
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
import { relaunch } from "@tauri-apps/plugin-process";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { command, type GadgetSourceKind } from "../../lib/command";
import { getGadgetsWithSettings } from "../../gadgets/registry";
import { useSetting } from "../../hooks/useSetting";
import { Icon } from "../../components/Icon";
import { Switch } from "../../components/Switch";
import { SectionHeader } from "../SectionHeader";
import { Section } from "../Section";
import { cn } from "../../lib/cn";
import { PermissionSummary } from "../install/PermissionSummary";
import type { PermissionItem } from "../install/types";

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
}

// =========================================================
// Component
// =========================================================

export function GadgetsManagementPanel() {
  const gadgetMetadata = useMemo(() => getGadgetsWithSettings(), []);
  const [sourceKinds, setSourceKinds] = useState<Record<string, GadgetSourceKind>>({});
  const [loadingSourceKinds, setLoadingSourceKinds] = useState(true);
  const [permissions, setPermissions] = useState<Record<string, PermissionItem[]>>({});
  const [banner, setBanner] = useState<Banner | null>(null);
  const [dragActive, setDragActive] = useState(false);

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

  const rows: PluginRow[] = useMemo(() => {
    return gadgetMetadata
      .filter((gadget) => sourceKinds[gadget.id] != null)
      .map((gadget) => ({
        id: gadget.id,
        label: gadget.label,
        description: gadget.description,
        icon: gadget.icon,
        sourceKind: sourceKinds[gadget.id],
        permissions: permissions[gadget.id] ?? [],
      }));
  }, [gadgetMetadata, sourceKinds, permissions]);

  // =========================================================
  // Install: file picker path
  // =========================================================

  const handleInstallClick = useCallback(async () => {
    const selected = await openFileDialog({
      multiple: false,
      filters: [{ name: "Torchsnap Gadget", extensions: ["torchsnap"] }],
    });
    if (typeof selected !== "string") {
      return;
    }
    await runInstall(selected, setBanner);
  }, []);

  // =========================================================
  // Install: drag-and-drop path
  //
  // Tauri exposes drag events through `getCurrentWebview`. The
  // listener is registered for the lifetime of the mounted
  // component; the returned unlistener cleans up on unmount.
  // Dropped files arrive as absolute paths — exactly what
  // `install_gadget_archive` expects.
  // =========================================================

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWebview()
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
          // Sequentially install each dropped archive so
          // collision errors for one do not block the others.
          (async () => {
            for (const path of paths) {
              await runInstall(path, setBanner);
            }
          })();
        }
      })
      .then((u) => {
        unlisten = u;
      });
    return () => {
      unlisten?.();
    };
  }, []);

  const handleUninstall = useCallback(async (gadgetId: string) => {
    try {
      const result = await command("uninstall_user_gadget", { gadgetId });
      setBanner({
        kind: "success",
        message: `Uninstalled gadget "${gadgetId}".`,
        requiresRestart: result.requiresRestart,
      });
    } catch (e) {
      setBanner({
        kind: "error",
        message: formatError(e, "Failed to uninstall gadget"),
      });
    }
  }, []);

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        icon="heroicons:puzzle-piece"
        title="Gadgets"
        description="Enable, disable, install, and uninstall gadgets. Built-in and system gadgets ship with the app and cannot be removed."
      />

      {banner && <BannerView banner={banner} onDismiss={() => setBanner(null)} />}

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
              <PluginRowView key={row.id} row={row} onUninstall={handleUninstall} />
            ))}
          </div>
        )}
      </Section>
    </div>
  );
}

// =========================================================
// Row — one gadget with badge, enable toggle, uninstall.
// =========================================================

function PluginRowView({
  row,
  onUninstall,
}: {
  row: PluginRow;
  onUninstall: (gadgetId: string) => void;
}) {
  const [enabled, setEnabled] = useSetting<boolean>(`enabled.${row.id}`);

  const canUninstall = row.sourceKind === "user";

  return (
    <div data-gadget={row.id} className="flex items-start gap-3 py-2 first:pt-0 last:pb-0">
      {row.icon && (
        <Icon
          icon={row.icon}
          className="h-8 w-8 shrink-0 rounded-md bg-surface-hover p-1.5 text-text-primary"
        />
      )}
      <div className="flex min-w-0 flex-col flex-1">
        <span className="text-sm font-medium text-text-primary truncate">{row.label}</span>
        {row.description && (
          <span className="text-xs text-text-tertiary truncate">{row.description}</span>
        )}
        <PermissionSummary items={row.permissions} className="mt-1.5" />
      </div>
      <Switch checked={enabled ?? true} onChange={setEnabled} />
      {/* Trailing slot: Uninstall for user gadgets, source badge
          for every other kind. Mutually exclusive by design. */}
      {canUninstall ? (
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
        label: "System",
        tooltip:
          "WASM gadget bundled with the app. Upgraded when the app is updated; not uninstallable.",
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
// Banner — success / error / restart-prompt.
// =========================================================

type Banner =
  | { kind: "success"; message: string; requiresRestart?: boolean }
  | { kind: "error"; message: string };

function BannerView({ banner, onDismiss }: { banner: Banner; onDismiss: () => void }) {
  const isError = banner.kind === "error";
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-3 rounded-lg border px-3 py-2 text-sm",
        isError
          ? "border-red-500/50 bg-red-500/10 text-red-500"
          : "border-accent/50 bg-accent/10 text-accent",
      )}
    >
      <span className="flex-1">{banner.message}</span>
      {banner.kind === "success" && banner.requiresRestart && (
        <button
          type="button"
          onClick={() => void relaunch()}
          className="rounded-md bg-accent px-2.5 py-1 text-xs font-medium text-white transition-colors hover:bg-accent/90"
        >
          Restart now
        </button>
      )}
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

// =========================================================
// Install shared handler — extracted so both the file-picker
// and drag-drop paths report results identically.
// =========================================================

async function runInstall(archivePath: string, setBanner: (banner: Banner) => void): Promise<void> {
  try {
    const info = await command("install_gadget_archive", { archivePath });
    setBanner({
      kind: "success",
      message: `Installed ${info.name} ${info.version}.`,
      requiresRestart: info.requiresRestart,
    });
  } catch (e) {
    setBanner({
      kind: "error",
      message: formatError(e, "Failed to install gadget"),
    });
  }
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
