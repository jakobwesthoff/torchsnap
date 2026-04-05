// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Logger Provider
//
// Provides a Logger instance via React context so that deep
// component trees can access the logger without prop drilling.
//
// Plugin views are wrapped in <LoggerProvider source={pluginId}>
// by the host. Components call useLogger() to get the logger.
// =========================================================

import { useMemo, type ReactNode } from "react";
import { createLogger } from "../lib/logger";
import { LoggerContext } from "./LoggerContext";

export function LoggerProvider({
  source,
  children,
}: {
  source: string;
  children: ReactNode;
}) {
  const logger = useMemo(() => createLogger(source), [source]);
  return (
    <LoggerContext.Provider value={logger}>{children}</LoggerContext.Provider>
  );
}
