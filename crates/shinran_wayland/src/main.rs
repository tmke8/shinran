// Largely based on
// https://github.com/tadeokondrak/anthywl/blob/21e290b0bb07c1dfe198d477c3f07b500ddc93ba/include/anthywl.h
// https://github.com/tadeokondrak/anthywl/blob/21e290b0bb07c1dfe198d477c3f07b500ddc93ba/src/anthywl.c
// https://github.com/emersion/wlhangul/blob/bd2758227779d7748dea185c38cab11665d55502/include/wlhangul.h
// https://github.com/emersion/wlhangul/blob/bd2758227779d7748dea185c38cab11665d55502/main.c

use std::{collections::HashMap, mem, os::unix::io::AsFd, rc::Rc, time::Duration};

use calloop::{
    timer::{TimeoutAction, Timer},
    Dispatcher, EventLoop, LoopHandle, RegistrationToken,
};
use calloop_wayland_source::WaylandSource;
use slotmap::{new_key_type, SlotMap};
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

use shinran_lib::Backend;

fn main() {
    // Set up the backend.
    let cli_overrides = HashMap::new();
    let backend = Backend::new(&cli_overrides).unwrap();

    // Set up the Wayland connection.
    let conn = Connection::connect_to_env()
        .unwrap_or_else(|_| panic!("Unable to connect to a Wayland compositor."));
    let display = conn.display();

    let mut event_queue = conn.new_event_queue::<State>();
    let qh = event_queue.handle();

    display.get_registry(&qh, ());

    // Create the calloop event loop to drive everything.
    let mut event_loop: EventLoop<State> = EventLoop::try_new().unwrap();
    let loop_handle = event_loop.handle();

    let mut state = State {
        running: true,
        seats: vec![],
        contexts: SlotMap::with_key(),
        wl_compositor: None,
        input_method_manager: None,
        virtual_keyboard_manager: None,
        loop_handle: loop_handle.clone(),
    };

    // Block until the server has received our `display.get_registry` request.
    event_queue.roundtrip(&mut state).unwrap();
    eprintln!("Round trip complete.");

    init_protocols(&mut state, &qh, Rc::new(backend));
    eprintln!("Protocols initialized.");

    // Insert the wayland source into the calloop's event loop.
    WaylandSource::new(conn, event_queue)
        .insert(loop_handle.clone())
        .expect("Insert Wayland source into calloop.");

    // This will start dispatching the event loop and processing pending wayland requests.
    while state.running {
        event_loop.dispatch(None, &mut state).unwrap();
    }
    eprintln!("Shutting down...");
    // Wait a bit to see if there are any pending requests.
    event_loop
        .dispatch(Duration::from_millis(200), &mut state)
        .unwrap();
}

fn init_protocols(state: &mut State, qh: &QueueHandle<State>, backend: Rc<Backend>) {
    let seats = mem::replace(&mut state.seats, vec![]);

    let Some(input_method_manager) = &state.input_method_manager else {
        panic!("Compositor does not support zwp_input_method_manager_v2");
    };

    let Some(virtual_keyboard_manager) = &state.virtual_keyboard_manager else {
        panic!("Compositor does not support zwp_virtual_keyboard_manager_v1");
    };

    let Some(wl_compositor) = &state.wl_compositor else {
        panic!("Compositor does not support wl_compositor");
    };

    for seat in seats.into_iter() {
        state.contexts.insert_with_key(|seat_index| {
            // We have to be a bit mindful of race conditions here.
            // What we are doing here is creating a new input method and virtual keyboard for each seat,
            // and we pass the seat index to the input method and virtual keyboard.
            // However, the context object that the seat index refers to is not initialized yet here.
            // It is only initialized at the end of the loop body.
            // I *think* this is fine because when `init_protocols()` is called,
            // the event queue hasn't been dispatched yet, so the context object should not be accessed
            // by events on `input_method` and `virtual_keyboard`.
            let input_method = input_method_manager.get_input_method(&seat, qh, seat_index);
            let virtual_keyboard = virtual_keyboard_manager.create_virtual_keyboard(&seat, qh, ());
            let wl_surface = wl_compositor.create_surface(qh, seat_index);
            let popup_surface = input_method.get_input_popup_surface(&wl_surface, qh, ());
            let backend = backend.clone();
            InputContext::new(
                seat,
                input_method,
                virtual_keyboard,
                wl_surface,
                popup_surface,
                backend,
            )
        });
    }
}

new_key_type! { struct SeatIndex; }

struct State {
    running: bool,
    wl_compositor: Option<WlCompositor>,
    input_method_manager: Option<ZwpInputMethodManagerV2>,
    virtual_keyboard_manager: Option<ZwpVirtualKeyboardManagerV1>,

