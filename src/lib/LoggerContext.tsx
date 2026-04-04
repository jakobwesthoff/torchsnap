// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Logger React Context
//
// Provides a Logger instance via React context so that deep
// component trees can access the logger without prop drilling.
//
// Plugin views are wrapped in <LoggerProvider source={pluginId}>
// by the host. Components call useLogger() to get the logger.
// =========================================================

import { createContext, useContext, useMemo, type ReactNode } from "react";
import { Logger, createLogger } from "./logger";

const LoggerContext = createContext<Logger | null>(null);

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

/**
 * Access the logger from context. Must be used within a
 * LoggerProvider.
 *
 * @throws Error if used outside a LoggerProvider.
 */
export function useLogger(): Logger {
  const logger = useContext(LoggerContext);
  if (!logger) {
    throw new Error("useLogger must be used within a LoggerProvider");
  }
  return logger;
}
