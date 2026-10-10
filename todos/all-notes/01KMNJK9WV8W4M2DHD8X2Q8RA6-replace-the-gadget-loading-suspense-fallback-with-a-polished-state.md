---
title: "Replace the gadget loading Suspense fallback with a polished state"
kind: feature
component: frontend
impact: low
tags: [ux]
---
# Replace the gadget loading Suspense fallback with a polished state

The Suspense fallback shown while a gadget's custom UI component lazy-loads is plain "Loading…" text. It should become a skeleton, shimmer or branded spinner that fits the launcher's visual language.

Priority is low. Gadget components are small and load almost instantly from the local bundle, so the fallback only becomes visible when a gadget is genuinely slow to load, such as a future WASM gadget.
