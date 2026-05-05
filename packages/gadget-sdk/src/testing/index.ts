// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// @torchsnap/gadget-sdk/testing — barrel
//
// Single import surface for plugin component tests:
//
//   import {
//     setupSdkGlobalsForTesting,
//     MockGadgetContextProvider,
//   } from "@torchsnap/gadget-sdk/testing";
// =========================================================

export { setupSdkGlobalsForTesting } from "./setup";
export {
  MockGadgetContextProvider,
  type MockGadgetContextProviderProps,
} from "./MockGadgetContextProvider";
