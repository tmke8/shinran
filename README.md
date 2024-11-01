# shinran
A text expander for Linux.

There are two backends for shinran: a native Wayland backend using [Input method v2](https://wayland.app/protocols/input-method-unstable-v2),
and an [IBus](https://github.com/ibus/ibus) backend, that’s mostly intended for GNOME which likely will never support the Wayland-native method.

Shinran uses the Espanso config format for defining text expansions.
