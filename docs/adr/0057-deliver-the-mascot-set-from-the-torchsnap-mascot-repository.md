# 57. Deliver the mascot set from the torchsnap-mascot repository

Date: 2026-09-29

## Status

Accepted

## Context

The Snappy mascots were made in this repository: 1024 px source PNGs in
`assets/mascot/`, their alt texts and NSFW flags in
`assets/mascot/mascots.json`, the variant lists and groups in
`src/mascotVariants.ts`, and tools to size, cut and check the images in
`tools/`. `just asset-mascots` built the webp files and
`just asset-mascot-data` wrote `src/derived/mascots.json` with trim
values from the alpha channel. The launcher placed each mascot by the
bottom and left edge of its visible pixels.

The set grew from 189 to 662 mascots in a separate project, which also
reviewed every alt text, the size of the owls and where each mascot
sits on the launcher card. Its results live in two repositories: the
public `torchsnap-mascot` (masters, webp files, the per-mascot data)
and the private `torchsnap-mascot-base` (base images, generation
records, the tools that make a master).

## Decision

The mascot set is made and changed in `torchsnap-mascot`. Its export
(`just export <torchsnap checkout>`) writes the webp files into
`public/images/mascot/` and the app's data into
`src/derived/mascots.json`. Both are tracked here.

Per mascot the data holds `alt`, `nsfw`, `group`, `groundAnchor` and
`boxLeft`, nothing else.

`src/mascotVariants.ts` builds the selection sets from each mascot's
`group`: the seasonal groups (Halloween, Christmas, Easter, NewYear,
FullMoon) keep their conditions and weights, Horror joins the Halloween
boost as before, and every other group joins the regular pool.

The launcher places a mascot by its `groundAnchor`
(`src/launcher/placement.ts`): `y` on the feet line in both modes, `x`
on the card centre in centre mode and 48 px from the card's right edge
in sidekick mode. A sidekick whose mirrored visible figure would pass
the card's right edge moves left until that edge (`boxLeft`) sits on
the card edge. The Escape pill is centred under the sidekick's anchor.

The source PNGs, `assets/mascot/mascots.json`, the mascot docs and
tools, and the two asset recipes are removed.
`assets/mascot/snappy-original-1024.png` stays as the app icon source;
it is a copy of `torchsnap-mascot`'s `masters/original.png`.

## Consequences

Adding or changing a mascot happens in `torchsnap-mascot` and reaches
this repository through the export. Hand edits to the webp files or to
`src/derived/mascots.json` here are overwritten by the next export.

A fresh checkout builds without either mascot repository, and the
build no longer needs `cwebp`.

A group that the code does not know joins the regular pool without a
code change. A new seasonal group needs an entry in `SEASONS` in
`src/mascotVariants.ts`.
