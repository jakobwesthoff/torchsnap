# Dynamic plugin component registration

Currently, the frontend uses a static map to resolve a plugin ID to its
React component (e.g., `"emoji-picker" → EmojiGrid`). Adding a new
plugin with custom UI requires touching this map.

This should be replaced with a dynamic registration mechanism once
plugin definitions are loaded from plugin files. The resolution of
plugin ID → React component should come from the plugin's own
definition, not a hardcoded host-side map.

Blocked on: plugin file format and dynamic loading design (Phase 3).

Related: ADR 0013 topic 6 (Shared Component Library / SDK Surface).
