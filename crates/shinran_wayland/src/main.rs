use std::{
    os::unix::io::AsFd,
    time::{Duration, Instant},
};

use wayland_client::{
    delegate_noop,
    protocol::{
        wl_keyboard::{self, KeyState},
        wl_registry,
        wl_seat::WlSeat,
    },
    Connection, Dispatch, QueueHandle, WEnum,
};
use wayland_protocols_misc::{
    zwp_input_method_v2::client::{
        zwp_input_method_keyboard_grab_v2::{self, ZwpInputMethodKeyboardGrabV2},
        zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
        zwp_input_method_v2::{self, ZwpInputMethodV2},
        zwp_input_popup_surface_v2::ZwpInputPopupSurfaceV2,
    },
    zwp_virtual_keyboard_v1::client::{
        zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
        zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
    },
};
use xkbcommon::xkb;

fn main() {
    let conn = Connection::connect_to_env()
        .unwrap_or_else(|_| panic!("Unable to connect to a Wayland compositor."));
    let display = conn.display();

    let mut event_queue = conn.new_event_queue::<State>();
    let qh = event_queue.handle();

    display.get_registry(&qh, ());

    let mut state = State {
        running: true,
        seats: vec![],
        configured: false,
        input_method_manager: None,
        virtual_keyboard_manager: None,
    };

    // Block until the server has received our `display.get_registry` request.
    event_queue.roundtrip(&mut state).unwrap();

    if state.input_method_manager.is_none() {
        panic!("Compositor does not support zwp_input_method_manager_v2");
    }

    if state.virtual_keyboard_manager.is_none() {
        panic!("Compositor does not support zwp_virtual_keyboard_manager_v1");
    }

    // let mut event_queue = conn.new_event_queue::<Seat>();
    // let qh = event_queue.handle();
    for (seat_index, seat) in state.seats.iter_mut().enumerate() {
        seat.input_method = Some(
            state
                .input_method_manager
                .as_ref()
                .unwrap()
                .get_input_method(&seat.wl_seat, &qh, SeatIndex(seat_index)),
        );
        seat.virtual_keyboard = Some(
            state
                .virtual_keyboard_manager
                .as_ref()
                .unwrap()
                .create_virtual_keyboard(&seat.wl_seat, &qh, ()),
        );
        seat.xkb_context = Some(xkb::Context::new(xkb::CONTEXT_NO_FLAGS));
    }

    while state.running {
        event_queue.blocking_dispatch(&mut state).unwrap();
    }
}

struct State {
    input_method_manager: Option<ZwpInputMethodManagerV2>,
    virtual_keyboard_manager: Option<ZwpVirtualKeyboardManagerV1>,

    running: bool,

    // TODO: Use a HashMap with identity hashing instead of a Vec.
    // https://docs.rs/identity-hash/latest/identity_hash/
    seats: Vec<Seat>,

    configured: bool,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy)]
struct SeatIndex(usize);

impl State {
    fn get_seat(&mut self, seat_index: SeatIndex) -> &mut Seat {
        &mut self.seats[seat_index.0]
    }
}

struct Seat {
    wl_seat: WlSeat,
    name: u32,
    input_method: Option<ZwpInputMethodV2>,
    virtual_keyboard: Option<ZwpVirtualKeyboardV1>,
    xkb_context: Option<xkb::Context>,
    xkb_keymap: Option<xkb::Keymap>,
    xkb_state: Option<xkb::State>,
    active: bool,
    // enabled: bool,
    serial: u32,
    pending_activate: bool,
    pending_deactivate: bool,
    keyboard_grab: Option<ZwpInputMethodKeyboardGrabV2>,
    preedit_str: Option<String>,

    // Handling repeating keys.
    repeat_rate: Option<Duration>,
    repeat_delay: Option<Duration>,
    pressed: [xkb::Keycode; 64],
    repeating_keycode: Option<xkb::Keycode>,
    repeating_timestamp: u32,
    repeat_timer: Option<Instant>,
}

impl Seat {
    fn new(wl_seat: WlSeat, name: u32) -> Self {
        Self {
            wl_seat,
            name,
            input_method: None,     // Set in `main()`.
            virtual_keyboard: None, // Set in `main()`.
            xkb_context: None,      // Set in `main()`.
            xkb_keymap: None,       // Set in `keymap` event.
            xkb_state: None,        // Set in `keymap` event.
            active: false,
            serial: 0,
            pending_activate: false,
            pending_deactivate: false,
            keyboard_grab: None, // Set in `done` event in InputMethod.
            preedit_str: None,   // Set as needed.
            repeat_rate: None,   // Set in `repeat_info` event.
            repeat_delay: None,  // Set in `repeat_info` event.
            pressed: [xkb::Keycode::default(); 64],
            repeating_keycode: None, // Set as needed.
            repeating_timestamp: 0,
            repeat_timer: None, // Set as needed.
        }
    }

