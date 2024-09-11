// Largely based on
// https://github.com/tadeokondrak/anthywl/blob/21e290b0bb07c1dfe198d477c3f07b500ddc93ba/include/anthywl.h
// https://github.com/tadeokondrak/anthywl/blob/21e290b0bb07c1dfe198d477c3f07b500ddc93ba/src/anthywl.c
// https://github.com/emersion/wlhangul/blob/bd2758227779d7748dea185c38cab11665d55502/include/wlhangul.h
// https://github.com/emersion/wlhangul/blob/bd2758227779d7748dea185c38cab11665d55502/main.c

use std::{
    os::unix::io::AsFd,
    time::{Duration, Instant},
};

use calloop::{
    timer::{TimeoutAction, Timer},
    EventLoop,
};
use calloop_wayland_source::WaylandSource;
use wayland_client::{
    delegate_noop,
    protocol::{
        wl_compositor::WlCompositor,
        wl_keyboard::{self, KeyState},
        wl_registry,
        wl_seat::{self, WlSeat},
        wl_surface::{self, WlSurface},
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
use xkbcommon::xkb::{self, Keysym};

use shinran_lib::check_command;

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
        wl_compositor: None,
        input_method_manager: None,
        virtual_keyboard_manager: None,
    };

    // Block until the server has received our `display.get_registry` request.
    event_queue.roundtrip(&mut state).unwrap();
    eprintln!("Round trip complete.");

    if state.input_method_manager.is_none() {
        panic!("Compositor does not support zwp_input_method_manager_v2");
    }

    if state.virtual_keyboard_manager.is_none() {
        panic!("Compositor does not support zwp_virtual_keyboard_manager_v1");
    }

    init_protocols(&mut state, &qh);
    eprintln!("Protocols initialized.");

    // Create the calloop event loop to drive everything.
    let mut event_loop: EventLoop<State> = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();

    // Insert the wayland source into the calloop's event loop.
    let wayland_source = WaylandSource::new(conn, event_queue);
    loop_handle
        .insert_source(wayland_source, |_, event_queue, state| {
            event_queue.dispatch_pending(state)
        })
        .unwrap();

    // Create a timer that fires every 1000 milliseconds.
    loop_handle
        .insert_source(Timer::immediate(), |_instant, _, _state| {
            eprintln!("Timer callback fired!");
            // Add your timer callback logic here

            // Return the timeout action to reschedule the timer
            TimeoutAction::ToDuration(Duration::from_millis(1000))
        })
        .unwrap();

    // This will start dispatching the event loop and processing pending wayland requests.
    while state.running {
        event_loop.dispatch(None, &mut state).unwrap();
    }
}

fn init_protocols(state: &mut State, qh: &QueueHandle<State>) {
    for (seat_index, seat) in state.seats.iter_mut().enumerate() {
        seat.input_method = Some(
            state
                .input_method_manager
                .as_ref()
                .unwrap()
                .get_input_method(&seat.wl_seat, qh, SeatIndex(seat_index)),
        );
        seat.virtual_keyboard = Some(
            state
                .virtual_keyboard_manager
                .as_ref()
                .unwrap()
                .create_virtual_keyboard(&seat.wl_seat, qh, ()),
        );
        seat.xkb_context = Some(xkb::Context::new(xkb::CONTEXT_NO_FLAGS));
        seat.wl_surface = Some(
            state
                .wl_compositor
                .as_ref()
                .unwrap()
                .create_surface(qh, SeatIndex(seat_index)),
        );
        seat.popup_surface = Some(seat.input_method.as_ref().unwrap().get_input_popup_surface(
            seat.wl_surface.as_ref().unwrap(),
            qh,
            (),
        ));
        seat.are_protocols_initted = true;
    }
}

struct State {
    running: bool,
    wl_compositor: Option<WlCompositor>,
    input_method_manager: Option<ZwpInputMethodManagerV2>,
    virtual_keyboard_manager: Option<ZwpVirtualKeyboardManagerV1>,

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

    are_protocols_initted: bool,
    input_method: Option<ZwpInputMethodV2>,
    virtual_keyboard: Option<ZwpVirtualKeyboardV1>,
    keyboard_grab: Option<ZwpInputMethodKeyboardGrabV2>,

