// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Settings component for the calculator gadget.
 *
 * Controls:
 * - Heuristic (prefix-free) parsing toggle
 * - History enable/disable toggle
 * - Retention period slider
 * - Storage stats + clear history
 *
 * The gadget's enabled flag (used to disable controls when the
 * user has turned the calculator off) comes from
 * `useGadgetInfo().enabled`. The three settings come from
 * `useGadgetSetting<T>(key)` and the `stats` / `clear_history`
 * RPC calls go through `useGadgetRuntime().sendMessage`.
 */

import { useCallback, useEffect, useState } from "react";
import {
  useGadgetInfo,
  useGadgetRuntime,
  useGadgetSetting,
} from "@torchsnap/gadget-sdk/hooks";
import { Entry, Section, Slider, Switch } from "@torchsnap/gadget-sdk/components";
import "../../styles/settings.css";

interface CalcStats {
  entryCount: number;
  dbSize: number;
}

function formatRetentionDays(days: number): string {
  if (days === 365) return "1 year";
  return `${days}d`;
}

export function CalculatorSettings() {
  const { enabled } = useGadgetInfo();
  const { sendMessage } = useGadgetRuntime();

  const [heuristicEnabled, setHeuristicEnabled] = useGadgetSetting<boolean>("heuristicEnabled");
  const [historyEnabled, setHistoryEnabled] = useGadgetSetting<boolean>("historyEnabled");
  const [retentionDays, setRetentionDays] = useGadgetSetting<number>("retentionDays");

  const [stats, setStats] = useState<CalcStats | null>(null);
  const [clearing, setClearing] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  const refreshStats = useCallback(() => {
    sendMessage<unknown, CalcStats>("stats", {})
      .then(setStats)
      .catch((e) => console.error("calculator: fetch stats failed:", e));
  }, [sendMessage]);

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
      await sendMessage("clear_history", {});
      refreshStats();
    } catch (e) {
      console.error("calculator: clear history failed:", e);
    } finally {
      setClearing(false);
      setConfirmClear(false);
    }
  }, [confirmClear, refreshStats, sendMessage]);

  const handleCancelClear = useCallback(() => {
    setConfirmClear(false);
  }, []);

  return (
    <>
      {/* ---- Heuristic toggle ---- */}
      <Section>
        <Entry label="Detect math expressions without prefix">
          <Switch checked={heuristicEnabled} onChange={setHeuristicEnabled} disabled={!enabled} />
        </Entry>
      </Section>

      {/* ---- History toggle ---- */}
      <Section>
        <Entry label="Save calculation history">
          <Switch checked={historyEnabled} onChange={setHistoryEnabled} disabled={!enabled} />
        </Entry>
      </Section>

      {/* ---- Retention ---- */}
      <Section title="Retention">
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
      </Section>

      {/* ---- Statistics ---- */}
      <Section>
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
      </Section>

      {/* ---- Clear History ---- */}
      <Section>
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
      </Section>
    </>
  );
}
