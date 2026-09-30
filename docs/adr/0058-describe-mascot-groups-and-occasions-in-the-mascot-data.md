# 58. Describe mascot groups and occasions in the mascot data

Date: 2026-09-30

## Status

Accepted

Amends [57. Deliver the mascot set from the torchsnap-mascot repository](0057-deliver-the-mascot-set-from-the-torchsnap-mascot-repository.md)

## Context

ADR 57 builds the selection from each mascot's `group`. The names of
the seasonal groups and their conditions are written into
`src/mascot/variants.ts` (`SEASONS`), and Horror joins the Halloween
boost through a rule in the same file (`EXTRA_GROUPS_IN_SEASON`).

A seasonal group's mascots show only on their occasion. Some of them
only relate to it, such as a cartoon rabbit in Easter or a moon-themed
hero in FullMoon, and were seen a few days a year at most.

The set was regrouped in torchsnap-mascot on 2026-09-30 into 18 groups
that show all year and 5 seasonal ones. Group settings are planned
(todo "Revisit mascot selection weighting and group settings"), and they
need a name and a short description per group.

## Decision

torchsnap-mascot keeps the groups in `groups.json`: per group id a
`label`, a `description` for the settings, and `seasonal`. Its export
writes the file to `src/derived/mascot-groups.json`, next to
`src/derived/mascots.json`.

A seasonal group holds the mascots that are the occasion itself (a
pumpkin, Santa, an Easter bunny). They show only on their occasion, as
before.

A mascot of any group may name seasonal groups in `occasions`. It shows
all year in its own group and also joins the boost of each occasion it
names. A seasonal mascot may name another occasion. The Horror mascots
name Halloween this way, which replaces `EXTRA_GROUPS_IN_SEASON`.

torchsnap takes the list of seasonal groups from the data. Code keeps
only what the data cannot express: each occasion's condition (the date
windows, Easter, the full moon at night) and its boost weight, both
unchanged. A test fails when a seasonal group of the data has no
condition, or a condition has no seasonal group.

Once group settings exist, a mascot joins an occasion only while its own
group is enabled.

## Consequences

Which occasion a mascot joins is decided in torchsnap-mascot and
arrives with the export. A new group that shows all year still needs no
code change; a new seasonal group needs its condition in code, and the
test points at it.

A mascot with `occasions` sits in the regular pool and in each of its
occasions' entries, so during an occasion it is drawn more often than
the other mascots of its group.

The settings can show each group's label and description from the
exported data.
