# Native Keep-Awake: Replacing Amphetamine

What Torchsnap needs to keep the Mac awake by itself instead of driving
Amphetamine, with the later Windows and Linux ports and an optional
mouse-jiggle feature in mind. Ends with a split of the work into
compartments and an evaluation of whether the awake gadget can stay a
WASM gadget, and which host API it would need.

Researched 2026-10-03 against Torchsnap `82df0c9` on macOS 27.0 (arm64).
Statements marked _(unverified)_ come from one source that could not be
checked further. The user's decisions on the open questions are in
section 9.

---

## 1. The gadget today

The awake gadget (`gadgets/awake/`) is a WASM gadget. It has no keep-awake
logic of its own. Everything goes through Amphetamine:

| Concern | How it works today |
|---|---|
| Start, stop, status | `osascript -e <AppleScript>` through the `command` host API, one regex argv rule in `manifest.toml` |
| Availability | `filesystem::file_exists("/Applications/Amphetamine.app/Contents/Info.plist")`, probed once in `enable()` |
| Timer | Amphetamine's. The gadget reads `session time remaining` and never counts down itself |
| Session state | Amphetamine's. The backends are stateless (`backend/mod.rs`: "all state lives in the backing tool") |
| Status freshness | Probe cached 15 s in a `RateLimitCache`, invalidated after every `execute` |
| Icon | `EntryIcon::AppIcon("com.if.Amphetamine")`, needs `icon-cache` |

The data model already separates the launcher from the mechanism.
`backend/mod.rs` defines a `KeepAwakeBackend` trait (`status`, `start`,
`stop`, `entry_icon`) and a `select_backend()` whose doc comment names a
native `caffeinate`, an IOKit assertion and a Windows power request as
future backends. The request is `SessionRequest { minutes: Option<u32>,
display_sleep_allowed: bool }`. The status distinguishes `Inactive`,
`AppNotRunning` and `Active { kind, display_sleep_allowed }`, with
`kind` one of `Infinite`, `Timed { remaining_secs }` or `External` (a
session Amphetamine started by itself through a Trigger).

Torchsnap has to take over two things Amphetamine does for the gadget now:
owning the timer and owning the session state.

---

## 2. How existing Mac tools do it

### 2.1 Amphetamine

Inspected read-only on Amphetamine 5.3.2. Imported symbols (`nm -u`):

- Sleep prevention: `IOPMAssertionCreateWithName`, `IOPMAssertionRelease`,
  `IOPMCopyAssertionsByProcess`.
- Cursor movement: `CGEventCreateMouseEvent`, `CGEventPost`,
  `CGEventSourceSecondsSinceLastEventType`, `AXIsProcessTrustedWithOptions`.

A running session showed up in `IOPMCopyAssertionsByProcess` as a
`PreventUserIdleSystemSleep` assertion named `"Amphetamine (Single-Use -
System)"`, without a `TimeoutSeconds` property. Amphetamine runs its own
timer and releases the assertion itself.

Strings in the binary show a mouse-movement feature with an interval, an
inactivity delay ("mouse SHOULD MOVE! secsSinceLastHID"), a smoothness
setting and a stop-after-inactivity option. It is disabled below
macOS 10.14.

Amphetamine features beyond what the gadget uses, from its AppleScript
dictionary (`sdef`): allowing or preventing the screen saver,
closed-display mode, Triggers, and Drive Alive. Closed-display mode (lid
closed, no external display) relies on the separate Amphetamine Enhancer;
the underlying switch is `sudo pmset -a disablesleep 1`, which needs root.

### 2.2 Vorssaint

Vorssaint (github.com/vorssaint/vorssaint-utils, read at `aa6ddcb`) is an
open-source menu bar toolkit with a keep-awake feature. It is GPL-3.0, so
it serves as a design reference only; no code can move into Torchsnap.
Nearly all of the feature lives in `KeepAwakeManager.swift`.

**Mechanism.** `IOPMAssertionCreateWithName` with
`PreventUserIdleSystemSleep`, plus `PreventUserIdleDisplaySleep` unless the
user allows display sleep. No OS timeout. The session end is a
`Timer(fire: Date)` on the main run loop, so it counts wall-clock time.
Sessions are not restored after a relaunch; only the last chosen duration
or end time is saved.

**Session options.** Durations of 15, 30, 60, 120, 240 and 480 minutes or
indefinite, an "until HH:MM" end (a time already passed today means
tomorrow), and extending a running session.

**Automatic sessions.** A session without an end starts while chosen
conditions hold:

