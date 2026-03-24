# Think about and plan further plugins

Brainstorm and evaluate additional plugin ideas beyond the initial
set. Consider what makes a launcher truly indispensable for daily
use.

## Plugin ideas to evaluate

- **File search**: Index and search files by name (like macOS
  Spotlight). Integration with `mdfind` on macOS, `locate`/`fd` on
  Linux, Windows Search on Windows.
- **Snippet manager**: Store and search text snippets, paste into
  active app. Synced across devices?
- **Emoji picker**: Search emoji by name/keyword, copy to clipboard.
- **Color picker/converter**: Convert between hex, rgb, hsl. Show
  color preview swatch.
- **Dictionary/thesaurus**: Word definitions and synonyms.
- **System commands**: Sleep, restart, shutdown, lock screen, empty
  trash.
- **SSH connections**: List and connect to saved SSH hosts from
  `~/.ssh/config`.
- **Docker/container management**: List running containers,
  start/stop/restart.
- **Git repositories**: Index local git repos, open in editor or
  terminal.
- **Password manager integration**: Search 1Password/Bitwarden
  entries, copy passwords (via CLI tools).
- **Timer/stopwatch**: Quick `timer 5m` to start a countdown.
- **Notes/scratch pad**: Quick capture for fleeting thoughts,
  searchable later.
- **Screen capture**: Screenshot region, window, or full screen.
  OCR on captured text?
- **AI assistant**: Send query to LLM, show response inline.
- **Music control**: Play/pause, skip, show now playing.
- **Calendar**: Show upcoming events, quick event creation.
- **Currency/unit converter**: `100 usd to eur`, `5kg to lbs`.
- **Process manager**: List processes by CPU/memory, kill processes.
- **Bookmarks**: Search browser bookmarks across Chrome/Firefox/Safari.

## Evaluation criteria

- How frequently would a user reach for this?
- Can it provide results fast enough (<100ms)?
- Does it need persistent background state?
- Platform-specific complexity?
- Privacy/security implications?
- Does it overlap with an OS-level feature?