    xkb_context: Option<xkb::Context>,
    xkb_keymap: Option<xkb::Keymap>,
    xkb_state: Option<xkb::State>,

    // wl_seat
    name: Option<String>,

    // zwp_input_method_v2
    active: bool,
    pending_activate: bool,
    pending_deactivate: bool,
    done_events_received: u32, // The "serial" number used for commits.

    // zwp_input_method_keyboard_grab_v2
    // Handling repeating keys.
    repeat_rate: Option<Duration>,
    repeat_delay: Option<Duration>,
    pressed: [xkb::Keycode; 64],
    repeating_keycode: Option<xkb::Keycode>,
    repeating_timestamp: u32,
    repeat_timer: Option<Instant>,

    buffer: Option<String>,

    // popup
    wl_surface: Option<WlSurface>,
    popup_surface: Option<ZwpInputPopupSurfaceV2>,
}

impl Seat {
    fn new(wl_seat: WlSeat) -> Self {
        Self {
            wl_seat,
            are_protocols_initted: false,
            name: None,             // Set in `name` event in WlSeat.
            input_method: None,     // Set in `init_protocols()`.
            virtual_keyboard: None, // Set in `init_protocols()`.
            xkb_context: None,      // Set in `init_protocols()`.
            xkb_keymap: None,       // Set in `keymap` event.
            xkb_state: None,        // Set in `keymap` event.
            active: false,
            done_events_received: 0,
            pending_activate: false,
            pending_deactivate: false,
            keyboard_grab: None, // Set in `done` event in InputMethod.
            repeat_rate: None,   // Set in `repeat_info` event.
            repeat_delay: None,  // Set in `repeat_info` event.
            pressed: [xkb::Keycode::default(); 64],
            repeating_keycode: None, // Set as needed.
            repeating_timestamp: 0,
            repeat_timer: None,  // Set as needed.
            wl_surface: None,    // Set in `init_protocols()`.
            popup_surface: None, // Set in `init_protocols()`.
            buffer: None,        // Set as needed.
        }
    }

