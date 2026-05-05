// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// MockGadgetContextProvider
//
// Test helper that wraps a plugin component under test in
// the host's real GadgetContextProvider with sensible mock
// defaults. Plugin authors only need to override the slices
// the test cares about — everything else gets a no-op
// stand-in.
//
// Usage:
//
//   import {
//     setupSdkGlobalsForTesting,
//     MockGadgetContextProvider,
//   } from "@torchsnap/gadget-sdk/testing";
//
//   beforeAll(() => setupSdkGlobalsForTesting());
//
//   test("CalculatorSettings renders", () => {
//     render(
//       <MockGadgetContextProvider info={{ id: "calculator", enabled: true }}>
//         <CalculatorSettings />
//       </MockGadgetContextProvider>
//     );
//   });
//
// The mock uses the host's real GadgetContext object so the
// shim hooks (`@torchsnap/gadget-sdk/hooks`) resolve through
// the same React context the production code uses.
// =========================================================

import { useMemo, useRef, type ReactNode } from "react";
import { GadgetContext } from "../../../src/contexts/GadgetContext";
import type {
  LauncherActions,
  GadgetContextValue,
  GadgetInfo,
  GadgetRuntime,
} from "../../../src/contexts/GadgetContext";
import type { Logger } from "../types/logger";

// ---------------------------------------------------------
// No-op defaults
// ---------------------------------------------------------

const noopLogger: Logger = {
  trace: () => {},
  debug: () => {},
  info: () => {},
  warn: () => {},
  error: () => {},
  spanStart: () => 0,
  spanEnd: () => {},
};

const noopSendMessage = async () => undefined as never;

// ---------------------------------------------------------
// Provider
// ---------------------------------------------------------

export interface MockGadgetContextProviderProps {
  /** Override fields for the `info` slice. Defaults:
   *  `{ id: "test-plugin", enabled: true }`. */
  info?: Partial<GadgetInfo>;
  /** Override fields for the `runtime` slice. Defaults to
   *  no-op `sendMessage` and a no-op `logger`. */
  runtime?: Partial<GadgetRuntime>;
  /** Override fields for the `launcher` slice. When the
   *  argument is omitted entirely, the launcher slice is
   *  absent — calling `useLauncher()` from a child throws
   *  exactly as it would in a settings panel. Pass an empty
   *  object `{}` to install the no-op defaults. */
  launcher?: Partial<LauncherActions>;
  children: ReactNode;
}

export function MockGadgetContextProvider({
  info,
  runtime,
  launcher,
  children,
}: MockGadgetContextProviderProps) {
  // The mouseActiveRef default has to be a real RefObject so
  // launcher consumers can read/write `.current` without
  // crashing.
  const mouseActiveRef = useRef(false);

  const value = useMemo<GadgetContextValue>(() => {
    const fullInfo: GadgetInfo = {
      id: "test-plugin",
      enabled: true,
      ...info,
    };
    const fullRuntime: GadgetRuntime = {
      sendMessage: noopSendMessage,
      logger: noopLogger,
      ...runtime,
    };

    const fullLauncher: LauncherActions | undefined =
      launcher === undefined
        ? undefined
        : {
            goBack: () => {},
            dismiss: () => {},
            onExecute: () => {},
            onFooterChange: () => {},
            setDisplayQuery: () => {},
            mouseActiveRef,
            ...launcher,
          };

    return {
      info: fullInfo,
      runtime: fullRuntime,
      launcher: fullLauncher,
    };
  }, [info, runtime, launcher]);

  return (
    <GadgetContext.Provider value={value}>{children}</GadgetContext.Provider>
  );
}
