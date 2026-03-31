# Todo Order

Structured priority list. Items within a phase can be parallelized,
but phases should be tackled roughly in order. Items marked with
`[discuss]` need design decisions before implementation.

---

## Phase 1: First internal plugins (build before abstracting)

Build 2–3 plugins as internal features to discover the real API
surface. These inform all later plugin architecture decisions.

- [plugin-open-url](01kmh0dspar1gga0ahh1pyp353-plugin-open-url.md)
- [plugin-web-search](01kmh0dspar1gga0ahh1pyp354-plugin-web-search.md)
- [accessibility](01kmh2c7pem81px3twgqhsz4tj-accessibility.md) *(incremental — add ARIA roles as result list is built)*

## Phase 2: Infrastructure and polish

Core systems that support everything else.

- [keybind-system](01kmh12n6r0mq94rwav32eczdn-keybind-system.md)
- [switch-to-nucleo-async-worker](01kmje4d33dgmnrz4xsg3b2j95-switch-to-nucleo-async-worker.md)
- [github-actions-ci](01kmh1wkmsrenpk98dbkhcq3cc-github-actions-ci.md)
- [i18n-system](01kmh1ah0j1c2cx7pa39rmp7k1-i18n-system.md)
- [create-dedicated-tray-icon](01kmgymgjvkr1e6egscdwctf2m-create-dedicated-tray-icon.md)
- [react-perf-audit](01kmnmb0v2rm892d51qj1j5zp0-react-perf-audit.md)
- [extract-reusable-virtual-scroll-hook](01kmpmt7j2p015zgy7mgjm7x38-extract-reusable-virtual-scroll-hook.md)
- [rename-search-module](01kmpc7et0w3erf8qysgcc25cj-rename-search-module.md)
- [plugin-suspense-fallback](01kmnjk9wv8w4m2dhd8x2q8ra6-plugin-suspense-fallback.md)
- [random-mascot-variants](01kmj9zbb6ysgtgpvfjmace188-random-mascot-variants.md)
- [visual-identity-brainstorm](01kmh1h2b95jvtfk76hwyd80kd-visual-identity-brainstorm.md) `[discuss]`
- [design-logo-and-icons](01kmh1h2b95jvtfk76hwyd80ke-design-logo-and-icons.md)
- [app-discovery-direct-api](01kmjpbr2saehjhhgkh3jdsgna-app-discovery-direct-api.md) `[discuss]`
- [frontend-memory-leak-cleanup](01kmtfq0erkn5vxqnzka4jz2ce-frontend-memory-leak-cleanup.md)
- [frontend-memleak-audit-followup](01kmtn27vgjzndwwgan03pwzyp-frontend-memleak-audit-followup.md)
- [investigate-double-hide-on-dismiss](01kmpjrsretm2sdvm71yknd2k3-investigate-double-hide-on-dismiss.md)
- [frontend-backend-error-handling](01kmpk3scmw3h8vpptdfjnp2bv-frontend-backend-error-handling.md)
- [onboarding-first-run](01kmh2c7pem81px3twgqhsz4th-onboarding-first-run.md)

## Phase 3: Plugin system extraction

Extract the plugin API from the patterns established in Phase 1.

- [decide-plugin-architecture](01kmh0ttdtq583cqcjqcsgmyys-decide-plugin-architecture.md) `[discuss]`
- [dynamic-plugin-component-registration](01kmnh1g0dy48yp93s3x7qxr8v-dynamic-plugin-component-registration.md)
- [execute-triggered-custom-ui](01kmnh3rx87jzknw7cfrdh0pfs-execute-triggered-custom-ui.md)
- [extract-shared-component-library](01kmh0dspar1gga0ahh1pyp34x-extract-shared-component-library.md)
- [plugin-system-wasm](01kmh0dspar1gga0ahh1pyp34y-plugin-system-wasm.md)
- [user-provided-themes](01kmh0jb5qrkkq7zzymwh50419-user-provided-themes.md) `[discuss]`

## Phase 4: Cross-platform

Make the launcher work properly on Linux and Windows.

- [linux-launcher-display](01kmh12n6qn7n3mte2abqp9zq9-linux-launcher-display.md)
- [windows-launcher-display](01kmh12n6r0mq94rwav32eczdm-windows-launcher-display.md)

## Phase 5: Additional plugins

Build out the plugin ecosystem once the API is stable.

- [plugin-clipboard-manager](01kmh0dspar1gga0ahh1pyp351-plugin-clipboard-manager.md)
- [clipboard-preview-syntax-highlighting](01kmppw7hbbv82591mcspd5ph6-clipboard-preview-syntax-highlighting.md) *(after plugin-clipboard-manager)* **← next**
- [execute-triggered-custom-ui-state-snapshot](01kmpdcmj1w94gtcnk8vwn8t4s-execute-triggered-custom-ui-state-snapshot.md) *(after plugin-clipboard-manager)*
- [clipboard-pinned-entries](01kmpamgxr089fmyx43e4gwdam-clipboard-pinned-entries.md) *(after plugin-clipboard-manager)*
- [clipboard-source-app-identification](01kmp9wsa756avh7nc72vfs0ez-clipboard-source-app-identification.md) *(after plugin-clipboard-manager)*
- [clipboard-load-image-from-copied-file](01kmphkx6n34kaqjq4cdsapkpw-clipboard-load-image-from-copied-file.md) *(after plugin-clipboard-manager)*
- [plugin-file-search](01kmh0n9gmxjbt0xxy56h0ajh3-plugin-file-search.md)
- [plugin-task-switcher](01kmh0dspar1gga0ahh1pyp355-plugin-task-switcher.md)
- [plugin-contact-search](01kmh0dspar1gga0ahh1pyp352-plugin-contact-search.md)
- [plan-further-plugins](01kmh0dspar1gga0ahh1pyp356-plan-further-plugins.md) `[discuss]`

## Phase 6: Distribution and long-term

- [auto-updater](01kmh1ah0j1c2cx7pa39rmp7k0-auto-updater.md)
- [data-export-import](01kmh1wkmsrenpk98dbkhcq3cb-data-export-import.md)

## Reference (decided, not actionable)

Design decisions documented in ADRs. These todo files are kept for
their "still open" sections as future reference.

- [decide-result-display-model](01kmh0ttdtq583cqcjqcsgmyyt-decide-result-display-model.md) → ADR 0008
- [decide-action-model](01kmh0ttdtq583cqcjqcsgmyyv-decide-action-model.md) → ADR 0009
- [decide-general-ux-concepts](01kmh0ttdtq583cqcjqcsgmyyw-decide-general-ux-concepts.md) → ADR 0010
- [query-prefix-conflict-resolution](01kmj7jdvts1mqa4wcd4mr4qq8-query-prefix-conflict-resolution.md)
- [pinning-and-favorites](01kmj8z6yx1wakt5qq4pwcbq79-pinning-and-favorites.md)
- [remove-squirly-nutty-references](01kmjm824r7rxndjty9gcs50hc-remove-squirly-nutty-references.md)
