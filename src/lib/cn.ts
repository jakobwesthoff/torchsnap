// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Utility combining `clsx` conditional class joining with `tailwind-merge`
 * conflict resolution. Use instead of `clsx` whenever Tailwind class
 * overrides need to be predictable (e.g. a component accepting a `className`
 * prop that may conflict with internal defaults).
 */

import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(...inputs));
}
