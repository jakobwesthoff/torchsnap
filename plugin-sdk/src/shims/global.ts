// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// Shared ambient declaration for the host-injected
// `window.__torchsnap` global. The slice contents
// (`hooks`, `components`, `keybindings`) are contributed
// piecemeal by the matching shim files via interface
// merging on `TorchsnapGlobal` — see those files for the
// per-slice declarations.

declare global {
  // eslint-disable-next-line @typescript-eslint/no-empty-object-type
  interface TorchsnapGlobal {}
  interface Window {
    __torchsnap?: TorchsnapGlobal;
  }
}

export {};
