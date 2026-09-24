// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

/**
 * Snappy mascot variant registry and weighted selection configuration.
 *
 * This file defines which mascot variants exist and how they are weighted
 * in the random selection system. It is the single place to add new Snappy
 * mascots — add the variant name to the appropriate group below, then wire
 * it into `SnappyHeroSets` with the desired weight and conditions.
 * Preparing the image itself (canvas, body size, optimization) is
 * described in `assets/mascot/docs/Adding-a-Mascot.md`.
 *
 * NSFW variants (those with visible weapons) are included in their
 * thematic groups alongside the SFW versions. The NSFW filter predicate
 * in `useMascotVariant` excludes them at selection time when the user
 * has disabled non-family-friendly mascots — no structural changes to
 * the weight tree needed.
 *
 * See `useRandomMascot.ts` for the full algorithm details.
 */

import type { MascotEntry } from "./hooks/useRandomMascot";
import { getFullMoonDistance } from "./hooks/useFullMoonDistance";
import { isHalloween, isChristmas, isEaster, isNewYear } from "./hooks/useHolidays";
import { isNighttime, isTwilight } from "./hooks/useNighttime";
import mascotData from "./derived/mascots.json";

// =========================================================
// Mascot Metadata
// =========================================================

interface MascotTrim {
  /** Percentage of transparent space from the top edge. */
  top: number;
  /** Percentage of transparent space from the right edge. */
  right: number;
  /** Percentage of transparent space from the bottom edge. */
  bottom: number;
  /** Percentage of transparent space from the left edge. */
  left: number;
}

interface MascotInfo {
  alt: string;
  /** When true, the mascot depicts content some users may find inappropriate
   *  (e.g. a visible weapon). Controlled by the `showNsfwMascots` setting. */
  nsfw?: boolean;
  /** Percentage of transparent space on each edge of the source image.
   *  Injected by `just asset-mascot-data` from ImageMagick trim detection. */
  trim?: MascotTrim;
}

// TypeScript infers a precise type from the JSON literal, so we widen it
// to a plain record for safe runtime lookups on unknown variant names.
const mascots = mascotData as Record<string, MascotInfo>;

/**
 * Returns the alt text for a given variant, falling back to the variant
 * name formatted as a human-readable string when no entry exists.
 */
export function getMascotAlt(variant: string): string {
  return mascots[variant]?.alt ?? variant.replace(/-/g, " ");
}

/**
 * Returns `true` when the variant is tagged as NSFW in `mascots.json`.
 * Unlisted variants are treated as safe.
 */
export function isNsfwVariant(variant: string): boolean {
  return mascots[variant]?.nsfw === true;
}

const DEFAULT_TRIM: MascotTrim = { top: 0, right: 0, bottom: 0, left: 0 };

/** Variants already reported as missing trim, so the dev-mode warning
 *  fires once per variant instead of on every render. */
const warnedMissingTrim = new Set<string>();

/**
 * Returns the trim data for a given variant — the percentage of
 * transparent space on each edge of the source image. Falls back to
 * zero trim when no data is available.
 *
 * Zero trim tells the placement math that the artwork fills its canvas
 * edge to edge, which floats the mascot above its intended resting
 * position by however much transparent padding the image really has.
 * The failure is subtle enough to survive review, so dev builds warn
 * about it — a variant reaching this fallback either has no source PNG
 * under `assets/mascot/` or predates the last `just asset-mascot-data`
 * run.
 */
export function getMascotTrim(variant: string): MascotTrim {
  const trim = mascots[variant]?.trim;
  if (trim) return trim;

  if (import.meta.env.DEV && !warnedMissingTrim.has(variant)) {
    warnedMissingTrim.add(variant);
    console.warn(
      `Mascot "${variant}" has no trim data — falling back to zero trim, ` +
        `which misplaces it vertically. Run \`just asset-mascot-data\` and ` +
        `confirm \`assets/mascot/snappy-${variant}-1024.png\` exists.`,
    );
  }

  return DEFAULT_TRIM;
}