- An external display is connected
  (`NSApplication.didChangeScreenParametersNotification`,
  `CGGetOnlineDisplayList` with `CGDisplayIsBuiltin`).
- The Mac is on power (`IOPSNotificationCreateRunLoopSource`).
- Selected apps are running (`NSWorkspace` launch and terminate
  notifications).

Conditions combine with "any" or "all". Stopping an automatic session by
hand suppresses it until the conditions clear. A timed session that ends
while the conditions hold turns into an automatic one.

**Safeguards.**

- While the screen is locked (`com.apple.screenIsLocked` distributed
  notification), the session releases its assertions and stops the jiggle.
  On unlock it re-applies them, or ends if the timer ran out meanwhile.
- Battery limit (0, 5, 10, 15 or 20 %, default 10): a 30 s check ends the
  session on battery at or below the limit, and automatic sessions do not
  start below it.
- A notification is posted when a session ends by timer or battery limit,
  none on manual stop or quit.

**Closed-lid mode.** `sudo -n /usr/bin/pmset disablesleep 1` and `0`.
Vorssaint optionally installs `/etc/sudoers.d/vorssaint-clamshell` once,
through `osascript ... with administrator privileges`, containing
`#<uid> ALL=(root) NOPASSWD: /usr/bin/pmset disablesleep 1, /usr/bin/pmset
disablesleep 0`. It checks the file with `visudo -c` and removes it if the
check fails. Without the rule, every toggle asks for the admin password.
Unlike an assertion, `disablesleep` is a system-wide setting that outlives
the process, so Vorssaint:

- stores a "sleep disabled by us" marker before turning it on and clears
  the marker only after a verified `disablesleep 0`;