    fn append(&mut self, ch: char) {
        if let Some(ref mut preedit_str) = self.buffer {
            preedit_str.push(ch);
        } else {
            self.buffer = Some(ch.to_string());
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
        eprintln!("Added key!");
        let mut pressed_num = [0u32; 64];
        for i in 0..self.pressed.len() {
            pressed_num[i] = self.pressed[i].into();
        }
        eprintln!("{:?}", pressed_num);
    }

    fn release_if_pressed(&mut self, keycode: xkb::Keycode) -> bool {
        for code in self.pressed.iter_mut() {
            if *code == keycode {
                // Clear the slot.
                *code = xkb::Keycode::default();
                eprintln!("Removed key!");
                let mut pressed_num = [0u32; 64];
                for i in 0..self.pressed.len() {
                    pressed_num[i] = self.pressed[i].into();
                }
                eprintln!("{:?}", pressed_num);
                return true;
            }
        }
        false
    }

    /// Returns None if the program should wind down.
    fn handle_key(&mut self, xkb_key: xkb::Keycode) -> Option<bool> {
        let handled: Option<bool>;
        let xkb_state = self.xkb_state.as_ref().unwrap();
        let sym = xkb_state.key_get_one_sym(xkb_key);
        match sym {
            Keysym::Escape => {
                // Close the keyboard.
                self.composing_update(String::default());
                // return None; // shutdown
                return Some(true);
            }
            Keysym::Return => {
                // Send the text.
                if let Some(buffer) = &mut self.buffer {
                    let output = check_command(buffer);
                    if let Some(output) = output {
                        // found match
                        self.composing_commit(output);
                    } else {
                        self.composing_update(String::default());
                    }
                    self.buffer = None;
                }
                // return None; // shutdown
                return Some(true);
            }
            Keysym::KP_Space | Keysym::space => {
                return None; // shutdown
            }
            _ => {
                // If the key corresponds to an ASCII character, add it to the buffer.
                // Otherwise, mark it as unhandled.
                if let Some(ch) = char::from_u32(xkb_state.key_get_utf32(xkb_key)) {
                    if ch.is_ascii() {
                        self.append(ch);
                        handled = Some(true);
                    } else {
                        handled = Some(false);
                    }
                } else {
                    handled = Some(false);
                }
            }
        }
        if let Some(text) = &self.buffer {
            self.composing_update(text.clone());
        }
        handled
    }

    fn composing_update(&mut self, text: String) {
        let input_method = self.input_method.as_ref().unwrap();
        let cursor_end = text.chars().count() as i32;
        input_method.set_preedit_string(text, cursor_end, cursor_end);
        input_method.commit(self.done_events_received);
    }

    fn composing_commit(&mut self, output: String) {
        let input_method = self.input_method.as_ref().unwrap();
        input_method.commit_string(output);
        input_method.commit(self.done_events_received);
    }

    fn draw_popup(&mut self) {}
}

fn create_seat(state: &mut State, wl_seat: WlSeat) {
    let seat = Seat::new(wl_seat);
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
                    create_seat(state, seat);
                }
                "wl_compositor" => {
                    let compositor = registry.bind::<WlCompositor, _, _>(name, 4, qh, ());
                    state.wl_compositor = Some(compositor);
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
                eprintln!("Key {} was {:?}.", key + 8, key_state);
                let seat = state.get_seat(*seat_index);
                if seat.xkb_state.is_none() {
                    return;
                };
                let mut handled = false;

                if matches!(key_state, KeyState::Pressed)
                    && seat.repeating_keycode.map_or(false, |k| k != keycode)
                {
                    if !seat.handle_key(keycode).unwrap_or(true) {
                        seat.repeating_keycode = None;
                        seat.repeat_timer = None;
                        seat.virtual_keyboard
                            .as_ref()
                            .unwrap()
                            .key(time, key, key_state.into());
                        return;
                    }
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
                    match seat.handle_key(keycode) {
                        Some(handled_key) => {
                            handled = handled_key;
                        }
                        None => {
                            eprintln!("Shutting down.");
                            state.running = false;
                            handled = true;
                        }
                    }
                }

                let seat = state.get_seat(*seat_index);
                if matches!(key_state, KeyState::Pressed)
                    && seat.xkb_keymap.as_ref().unwrap().key_repeats(keycode)
                    && handled
                {
                    seat.repeating_keycode = Some(keycode);
                    seat.repeating_timestamp = time + seat.repeat_delay.unwrap().as_millis() as u32;
                    let mut repeat_timer = Instant::now();
                    repeat_timer += seat.repeat_delay.unwrap();
                    seat.repeat_timer = Some(repeat_timer);
                    eprintln!("Repeat timer set for {}", key + 8);
                }
                if matches!(key_state, KeyState::Pressed) && handled {
                    // Add key to our pressed keys list if we did something with it.
                    seat.mark_as_pressed(keycode);
                }
                if matches!(key_state, KeyState::Released) {
                    // Remove key from our pressed keys list.
                    let found = seat.release_if_pressed(keycode);
                    if found {
                        return;
                    }
                }

                if !handled {
                    // If we didn't handle the key, send it to the virtual keyboard.
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
                let seat = state.get_seat(*seat_index);
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
                let seat = state.get_seat(*seat_index);
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
                let seat = state.get_seat(*seat_index);
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
                seat.done_events_received += 1;
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

impl Dispatch<WlSurface, SeatIndex> for State {
    fn event(
        state: &mut Self,
        wl_surface: &WlSurface,
        event: wl_surface::Event,
        seat_index: &SeatIndex,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let seat = state.get_seat(*seat_index);
        match event {
            wl_surface::Event::Enter { output } => {}
            wl_surface::Event::Leave { output } => {}
            _ => todo!(),
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        wl_seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_seat::Event::Name { name } => {
                eprintln!("Seat name: {}.", name);
            }
            wl_seat::Event::Capabilities { capabilities } => {}
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
// We'll ignore the event from ZwpInputPopupSurfaceV2 for now. (Event is "text_input_rectangle".)
delegate_noop!(State: ignore ZwpInputPopupSurfaceV2);
// WlCompositor has no events.
delegate_noop!(State: ignore WlCompositor);
