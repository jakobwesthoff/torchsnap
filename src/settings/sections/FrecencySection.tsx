// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useSetting } from "../../hooks/useSetting";
import { SettingsSection } from "../../components/SettingsSection";
import { SettingsEntry } from "../../components/SettingsEntry";
import { Switch } from "../../components/Switch";

// =========================================================
// Types
// =========================================================

interface FrecencyStats {
  totalEvents: number;
  uniqueItems: number;
  eventsByPlugin: Record<string, number>;
  oldestEvent: number | null;
}

// =========================================================
// Helpers
// =========================================================

/** Format a millisecond timestamp as a relative age string. */
function formatAge(timestampMs: number): string {
  const ageMs = Date.now() - timestampMs;
  const days = Math.floor(ageMs / (1000 * 60 * 60 * 24));
  if (days === 0) return "today";
  if (days === 1) return "1 day ago";
  if (days < 30) return `${days} days ago`;
  const months = Math.floor(days / 30);
  if (months === 1) return "1 month ago";
  return `${months} months ago`;
}

// =========================================================
// Component
// =========================================================

export function FrecencySection() {
  const [enabled, setEnabled] = useSetting<boolean>("frecency.enabled");

  const [stats, setStats] = useState<FrecencyStats | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);

  const refreshStats = useCallback(() => {
    invoke<FrecencyStats>("frecency_stats")
      .then(setStats)
      .catch((e) => console.error("frecency: fetch stats failed:", e));
  }, []);

  // Fetch stats on mount.
  useEffect(() => {
    refreshStats();
  }, [refreshStats]);

  const handleClearHistory = useCallback(async () => {
    if (!confirmClear) {
      setConfirmClear(true);
      return;
    }

    setClearing(true);
    try {
      await invoke("frecency_clear");
      refreshStats();
    } catch (e) {
      console.error("frecency: clear failed:", e);
    } finally {
      setClearing(false);
      setConfirmClear(false);
    }
  }, [confirmClear, refreshStats]);

  const handleCancelClear = useCallback(() => {
    setConfirmClear(false);
  }, []);

  return (
    <div className="flex flex-col gap-4">
      {/* ---- Enable/disable toggle ---- */}
      <SettingsSection>
        <SettingsEntry label="Enable frecency tracking">
          <Switch checked={enabled} onChange={setEnabled} />
        </SettingsEntry>
      </SettingsSection>

      {/* ---- Statistics ---- */}
      <SettingsSection>
        <div className="flex items-center justify-between -mt-0.5 mb-1">
          <h3 className="text-sm font-medium text-text-secondary">Statistics</h3>
          <button
            onClick={refreshStats}
            className="rounded-lg px-2 py-0.5 text-xs text-text-tertiary transition-colors hover:text-text-secondary hover:bg-surface-hover"
          >
            Refresh
          </button>
        </div>
        {stats ? (
          <div className="flex flex-col gap-2">
            <StatRow label="Total events" value={String(stats.totalEvents)} />
            <StatRow label="Unique items" value={String(stats.uniqueItems)} />
            {Object.entries(stats.eventsByPlugin)
              .sort(([, a], [, b]) => b - a)
              .map(([pluginId, count]) => (
                <StatRow key={pluginId} label={pluginId} value={String(count)} indent />
              ))}
            {stats.oldestEvent != null && (
              <StatRow label="Oldest event" value={formatAge(stats.oldestEvent)} />
            )}
          </div>
        ) : (
          <span className="text-sm text-text-muted">Loading...</span>
        )}
      </SettingsSection>

      {/* ---- Clear history ---- */}
      <SettingsSection>
        <div className="flex items-center justify-between">
          <div className="flex flex-col">
            <span className="text-sm text-text-primary">Clear history</span>
            <span className="text-xs text-text-tertiary">Permanently delete all frecency data</span>
          </div>
          {confirmClear ? (
            <div className="flex gap-2">
              <button
                onClick={handleCancelClear}
                className="rounded-lg px-3 py-1.5 text-xs font-medium text-text-secondary transition-colors hover:bg-surface-hover"
              >
                Cancel
              </button>
              <button
                onClick={handleClearHistory}
                disabled={clearing}
                className="rounded-lg px-3 py-1.5 text-xs font-medium text-white bg-red-500 transition-colors hover:bg-red-600 disabled:opacity-50"
              >
                {clearing ? "Clearing..." : "Confirm"}
              </button>
            </div>
          ) : (
            <button
              onClick={handleClearHistory}
              className="rounded-lg px-3 py-1.5 text-xs font-medium text-red-500 transition-colors hover:bg-red-500/10"
            >
              Clear All
            </button>
          )}
        </div>
      </SettingsSection>
    </div>
  );
}

// =========================================================
// Sub-components
// =========================================================

function StatRow({
  label,
  value,
  indent = false,
}: {
  label: string;
  value: string;
  indent?: boolean;
}) {
  return (
    <div className="flex items-center justify-between">
      <span className={`text-sm ${indent ? "text-text-tertiary pl-3" : "text-text-secondary"}`}>
        {label}
      </span>
      <span className="text-sm text-text-primary tabular-nums">{value}</span>
    </div>
  );
}
