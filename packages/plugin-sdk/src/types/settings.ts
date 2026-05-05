// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Settings Hook Type
//
// Describes the signature of the `useGadgetSetting` hook
// that the host provides to each plugin's settings
// component. The hook is pre-bound to the plugin's namespace
// so calling `useGadgetSetting("greeting")` reads and writes
// `plugins.<id>.greeting` in the global settings store.
//
// This type decouples the SDK from the host's concrete
// `createGadgetSettingHook` factory (src/hooks/useGadgetSetting.ts).
// =========================================================

export type UsePluginSetting = <T>(key: string) => [value: T, setValue: (v: T) => Promise<void>];
