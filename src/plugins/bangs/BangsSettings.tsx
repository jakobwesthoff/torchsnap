// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Settings component for the DuckDuckGo Bangs plugin.
 *
 * Controls:
 * - Enable/disable toggle
 * - Bang database stats (import date, source, counts)
 * - Refresh database button
 * - Attribution section
 */

import { useCallback, useEffect, useState } from "react";
import type { PluginSettingsProps } from "../types";
import { sendPluginMessage } from "../../lib/pluginMessage";
import { SectionHeader } from "../../settings/SectionHeader";
import { Section } from "../../settings/Section";
import { Entry } from "../../settings/Entry";
import { Switch } from "../../components/Switch";

// =========================================================
// Types
// =========================================================

interface BangStats {
  importDate: string | null;
  source: string | null;
  bangCount: number | null;
  domainCount: number | null;
  error?: string;
}

// =========================================================
// Helpers
// =========================================================

const PLUGIN_ID = "bangs";

function pluginMessage<T>(method: string, payload: unknown = {}): Promise<T> {
  return sendPluginMessage<unknown, T>(PLUGIN_ID, method, payload);
}

/** Format an ISO 8601 timestamp into a human-readable date. */
function formatDate(iso: string): string {
  const date = new Date(iso);
  return date.toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** Format a source string into a human-readable label. */
function formatSource(source: string): string {
  return source === "network" ? "Downloaded from DuckDuckGo" : "Built-in fallback";
}

// =========================================================
// Stat Row
// =========================================================

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-sm text-text-secondary">{label}</span>
      <span className="text-sm text-text-primary tabular-nums">{value}</span>
    </div>
  );
}

// =========================================================
// Component
// =========================================================

export default function BangsSettings({ usePluginSetting }: PluginSettingsProps) {
  const [enabled, setEnabled] = usePluginSetting<boolean>("enabled");

  const [stats, setStats] = useState<BangStats | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);

  const fetchStats = useCallback(() => {
    pluginMessage<BangStats>("stats")
      .then(setStats)
      .catch((e) => console.error("bangs: fetch stats failed:", e));
  }, []);

  useEffect(() => {
    fetchStats();
  }, [fetchStats]);

  const handleRefresh = useCallback(async () => {
    setRefreshing(true);
    setRefreshError(null);
    try {
      const result = await pluginMessage<BangStats>("refresh");
      if (result.error) {
        setRefreshError(result.error);
      } else {
        setStats(result);
      }
    } catch (e) {
      setRefreshError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, []);

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        icon="heroicons:arrow-top-right-on-square"
        title="Bangs"
        description="Use DuckDuckGo bang commands to quickly search services. Type !g for Google, !yt for YouTube, !crates for crates.io, and thousands more."
      />

      {/* ---- Plugin toggle ---- */}
      <Section>
        <Entry label="Enable bangs">
          <Switch checked={enabled} onChange={setEnabled} />
        </Entry>
      </Section>

      {/* ---- Database Stats ---- */}
      <Section>
        <div className="flex items-center justify-between -mt-0.5 mb-1">
          <h3 className="text-sm font-medium text-text-secondary">Database</h3>
          <button
            onClick={fetchStats}
            className="rounded-lg px-2 py-0.5 text-xs text-text-tertiary transition-colors hover:text-text-secondary hover:bg-surface-hover"
          >
            Refresh stats
          </button>
        </div>
        {stats ? (
          <div className="flex flex-col gap-1">
            {stats.bangCount != null && (
              <StatRow label="Total bangs" value={stats.bangCount.toLocaleString()} />
            )}
            {stats.domainCount != null && (
              <StatRow label="Unique services" value={stats.domainCount.toLocaleString()} />
            )}
            {stats.source != null && <StatRow label="Source" value={formatSource(stats.source)} />}
            {stats.importDate != null && (
              <StatRow label="Imported" value={formatDate(stats.importDate)} />
            )}
          </div>
        ) : (
          <span className="text-sm text-text-muted">Loading...</span>
        )}
      </Section>

      {/* ---- Refresh Button ---- */}
      <Section>
        <div className="flex items-center justify-between">
          <div className="flex flex-col">
            <span className="text-sm text-text-primary">Refresh bang database</span>
            <span className="text-xs text-text-tertiary">
              Download the latest bang definitions from DuckDuckGo
            </span>
          </div>
          <button
            onClick={handleRefresh}
            disabled={refreshing}
            className="rounded-lg px-3 py-1.5 text-xs font-medium text-accent transition-colors hover:bg-accent/10 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {refreshing ? "Refreshing..." : "Refresh"}
          </button>
        </div>
        {refreshError && (
          <p className="mt-2 text-xs text-red-500">Failed to refresh: {refreshError}</p>
        )}
      </Section>

      {/* ---- Attribution ---- */}
      <Section>
        <div className="flex flex-col gap-1.5">
          <p className="text-xs text-text-tertiary leading-relaxed">
            Bang definitions are provided by{" "}
            <a
              href="https://duckduckgo.com/bangs"
              target="_blank"
              rel="noopener noreferrer"
              className="text-accent hover:underline"
            >
              DuckDuckGo
            </a>
            . A huge thank you to DuckDuckGo and the community of contributors who submit and
            maintain these bang shortcuts. If you don&apos;t already, give DuckDuckGo a try — they
            are awesome.
          </p>
        </div>
      </Section>
    </div>
  );
}
