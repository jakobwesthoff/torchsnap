// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Universal icon component that renders from a string identifier.
 *
 * Supports multiple icon source formats via a prefix-based protocol:
 *
 * - `heroicons:<name>` — HeroIcon component (kebab-case name,
 *   e.g. `"heroicons:cog-6-tooth"`)
 * - `emoji:<character>` — renders the emoji character as text
 * - `data:<url>` — data URL rendered as an `<img>`
 * - `asset:<path-or-url>` — either an absolute filesystem
 *   path (run through Tauri's `convertFileSrc`) or a fully-
 *   qualified URL such as `torchsnap-gadget://...` produced
 *   by the WASM bridge for gadget-relative `AssetIcon`s; URLs
 *   are passed through to `<img src>` directly.
 *
 * Sizing and color are controlled via `className` — the component
 * renders no wrapper div, just the icon element itself.
 */

import * as HeroIcons from "@heroicons/react/24/outline";
import { convertFileSrc } from "@tauri-apps/api/core";

interface IconProps {
  /** String identifier following the prefix protocol. */
  icon: string;
  /** Tailwind classes for sizing and color. */
  className?: string;
}

/**
 * Resolve a kebab-case HeroIcon name to the corresponding React
 * component. Converts "kebab-case" → "PascalCaseIcon" to match
 * the `@heroicons/react` export names.
 */
function resolveHeroIcon(
  name: string,
): React.ComponentType<React.SVGProps<SVGSVGElement>> | undefined {
  const pascal =
    name
      .split("-")
      .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
      .join("") + "Icon";
  return (HeroIcons as Record<string, React.ComponentType<React.SVGProps<SVGSVGElement>>>)[pascal];
}

// ESLINT: `resolveHeroIcon` does a property lookup on the static
// `HeroIcons` module — the returned component reference is
// referentially stable for any given name.
/* eslint-disable react-hooks/static-components */
export function Icon({ icon, className }: IconProps) {
  // heroicons:<name>
  if (icon.startsWith("heroicons:")) {
    const name = icon.slice("heroicons:".length);
    const HeroIcon = resolveHeroIcon(name) ?? HeroIcons.CommandLineIcon;
    return <HeroIcon className={className} />;
  }

  // emoji:<character>
  if (icon.startsWith("emoji:")) {
    const char = icon.slice("emoji:".length);
    return <span className={className}>{char}</span>;
  }

  // data:<url> (data URLs start with "data:")
  if (icon.startsWith("data:")) {
    return <img src={icon} alt="" className={className} draggable={false} />;
  }

  // asset:<path-or-url>
  // Fully-qualified URLs (e.g. `torchsnap-gadget://...` produced
  // by the WASM bridge for gadget-relative AssetIcons) bypass
  // `convertFileSrc` — they're already resolvable as `<img src>`.
  // Bare filesystem paths still go through Tauri's asset protocol.
  if (icon.startsWith("asset:")) {
    const value = icon.slice("asset:".length);
    const src = value.includes("://") ? value : convertFileSrc(value);
    return <img src={src} alt="" className={className} draggable={false} />;
  }

  // Fallback: treat as unknown, render default icon
  return <HeroIcons.CommandLineIcon className={className} />;
}
/* eslint-enable react-hooks/static-components */
