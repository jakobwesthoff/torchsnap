# Plugin: System commands

Built-in commands for common system operations — zero config,
universally useful.

## Scope

Commands to include:
- **Lock screen**
- **Sleep / suspend**
- **Restart**
- **Shutdown**
- **Log out**
- **Empty trash / recycle bin**
- **Eject volume** (with volume selection)
- **Toggle Do Not Disturb / Focus mode**
- **Screen saver**

## Platform considerations

- **macOS**:
  - Lock: `pmset displaysleepnow` or `CGSession -suspend`
  - Sleep: `pmset sleepnow`
  - Restart/Shutdown: `osascript -e 'tell app "System Events" to restart/shut down'`
  - Empty trash: `osascript -e 'tell app "Finder" to empty trash'`
  - Eject: `diskutil eject`
- **Linux**:
  - Lock: `loginctl lock-session` or DE-specific (gnome-screensaver, etc.)
  - Sleep: `systemctl suspend`
  - Restart/Shutdown: `systemctl reboot/poweroff`
  - Trash: `gio trash --empty`
- **Windows**:
  - Lock: `rundll32.exe user32.dll,LockWorkStation`
  - Sleep: `rundll32.exe powrprof.dll,SetSuspendState`
  - Restart/Shutdown: `shutdown /r /t 0` / `shutdown /s /t 0`
  - Trash: Shell API `SHEmptyRecycleBin`

## UX

- Destructive commands (restart, shutdown, empty trash) should
  require confirmation — either a second Enter press or a
  confirmation dialog
- Show appropriate system icons for each command
- Commands should be fuzzy-searchable by aliases (e.g. "reboot"
  matches Restart, "lock" matches Lock Screen)