- restores synchronously on quit;
- restores at the next launch if the marker is still set;
- calls `IOPMSleepSystem` after restoring when the lid is already closed,
  because clearing the setting does not put a closed laptop to sleep
  (Vorssaint issue #1729).

A crash leaves sleep disabled until Vorssaint runs again.

**Mouse jiggle.** Opt-in, every 1, 2, 5, 10 or 15 minutes (default 5)
while a session is active and the screen is unlocked. It posts a
`mouseMoved` `CGEvent` (source `hidSystemState`, tap `cghidEventTap`) one
pixel away from the cursor, kept 2 px inside the display bounds, and after
80 ms moves back only if the cursor is still within 2 px of that point, so
it does not fight a user who moved the mouse. Accessibility is required
only while the jiggle is enabled.

**UI.** Menu bar icon with selectable symbol and tint, optional countdown
in the menu bar, a context menu with the durations, a global hotkey, and
command bar actions per duration.

---

## 3. The macOS mechanism: IOKit power assertions

### 3.1 API

`IOPMAssertionCreateWithDescription` is the creation call the SDK header
(`IOKit/pwr_mgt/IOPMLib.h`) calls "the preferred API". It takes the
assertion type, a name, details, a human-readable reason, a timeout in
seconds (0 for none) and a timeout action. The caller releases with
`IOPMAssertionRelease`.

| Assertion type | Effect (from `IOPMLib.h`) | Gadget mapping |
|---|---|---|
| `PreventUserIdleDisplaySleep` | Display does not turn off from idleness; system does not idle-sleep either | `display_sleep_allowed = false` |
| `PreventUserIdleSystemSleep` | No idle system sleep; display may dim and sleep | `display_sleep_allowed = true` |
| `PreventSystemSleep` | Header: "Deprecated in 10.9 ... not supported". `caffeinate(8)`: valid only on AC power | none today |
| `PreventDiskIdle`, `NetworkClientActive` | Disk idle, serving network clients | none |

`IOPMLib.h` lists what an idle assertion does not stop: "The system may
still sleep for lid close, Apple menu, low battery, or other sleep
reasons."

`caffeinate` uses the same calls (`caffeinate.c` in Apple's open-source
PowerManagement project): one `IOPMAssertionCreateWithDescription` per
requested type, always with `kIOPMAssertionTimeoutActionRelease` and the
`-t` value as timeout (0 for none; `-u` forces 5 s). With `-t` and no
utility to run, it also exits by itself after the timeout
(`dispatch_after`). KeepingYouAwake does not call IOKit; it spawns `/usr/bin/caffeinate
-di` or `-i`, with `-t <secs>` and `-w <own pid>`, and kills the child to
stop.

Status and introspection calls: `IOPMAssertionCopyProperties(id)`,
`IOPMCopyAssertionsByProcess`, `IOPMCopyAssertionsStatus`. On the command
line, `pmset -g assertions` lists all assertions with process and name.

### 3.2 Behavior observed on this Mac

Small C programs against the macOS 27.0 SDK, run 2026-10-03:

| Test | Result |
|---|---|
| Create `PreventUserIdleDisplaySleep`, then `kill -9` the process | Assertion listed in `pmset -g assertions` under the process name while held; global count back to 0 after the kill. powerd releases a dead process's assertions |
| Create with timeout 10 s and `TimeoutActionRelease`, wait 12 s | `IOPMAssertionCopyProperties` returned NULL afterwards and `IOPMAssertionRelease` returned `0xe00002c2` (`kIOReturnBadArgument`). The OS released it |
| Read `AssertTimeoutTimeLeft` twice, 4 s apart | Both reads `60` with the same `AssertTimeoutUpdateTime`. The value is a snapshot, not a live countdown |
| Create `PreventSystemSleep` | Returned success and appeared in `pmset -g assertions`. Whether it blocks lid-close sleep was not tested |
| `IOPMAssertionDeclareUserActivity` with `kIOPMUserActiveLocal` | Succeeded and created a `UserIsActive` assertion. None of three idle clocks reset: `HIDIdleTime` of `IOHIDSystem` (read with `ioreg`) and both `CGEventSourceSecondsSinceLastEventType` clocks (HID state, combined session state) |
| Post a zero-distance `kCGEventMouseMoved` with `CGEventPost` from the terminal | The terminal process had no event-synthesizing access (`AXIsProcessTrusted` and `CGPreflightPostEventAccess` both false). Neither idle clock reset. The cause was not isolated: missing access or zero distance |

The Mac was driven remotely during these runs: HID idle time stood at
about 18.9 hours while the combined session clock showed a few seconds.
The two idle-clock rows still hold, since none of the clocks moved.

Consequences for the design, each from the rows above:

- A host crash cannot leave an assertion behind, so C1 (section 6) needs no
  cleanup code for the crash case. Closed-lid mode (C11) is the exception,
  see section 2.2.
- On macOS the OS can end a timed assertion by itself. This was tested with
  the Mac awake only.
- The remaining time of an OS-timed assertion is
  `AssertTimeoutTimeLeft - (now - AssertTimeoutUpdateTime)`, or the host
  keeps its own deadline.
- `DeclareUserActivity` does not reset the idle clocks listed above, so it
  does not make the user look active to applications that read them. A
  mouse jiggle needs synthetic input events.

Untested: whether the OS timeout counts time while the Mac sleeps.

### 3.3 Rust bindings

`objc2-io-kit` 0.3.2 with the `pwr_mgt` feature exposes
`IOPMAssertionCreateWithDescription`, `IOPMAssertionCreateWithName`,
`IOPMAssertionRelease`, `IOPMAssertionCopyProperties`,
`IOPMAssertionSetProperty`, `IOPMAssertionDeclareUserActivity`,
`IOPMCopyAssertionsByProcess`, `IOPMCopyAssertionsStatus` and
`IORegisterForSystemPower`. The assertion type and timeout action strings
are C macros (`CFSTR(...)`) and are not exported; the `keepawake` crate
defines them as `&str` and wraps them with `CFString::from_static_str`.
`objc2-core-foundation` 0.3.2 is already a Torchsnap dependency;
`objc2-io-kit` is not.

The `keepawake` crate (0.6.1, MIT) wraps all three platforms behind one
RAII guard with `display`, `idle` and `sleep` flags and a reason string.
On macOS it calls `IOPMAssertionCreateWithName`. It has no timeout and no
status query.

### 3.4 Permissions and signing

The host's `src-tauri/Entitlements.plist` holds only the two
hardened-runtime exceptions for the compile cache, and the app is not
sandboxed. Power assertions need no entitlement: Amphetamine's code
signature carries `com.apple.security.app-sandbox` and no power-related
entitlement, and it creates them.

### 3.5 Timers and system sleep

A session timer cannot use `std::time::Instant` or tokio timers on macOS
if "30 minutes" should mean 30 minutes of wall time:

- Rust's `Instant` reads `CLOCK_UPTIME_RAW` on Darwin (std docs). The same
  docs add: "it is also not specified whether system suspends count as
  elapsed time or not. The behavior varies across platforms and Rust
  versions."
- `tokio::time::Instant` wraps `std::time::Instant` (tokio 1.53.1,
  `src/time/instant.rs`).
- `man clock_gettime` on macOS: `CLOCK_UPTIME_RAW` "does not increment
  while the system is asleep".

Example: a session starts at 10:00 for 30 minutes, and the lid is closed
from 10:10 to 11:10. An `Instant` timer has 20 minutes left at wake and
ends at 11:30. A wall-clock deadline (10:30) has passed during sleep, so a
host that re-checks its deadline after wake (section 7.3) ends the session
at about 11:10.

Torchsnap's cron scheduler is no model for this. It reads wall time once
per iteration and then waits with `tokio::time::sleep`
(`src-tauri/src/wasm/bridge.rs`, `scheduler_loop`), so a task due while
the Mac sleeps is skipped (todo
`01kwfz4kkaq7spwnm2ncket1gk-scheduler-missed-fires`). The session timer
counts wall time (section 9).

---

## 4. Mouse jiggle

Section 3.2 shows that the assertion API alone does not reset the idle
clocks applications read. A jiggle has to post real input events.

macOS:

- Posting events requires "event synthesizing access" (the SDK's
  `CGEvent.h` wording). Both tools that jiggle tie it to the Accessibility
  permission: Amphetamine imports `AXIsProcessTrustedWithOptions`, and
  Vorssaint requires Accessibility only while its jiggle is on. The probe
  in section 3.2 ran as the terminal process, not as Torchsnap.
- `CGPreflightPostEventAccess` checks the access; `CGRequestPostEventAccess`
  asks for it. Both, plus `CGEvent::new_mouse_event`, `CGEvent::post` and
  `CGEventSource::seconds_since_last_event_type`, are in
  `objc2-core-graphics` 0.3.2, which Torchsnap already depends on (with
  other features enabled).
- The access belongs to a process (`CGEvent.h`: "whether the current
  process already has event synthesizing access"), so for Torchsnap it is
  granted to the whole app, not to a gadget. If a gadget asks for activity
  simulation, the host prompts on behalf of the app.
- Amphetamine's settings (section 2.1) add an inactivity delay: move only
  after N seconds without HID input, every M seconds, optionally stop after
  a longer inactivity period. Vorssaint (section 2.2) moves on a fixed
  interval, one pixel out and back, and backs off if the user moved the
  cursor in between.

Untested: whether a granted zero-distance move resets the idle clocks.
Vorssaint moves by one pixel, and Torchsnap's jiggle will too (section 9).

None of the other keep-awake tools read for this research (PowerToys
Awake, GNOME Caffeine, the two Tauri plugins, `keepawake`) implements a
jiggle.

---

## 5. Other platforms

### 5.1 Windows

| API | Scope | Notes (Microsoft Learn) |
|---|---|---|
| `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED [\| ES_DISPLAY_REQUIRED])` | "The thread's execution requirements" | Cannot stop user-initiated sleep (lid, power button); does not stop the screen saver; `ES_USER_PRESENT` makes the call fail |
| `PowerCreateRequest` + `PowerSetRequest` / `PowerClearRequest` | A handle, reference-counted per type | Windows 7+. `DisplayRequired` needs `SystemRequired` too. "On Modern Standby systems on DC power, system and execution required power requests are terminated 5 minutes after the system sleep timeout has expired." Requests end on user-initiated sleep |

PowerToys Awake uses `SetThreadExecutionState` and runs one dedicated
thread that is the only caller; every other component enqueues the
desired state on a `BlockingCollection` (`Manager.cs`, `StartMonitor`).
The `keepawake` crate calls it from whichever thread creates or drops the
guard, and its source notes `PowerSetRequest` as the alternative.
`PowerCreateRequest` is handle-based and needs no thread of its own.

Mouse jiggle on Windows: `SendInput` "is subject to UIPI. Applications are
permitted to inject input only into applications that are at an equal or
lesser integrity level", and a UIPI block is not reported by the return
value or `GetLastError`. The Microsoft Learn page names no other
permission. Whether injected input resets the idle timer that
`GetLastInputInfo` reports was not checked.

### 5.2 Linux

| Mechanism | Bus | Lifetime | Used by |
|---|---|---|---|
| `org.freedesktop.ScreenSaver.Inhibit(app, reason) -> cookie` | session | until `UnInhibit(cookie)` | `keepawake` (display) |
| `org.gnome.SessionManager.Inhibit(app_id, xid, reason, flags) -> cookie` | session | until `Uninhibit` | GNOME Caffeine (flag 4 suspend or 8 idle) |
| `org.freedesktop.login1.Manager.Inhibit(what, who, why, mode) -> fd` | system | until the fd is closed | `keepawake` (`idle`, `sleep`, mode `block`) |
| Wayland `zwp_idle_inhibit_manager_v1` | Wayland | tied to a visible surface _(unverified)_ | none of the tools read |

`keepawake` releases the ScreenSaver cookie in `Drop` with `.unwrap()`, so
a D-Bus failure during release panics.

Mouse jiggle on Linux depends on the display server:

- X11: XTest (`xdotool` is the usual tool) _(unverified beyond tool
  write-ups)_.
- Wayland, kernel level: writing to `/dev/uinput` (`ydotool`). Jiggler
  projects document that this needs root or membership in the `input`
  group.
- Wayland, portal: `org.freedesktop.portal.RemoteDesktop` has
  `NotifyPointerMotion(session_handle, options, dx, dy)` for relative
  motion. `Start()` shows a dialog where the user picks what to share.
  `SelectDevices()` takes `persist_mode` (0 none, 1 while the app runs,
  2 until revoked) and a `restore_token` that restores a previous session;
  "If the stored session cannot be restored, this value is ignored and the
  user will be prompted normally." The token works once; each start returns
  a new one. Both options need version 2 of the interface.

### 5.3 What carries over

Every platform offers a "system stays awake" and a "display stays on"
level, released by dropping a handle (macOS assertion id, Windows request
handle, Linux cookie or fd). Of the APIs above, only the macOS one takes a
timeout. On macOS and Windows the request does not stop user-initiated
sleep: Apple's header lists lid close and the Apple menu, Microsoft's pages
list lid, power button and Start menu. For logind's `sleep` inhibitor in
`block` mode this was not checked.

---

## 6. Feature compartments

| # | Compartment | What it needs | Platform-specific | Can a WASM gadget do it today? |
|---|---|---|---|---|
| C1 | Wake lock | Acquire and release a "system" or "display" level with a reason string | Yes (section 3, 5) | No host API |
| C2 | Session timer | End a lock after a duration; report remaining time; behavior across sleep (sections 3.5, 9) | macOS can delegate to the OS; Windows and Linux cannot | Cron tasks only, minimum 1 minute, no one-shot tasks (ADR 0032) |
| C3 | Lifetime and cleanup | Release on stop, gadget disable, app quit, crash, guest trap | Crash covered by the OS on macOS (section 3.2) | Guest state dies with the instance on disable (ADR 0033); host must own release |
| C4 | Status | Active, level, remaining, kind | Partly (OS-timed remaining time on macOS) | Only through a host API |
| C5 | Mode change | Switch display level inside a running session | Release and re-acquire, or two locks | Through C1 |
| C6 | Activity simulation (jiggle) | Idle detection, periodic synthetic input, the OS permission flow | Yes (section 4) | No host API; no sub-minute timers |
| C7 | Foreign sessions | Wake requests not held by this gadget: other Torchsnap gadgets (from the host's lock table, by gadget id) and other processes such as Amphetamine or `caffeinate`; today's `SessionKind::External` | Other processes on macOS: `IOPMCopyAssertionsByProcess` | No host API |
| C8 | Launcher UI | Query parsing, entries, titles, icons | No | Yes, but titles ("Keep Mac awake"), the error title ("Amphetamine error"), the `External` wording, the manifest description and the "amphetamine" keyword are Amphetamine or macOS specific, and tests pin them |
| C9 | Ambient indicator and end notice | Tray state while a lock is held; notice when a timed session ends | Tray is platform code (`Tray` trait) | WASM gadgets cannot push (ADR 0030) |
| C10 | Triggers and conditions | Auto start or stop on external display, app running, power source, battery level, screen lock, Wi-Fi, fullscreen, media | Yes, each its own event source | No |
| C11 | Closed-lid mode | `pmset disablesleep`, admin rights or a sudoers rule, a persisted marker and launch-time recovery | macOS only | No; needs a privileged host path |

C1 to C5 and C8 cover what the gadget does with Amphetamine today. C6 is
the jiggle the user asked to keep in mind. C7, C9, C10 and C11 exist in
other tools (Amphetamine Triggers and Enhancer; Vorssaint's automatic
sessions, screen-lock pause, battery limit and closed-lid mode;
KeepingYouAwake battery and Low Power Mode monitors; GNOME Caffeine
fullscreen and media triggers; PowerToys tray) and are listed so the API
does not block them later.

---

## 7. Where the code can live

### 7.1 Facts that constrain the choice

- Host capabilities reach WASM gadgets only through WIT interfaces. Adding
  one touches the WIT file, `wasm/runtime/host/`, `caps/`, the manifest
  permission parser, `wasm/bridge.rs`, `gadget_host.rs`,
  `wasm/interface_gate.rs`, the SDK, test fixtures,
  `docs/api/gadget-development.md` and an ADR (ADR 0036 calls each new WIT
  interface "an ADR-worthy decision"). A WIT `resource` with host-side
  state also needs a `with:` mapping in `wasm/bindings.rs` and, if its
  handles are tracked, a list on `GadgetState` in `wasm/runtime/state.rs`
  (as `sql_handle_reps`). New interfaces are gated by default.
- A WASM instance lives from `enable()` to `disable()`. Disable drops the
  store and all guest state (ADR 0033, `bridge.rs` disable path).
- A failed guest call poisons the component: wasmtime 49 marks the store as
  trapped, and every later call fails with `Trap::CannotEnterComponent`.
  The bridge only logs a failed `search()` or returns the `execute()`
  error; it keeps the instance, and the gadget stays unreachable until it
  is disabled. Only a trap in `enable()` tears the instance down
  (ADR 0033). This affects every gadget and is tracked in
  `todos/gadget-host/wasm/01m40nwejxkmgyhxpf0jdjmre4-trapped-gadget-stays-enabled-but-unreachable.md`.
- `clear_caps` (`wasm/runtime/instance.rs`) is the only cleanup hook that
  runs synchronously at disable before the store is freed, and it only
  knows SQL handles. The host's resource `drop` (when the guest drops a
  handle) and dropping the store's `ResourceTable` also release resource
  entries. The only WIT `resource` today is `sql-storage.sql-handle`.
- WASM gadgets have no timers below one minute and cannot push to the UI
  (ADR 0030, ADR 0032).
- App quit runs `GadgetHost::disable_all()` from `RunEvent::Exit`.
- The strategy document's decision log (2026-04-05) keeps gadgets native
  when they need "deep platform APIs or host operations", and plans to
  remove the native gadget system once the rest is converted.
- Platform code follows ADR 0002: a trait per concern under
  `src-tauri/src/platform/`, a `macos/` and a `fallback/` implementation,
  `Platform*` aliases in `platform/mod.rs`.

### 7.2 Options

**A. Native gadget.** Move the gadget into `src-tauri/src/gadgets/` like
`system_commands`. No WIT changes. It goes against the plan to remove the
native gadget system, and the gadget's launcher logic (C8) is already
written for WASM.

**B. WASM gadget, low-level wake-lock interface.** The host offers a
`wake-lock` resource (C1) with an optional timeout (C2) and status (C4).
The gadget keeps all launcher logic and holds the lock handle in its
`thread_local!` runtime. Disabling the gadget drops the instance and with
it the handle, which the host releases (C3).

Under B a session ends when the gadget is disabled, updated or
reinstalled, and when Torchsnap quits or restarts. Today the Amphetamine
session survives all of these, because Amphetamine owns it. The user
accepted ending with Torchsnap for the first version (2026-10-03); the
gadget's UI or documentation has to say so.

Other gadgets can use the same interface (for example to keep the Mac
awake during a long operation). On macOS each holder gets its own
assertion and `pmset -g assertions` shows a count per type, so two gadgets
holding locks do not interfere with each other. The awake gadget's status
then misses a lock held by another Torchsnap gadget unless C7 covers it.

**C. WASM gadget, high-level session service.** The host owns one global
"keep-awake session" with start, stop and status, and the gadget is a thin
front end. Simpler for the gadget, but the host then encodes the gadget's
product decisions (one session, replace semantics), and a second gadget
would share or fight over that session.

Assessment: B fits the stated constraints. The mechanism (C1 to C4, C6)
is a deep platform API and lives in the host, which matches the strategy
log; the launcher behavior (C8) stays in the gadget, which matches the
plan to retire native gadgets. Option C would move C8-adjacent decisions
into the host. The user chose option B on 2026-10-03.

### 7.3 Host layering for option B

```
gadget (WASM)        launcher logic, session policy, holds wake-lock handle
   │ WIT `power`
caps/power.rs        WakeLockService: per-gadget lock table, deadlines,
                     release on drop / disable / trap / exit
   │
platform/            PowerManagement trait (ADR 0002)
   ├─ macos/         IOPMAssertionCreateWithDescription / Release
   └─ fallback/      every call returns Unsupported
```

- The platform trait stays minimal: acquire a level with a reason, release
  on drop, report support per level. That is the part each port has to
  write; the PowerToys thread model and the `keepawake` `Drop` panic show
  where ports differ.
- The deadline lives in the platform-neutral service and counts wall
  time (section 9). A wall-clock deadline needs a
  re-check after wake, for example on the wake message of
  `IORegisterForSystemPower` or by a periodic `SystemTime` check, because
  a `tokio::time::sleep` pauses while the Mac sleeps (section 3.5).
- The host deadline is authoritative for `is-held` and `remaining-secs`.
  On macOS the service can also pass the timeout to the OS as a safety net
  that does not depend on the host timer. Whether that OS timeout counts
  sleep time is untested (section 3.2), so it may fire later or earlier
  than the host deadline.
- Release on disable: either the lock entry gets a `Drop` impl that
  releases the platform lock (then the resource `drop`, `clear_caps` and
  the store drop all release it without a new list), or the service tracks
  handle reps like `sql_handle_reps`.
- Release on trap (proposal): when a guest call fails, the service
  releases that gadget's locks, or the host disables the gadget (the
  general fix in the todo linked in section 7.1 would cover this). A bounded
  OS timeout on macOS is a second safety net, with the same sleep caveat.
  Without one of these, a trapped gadget's infinite lock stays held with no
  launcher entry to end it until the gadget is disabled or Torchsnap quits
  (section 7.1).

---

## 8. Host API in general form

A sketch to make the shape concrete. Names, types and the split into
interfaces are open.

```wit
/// Keeps the system or the display awake while a gadget holds a lock.
interface power {
    enum wake-level {
        /// No idle system sleep; the display may still turn off.
        system,
        /// Display stays on, which also prevents idle system sleep.
        display,
    }

    variant power-error {
        unsupported,
        permission-denied,
        backend-failure(string),
    }

    record wake-lock-request {
        level: wake-level,
        /// Shown by OS tools such as `pmset -g assertions`.
        reason: string,
        /// Wall-clock seconds until the host releases the lock itself
        /// (section 9). `none` holds the lock until
        /// the handle is dropped.
        timeout-secs: option<u32>,
    }

    /// Dropping the handle releases the lock.
    resource wake-lock {
        /// `false` once the timeout has fired.
        is-held: func() -> bool;
        /// `none` when the lock has no timeout; `some(0)` once the timeout
        /// has fired (`is-held` is then `false`).
        remaining-secs: func() -> option<u32>;
        set-level: func(level: wake-level) -> result<_, power-error>;
    }

    acquire: func(request: wake-lock-request) -> result<wake-lock, power-error>;
    is-supported: func(level: wake-level) -> bool;
}
```

`set-level` semantics to define. On macOS a level change means a new
assertion (one per type, as `caffeinate` does). Proposal: `set-level`
acquires the new level before releasing the old one; on failure the old
lock stays and the error is returned; the host deadline is unchanged and
the remaining time becomes the new OS timeout.

Manifest permission: a boolean named like the interface (`power = true`),
as `settings` and `frecency` are. The interface gate matches gated WIT
interface names and manifest permission keys by string
(`wasm/interface_gate.rs`).

Electron's `powerSaveBlocker` has the same two levels
(`prevent-app-suspension`, `prevent-display-sleep`) with
`start(type) -> id`, `stop(id)` and `isStarted(id)`; when both are active,
"Only the highest precedence type takes effect". Tauri core has no
equivalent. Feature request tauri-apps/tauri#3697 has been open since
2022-03-14; in May 2022 a Tauri team member declined adding it to core
before v1 and suggested a plugin, and a commenter (pevers) then published
tauri-plugin-nosleep. No activity since 2022-05-08.

Activity simulation (C6) is a separate concern with its own permission,
because it needs an app-wide OS grant (section 4) and posts input events.
In general form:

- `access() -> granted | denied | not-determined` and `request-access()`,
  mapping to `CGPreflightPostEventAccess` / `CGRequestPostEventAccess` on
  macOS.
- `start(idle-threshold-secs, interval-secs) -> activity-simulation`
  resource; dropping it stops the jiggle.
- The host runs the timer and the idle check. The gadget never gets a
  generic "post input event" call.

Foreign sessions (C7) would be a read-only call returning wake requests
not held by the calling gadget: other Torchsnap gadgets' locks (by gadget
id) and other processes' assertions (name and level). It exposes what
other apps are doing, so it would need its own permission. The awake
gadget reports foreign sessions and never ends them (section 9).

