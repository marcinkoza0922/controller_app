// Stand-in for GNOME Shell's org.gnome.Shell.Extensions interface, for tests/gnome_focus.rs.
// It enables an extension only when its files are in $XDG_DATA_HOME/gnome-shell/extensions, as
// GNOME Shell would load them. It doesn't check anything else, so it proves the D-Bus calls and
// the file layout, not that GNOME Shell accepts the extension.

const {Gio, GLib} = imports.gi;

const XML = `<node>
  <interface name="org.gnome.Shell.Extensions">
    <method name="GetExtensionInfo">
      <arg type="s" name="uuid" direction="in"/>
      <arg type="a{sv}" name="info" direction="out"/>
    </method>
    <method name="EnableExtension">
      <arg type="s" name="uuid" direction="in"/>
      <arg type="b" name="ok" direction="out"/>
    </method>
  </interface>
</node>`;

const extensions = GLib.build_filenamev([GLib.getenv('XDG_DATA_HOME'), 'gnome-shell', 'extensions']);
const enabled = new Set();

const impl = {
    GetExtensionInfo(uuid) {
        const info = {};
        if (enabled.has(uuid))
            info.state = GLib.Variant.new_double(1.0);
        return new GLib.Variant('(a{sv})', [info]);
    },
    EnableExtension(uuid) {
        const installed = GLib.file_test(GLib.build_filenamev([extensions, uuid, 'metadata.json']), GLib.FileTest.EXISTS);
        if (installed)
            enabled.add(uuid);
        return new GLib.Variant('(b)', [installed]);
    },
};

const info = Gio.DBusNodeInfo.new_for_xml(XML).interfaces[0];
const exported = Gio.DBusExportedObject.wrapJSObject(info, impl);

Gio.bus_own_name(Gio.BusType.SESSION, 'org.gnome.Shell', Gio.BusNameOwnerFlags.NONE,
    (connection) => exported.export(connection, '/org/gnome/Shell'), null, null);

GLib.MainLoop.new(null, false).run();