    fn append(&mut self, ch: char) {
        if let Some(ref mut preedit_str) = self.preedit_str {
            preedit_str.push(ch);
        } else {
            self.preedit_str = Some(ch.to_string());
        }
    }

    fn mark_as_pressed(&mut self, keycode: xkb::Keycode) {
        for code in self.pressed.iter_mut() {
            // Find an empty slot and store the keycode.
            // With 64 slots, there should always be an empty one.
            if *code == xkb::Keycode::default() {
                *code = keycode;
                break;
            }
        }
    }

    fn release_if_pressed(&mut self, keycode: xkb::Keycode) -> bool {
        for code in self.pressed.iter_mut() {
            if *code == keycode {
                // Clear the slot.
                *code = xkb::Keycode::default();
                return true;
            }
        }
        return false;
    }

    fn handle_key_pressed(&mut self, xkb_key: xkb::Keycode) -> bool {
        let handled: bool;
        let xkb_state = self.xkb_state.as_ref().unwrap();
        let sym = xkb_state.key_get_one_sym(xkb_key);
        match sym {
            xkb::Keysym::Escape => {
                // Close the keyboard.
                handled = true;
            }
            xkb::Keysym::Return => {
                // Send the text.
                handled = true;
            }
            _ => {
                // Send the key.
                let ch = char::from_u32(xkb_state.key_get_utf32(xkb_key)).unwrap();
                if ch.is_ascii() {
                    self.append(ch);
                    handled = true;
                } else {
                    handled = false;
                }
            }
        }
        let input_method = self.input_method.as_ref().unwrap();
        let text = self
            .preedit_str
            .as_ref()
            .map_or("", |s| s.as_str())
            .to_string();
        let cursor_end = text.len() as i32;
        input_method.set_preedit_string(text, 0, cursor_end);
        input_method.commit(self.serial);
        handled
    }
}

