# Plugin: Contact search and display

Search system contacts and display contact details in the launcher.

## Scope

- Index contacts from the system address book
- Fuzzy search by name, email, phone, company
- Display contact card with photo, name, key details
- Actions: copy email, copy phone, compose email, call (if
  supported)

## Platform considerations

- **macOS**: Contacts.framework (requires permission prompt)
- **Linux**: Evolution Data Server, or CardDAV
- **Windows**: Windows.ApplicationModel.Contacts API

## Privacy

- Requires explicit user permission to access contacts
- Contact data should never leave the local machine
- Index only — no copying of contact data to plugin storage

## Architecture decisions (2026-04-08 discussion)

Plugins are WASM-sandboxed, so the plugin itself never touches the OS.
The Torchsnap **host** must own contact access and expose a normalized
capability to plugins. The contacts plugin then becomes a thin
consumer of that capability — same as how other system-integrated
plugins work.

Proposed host capability surface (to be refined):

- `contacts.list() -> Vec<Contact>` — cached, cheap after first call
- `contacts.search(query, limit) -> Vec<ScoredContact>` — fuzzy, host-side
- `contacts.subscribe_changes()` — later, for live invalidation

Search lives in the **host**, not the plugin: the host owns the
normalized index, fuzzy ranking should not be reimplemented per
plugin, and contact lists are small enough (hundreds–low thousands)
that fetch-all + in-memory index is the right strategy on every
platform.

### macOS access option survey

| Option | Verdict |
|---|---|
| **Contacts.framework / `CNContactStore`** via `objc2-contacts` | **Chosen.** Official API, handles iCloud/Exchange/local sources transparently, clean TCC permission model (`NSContactsUsageDescription` + first-call prompt), `CNContactStoreDidChange` Darwin notification for cache invalidation. Bindings are verbose but workable. |
| AppleScript / `osascript` bridge | Rejected. Slow (hundreds of ms per query), brittle, requires Contacts.app, separate Automation TCC bucket. Not viable for launcher-style interactive search. |
| Direct SQLite read of `AddressBook-v22.abcddb` | Rejected. Requires Full Disk Access on the host (much worse UX than the contacts prompt), undocumented schema Apple has changed before, misses unsynced iCloud data. Trades a small permission for a scary one. |
| CardDAV directly | Rejected as primary. Per-account auth, doesn't reflect what the user sees in Contacts.app. Only sensible as a fallback on platforms with no native API. |

Apple's built-in predicates (`predicateForContactsMatchingName:` etc.)
are prefix/substring on name tokens only — no fuzzy matching. We
fetch-all once, build our own index, and rank ourselves. That's the
value-add over Contacts.app's own search.

### Cross-platform abstraction crate research (2026-04-08)

Researched whether any Rust crate already abstracts native contact
store access across macOS/Linux/Windows so we don't have to write
three backends ourselves.

**Verdict: no viable abstraction crate exists.** We are the first.

Candidates evaluated:

- **`objc2-contacts`** — raw `Contacts.framework` bindings, macOS/iOS
  only. The ceiling of the macOS Rust ecosystem; nothing wraps it.
- **Pimalaya `io-addressbook` / `cardamum`** — sounds right but isn't.
  Targets CardDAV servers and local vCard directories, not the OS
  contact store. Wrong layer.
- **`mates` / `mates-rs`** — CLI tools over local vCard directories.
  File-based, no OS integration.
- **`vcard`, `ical-rs`, `calcard`** — vCard parsers only.
- **Tauri plugins** — no desktop contacts plugin exists. Mobile-only
  community plugins.
- **`windows` crate** — has `Windows::ApplicationModel::Contacts`
  WinRT bindings, but raw, same ergonomics problem as
  `objc2-contacts` on macOS.
- **Linux**: no maintained Rust binding to Evolution Data Server or
  libfolks. Would mean talking D-Bus directly via `zbus`.
- **`google-people1`, `rscontacts`** — REST clients for Google
  Contacts, not local stores.

### Implementation approach

Build a small in-tree abstraction inside the host:

1. Define a `ContactsBackend` trait + normalized `Contact` struct
   (name parts, emails, phones in E.164, org, photo handle).
2. Implement `MacOsBackend` against `objc2-contacts` /
   `CNContactStore`.
3. Defer Linux (`zbus` → EDS) and Windows (`windows` crate →
   `ContactStore` WinRT) backends until those platforms ship.
4. The trait shape must be designed up-front so the deferred backends
   don't force a redesign. Validate the design against EDS and WinRT
   data models on paper before locking it in.
5. Host-side fuzzy index built on first capability use, invalidated
   on `CNContactStoreDidChange`.
6. Expose host capability to WASM plugins via the existing plugin
   host API surface.

Open questions for the design discussion:

- Photo data: pass through as bytes, file path, or opaque handle the
  plugin resolves later? Bytes are simplest but blow up the
  serialization cost of `list()`.
- Should `search` accept structured filters (only emails, only
  phones) or just a free-text query? Launcher use case is free-text;
  a directory-style "find everyone at company X" use case wants
  structure.
- Permission UX: do we prompt on first plugin install, on first
  search, or expose a manual "grant access" affordance in settings?
- Whether the contacts capability should be opt-in per plugin
  (capability grant in manifest) — almost certainly yes, given the
  privacy sensitivity.

### Tests and docs (required before merge)

- Unit tests for the fuzzy index and ranking with synthetic contact
  fixtures (no OS access needed).
- Integration test path on macOS that exercises the real
  `CNContactStore` against a known test contact — gated behind a
  feature flag or env var so CI doesn't need TCC grants.
- Edge cases: contacts with no name, multiple emails/phones,
  non-ASCII names, duplicate entries across sources, very large
  contact lists.
- Doc updates: plugin SDK docs for the new capability, host
  architecture doc for the backend trait, README note about the
  macOS permission prompt.
