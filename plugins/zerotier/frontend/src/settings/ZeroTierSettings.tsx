// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * ZeroTier plugin settings panel.
 *
 * Two top-level sections:
 *
 * 1. **Authentication** — surfaces the resolved token state.
 *    The manual-paste field is disabled when the daemon's
 *    auth-token file was successfully auto-detected; enabled
 *    with explanatory info text otherwise.
 * 2. **Remembered networks** — a list of every network the
 *    plugin has observed live, plus everything imported from
 *    the macOS UI's `saved_networks.json`. Per-row Forget,
 *    plus toolbar Clear-all and (macOS only) "Re-import from
 *    ZeroTier UI".
 */

import { useCallback, useEffect, useState } from "react";
import { useGadgetInfo, useGadgetRuntime, useGadgetSetting } from "@torchsnap/gadget-sdk/hooks";
import { Entry, List, type ListItem, Section } from "@torchsnap/gadget-sdk/components";
import "../../styles/settings.css";

type AuthSource = "auto" | "manual" | "none";
type AuthValidation = "validated" | "rejected" | "unconfigured" | "daemon-unreachable";

interface AuthState {
  state: AuthValidation;
  source: AuthSource;
  is_macos: boolean;
}

interface KnownNetwork {
  id: string;
  name: string;
  last_seen: number;
  last_status: string | null;
  state: "connected" | "joined-offline" | "known-only";
}

function formatLastSeen(unixMs: number): string {
  if (!unixMs) return "—";
  const seen = new Date(unixMs);
  const now = new Date();
  const diffMs = now.getTime() - seen.getTime();
  const oneDay = 24 * 60 * 60 * 1000;
  if (diffMs < oneDay) return "today";
  if (diffMs < 2 * oneDay) return "yesterday";
  if (diffMs < 7 * oneDay) return `${Math.floor(diffMs / oneDay)} days ago`;
  return seen.toLocaleDateString();
}

function stateBadge(net: KnownNetwork): string {
  switch (net.state) {
    case "connected":
      return "● Connected";
    case "joined-offline":
      return net.last_status ? `○ ${net.last_status.replace(/_/g, " ").toLowerCase()}` : "○ Joined";
    case "known-only":
      return "Stored";
  }
}

