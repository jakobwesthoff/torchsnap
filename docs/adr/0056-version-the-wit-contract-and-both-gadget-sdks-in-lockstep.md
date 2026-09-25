# 56. Version the WIT contract and both gadget SDKs in lockstep

Date: 2026-09-25

## Status

Accepted

Amends [55. Dispatch entry actions through gadget commands in fixed slots](0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md) (the SDKs get the WIT's new version too)

## Context

Three packages make up the gadget API, each with its own version:

- the WIT package `torchsnap:gadget`, at `0.1.0`,
- the Rust SDK crate `torchsnap-gadget-sdk`, at `0.1.0`,
- the TypeScript SDK package `@torchsnap/gadget-sdk`, at `0.0.1`.

Both SDKs are unpublished (`publish = false`, `"private": true`) and
live in this repository. ADR 55 changes the WIT incompatibly and bumps
it to `0.2.0`, and it changes both SDKs with it.

## Decision

The WIT package and both SDKs share one version number. With ADR 55,
all three become `0.2.0`.

Whenever any of the three changes incompatibly, all three move to the
same new version.

## Consequences

A breaking change to one of the three, for example only to the
TypeScript SDK's hooks, also bumps the version of the other two.
