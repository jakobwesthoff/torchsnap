# Todo Order

Structured priority list. Items within a phase can be parallelized,
but phases should be tackled roughly in order. Items marked with
`[discuss]` need design decisions before implementation.

---

## Phase 1: First internal plugins (build before abstracting)

Build 2–3 plugins as internal features to discover the real API
surface. These inform all later plugin architecture decisions.

1. [plugin-app-launcher](01kmh0dspar1gga0ahh1pyp34z-plugin-app-launcher.md)
2. [plugin-calculator](01kmh0dspar1gga0ahh1pyp350-plugin-calculator.md)
3. [plugin-system-commands](01kmh0n9gmxjbt0xxy56h0ajh4-plugin-system-commands.md)
4. [plugin-open-url](01kmh0dspar1gga0ahh1pyp353-plugin-open-url.md)
5. [plugin-web-search](01kmh0dspar1gga0ahh1pyp354-plugin-web-search.md)
6. [result-ranking-system](01kmh1ah0j1c2cx7pa39rmp7jz-result-ranking-system.md)
7. [accessibility](01kmh2c7pem81px3twgqhsz4tj-accessibility.md) *(incremental — add ARIA roles as result list is built)*

## Phase 2: Infrastructure and polish

Core systems that support everything else.

8. [keybind-system](01kmh12n6r0mq94rwav32eczdn-keybind-system.md)
9. [switch-to-nucleo-async-worker](01kmje4d33dgmnrz4xsg3b2j95-switch-to-nucleo-async-worker.md)
10. [github-actions-ci](01kmh1wkmsrenpk98dbkhcq3cc-github-actions-ci.md)
11. [i18n-system](01kmh1ah0j1c2cx7pa39rmp7k1-i18n-system.md)
12. [create-dedicated-tray-icon](01kmgymgjvkr1e6egscdwctf2m-create-dedicated-tray-icon.md)
13. [random-mascot-variants](01kmj9zbb6ysgtgpvfjmace188-random-mascot-variants.md)
14. [visual-identity-brainstorm](01kmh1h2b95jvtfk76hwyd80kd-visual-identity-brainstorm.md) `[discuss]`
15. [design-logo-and-icons](01kmh1h2b95jvtfk76hwyd80ke-design-logo-and-icons.md)
16. [onboarding-first-run](01kmh2c7pem81px3twgqhsz4th-onboarding-first-run.md)

## Phase 3: Plugin system extraction

Extract the plugin API from the patterns established in Phase 1.

17. [decide-plugin-architecture](01kmh0ttdtq583cqcjqcsgmyys-decide-plugin-architecture.md) `[discuss]`
18. [extract-shared-component-library](01kmh0dspar1gga0ahh1pyp34x-extract-shared-component-library.md)
19. [plugin-system-wasm](01kmh0dspar1gga0ahh1pyp34y-plugin-system-wasm.md)
20. [user-provided-themes](01kmh0jb5qrkkq7zzymwh50419-user-provided-themes.md) `[discuss]`

## Phase 4: Cross-platform

Make the launcher work properly on Linux and Windows.

21. [linux-launcher-display](01kmh12n6qn7n3mte2abqp9zq9-linux-launcher-display.md)
22. [windows-launcher-display](01kmh12n6r0mq94rwav32eczdm-windows-launcher-display.md)

## Phase 5: Additional plugins

Build out the plugin ecosystem once the API is stable.

23. [plugin-clipboard-manager](01kmh0dspar1gga0ahh1pyp351-plugin-clipboard-manager.md)
24. [plugin-file-search](01kmh0n9gmxjbt0xxy56h0ajh3-plugin-file-search.md)
25. [plugin-emoji-picker](01kmh0n9gmxjbt0xxy56h0ajh5-plugin-emoji-picker.md)
26. [plugin-task-switcher](01kmh0dspar1gga0ahh1pyp355-plugin-task-switcher.md)
27. [plugin-contact-search](01kmh0dspar1gga0ahh1pyp352-plugin-contact-search.md)
28. [plan-further-plugins](01kmh0dspar1gga0ahh1pyp356-plan-further-plugins.md) `[discuss]`

## Phase 6: Distribution and long-term

29. [auto-updater](01kmh1ah0j1c2cx7pa39rmp7k0-auto-updater.md)
30. [data-export-import](01kmh1wkmsrenpk98dbkhcq3cb-data-export-import.md)

## Reference (decided, not actionable)

Design decisions documented in ADRs. These todo files are kept for
their "still open" sections as future reference.

- [decide-result-display-model](01kmh0ttdtq583cqcjqcsgmyyt-decide-result-display-model.md) → ADR 0008
- [decide-action-model](01kmh0ttdtq583cqcjqcsgmyyv-decide-action-model.md) → ADR 0009
- [decide-general-ux-concepts](01kmh0ttdtq583cqcjqcsgmyyw-decide-general-ux-concepts.md) → ADR 0010
- [fuzzy-matching-library](01kmh1ah0j1c2cx7pa39rmp7jy-fuzzy-matching-library.md) → ADR 0011
- [query-prefix-conflict-resolution](01kmj7jdvts1mqa4wcd4mr4qq8-query-prefix-conflict-resolution.md)
- [pinning-and-favorites](01kmj8z6yx1wakt5qq4pwcbq79-pinning-and-favorites.md)
- [remove-squirly-nutty-references](01kmjm824r7rxndjty9gcs50hc-remove-squirly-nutty-references.md)
