# Todo Order

Structured priority list. Items within a phase can be parallelized,
but phases should be tackled roughly in order. Items marked with
`[discuss]` need design decisions before implementation.

---

## Phase 1: Make the launcher actually usable

The current launcher is a shell — these make it functional.

1. [handle-esc-press](01kmh0dspar1gga0ahh1pyp34r-handle-esc-press.md)
2. [key-press-handling-generalization](01kmh0dspar1gga0ahh1pyp34q-key-press-handling-generalization.md)
3. [transfer-emacs-keybindings](01kmh0dspar1gga0ahh1pyp34t-transfer-emacs-keybindings.md)
4. [decide-result-display-model](01kmh0ttdtq583cqcjqcsgmyyt-decide-result-display-model.md) `[discuss]`
5. [decide-action-model](01kmh0ttdtq583cqcjqcsgmyyv-decide-action-model.md) `[discuss]`
6. [decide-general-ux-concepts](01kmh0ttdtq583cqcjqcsgmyyw-decide-general-ux-concepts.md) `[discuss]`
7. [add-default-command-entries](01kmh0dspar1gga0ahh1pyp34s-add-default-command-entries.md)
8. [light-dark-mode-switching](01kmh0dspar1gga0ahh1pyp34v-light-dark-mode-switching.md)
9. [settings-theme-toggle](01kmh0dspar1gga0ahh1pyp34w-settings-theme-toggle.md)
10. [fuzzy-matching-library](01kmh1ah0j1c2cx7pa39rmp7jy-fuzzy-matching-library.md)

## Phase 2: First internal plugins (build before abstracting)

Build 2–3 plugins as internal features to discover the real API
surface. These inform all later plugin architecture decisions.

11. [plugin-app-launcher](01kmh0dspar1gga0ahh1pyp34z-plugin-app-launcher.md)
12. [plugin-calculator](01kmh0dspar1gga0ahh1pyp350-plugin-calculator.md)
13. [plugin-system-commands](01kmh0n9gmxjbt0xxy56h0ajh4-plugin-system-commands.md)
14. [plugin-open-url](01kmh0dspar1gga0ahh1pyp353-plugin-open-url.md)
15. [plugin-web-search](01kmh0dspar1gga0ahh1pyp354-plugin-web-search.md)
16. [result-ranking-system](01kmh1ah0j1c2cx7pa39rmp7jz-result-ranking-system.md)
17. [accessibility](01kmh2c7pem81px3twgqhsz4tj-accessibility.md) *(incremental — add ARIA roles as result list is built)*

## Phase 3: Infrastructure and polish

Core systems that support everything else.

18. [keybind-system](01kmh12n6r0mq94rwav32eczdn-keybind-system.md)
19. [github-actions-ci](01kmh1wkmsrenpk98dbkhcq3cc-github-actions-ci.md)
20. [i18n-system](01kmh1ah0j1c2cx7pa39rmp7k1-i18n-system.md)
21. [create-dedicated-tray-icon](01kmgymgjvkr1e6egscdwctf2m-create-dedicated-tray-icon.md)
22. [visual-identity-brainstorm](01kmh1h2b95jvtfk76hwyd80kd-visual-identity-brainstorm.md) `[discuss]`
23. [design-logo-and-icons](01kmh1h2b95jvtfk76hwyd80ke-design-logo-and-icons.md)
24. [onboarding-first-run](01kmh2c7pem81px3twgqhsz4th-onboarding-first-run.md)

## Phase 4: Plugin system extraction

Extract the plugin API from the patterns established in Phase 2.

25. [decide-plugin-architecture](01kmh0ttdtq583cqcjqcsgmyys-decide-plugin-architecture.md) `[discuss]`
26. [extract-shared-component-library](01kmh0dspar1gga0ahh1pyp34x-extract-shared-component-library.md)
27. [plugin-system-wasm](01kmh0dspar1gga0ahh1pyp34y-plugin-system-wasm.md)
28. [user-provided-themes](01kmh0jb5qrkkq7zzymwh50419-user-provided-themes.md) `[discuss]`

## Phase 5: Cross-platform

Make the launcher work properly on Linux and Windows.

29. [linux-launcher-display](01kmh12n6qn7n3mte2abqp9zq9-linux-launcher-display.md)
30. [windows-launcher-display](01kmh12n6r0mq94rwav32eczdm-windows-launcher-display.md)

## Phase 6: Additional plugins

Build out the plugin ecosystem once the API is stable.

31. [plugin-clipboard-manager](01kmh0dspar1gga0ahh1pyp351-plugin-clipboard-manager.md)
32. [plugin-file-search](01kmh0n9gmxjbt0xxy56h0ajh3-plugin-file-search.md)
33. [plugin-emoji-picker](01kmh0n9gmxjbt0xxy56h0ajh5-plugin-emoji-picker.md)
34. [plugin-task-switcher](01kmh0dspar1gga0ahh1pyp355-plugin-task-switcher.md)
35. [plugin-contact-search](01kmh0dspar1gga0ahh1pyp352-plugin-contact-search.md)
36. [plan-further-plugins](01kmh0dspar1gga0ahh1pyp356-plan-further-plugins.md) `[discuss]`

## Phase 7: Distribution and long-term

37. [auto-updater](01kmh1ah0j1c2cx7pa39rmp7k0-auto-updater.md)
38. [data-export-import](01kmh1wkmsrenpk98dbkhcq3cb-data-export-import.md)