fn create_seat(state: &mut State, wl_seat: WlSeat, name: u32) {
    let seat = Seat::new(wl_seat, name);
    state.seats.push(seat);
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version: _version,
            } => match &interface[..] {
                "wl_seat" => {
                    let seat = registry.bind::<WlSeat, _, _>(name, 1, qh, ());
                    create_seat(state, seat, name);
                }
                "zwp_input_method_manager_v2" => {
                    let input_man = registry.bind::<ZwpInputMethodManagerV2, _, _>(name, 1, qh, ());
                    state.input_method_manager = Some(input_man);
                }
                "zwp_virtual_keyboard_manager_v1" => {
                    let keyboard_man =
                        registry.bind::<ZwpVirtualKeyboardManagerV1, _, _>(name, 1, qh, ());
                    state.virtual_keyboard_manager = Some(keyboard_man);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { .. } => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpInputMethodKeyboardGrabV2, SeatIndex> for State {
    fn event(
        state: &mut Self,
        _: &ZwpInputMethodKeyboardGrabV2,
        event: zwp_input_method_keyboard_grab_v2::Event,
        seat_index: &SeatIndex,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let seat = state.get_seat(*seat_index);
        match event {
            zwp_input_method_keyboard_grab_v2::Event::Key {
                time,
                key,
                state: key_state,
                ..
            } => {
                // With an X11-compatible keymap and Linux evdev scan codes (see linux/input.h),
                // a fixed offset is used:
                let keycode = xkb::Keycode::new(key + 8);

                let WEnum::Value(key_state) = key_state else {
                    return;
                };
                if seat.xkb_state.is_none() {
                    return;
                };
                let mut handled = false;

                if matches!(key_state, KeyState::Pressed)
                    && seat.repeating_keycode.map_or(false, |k| k != keycode)
                {
                    if seat.xkb_keymap.as_ref().unwrap().key_repeats(keycode) {
                        seat.repeating_keycode = Some(keycode);
                        seat.repeating_timestamp =
                            time + seat.repeat_delay.unwrap().as_millis() as u32;
                        let mut repeat_timer = Instant::now();
                        repeat_timer += seat.repeat_delay.unwrap();
                        seat.repeat_timer = Some(repeat_timer);
                    } else {
                        seat.repeating_keycode = None;
                    }
                    return;
                }
                if matches!(key_state, KeyState::Released)
                    && seat.repeating_keycode.map_or(false, |k| k == keycode)
                {
                    seat.repeating_keycode = None;
                    seat.repeat_timer = None;
                }

                if matches!(key_state, KeyState::Pressed) {
                    handled = seat.handle_key_pressed(keycode);
                }

                if matches!(key_state, KeyState::Pressed)
                    && seat.xkb_keymap.as_ref().unwrap().key_repeats(keycode)
                    && handled
                {
                    seat.repeating_keycode = Some(keycode);
                    seat.repeating_timestamp = time + seat.repeat_delay.unwrap().as_millis() as u32;
                    let mut repeat_timer = Instant::now();
                    repeat_timer += seat.repeat_delay.unwrap();
                    seat.repeat_timer = Some(repeat_timer);
                }
                if matches!(key_state, KeyState::Pressed) && handled {
                    seat.mark_as_pressed(keycode);
                }
                if matches!(key_state, KeyState::Released) {
                    let found = seat.release_if_pressed(keycode);
                    if found {
                        return;
                    }
                }

                if !handled {
                    seat.virtual_keyboard
                        .as_ref()
                        .unwrap()
                        .key(time, key, key_state.into());
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(xkb_state) = &mut seat.xkb_state {
                    xkb_state.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                    seat.virtual_keyboard.as_ref().unwrap().modifiers(
                        mods_depressed,
                        mods_latched,
                        mods_locked,
                        group,
                    );
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Keymap { format, fd, size } => {
                let WEnum::Value(format) = format else {
                    return;
                };
                seat.virtual_keyboard
                    .as_ref()
                    .unwrap()
                    .keymap(format.into(), fd.as_fd(), size);

                if !matches!(format, wl_keyboard::KeymapFormat::XkbV1) {
                    return;
                }
                seat.xkb_keymap = unsafe {
                    xkb::Keymap::new_from_fd(
                        seat.xkb_context.as_ref().unwrap(),
                        fd,
                        size as usize,
                        xkb::KEYMAP_FORMAT_TEXT_V1,
                        xkb::KEYMAP_COMPILE_NO_FLAGS,
                    )
                }
                .unwrap_or_else(|_| {
                    panic!("Failed to create xkb keymap from fd");
                });
                if seat.xkb_keymap.is_none() {
                    println!("Failed to compile keymap.");
                    return;
                }
                let xkb_state = xkb::State::new(seat.xkb_keymap.as_ref().unwrap());
                if xkb_state.get_raw_ptr().is_null() {
                    println!("Failed to create xkb state.");
                }
                seat.xkb_state = Some(xkb_state);
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { rate, delay } => {
                seat.repeat_rate = Some(Duration::from_millis(rate as u64));
                seat.repeat_delay = Some(Duration::from_millis(delay as u64));
            }
            _ => todo!(),
        }
    }
}

impl Dispatch<ZwpInputMethodV2, SeatIndex> for State {
    fn event(
        state: &mut Self,
        input_method: &ZwpInputMethodV2,
        event: zwp_input_method_v2::Event,
        seat_index: &SeatIndex,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let seat = state.get_seat(*seat_index);
        match event {
            zwp_input_method_v2::Event::Activate => {
                seat.pending_activate = true;
            }
            zwp_input_method_v2::Event::Deactivate => {
                seat.pending_deactivate = true;
            }
            zwp_input_method_v2::Event::SurroundingText { .. } => {
                // Nothing.
            }
            zwp_input_method_v2::Event::TextChangeCause { .. } => {
                // Nothing.
            }
            zwp_input_method_v2::Event::ContentType { .. } => {
                // Nothing.
            }
            zwp_input_method_v2::Event::Done => {
                seat.serial += 1;
                if seat.pending_activate && !seat.active {
                    let keyboard_grab = input_method.grab_keyboard(qh, *seat_index);
                    seat.keyboard_grab = Some(keyboard_grab);
                    seat.active = true;
                } else if seat.pending_deactivate && seat.active {
                    seat.keyboard_grab.as_ref().unwrap().release();
                    seat.pressed = [xkb::Keycode::default(); 64];
                    seat.keyboard_grab = None;
                    seat.active = false;
                }
                seat.pending_activate = false;
                seat.pending_deactivate = false;
            }
            zwp_input_method_v2::Event::Unavailable => {
                // Nothing.
            }
            _ => todo!(),
        }
    }
}

// Input method manager has no events.
delegate_noop!(State: ignore ZwpInputMethodManagerV2);
// Virtual keyboard has no events.
delegate_noop!(State: ignore ZwpVirtualKeyboardV1);
// Virtual keyboard manager has no events.
delegate_noop!(State: ignore ZwpVirtualKeyboardManagerV1);
// We'll ignore events from WlSeat for now. (Events are "name" and "capabilities".)
delegate_noop!(State: ignore WlSeat);