// =========================================================
// Variant Registry
// =========================================================

// All known Snappy mascot variant names, organized by theme.
// Each variant name corresponds to a set of image files in
// `public/images/mascot/snappy-{variant}-{size}.webp`.
//
// NSFW variants live alongside their SFW counterparts in the
// same group — the runtime filter handles exclusion.
export const Variants = {
  // The original Snappy — always available, always charming.
  Original: ["original"],

  // =========================================================
  // Torch-themed — the "home team" variants, on-brand with the
  // Torchsnap identity.
  // =========================================================

  TorchBearers: [
    "torch-raised",
    "torchlight",
    "cloak-and-torch",
    "cloak-and-lantern",
    "torchbearer",
    "torchbearer-backpack",
    "temporal-bureau-torch",
  ],

  // =========================================================
  // Explorers & Adventurers
  // =========================================================

  Explorers: [
    "explorer-cartographer",
    "lamp-and-staff-explorer",
    "rugged-explorer",
    "whip-and-torch-explorer",
    "whip-and-skull-torch",
    "relic-hunter-idol",
    "deep-sea-diver",
    "gold-miner",
    "sea-captain-lantern",
    "desert-face-wrapped-rider",
    "astronaut",
    "temporal-bureau",
    "pirate-nsfw",
    "magic-compass-captain",
  ],

  // =========================================================
  // Steampunk
  // =========================================================

  Steampunk: ["steampunk", "steampunk-aviator"],

  // =========================================================
  // Stone Gargoyles
  // =========================================================

  Gargoyles: [
    "stone-gargoyle",
    "stone-gargoyle-2",
    "stone-gargoyle-3",
    "stone-gargoyle-armored",
    "stone-gargoyle-battlement",
  ],

  // =========================================================
  // Detectives
  // =========================================================

  Detectives: ["clues-were-elementary", "clues-were-elementary-2", "damn-fine-coffee"],

  // =========================================================
  // Time Wanderers — inspired by a certain long-running
  // British sci-fi show about a traveller in a blue box.
  // =========================================================

  TimeWanderers: [
    "time-wanderer-top-hat",
    "time-wanderer-pocket-watch",
    "time-wanderer-mop-top",
    "time-wanderer-velvet-cape",
    "time-wanderer-striped-scarf",
    "time-wanderer-striped-scarf-2",
    "time-wanderer-striped-scarf-3",
    "time-wanderer-cricket-whites",
    "time-wanderer-celery-lapel",
    "time-wanderer-loud-coat",
    "time-wanderer-question-marks",
    "time-wanderer-question-marks-2",
    "time-wanderer-forgotten",
    "time-wanderer-waistcoat",
    "time-wanderer-leather-jacket",
    "time-wanderer-pinstripe",
    "time-wanderer-bowtie-duster",
    "time-wanderer-fez-and-bowtie",
    "time-wanderer-silver-sweep",
    "time-wanderer-silver-sweep-2",
    "time-wanderer-rainbow-stripe",
    "time-wanderer-rainbow-stripe-2",
    "time-wanderer-rainbow-stripe-3",
    "time-wanderer-rainbow-stripe-4",
    "time-wanderer-orange-coat",
  ],

  // =========================================================
  // Century Hoppers — inspired by a certain flux capacitor.
  // =========================================================

  CenturyHoppers: ["century-hopper-red-vest", "century-hopper-walkie", "century-hopper-walkman"],

  // =========================================================
  // Sci-Fi — starships, robots, cyborgs, and the far future.
  // =========================================================

  SciFi: [
    "battle-scarred-cyborg",
    "battle-scarred-cyborg-nsfw",
    "liquid-metal-blade-nsfw",
    "chrome-officer-visor",
    "chrome-officer-visor-nsfw",
    "iron-circuit",
    "chrome-centurion",
    "chrome-centurion-nsfw",
    "retro-chrome-centurion",
    "gold-pepper-pot",
    "retro-computer-head",
    "brave-little-astromech",
    "hooded-force-wanderer",
    "multipass-cabbie",
    "multipass-cabbie-nsfw",
    "multipass-supreme",
    "pinstripe-zf1-nsfw",
    "boldly-go-gold",
    "boldly-go-gold-nsfw",
    "engage-crimson",
    "engage-crimson-nsfw",
    "perfect-organism",
    "cosmic-surfer-silver",
    "cosmic-surfer-silver-2",
    "red-pill-blue-pill",
    "morpheus-pill-choice",
    "black-coat-shades",
    "black-coat-shades-2",
    "black-coat-shades-3",
    "proton-pack-buster",
    "proton-pack-buster-nsfw",
    "proton-pack-buster-2-nsfw",
    "proton-pack-buster-3-nsfw",
    "proton-pack-buster-4-nsfw",
    "desert-force-wielder-nsfw",
    "desert-force-wielder-2-nsfw",
    "dreadlock-hunter-nsfw",
    "dreadlock-hunter-2-nsfw",
    "triangle-ops-nsfw",
    "triangle-ops-2-nsfw",
    "towel-and-dressing-gown",
    "wormhole-gun-spring-boots",
  ],

  // =========================================================
  // Superheroes — capes, cowls, and gamma radiation.
  // =========================================================

  Superheroes: [
    "lightning-bolt-scarlet",
    "lightning-bolt-scarlet-2",
    "jade-giant-rage",
    "scarlet-merc",
    "scarlet-merc-nsfw",
    "automail-fire-alchemist",
    "horned-trickster",
    "horned-trickster-2",
    "adamantium-claw-berserker-nsfw",
    "infinity-gauntlet-titan-nsfw",
    "obsidian-panther",
    "shrinking-red-helmet",
    "winged-helm-hammer-nsfw",
    "scale-king-trident-nsfw",
    "red-horns-billies-nsfw",
  ],

  // =========================================================
  // Horror — things that go bump in the night.
  // =========================================================

  Horror: [
    "cape-and-fangs",
    "cape-and-fangs-2",
    "hockey-mask-slasher",
    "hockey-mask-slasher-nsfw",
    "striped-sweater-fedora",
    "striped-sweater-claw-nsfw",
    "remember-remember",
    "remember-remember-nsfw",
    "daywalker-blade-nsfw",
    "frilly-clown-balloon",
    "frilly-clown-balloon-nsfw",
    "zombie-brain-buffet-nsfw",
  ],

  // =========================================================
  // Fantasy — swords, sorcery, and pointed ears.
  // =========================================================

  Fantasy: [
    "fire-mage",
    "fire-wizard-star",
    "flame-paladin-nsfw",
    "forest-tunic-hero-nsfw",
    "hooded-blade-bearer-nsfw",
    "fur-barbarian-blade-nsfw",
    "owl-post-letter",
  ],

  // =========================================================
  // Realm Defenders — tournament fighters, elemental ninjas,
  // and champions of interdimensional combat.
  // =========================================================

  RealmDefenders: [
    "hellfire-specter",
    "hellfire-specter-2",
    "hellfire-specter-nsfw",
    "hellfire-specter-2-nsfw",
    "hellfire-specter-3-nsfw",
    "frozen-veil-ninja",
    "frozen-veil-ninja-nsfw",
    "venom-scale-lurker",
    "venom-scale-lurker-nsfw",
    "wide-hat-monk",
    "wide-hat-monk-2",
    "red-eye-enforcer",
    "red-eye-enforcer-nsfw",
    "iron-fist-brawler",
    "spec-ops-commander-nsfw",
    "soul-stealing-sorcerer",
    "azure-veil-princess",
    "azure-veil-princess-nsfw",
    "thunder-hat-elder",
    "green-glow-movie-star",
    "fire-fist-champion",
    "fire-fist-champion-nsfw",
  ],

  // =========================================================
  // Pop Culture — everything else that doesn't fit neatly
  // into the genre groups above.
  // =========================================================

  PopCulture: [
    "banana-goggle-minion",
    "yellow-chomping-ghost",
    "yellow-chomping-ghost-2",
    "pac-dome-suit",
    "pixel-gamer-king",
    "dashing-captain",
    "dashing-captain-nsfw",
    "daywalker-shades",
    "oversized-dark-helmet",
    "ninja-nsfw",
    "dino-kigurumi",
    "pork-pie-chemist",
    "umbrella-nanny",
    "aviator-flight-jacket",
    "cardboard-box-infiltrator",
  ],

  // =========================================================
  // Seasonal & Holiday
  // =========================================================

  Halloween: [
    "halloween-witch-lantern",
    "halloween-witch-pumpkins",
    "ghost-witch-hat-candy",
    "ghost-candy-corn-horn",
  ],
  // The Christmas-movie cosplays live here instead of in PopCulture so
  // they only surface during the Christmas window.
  Christmas: [
    "santa",
    "christmas-thief-green-fur",
    "christmas-thief-santa-sack",
    "nakatomi-ho-ho-ho",
    "nakatomi-ho-ho-ho-2",
    "nakatomi-ho-ho-ho-3",
  ],
  Easter: ["easter-bunny-flowers", "easter-bunny-nest", "easter-bunny-wreath"],
  NewYear: ["party"],
  FullMoon: ["werewolf", "werewolf-moon"],
};