export function ZeroTierSettings() {
  const { enabled } = useGadgetInfo();
  const { sendMessage } = useGadgetRuntime();

  const [manualToken, setManualToken] = useGadgetSetting<string>("manualToken");
  const [authState, setAuthState] = useState<AuthState | null>(null);
  const [networks, setNetworks] = useState<KnownNetwork[] | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [reimporting, setReimporting] = useState(false);
  const [reimportResult, setReimportResult] = useState<string | null>(null);

  const refreshAuthState = useCallback(() => {
    sendMessage<unknown, AuthState>("auth_state", {})
      .then(setAuthState)
      .catch((e) => console.error("zerotier: auth_state failed:", e));
  }, [sendMessage]);

  const refreshNetworks = useCallback(() => {
    sendMessage<unknown, KnownNetwork[]>("list_known", {})
      .then(setNetworks)
      .catch((e) => console.error("zerotier: list_known failed:", e));
  }, [sendMessage]);

  useEffect(() => {
    refreshAuthState();
    refreshNetworks();
  }, [refreshAuthState, refreshNetworks]);

  const handleForget = useCallback(
    async (id: string) => {
      try {
        await sendMessage("forget", { id });
        refreshNetworks();
      } catch (e) {
        console.error("zerotier: forget failed:", e);
      }
    },
    [sendMessage, refreshNetworks],
  );

  const handleClearAll = useCallback(async () => {
    if (!confirmClear) {
      setConfirmClear(true);
      return;
    }
    try {
      await sendMessage("clear_all", {});
      refreshNetworks();
    } catch (e) {
      console.error("zerotier: clear_all failed:", e);
    } finally {
      setConfirmClear(false);
    }
  }, [confirmClear, sendMessage, refreshNetworks]);

  const handleReimport = useCallback(async () => {
    setReimporting(true);
    setReimportResult(null);
    try {
      const r = await sendMessage<unknown, { inserted: number }>("reimport", {});
      setReimportResult(
        r.inserted === 0
          ? "No new entries"
          : `Imported ${r.inserted} ${r.inserted === 1 ? "entry" : "entries"}`,
      );
      refreshNetworks();
    } catch (e) {
      setReimportResult(`Failed: ${e instanceof Error ? e.message : String(e)}`);
    } finally {
      setReimporting(false);
    }
  }, [sendMessage, refreshNetworks]);

  const tokenAutoDetected = authState?.source === "auto";
  const tokenInputDisabled = !enabled || tokenAutoDetected;

  const items: ListItem[] = (networks ?? []).map((net) => ({
    key: net.id,
    primary: net.name || "(unnamed network)",
    secondary: (
      <span>
        <code className="font-mono text-text-tertiary">{net.id}</code>
        {" · "}
        {stateBadge(net)} · last seen {formatLastSeen(net.last_seen)}
      </span>
    ),
    actions: [
      {
        label: "Forget",
        variant: "danger" as const,
        onClick: () => handleForget(net.id),
      },
    ],
  }));

  return (
    <div className="flex flex-col gap-4">
      <Section title="Authentication">
        <Entry
          label="Auth token"
          description={
            tokenAutoDetected
              ? "Auto-detected — paste a token only if auto-detection fails."
              : authState?.state === "rejected"
                ? "ZeroTier rejected this token. Verify it matches the daemon."
                : authState?.state === "daemon-unreachable"
                  ? "ZeroTier daemon is not reachable. Start zerotier-one and try again."
                  : "Paste the contents of authtoken.secret. Path: macOS /Library/Application Support/ZeroTier/One/, Linux /var/lib/zerotier-one/, Windows %ProgramData%\\ZeroTier\\One\\."
          }
        >
          <input
            type="text"
            value={manualToken ?? ""}
            onChange={(e) => setManualToken(e.target.value)}
            disabled={tokenInputDisabled}
            placeholder={tokenAutoDetected ? "Auto-detected" : "Paste token here"}
            className="rounded-md border border-border-input bg-surface px-3 py-1.5 text-sm font-mono text-text-primary placeholder:text-text-muted focus:border-accent focus:outline-none disabled:opacity-50 disabled:cursor-not-allowed w-72"
          />
        </Entry>
      </Section>

      <Section title="Remembered networks">
        <div className="flex flex-col gap-3">
          <div className="flex items-center justify-end gap-2">
            {authState?.is_macos && (
              <button
                type="button"
                onClick={handleReimport}
                disabled={!enabled || reimporting}
                className="px-2 py-1 text-xs rounded border border-border text-text-primary hover:bg-surface-inset disabled:opacity-40 disabled:cursor-not-allowed"
              >
                {reimporting ? "Re-importing…" : "Re-import from ZeroTier UI"}
              </button>
            )}
            {confirmClear ? (
              <>
                <button
                  type="button"
                  onClick={() => setConfirmClear(false)}
                  className="px-2 py-1 text-xs rounded border border-border text-text-primary hover:bg-surface-inset"
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={handleClearAll}
                  className="px-2 py-1 text-xs rounded border border-border text-red-500 hover:bg-red-500/10"
                >
                  Confirm clear
                </button>
              </>
            ) : (
              <button
                type="button"
                onClick={handleClearAll}
                disabled={!enabled || items.length === 0}
                className="px-2 py-1 text-xs rounded border border-border text-red-500 hover:bg-red-500/10 disabled:opacity-40 disabled:cursor-not-allowed"
              >
                Clear all
              </button>
            )}
          </div>
          {reimportResult && <p className="text-xs text-text-tertiary">{reimportResult}</p>}
          <List
            items={items}
            emptyState={
              networks === null
                ? "Loading…"
                : "No remembered networks yet. Connect to one from the launcher to populate this list."
            }
          />
        </div>
      </Section>
    </div>
  );
}
