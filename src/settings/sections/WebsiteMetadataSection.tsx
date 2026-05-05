// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

import { useCallback, useEffect, useState } from "react";
import { command } from "../../lib/command";
import { useSetting } from "../../hooks/useSetting";
import { createLogger } from "../../lib/logger";
import { SectionHeader } from "../SectionHeader";
import { Section } from "../Section";
import { Slider } from "../../components/Slider";

// =========================================================
// Helpers
// =========================================================

function formatRetentionDays(days: number): string {
  if (days === 90) return "90d";
  return `${days}d`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

const logger = createLogger("website-metadata");

// =========================================================
// Component
// =========================================================

export function WebsiteMetadataSection() {
  const [cacheTtlDays, setCacheTtlDays] = useSetting<number>("websiteMetadata.cacheTtlDays");

  const [stats, setStats] = useState<{ entryCount: number; faviconBytes: number } | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const [clearing, setClearing] = useState(false);

  const refreshStats = useCallback(() => {
    command("website_metadata_stats")
      .then(setStats)
      .catch((e: unknown) => logger.error(`fetch stats failed: ${String(e)}`));
  }, []);

  useEffect(() => {
    refreshStats();
  }, [refreshStats]);

  const handleClearCache = useCallback(async () => {
    if (!confirmClear) {
      setConfirmClear(true);
      return;
    }

    setClearing(true);
    try {
      await command("website_metadata_clear_cache");
      refreshStats();
    } catch (e) {
      logger.error(`clear cache failed: ${String(e)}`);
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
      <SectionHeader
        icon="heroicons:globe-alt"
        title="Website Metadata"
        description="Caches website favicons and metadata (title, description) so gadgets can show enriched results without repeated network requests."
      />

      {/* ---- Retention ---- */}
      <Section title="Retention">
        <div className="flex flex-col gap-1.5">
          <div className="flex items-center justify-between">
            <span className="text-sm text-text-primary">Keep entries for</span>
          </div>
          <Slider
            value={cacheTtlDays}
            onChange={setCacheTtlDays}
            min={1}
            max={90}
            step={1}
            formatValue={formatRetentionDays}
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
          <div className="flex flex-col gap-2">
            <StatRow label="Cached domains" value={String(stats.entryCount)} />
            <StatRow label="Favicon storage" value={formatBytes(stats.faviconBytes)} />
          </div>
        ) : (
          <span className="text-sm text-text-muted">Loading...</span>
        )}
      </Section>

      {/* ---- Clear cache ---- */}
      <Section>
        <div className="flex items-center justify-between">
          <div className="flex flex-col">
            <span className="text-sm text-text-primary">Clear cache</span>
            <span className="text-xs text-text-tertiary">
              Remove all cached metadata and favicon files
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
                onClick={handleClearCache}
                disabled={clearing}
                className="rounded-lg px-3 py-1.5 text-xs font-medium text-white bg-red-500 transition-colors hover:bg-red-600 disabled:opacity-50"
              >
                {clearing ? "Clearing..." : "Confirm"}
              </button>
            </div>
          ) : (
            <button
              onClick={handleClearCache}
              className="rounded-lg px-3 py-1.5 text-xs font-medium text-red-500 transition-colors hover:bg-red-500/10"
            >
              Clear All
            </button>
          )}
        </div>
      </Section>
    </div>
  );
}

// =========================================================
// Sub-components
// =========================================================

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-sm text-text-secondary">{label}</span>
      <span className="text-sm text-text-primary tabular-nums">{value}</span>
    </div>
  );
}