// =========================================================
// Hero Selection Sets
// =========================================================

/**
 * Weighted mascot selection configuration for the launcher hero slot.
 *
 * All non-seasonal variants (including the original) are merged into
 * a single flat pool with uniform probability. Holiday and full-moon
 * variants are dormant (weight 0) until their condition activates,
 * at which point they inject themselves with high weight so the user
 * consistently sees seasonal mascots when the occasion calls for it.
 */
export const SnappyHeroSets: MascotEntry[] = [
  // All regular variants — one flat pool, equal probability.
  {
    variants: [
      ...Variants.Original,
      ...Variants.TorchBearers,
      ...Variants.Explorers,
      ...Variants.Steampunk,
      ...Variants.Gargoyles,
      ...Variants.Detectives,
      ...Variants.TimeWanderers,
      ...Variants.CenturyHoppers,
      ...Variants.SciFi,
      ...Variants.Superheroes,
      ...Variants.Horror,
      ...Variants.Fantasy,
      ...Variants.RealmDefenders,
      ...Variants.PopCulture,
    ],
    weight: 100,
  },

  // =========================================================
  // Holidays — dormant (weight 0) until their condition fires,
  // then they inject themselves at root level with high weight
  // so the user consistently sees seasonal mascots when the
  // occasion calls for it.
  // =========================================================

  {
    variants: [...Variants.Halloween, ...Variants.Horror],
    weight: 0,
    condition: isHalloween,
    conditionalWeight: 35,
  },
  {
    variants: [...Variants.Christmas],
    weight: 0,
    condition: isChristmas,
    conditionalWeight: 70,
  },
  {
    variants: [...Variants.Easter],
    weight: 0,
    condition: isEaster,
    conditionalWeight: 35,
  },
  {
    variants: [...Variants.NewYear],
    weight: 0,
    condition: isNewYear,
    conditionalWeight: 1000,
  },

  // Full moon — very high boost when within one day of a full moon,
  // but only after dark so the werewolf doesn't show up at noon.
  {
    variants: [...Variants.FullMoon],
    weight: 0,
    condition: () => getFullMoonDistance() <= 1 && (isNighttime() || isTwilight()),
    conditionalWeight: 1000,
  },
];