    seats: Vec<WlSeat>,
    contexts: SlotMap<SeatIndex, InputContext>,
    // configured: bool,
    loop_handle: LoopHandle<'static, State>,
}

impl State {
    fn get_context(&mut self, seat_index: SeatIndex) -> &mut InputContext {
        &mut self.contexts[seat_index]
    }
}

struct InputContext {
    seat: WlSeat,

    input_method: ZwpInputMethodV2,
    virtual_keyboard: ZwpVirtualKeyboardV1,
    keyboard_grab: Option<ZwpInputMethodKeyboardGrabV2>,

    xkb_context: xkb::Context,
    xkb_keymap: Option<xkb::Keymap>,
    xkb_state: Option<xkb::State>,

    // wl_seat
    name: Option<String>,

    // zwp_input_method_v2
    pending_activate: bool,
    pending_deactivate: bool,
    num_done_events: u32, // This number is needed for the commit method.

    // zwp_input_method_keyboard_grab_v2
    // Handling repeating keys.
    repeat_rate: Option<Duration>,
    repeat_delay: Option<Duration>,
    pressed: [xkb::Keycode; 64],
    repeat_timer: Option<RepeatTimer>,

    buffer: Option<String>,

    // popup
    wl_surface: WlSurface,
    popup_surface: ZwpInputPopupSurfaceV2,

    // backend
    backend: Rc<Backend>,
}

impl InputContext {
    fn new(
        seat: WlSeat,
        input_method: ZwpInputMethodV2,
        virtual_keyboard: ZwpVirtualKeyboardV1,
        wl_surface: WlSurface,
        popup_surface: ZwpInputPopupSurfaceV2,
        backend: Rc<Backend>,
    ) -> Self {
        Self {
            seat,
            name: None, // Set in `name` event in WlSeat.
            input_method,
            virtual_keyboard,
            xkb_context: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
            xkb_keymap: None, // Set in `keymap` event.
            xkb_state: None,  // Set in `keymap` event.
            num_done_events: 0,
            pending_activate: false,
            pending_deactivate: false,
            keyboard_grab: None, // Set in `done` event in InputMethod.
            repeat_rate: None,   // Set in `repeat_info` event.
            repeat_delay: None,  // Set in `repeat_info` event.
            pressed: [xkb::Keycode::default(); 64],
            repeat_timer: None, // Set as needed.
            wl_surface,
            popup_surface,
            buffer: None, // Set as needed.
            backend,
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
                // Commit an empty string.
                self.composing_update(String::default());
                // return Some(true);
                return None; // shutdown
            }
            Keysym::Return => {
                // Send the text.
                if let Some(buffer) = &mut self.buffer {
                    let output = self.backend.check_trigger(buffer).unwrap();
                    if let Some(output) = output {
                        // found match
                        self.composing_commit(output);
                    } else {
                        self.composing_update(String::default());
                    }
                    self.buffer = None;
                }
                // return Some(true);
                return None; // shutdown
            }
            Keysym::KP_Space | Keysym::space => {
                return Some(false);
            }
            _ => {
                // If the key corresponds to an ASCII character, add it to the buffer.
                // Otherwise, mark it as unhandled.
                if let Some(ch) = char::from_u32(xkb_state.key_get_utf32(xkb_key)) {
                    if ch.is_ascii() {
                        if ch == '\0' {
                            // If the key does not represent a character,
                            // `key_get_utf32` returns 0.
                            handled = Some(false);
                        } else {
                            self.append(ch);
                            handled = Some(true);
                        }
                    } else {
                        handled = Some(false);
                    }
                } else {
                    handled = Some(false);
                }
            }
        }
        if let Some(text) = &self.buffer {
            // TODO: Only update if the text has changed.
            self.composing_update(text.clone());
        }
        handled
    }

    fn repeat_key(&mut self) -> TimeoutAction {
        let repeat_rate = self.repeat_rate.expect("Repeat rate should have been set.");
        let repeating = self
            .repeat_timer
            .as_mut()
            .expect("Repeat timer should have been set.");
        let key_code = repeating.keycode;
        let key = u32::from(key_code) - 8;
        eprintln!("Timer repeats {}", key);
        let time = repeating.timestamp;
        // Update the timestamp for the next repetition.
        repeating.timestamp += 1000 / (repeat_rate.as_millis() as u32);
        if self.handle_key(key_code).is_some_and(|x| !x) {
            self.virtual_keyboard
                .key(time, key, KeyState::Pressed.into());
            self.repeat_timer = None;
            // Don't schedule the timer again.
            eprintln!("Timer dropped.");
            return TimeoutAction::Drop;
        }
        TimeoutAction::ToDuration(repeat_rate)
    }

    fn composing_update(&mut self, text: String) {
        let cursor_end = text.chars().count() as i32;
        self.input_method
            .set_preedit_string(text, cursor_end, cursor_end);
        self.input_method.commit(self.num_done_events);
    }

    fn composing_commit(&mut self, output: String) {
        self.input_method.commit_string(output);
        self.input_method.commit(self.num_done_events);
    }

    fn draw_popup(&mut self) {}
}

