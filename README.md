# shinran
A text expander for Linux.

[shin](https://github.com/p-e-w/shin) + [espanso](https://github.com/espanso/espanso) = **shinran**

## Screencast
![Demo](assets/screencast.gif)

## Architecture
There are two frontends for shinran: a native Wayland frontend using [Input method v2](https://wayland.app/protocols/input-method-unstable-v2),
and an [IBus](https://github.com/ibus/ibus) frontend, that’s mostly intended for GNOME (which likely will never support the Wayland-native method).

Shinran uses the Espanso config format for defining text expansions. See “Implementation status” below for information on which Espanso features are currently supported by shinran.

## Install
### Prerequisites
#### Rust compiler
#### `libxkbcommon` headers

E.g., on Ubuntu:
```
sudo apt install libxkbcommon-dev
```

### Wayland frontend
Simply compile the binary and set up a trigger to execute it.

```sh
cargo build --bin shinran_wayland --release
cp target/release/shinran_wayland ~/.local/bin/shinran_wayland
```

And then, e.g., for sway, put this in your config:

```
bindsym $mod+x exec ~/.local/bin/shinran_wayland
```

### IBus frontend
In order to set up the IBus frontend, the binary needs to be registered with IBus. This is handled by the `Makefile`:

```sh
make
sudo make install
```

To start shinran, the following has to be executed:

```sh
ibus engine shinran
```

You have to bind this to a keyboard shortcut. See [here](https://docs.fedoraproject.org/en-US/quick-docs/gnome-setting-key-shortcut/) for how that works in GNOME.

## Implementation status

- [ ] Wayland frontend
	- [x] basic functionality
	- [ ] popup with candidates
	- [ ] retrieve application name (for filtering)
	- [ ] [cursor hints](https://github.com/espanso/website/blob/486c44e09959bbca81244cdb62f8fdb69b7834a8/docs/matches/basics.md#cursor-hints)
	- [ ] [image matches](https://github.com/espanso/website/blob/486c44e09959bbca81244cdb62f8fdb69b7834a8/docs/matches/basics.md#image-matches)
- [ ] IBus frontend
	- [x] basic functionality
	- [ ] popup with candidates
		- [x] basic popup
		- [ ] candidate selection
	- [ ] retrieve application name (for filtering)
	- [ ] [cursor hints](https://github.com/espanso/website/blob/486c44e09959bbca81244cdb62f8fdb69b7834a8/docs/matches/basics.md#cursor-hints)
	- [ ] [image matches](https://github.com/espanso/website/blob/486c44e09959bbca81244cdb62f8fdb69b7834a8/docs/matches/basics.md#image-matches)
- [ ] Backend
	- [x] load Espanso format
	- [x] trigger matches
	- [x] regex matches
	- [ ] built-in matches
		- [ ] basic text insertion of built-in matches
		- [ ] popup with debug information
	- [x] date extension
	- [x] random extension
	- [x] echo extension
	- [x] shell extension
	- [x] script extension
	- [ ] [clipboard extension](https://github.com/espanso/website/blob/486c44e09959bbca81244cdb62f8fdb69b7834a8/docs/matches/extensions.mdx#clipboard-extension), see https://github.com/tmke8/shinran/issues/27
	- [ ] [choice extension](https://github.com/espanso/website/blob/486c44e09959bbca81244cdb62f8fdb69b7834a8/docs/matches/extensions.mdx#choice-extension) (can this be done with the candidate popup?)
	- [ ] [form extension](https://github.com/espanso/website/blob/486c44e09959bbca81244cdb62f8fdb69b7834a8/docs/matches/extensions.mdx#form-extension) (not sure I want to implement that…)

## Acknowledgments
The IBus frontend code started out as a Rust-port of [shin](https://github.com/p-e-w/shin) (GPLv3).

The Wayland frontend code started out as a Rust-port of [anthywl](https://github.com/tadeokondrak/anthywl) (ISC license) with inspiration from [wlhangul](https://github.com/emersion/wlhangul) (MIT license).

A lot of the backend code started out as a copy of [espanso](https://github.com/espanso/espanso) (GPLv3), especially the `shinran_config` crate and the `shinran_render` crate (which were copies of `espanso-config` and `espanso-render` respectively).
