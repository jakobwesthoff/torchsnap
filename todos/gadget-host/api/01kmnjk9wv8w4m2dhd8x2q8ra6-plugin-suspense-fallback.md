# Gadget loading suspense fallback design

The Suspense fallback when lazy-loading a gadget custom UI component
is currently a plain "Loading…" text. This should be replaced with a
more polished loading state — a skeleton, shimmer, or branded
spinner that fits the launcher's visual language.

Low priority — gadget components are small and load near-instantly
from the local bundle. Only becomes visible if a gadget is
genuinely slow to load (e.g., future WASM gadgets).