Changes in the awake gadget:

- It keeps its `KeepAwakeBackend` trait, with a new backend on top of
  `power`. The trait keeps its shape but loses its "stateless value types"
  contract: the `power` backend owns the `wake-lock` handle, replaces it on
  `start` and drops it on `stop`, which needs interior mutability in the
  backend or `&mut self` with `borrow_mut()` in `execute()`.
- The `power` backend reads `is-held` and `remaining-secs` on every
  `search()`. The 15 s `RateLimitCache` exists for the `osascript` probe
  and is dropped for this backend; with it, the gadget would show an ended
  session for up to 15 s.
- Titles, the error title, the `External` wording, the manifest
  description and the "amphetamine" keyword become platform neutral, for
  example supplied by the backend.

The Amphetamine backend is removed (section 9).

---

## 9. Decisions and open questions

Decided by the user on 2026-10-03:

| Question | Decision |
|---|---|
| Where the code lives (section 7.2) | Option B: WASM gadget on a low-level `power` interface with a `wake-lock` resource |
| Session lifetime | A session ends when Torchsnap quits or the gadget is disabled or updated (section 7.2) |
| Timer semantics | Wall time. A timed session ends at the announced time even if the Mac slept in between |
| Amphetamine backend | Removed |
| Wake locks of other Torchsnap gadgets and other processes (C7) | The awake gadget reports them and never ends them |
| Closed-lid mode (C11) | Out of scope |
| Safeguards (screen lock, battery limit, Low Power Mode) | None in the first version. Evaluated later in `todos/gadgets/awake/01m414x1hq141vaaec7aj0y7x7-keep-awake-safeguards.md` |
| Tray indicator while a lock is held (C9) | Not in the first version. Tracked in `todos/product/features/01m414x1hq141vaaec7aj0y7x8-tray-wake-lock-indicator.md` |
| Notice when a timed session ends (C9) | Not in the first version; a session ends silently. A general notification host API is tracked in `todos/gadget-host/api/01m414x1hq141vaaec7aj0y7x6-notification-host-api.md` |
| Mouse jiggle (C6) | Not in the first version. When built, it moves one pixel out and back |

