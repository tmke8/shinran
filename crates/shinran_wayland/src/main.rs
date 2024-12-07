// Largely based on
// https://github.com/tadeokondrak/anthywl/blob/21e290b0bb07c1dfe198d477c3f07b500ddc93ba/include/anthywl.h
// https://github.com/tadeokondrak/anthywl/blob/21e290b0bb07c1dfe198d477c3f07b500ddc93ba/src/anthywl.c
// https://github.com/emersion/wlhangul/blob/bd2758227779d7748dea185c38cab11665d55502/include/wlhangul.h
// https://github.com/emersion/wlhangul/blob/bd2758227779d7748dea185c38cab11665d55502/main.c

use std::{
    collections::HashMap, os::unix::io::AsFd, path::PathBuf, rc::Rc, sync::LazyLock, time::Duration,
};

use calloop::{timer::Timer, Dispatcher, EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use log::{debug, error, info};
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
use xkbcommon::xkb;

use shinran_backend::{Backend, Configuration};

mod input_context;

use input_context::{InputContext, RepeatTimer};

// TODO: Replace with a `OnceLock` when we want to actually parse CLI arguments.
static CONFIG: LazyLock<(Configuration, PathBuf)> = LazyLock::new(|| {
    let cli_overrides = HashMap::new();
    Configuration::new(&cli_overrides)
});

fn main() {
    // Set up the logger.
    env_logger::init();

    if !CONFIG.0.loaded_from_cache {
        debug!("Save config in cache file {}.", CONFIG.1.display());
        let bytes = CONFIG.0.serialize();
        std::fs::write(&CONFIG.1, &bytes)
            .unwrap_or_else(|err| error!("Failed to save cache: {:?}", err));
    }

    // Set up the backend.
    let backend = Backend::new(&CONFIG.0).unwrap();

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
    info!("Round trip complete.");

    // All the globals should be initialized now, so we can start initializing the protocols.
    init_protocols(&mut state, &qh, Rc::new(backend));
    info!("Protocols initialized.");

    // Insert the `event_queue` into the calloop's event loop.
    WaylandSource::new(conn, event_queue)
        .insert(loop_handle)
        .expect("Insert Wayland source into calloop.");

    // This will start dispatching the event loop and processing pending wayland requests.
    while state.running {
        event_loop.dispatch(None, &mut state).unwrap();
    }
    info!("Shutting down...");
    // Wait a bit to see if there are any pending requests.
    event_loop
        .dispatch(Duration::from_millis(200), &mut state)
        .unwrap();
}

/// Initialize the protocols we need for the input method.
///
/// This can only be done after we have received all the global objects from the server.
fn init_protocols(state: &mut State, qh: &QueueHandle<State>, backend: Rc<Backend<'static>>) {
    let Some(input_method_manager) = &state.input_method_manager else {
        panic!("Compositor does not support zwp_input_method_manager_v2");
    };

    let Some(virtual_keyboard_manager) = &state.virtual_keyboard_manager else {
        panic!("Compositor does not support zwp_virtual_keyboard_manager_v1");
    };

    let Some(wl_compositor) = &state.wl_compositor else {
        panic!("Compositor does not support wl_compositor");
    };

    for (seat, seat_id) in state.seats.iter() {
        state.contexts.insert_with_key(|seat_index| {
            // We have to be a bit mindful of race conditions here.
            // What we are doing here is creating a new input method and virtual keyboard for each seat,
            // and we pass the seat index to the input method and virtual keyboard.
            // However, the context object that the seat index refers to is not initialized yet here.
            // It is only initialized at the end of the loop body.
            // I *think* this is fine because when `init_protocols()` is called,
            // the event queue hasn't been dispatched yet, so the context object should not be accessed
            // by events on `input_method` and `virtual_keyboard`.
            let input_method = input_method_manager.get_input_method(seat, qh, seat_index);
            let virtual_keyboard = virtual_keyboard_manager.create_virtual_keyboard(seat, qh, ());
            let wl_surface = wl_compositor.create_surface(qh, seat_index);
            let popup_surface = input_method.get_input_popup_surface(&wl_surface, qh, ());
            let backend = backend.clone();
            InputContext::new(
                *seat_id,
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

    seats: Vec<(WlSeat, u32)>,
    contexts: SlotMap<SeatIndex, InputContext<State>>,
    // configured: bool,
    loop_handle: LoopHandle<'static, State>,
}

impl State {
    fn get_context(&mut self, seat_index: SeatIndex) -> &mut InputContext<State> {
        &mut self.contexts[seat_index]
    }
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
                name: id,
                interface,
                version: _version,
            } => match &interface[..] {
                "wl_seat" => {
                    let seat = registry.bind::<WlSeat, _, _>(id, 2, qh, id);
                    // Collect all seats.
                    state.seats.push((seat, id));
                }
                "wl_compositor" => {
                    let compositor = registry.bind::<WlCompositor, _, _>(id, 4, qh, ());
                    state.wl_compositor = Some(compositor);
                }
                "zwp_input_method_manager_v2" => {
                    let input_man = registry.bind::<ZwpInputMethodManagerV2, _, _>(id, 1, qh, ());
                    state.input_method_manager = Some(input_man);
                }
                "zwp_virtual_keyboard_manager_v1" => {
                    let keyboard_man =
                        registry.bind::<ZwpVirtualKeyboardManagerV1, _, _>(id, 1, qh, ());
                    state.virtual_keyboard_manager = Some(keyboard_man);
                }
                _ => {}
            },
            wl_registry::Event::GlobalRemove { name: id } => {
                // Iterating over these two vectors is somewhat expensive,
                // but this event should be very rare.
                if let Some(index) = state.seats.iter().position(|(_, seat_id)| *seat_id == id) {
                    state.seats.swap_remove(index);
                }
                // Retain only the contexts that do not correspond to the removed seat.
                state.contexts.retain(|_, context| context.seat_id != id);
            }
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

                debug!("Key {} was {:?}.", key + SCANCODE_OFFSET, key_state);
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
                            info!("Shutting down.");
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
                        debug!("Update repeat timer for {}", key + SCANCODE_OFFSET);
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
                    debug!("Delete repeat timer for {}", key + SCANCODE_OFFSET);
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
                            info!("Shutting down.");
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
                    debug!("Repeat timer set for {}", key + SCANCODE_OFFSET);
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
                    debug!("Forwarded key {}", key + SCANCODE_OFFSET);
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
                    error!("Failed to compile keymap.");
                    return;
                }
                let xkb_state = xkb::State::new(input_context.xkb_keymap.as_ref().unwrap());
                if xkb_state.get_raw_ptr().is_null() {
                    error!("Failed to create xkb state.");
                }
                input_context.xkb_state = Some(xkb_state);
            }
            zwp_input_method_keyboard_grab_v2::Event::RepeatInfo { rate, delay } => {
                let input_context = state.get_context(*seat_index);
                input_context.repeat_rate = Some(Duration::from_millis(rate as u64));
                input_context.repeat_delay = Some(Duration::from_millis(delay as u64));
                debug!("Repeat rate: {} ms, delay: {} ms.", rate, delay);
            }
            _ => unreachable!("Unknown event."),
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
            _ => unreachable!("Unknown event."),
        }
    }
}

impl Dispatch<WlSurface, SeatIndex> for State {
    fn event(
        _state: &mut Self,
        _wl_surface: &WlSurface,
        event: wl_surface::Event,
        _seat_index: &SeatIndex,
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // let input_context = state.get_context(*seat_index);
        match event {
            wl_surface::Event::Enter { .. } => {
                todo!("Enter event.");
            }
            wl_surface::Event::Leave { .. } => {
                todo!("Leave event.");
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, u32> for State {
    fn event(
        state: &mut Self,
        _seat: &WlSeat,
        event: wl_seat::Event,
        seat_id: &u32,
        _: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_seat::Event::Name { name } => {
                info!("Seat name: {}.", name);
                // Find the context with the given seat id and set the name.
                if let Some(context) = state.contexts.values_mut().find(|c| c.seat_id == *seat_id) {
                    context.seat_name = Some(name);
                };
            }
            wl_seat::Event::Capabilities {
                capabilities: WEnum::Value(capabilities),
            } => {
                info!("Seat capabilities: {:?}.", capabilities);
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
