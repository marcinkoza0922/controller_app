// Reports every focus change to the controller_app daemon's D-Bus service.
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'io.github.marcinkoza0922.ControllerApp';
const OBJECT_PATH = '/Focus';
const INTERFACE = 'io.github.marcinkoza0922.ControllerApp.Focus';

export default class ControllerAppFocus extends Extension {
    enable() {
        this._handler = global.display.connect('notify::focus-window', () => this._report());
        this._report();
    }

    disable() {
        if (this._handler)
            global.display.disconnect(this._handler);
        this._handler = null;
    }

    _report() {
        const window = global.display.focus_window;
        if (!window)
            return;
        // Fire and forget: nobody is listening while the daemon is not running.
        Gio.DBus.session.call(
            BUS_NAME, OBJECT_PATH, INTERFACE, 'WindowActivated',
            new GLib.Variant('(sss)', [
                window.get_wm_class() || '',
                String(window.get_pid()),
                window.get_title() || '',
            ]),
            null, Gio.DBusCallFlags.NONE, -1, null, null);
    }
}
