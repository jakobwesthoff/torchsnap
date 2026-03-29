// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Settings component for the calculator plugin.
 *
 * Controls:
 * - Enable/disable toggle
 * - Heuristic (prefix-free) parsing toggle
 * - History enable/disable toggle
 * - Retention period slider
 * - Storage stats + clear history
 */

import { useCallback, useEffect, useState } from "react";
import type { PluginSettingsProps } from "../types";
import { sendPluginMessage } from "../../lib/pluginMessage";
import { SettingsSection } from "../../components/SettingsSection";
import { SettingsEntry } from "../../components/SettingsEntry";
import { Switch } from "../../components/Switch";
import { Slider } from "../../components/Slider";

// =========================================================
// Types
// =========================================================

interface CalcStats {
  entryCount: number;
  dbSize: number;
}

// =========================================================
// Helpers
// =========================================================

const PLUGIN_ID = "calculator";

function pluginMessage<T>(method: string, payload: unknown = {}): Promise<T> {
  return sendPluginMessage<unknown, T>(PLUGIN_ID, method, payload);
}

function formatRetentionDays(days: number): string {
  if (days === 365) return "1 year";
  return `${days}d`;
}

// =========================================================
// Component
// =========================================================

export default function CalculatorSettings({ usePluginSetting }: PluginSettingsProps) {
  const [enabled, setEnabled] = usePluginSetting<boolean>("enabled");
  const [heuristicEnabled, setHeuristicEnabled] = usePluginSetting<boolean>("heuristicEnabled");
  const [historyEnabled, setHistoryEnabled] = usePluginSetting<boolean>("historyEnabled");
  const [retentionDays, setRetentionDays] = usePluginSetting<number>("retentionDays");

  const [stats, setStats] = useState<CalcStats | null>(null);
  const [clearing, setClearing] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  const refreshStats = useCallback(() => {
    pluginMessage<CalcStats>("stats")
      .then(setStats)
      .catch((e) => console.error("calculator: fetch stats failed:", e));
  }, []);

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
      await pluginMessage("clear_history");
      refreshStats();
    } catch (e) {
      console.error("calculator: clear history failed:", e);
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
      {/* ---- Plugin toggle ---- */}
      <SettingsSection>
        <SettingsEntry label="Enable calculator">
          <Switch checked={enabled} onChange={setEnabled} />
        </SettingsEntry>
      </SettingsSection>

      {/* ---- Heuristic toggle ---- */}
      <SettingsSection>
        <SettingsEntry label="Detect math expressions without prefix">
          <Switch checked={heuristicEnabled} onChange={setHeuristicEnabled} disabled={!enabled} />
        </SettingsEntry>
      </SettingsSection>

      {/* ---- History toggle ---- */}
      <SettingsSection>
        <SettingsEntry label="Save calculation history">
          <Switch checked={historyEnabled} onChange={setHistoryEnabled} disabled={!enabled} />
        </SettingsEntry>
      </SettingsSection>

      {/* ---- Retention ---- */}
      <SettingsSection title="Retention">
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between">
            <span className="text-sm text-text-primary">Keep entries for</span>
          </div>
          <Slider
            value={retentionDays}
            onChange={setRetentionDays}
            min={1}
            max={365}
            step={1}
            formatValue={formatRetentionDays}
            disabled={!enabled || !historyEnabled}
          />
        </div>
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
          <div className="flex items-center justify-between">
            <span className="text-sm text-text-secondary">History entries</span>
            <span className="text-sm text-text-primary tabular-nums">
              {String(stats.entryCount)}
            </span>
          </div>
        ) : (
          <span className="text-sm text-text-muted">Loading...</span>
        )}
      </SettingsSection>

      {/* ---- Clear History ---- */}
      <SettingsSection>
        <div className="flex items-center justify-between">
          <div className="flex flex-col">
            <span className="text-sm text-text-primary">Clear history</span>
            <span className="text-xs text-text-tertiary">
              Permanently delete all calculation history
            </span>
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
              disabled={!historyEnabled && stats?.entryCount === 0}
              className="rounded-lg px-3 py-1.5 text-xs font-medium text-red-500 transition-colors hover:bg-red-500/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              Clear All
            </button>
          )}
        </div>
      </SettingsSection>
    </div>
  );
}
