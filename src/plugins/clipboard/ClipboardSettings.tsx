// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Settings component for the clipboard manager plugin.
 *
 * Controls:
 * - Enable/disable toggle (starts/stops the clipboard watcher)
 * - Retention period slider (1–365 days)
 *
 * Also displays clipboard history statistics (entry counts by
 * type, total storage size) and a "Clear History" button.
 */

import { useCallback, useEffect, useState } from "react";
import type { PluginSettingsProps } from "../types";
import { sendPluginMessage } from "../../lib/pluginMessage";
import { SettingsSection } from "../../components/SettingsSection";
import { SettingsEntry } from "../../components/SettingsEntry";
import { Switch } from "../../components/Switch";
import { Slider } from "../../components/Slider";
import { ShortcutRecorder } from "../../components/ShortcutRecorder";

// =========================================================
// Types
// =========================================================

interface ClipboardStats {
  totalEntries: number;
  entriesByFormat: Record<string, number>;
  totalSizeBytes: number;
}

// =========================================================
// Helpers
// =========================================================

const PLUGIN_ID = "clipboard-manager";

/** Send a message to the clipboard plugin's backend handler. */
function pluginMessage<T>(method: string, payload: unknown = {}): Promise<T> {
  return sendPluginMessage<unknown, T>(PLUGIN_ID, method, payload);
}

/** Format a byte count as a human-readable string. */
function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

/** Format retention days for the slider display. */
function formatRetentionDays(days: number): string {
  if (days === 365) return "1 year";
  return `${days}d`;
}

// =========================================================
// Component
// =========================================================

export default function ClipboardSettings({ usePluginSetting }: PluginSettingsProps) {
  const [enabled, setEnabled] = usePluginSetting<boolean>("enabled");
  const [retentionDays, setRetentionDays] = usePluginSetting<number>("retentionDays");
  const [bringToFrontOnPaste, setBringToFrontOnPaste] =
    usePluginSetting<boolean>("bringToFrontOnPaste");
  const [shortcut, setShortcut] = usePluginSetting<string>("shortcut.open-clipboard");

  const [stats, setStats] = useState<ClipboardStats | null>(null);
  const [clearing, setClearing] = useState(false);
  const [confirmClear, setConfirmClear] = useState(false);

  const refreshStats = useCallback(() => {
    pluginMessage<ClipboardStats>("stats")
      .then(setStats)
      .catch((e) => console.error("clipboard: fetch stats failed:", e));
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
      await pluginMessage("clear_history");
      refreshStats();
    } catch (e) {
      console.error("clipboard: clear history failed:", e);
    } finally {
      setClearing(false);
      setConfirmClear(false);
    }
  }, [confirmClear, refreshStats]);

  // Reset confirmation when clicking elsewhere.
  const handleCancelClear = useCallback(() => {
    setConfirmClear(false);
  }, []);

  return (
    <div className="flex flex-col gap-4">
      {/* ---- Plugin toggle ---- */}
      <SettingsSection>
        <SettingsEntry label="Enable clipboard history">
          <Switch checked={enabled} onChange={setEnabled} />
        </SettingsEntry>
      </SettingsSection>

      {/* ---- Shortcut ---- */}
      <SettingsSection>
        <SettingsEntry label="Open Clipboard History">
          <ShortcutRecorder value={shortcut} onChange={setShortcut} disabled={!enabled} />
        </SettingsEntry>
      </SettingsSection>

      {/* ---- Behaviour ---- */}
      <SettingsSection>
        <SettingsEntry
          label="Bring to front on paste"
          description="Move pasted entries to the top of the history list"
        >
          <Switch
            checked={bringToFrontOnPaste}
            onChange={setBringToFrontOnPaste}
            disabled={!enabled}
          />
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
            disabled={!enabled}
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
          <div className="flex flex-col gap-2">
            <StatRow label="Total entries" value={String(stats.totalEntries)} />
            {Object.entries(stats.entriesByFormat)
              .sort(([, a], [, b]) => b - a)
              .map(([format, count]) => (
                <StatRow key={format} label={formatLabel(format)} value={String(count)} indent />
              ))}
            <StatRow label="Storage used" value={formatBytes(stats.totalSizeBytes)} />
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
              Permanently delete all clipboard entries
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
              disabled={!enabled && stats?.totalEntries === 0}
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

/** Capitalize a format name for display. */
function formatLabel(format: string): string {
  switch (format) {
    case "text":
      return "Text";
    case "image":
      return "Images";
    case "files":
      return "Files";
    case "html":
      return "HTML";
    case "rtf":
      return "Rich Text";
    default:
      return format.charAt(0).toUpperCase() + format.slice(1);
  }
}