Still open:

- Does an IOKit assertion timeout count time the Mac spends asleep? Needs
  a test with a sleep cycle; the user agreed to run it.
- Jiggle on Wayland: accept the portal dialog, or leave Wayland out?
  Decided when the Linux port starts.
- Does `PreventSystemSleep` keep a MacBook awake with the lid closed on AC
  power? Needs a test on a laptop.

---

## 10. Sources

- Torchsnap at `82df0c9`: `gadgets/awake/`, `src-tauri/src/wasm/`,
  `src-tauri/src/caps/`, `src-tauri/src/platform/`,
  `src-tauri/Entitlements.plist`, ADRs 0002, 0030, 0032, 0033, 0036,
  `docs/strategy/Selfcontained-Gadget-System.md`, todo
  `01kwfz4kkaq7spwnm2ncket1gk-scheduler-missed-fires`.
- wasmtime 49.0.0 sources (`src/runtime/component/func.rs`,
  `src/runtime/component/store.rs`).
- macOS 27.0 SDK: `IOKit.framework/Headers/pwr_mgt/IOPMLib.h`,
  `CoreGraphics.framework/Headers/CGEvent.h`; `man clock_gettime`.
- Apple PowerManagement (github.com/apple-oss-distributions/PowerManagement,
  `d415e45`): `caffeinate/caffeinate.c`, `caffeinate/caffeinate.8`.