struct RepeatTimer {
    keycode: xkb::Keycode,
    timestamp: u32,
    timer: Dispatcher<'static, Timer, State>,
    registration: RegistrationToken,
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
                    // TODO: Perhaps we can create the seat identifier already here and pass it to
                    // the handler of `WlSeat` events, so that we can actually associate the name
                    // we get in the `WlSeat` events with the right seat object.
                    let seat = registry.bind::<WlSeat, _, _>(name, 2, qh, ());
                    // Collect all seats.
                    state.seats.push(seat);
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
                let WEnum::Value(key_state) = key_state else {
                    panic!("Invalid key state.");
                };

                // With an X11-compatible keymap and Linux evdev scan codes (see linux/input.h),
                // a fixed offset is used:
                const SCANCODE_OFFSET: u32 = 8;
                let keycode = xkb::Keycode::new(key + SCANCODE_OFFSET);

                eprintln!("Key {} was {:?}.", key + SCANCODE_OFFSET, key_state);
                let input_context = state.get_context(*seat_index);
                if input_context.xkb_state.is_none() {
                    return;
                };

                // First check:
                // A key was pressed and we are currently repeating a key, but not this one.
                if matches!(key_state, KeyState::Pressed)
                    && input_context
                        .repeat_timer
                        .as_ref()
                        .map_or(false, |t| t.keycode != keycode)
                {
                    match input_context.handle_key(keycode) {
                        Some(true) => {
                            input_context.mark_as_pressed(keycode);
                        }
                        Some(false) => {
                            // Forward the key event to the virtual keyboard.
                            input_context
                                .virtual_keyboard
                                .key(time, key, key_state.into());
                            // Remove the repeat timer.
                            let token = input_context.repeat_timer.as_ref().unwrap().registration;
                            input_context.repeat_timer = None;
                            state.loop_handle.remove(token);
                            return;
                        }
                        None => {
                            eprintln!("Shutting down.");
                            state.running = false;
                            return;
                        }
                    }
                    if input_context
                        .xkb_keymap
                        .as_ref()
                        .unwrap()
                        .key_repeats(keycode)
                    {
                        let repeating = input_context.repeat_timer.as_mut().unwrap();
                        // Update the timer to repeat the new key.
                        eprintln!("Update repeat timer for {}", key + SCANCODE_OFFSET);
                        repeating.keycode = keycode;
                        let repeat_delay = input_context.repeat_delay.unwrap();
                        repeating.timestamp = time + repeat_delay.as_millis() as u32;
                        // Set timer to start repeating starting from `repeat_delay` milliseconds.
                        repeating.timer.as_source_mut().set_duration(repeat_delay);
                        let token = repeating.registration;
                        // Update registration of the timer after we have updated the deadline.
                        state
                            .loop_handle
                            .update(&token)
                            .expect("Failed to update timer.");
                    } else {
                        input_context.repeat_timer = None;
                    }
                    return;
                }

                // Second check:
                // A key was released and we are currently repeating precisely this key.
                if matches!(key_state, KeyState::Released)
                    && input_context
                        .repeat_timer
                        .as_ref()
                        .map_or(false, |t| t.keycode == keycode)
                {
                    eprintln!("Delete repeat timer for {}", key + 8);
                    input_context.release_if_pressed(keycode);
                    // Turn off the repeat timer.
                    let registration = input_context.repeat_timer.as_ref().unwrap().registration;
                    input_context.repeat_timer = None;
                    state.loop_handle.remove(registration);
                    return;
                }

                // Third check:
                // A key was pressed and we haven't dealt with it yet.
                let handled = if matches!(key_state, KeyState::Pressed) {
                    match input_context.handle_key(keycode) {
                        Some(handled_key) => handled_key,
                        None => {
                            eprintln!("Shutting down.");
                            state.running = false;
                            true
                        }
                    }
                } else {
                    false
                };

