# Todo Order

Structured priority list. Items within a phase can be parallelized,
but phases should be tackled roughly in order. Items marked with
`[discuss]` need design decisions before implementation.

---

## Phase 1: Make the launcher actually usable

The current launcher is a shell — these make it functional.

1. [decide-result-display-model](01kmh0ttdtq583cqcjqcsgmyyt-decide-result-display-model.md) `[discuss]`
2. [decide-action-model](01kmh0ttdtq583cqcjqcsgmyyv-decide-action-model.md) `[discuss]`
3. [decide-general-ux-concepts](01kmh0ttdtq583cqcjqcsgmyyw-decide-general-ux-concepts.md) `[discuss]`
4. [add-default-command-entries](01kmh0dspar1gga0ahh1pyp34s-add-default-command-entries.md)
5. [light-dark-mode-switching](01kmh0dspar1gga0ahh1pyp34v-light-dark-mode-switching.md)
6. [settings-theme-toggle](01kmh0dspar1gga0ahh1pyp34w-settings-theme-toggle.md)
7. [fuzzy-matching-library](01kmh1ah0j1c2cx7pa39rmp7jy-fuzzy-matching-library.md)

## Phase 2: First internal plugins (build before abstracting)

Build 2–3 plugins as internal features to discover the real API
surface. These inform all later plugin architecture decisions.

8. [plugin-app-launcher](01kmh0dspar1gga0ahh1pyp34z-plugin-app-launcher.md)
9. [plugin-calculator](01kmh0dspar1gga0ahh1pyp350-plugin-calculator.md)
10. [plugin-system-commands](01kmh0n9gmxjbt0xxy56h0ajh4-plugin-system-commands.md)
11. [plugin-open-url](01kmh0dspar1gga0ahh1pyp353-plugin-open-url.md)
12. [plugin-web-search](01kmh0dspar1gga0ahh1pyp354-plugin-web-search.md)
13. [result-ranking-system](01kmh1ah0j1c2cx7pa39rmp7jz-result-ranking-system.md)
14. [accessibility](01kmh2c7pem81px3twgqhsz4tj-accessibility.md) *(incremental — add ARIA roles as result list is built)*

## Phase 3: Infrastructure and polish

Core systems that support everything else.

15. [keybind-system](01kmh12n6r0mq94rwav32eczdn-keybind-system.md)
16. [github-actions-ci](01kmh1wkmsrenpk98dbkhcq3cc-github-actions-ci.md)
17. [i18n-system](01kmh1ah0j1c2cx7pa39rmp7k1-i18n-system.md)
18. [create-dedicated-tray-icon](01kmgymgjvkr1e6egscdwctf2m-create-dedicated-tray-icon.md)
19. [visual-identity-brainstorm](01kmh1h2b95jvtfk76hwyd80kd-visual-identity-brainstorm.md) `[discuss]`
20. [design-logo-and-icons](01kmh1h2b95jvtfk76hwyd80ke-design-logo-and-icons.md)
21. [onboarding-first-run](01kmh2c7pem81px3twgqhsz4th-onboarding-first-run.md)

## Phase 4: Plugin system extraction

Extract the plugin API from the patterns established in Phase 2.

22. [decide-plugin-architecture](01kmh0ttdtq583cqcjqcsgmyys-decide-plugin-architecture.md) `[discuss]`
23. [extract-shared-component-library](01kmh0dspar1gga0ahh1pyp34x-extract-shared-component-library.md)
24. [plugin-system-wasm](01kmh0dspar1gga0ahh1pyp34y-plugin-system-wasm.md)
25. [user-provided-themes](01kmh0jb5qrkkq7zzymwh50419-user-provided-themes.md) `[discuss]`

## Phase 5: Cross-platform

Make the launcher work properly on Linux and Windows.

26. [linux-launcher-display](01kmh12n6qn7n3mte2abqp9zq9-linux-launcher-display.md)
27. [windows-launcher-display](01kmh12n6r0mq94rwav32eczdm-windows-launcher-display.md)

## Phase 6: Additional plugins

Build out the plugin ecosystem once the API is stable.

28. [plugin-clipboard-manager](01kmh0dspar1gga0ahh1pyp351-plugin-clipboard-manager.md)
29. [plugin-file-search](01kmh0n9gmxjbt0xxy56h0ajh3-plugin-file-search.md)
30. [plugin-emoji-picker](01kmh0n9gmxjbt0xxy56h0ajh5-plugin-emoji-picker.md)
31. [plugin-task-switcher](01kmh0dspar1gga0ahh1pyp355-plugin-task-switcher.md)
32. [plugin-contact-search](01kmh0dspar1gga0ahh1pyp352-plugin-contact-search.md)
33. [plan-further-plugins](01kmh0dspar1gga0ahh1pyp356-plan-further-plugins.md) `[discuss]`

## Phase 7: Distribution and long-term

34. [auto-updater](01kmh1ah0j1c2cx7pa39rmp7k0-auto-updater.md)
35. [data-export-import](01kmh1wkmsrenpk98dbkhcq3cb-data-export-import.md)
