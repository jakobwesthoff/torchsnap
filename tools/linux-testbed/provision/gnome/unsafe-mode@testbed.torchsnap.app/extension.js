// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// GNOME Shell answers Eval, Screenshot and Screencast over D-Bus only to
// a short list of its own callers, unless the shell runs in unsafe mode.
// GNOME Shell 48 has no command-line switch for that mode, so the testbed
// sets it from inside the shell, which lets its scripts take screenshots,
// record the screen and read window state.

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as MessageTray from 'resource:///org/gnome/shell/ui/messageTray.js';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

export default class TestbedExtension extends Extension {
    enable() {
        // Switching unsafe mode on makes GNOME Shell post a notification
        // about it at once, whose banner would cover the top of the screen
        // in the testbed's screenshots. The notification posted by this
        // switch is removed again.
        const source = MessageTray.getSystemSource();
        const earlier = new Set(source.notifications);
        global.context.unsafe_mode = true;
        for (const notification of [...source.notifications]) {
            if (!earlier.has(notification))
                notification.destroy();
        }

        // GNOME Shell opens the overview after login, and its search entry
        // takes every key typed until it closes. Enabling the extension
        // later, in a running session, leaves the overview alone.
        this._startupId = Main.layoutManager.connect('startup-complete', () => Main.overview.hide());
    }

    disable() {
        Main.layoutManager.disconnect(this._startupId);
        global.context.unsafe_mode = false;
    }
}