                // Fourth check:
                // A key was pressed and we have handled it, and it *could* be repeated.
                let seat_index = *seat_index;
                let input_context = state.get_context(seat_index);
                if matches!(key_state, KeyState::Pressed)
                    && input_context
                        .xkb_keymap
                        .as_ref()
                        .unwrap()
                        .key_repeats(keycode)
                    && handled
                {
                    let repeat_delay = input_context.repeat_delay.unwrap();
                    // Set timer to start repeating starting from `repeat_delay` milliseconds.
                    let timer = Dispatcher::<'static, Timer, State>::new(
                        Timer::from_duration(repeat_delay),
                        move |_instant, _, state| state.get_context(seat_index).repeat_key(),
                    );
                    input_context.mark_as_pressed(keycode);
                    // Register the timer with the event loop.
                    let registration = state
                        .loop_handle
                        .register_dispatcher(timer.clone())
                        .expect("Insert timer into calloop.");
                    let input_context = state.get_context(seat_index);
                    input_context.repeat_timer = Some(RepeatTimer {
                        registration,
                        timer,
                        timestamp: time + repeat_delay.as_millis() as u32,
                        keycode,
                    });
                    eprintln!("Repeat timer set for {}", key + 8);
                    return;
                }

                // Fifth check:
                // A key was pressed and we have handled it, but it cannot be repeated.
                if matches!(key_state, KeyState::Pressed) && handled {
                    // Add key to our pressed keys list if we did something with it.
                    input_context.mark_as_pressed(keycode);
                }

                // Sixth check:
                // A key was released but we were not repeating it.
                if matches!(key_state, KeyState::Released) {
                    // Remove key from our pressed keys list.
                    let found = input_context.release_if_pressed(keycode);
                    if found {
                        return; // We handled the corresponding pressed event, so we're done.
                    }
                }

                if !handled {
                    // If we didn't handle the key, send it to the virtual keyboard.
                    eprintln!("Forwarded key {}", key + 8);
                    input_context
                        .virtual_keyboard
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
                let input_context = state.get_context(*seat_index);
                if let Some(xkb_state) = &mut input_context.xkb_state {
                    xkb_state.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                    input_context.virtual_keyboard.modifiers(
                        mods_depressed,
                        mods_latched,
                        mods_locked,
                        group,
                    );
                }
            }
            zwp_input_method_keyboard_grab_v2::Event::Keymap {
                format: WEnum::Value(format),
                fd,
                size,
            } => {
                let input_context = state.get_context(*seat_index);
                input_context
                    .virtual_keyboard
                    .keymap(format.into(), fd.as_fd(), size);

                if !matches!(format, wl_keyboard::KeymapFormat::XkbV1) {
                    return;
                }
                input_context.xkb_keymap = unsafe {
                    xkb::Keymap::new_from_fd(
                        &input_context.xkb_context,
                        fd,
                        size as usize,
                        xkb::KEYMAP_FORMAT_TEXT_V1,
                        xkb::KEYMAP_COMPILE_NO_FLAGS,
                    )
                }
                .unwrap_or_else(|_| {
                    panic!("Failed to create xkb keymap from fd");
                });
                if input_context.xkb_keymap.is_none() {
                    println!("Failed to compile keymap.");
                    return;
                }
                let xkb_state = xkb::State::new(input_context.xkb_keymap.as_ref().unwrap());
                if xkb_state.get_raw_ptr().is_null() {
                    println!("Failed to create xkb state.");
                }
                input_context.xkb_state = Some(xkb_state);
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { rate, delay } => {
                let input_context = state.get_context(*seat_index);
                input_context.repeat_rate = Some(Duration::from_millis(rate as u64));
                input_context.repeat_delay = Some(Duration::from_millis(delay as u64));
                eprintln!("Repeat rate: {} ms, delay: {} ms.", rate, delay);
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
        let input_context = state.get_context(*seat_index);
        match event {
            zwp_input_method_v2::Event::Activate => {
                input_context.pending_activate = true;
            }
            zwp_input_method_v2::Event::Deactivate => {
                input_context.pending_deactivate = true;
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
                input_context.num_done_events += 1;
                if let Some(keyboard_grab) = &input_context.keyboard_grab {
                    if input_context.pending_deactivate {
                        // We have a keyboard grab, but we are deactivating.
                        keyboard_grab.release();
                        input_context.keyboard_grab = None;
                        input_context.pressed = [xkb::Keycode::default(); 64];
                    }
                } else if input_context.pending_activate {
                    // We don't have a keyboard grab, but we are activating.
                    let keyboard_grab = input_method.grab_keyboard(qh, *seat_index);
                    input_context.keyboard_grab = Some(keyboard_grab);
                }
                input_context.pending_activate = false;
                input_context.pending_deactivate = false;
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
        let input_context = state.get_context(*seat_index);
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
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_seat::Event::Name { name } => {
                eprintln!("Seat name: {}.", name);
            }
            wl_seat::Event::Capabilities {
                capabilities: WEnum::Value(capabilities),
            } => {
                eprintln!("Seat capabilities: {:?}.", capabilities);
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
// We'll ignore the event from ZwpInputPopupSurfaceV2 for now. (Event is "text_input_rectangle".)
delegate_noop!(State: ignore ZwpInputPopupSurfaceV2);
// WlCompositor has no events.
delegate_noop!(State: ignore WlCompositor);
