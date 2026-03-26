# Footer custom content beyond key hints

ADR 0013 (topic 7) uses a generic `FooterState` model with a primary
hint and a list of secondary hints (each being a key combo + label).

If a future plugin needs footer content that doesn't map to this model
(e.g., mode indicators, progress bars, contextual text without
keybindings), the `FooterState` type and footer component will need
to be extended.

Revisit when a concrete plugin requires this.