- Amphetamine 5.3.2 bundle (`sdef`, `nm -u`, `strings`) and a live
  session's assertion properties.
- `keepawake` 0.6.1 (github.com/segevfiner/keepawake-rs, `6e7aa92`).
- KeepingYouAwake (github.com/newmarcel/KeepingYouAwake, `2be93dd`).
- Vorssaint (github.com/vorssaint/vorssaint-utils, `aa6ddcb`, GPL-3.0):
  `KeepAwakeManager.swift`, `SudoersSupport.swift`, `ShellSupport.swift`,
  `docs/PERMISSIONS.md`.
- PowerToys Awake (github.com/microsoft/PowerToys, `1400fd8`,
  `src/modules/awake`).
- GNOME Caffeine (github.com/eonpatapon/gnome-shell-extension-caffeine,
  `be18b35`).
- tauri-plugin-nosleep (`17c692f`, Tauri 1, last commit 2023-11-08) and
  tauri-plugin-screen-wake-lock (`d7676ea`, Tauri 2, display only).
- `objc2-io-kit` 0.3.2 and `objc2-core-graphics` 0.3.2 sources.
- Microsoft Learn: `SetThreadExecutionState`, `PowerSetRequest`,
  `SendInput` (fetched 2026-10-03).
- xdg-desktop-portal `org.freedesktop.portal.RemoteDesktop` documentation;
  Linux jiggler write-ups (slack.green "Mouse Jiggler for Linux",
  github.com/mucahitkurtlar/pudding) (fetched 2026-10-03).
- Electron `powerSaveBlocker` documentation (fetched 2026-10-03);
  tauri-apps/tauri#3697 and its comments (GitHub API, 2026-10-03).
- Rust std `Instant` documentation (fetched 2026-10-03).
