// Reports every focus change to padwight's daemon, which matches the window against the
// game rules. The daemon's D-Bus service is the same one the KWin script reports to.

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'io.github.marcinkoza0922.Padwight';
const OBJECT_PATH = '/Focus';
const INTERFACE = 'io.github.marcinkoza0922.Padwight.Focus';

export default class PadwightFocus extends Extension {
    enable() {
        this._display = global.display;
        this._focusId = this._display.connect('notify::focus-window', () => this._report());
        this._report();
    }

    disable() {
        this._display.disconnect(this._focusId);
        this._display = null;
    }

    _report() {
        const win = global.display.focus_window;
        if (!win) {
            // Nothing has focus, such as the desktop.
            Gio.DBus.session.call(BUS_NAME, OBJECT_PATH, INTERFACE, 'FocusCleared', new GLib.Variant('()', []),
                null, Gio.DBusCallFlags.NONE, -1, null, null);
            return;
        }
        // Everything is sent as a string, as the daemon expects (see focus/dbus.rs).
        const args = GLib.Variant.new_tuple([
            GLib.Variant.new_string(win.get_wm_class() ?? ''),
            GLib.Variant.new_string(String(win.get_pid())),
            GLib.Variant.new_string(win.get_title() ?? ''),
        ]);
        // No reply is needed. If the daemon isn't running, the call just fails.
        Gio.DBus.session.call(BUS_NAME, OBJECT_PATH, INTERFACE, 'WindowActivated', args,
            null, Gio.DBusCallFlags.NONE, -1, null, null);
    }
}
