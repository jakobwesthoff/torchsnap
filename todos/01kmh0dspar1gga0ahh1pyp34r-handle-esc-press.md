# Handle ESC press in launcher

Currently the ESC key badge is displayed in the search bar but
pressing ESC does nothing in the launcher window.

## What needs to happen

1. Add a keydown listener for Escape in the launcher
2. On ESC press: dismiss the launcher (hide window + reset state)
3. Coordinate with the generalized key press handling system
4. The ESC badge in the search input already exists — just needs
   the actual behavior wired up
