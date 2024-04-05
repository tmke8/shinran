use event_listener::{Event, Listener};
use zbus::connection;
use zbus::object_server::SignalContext;
use zbus::{interface, AuthMechanism};

mod address;
// mod input_context;

const REQUEST_NAME: &str = "org.freedesktop.IBus.Shinran";
const SERVE_AT: &str = "/org/freedesktop/IBus/Engine/Shinran";

struct ShinranEngine {
    done: Event,
    text: String,
    cursor_pos: u32,
}

#[interface(name = "org.freedesktop.IBus.Engine")]
impl ShinranEngine {
    /// ProcessKeyEvent method
    async fn process_key_event(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        keyval: u32,
        keycode: u32,
        state: u32,
    ) -> bool {
        println!("process_key_event: keyval={}, keycode={}, state={}", keyval, keycode, state);
        true
    }
    ///
    /// FocusIn method
    fn focus_in(&self) {}

    /// FocusOut method
    fn focus_out(&self) {}

    /// Destroy method
    async fn destroy(&self) {}

    /// UpdatePreeditText signal
    #[zbus(signal)]
    async fn update_preedit_text(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
        cursor_pos: u32,
        visible: bool,
    ) -> zbus::Result<()>;

    /// CommitText signal
    #[zbus(signal)]
    async fn commit_text(
        ctxt: &SignalContext<'_>,
        text: zbus::zvariant::Value<'_>,
    ) -> zbus::Result<()>;
}

#[async_std::main]
async fn main() -> zbus::Result<()> {
    let engine = ShinranEngine {
        done: Event::new(),
        text: "".to_string(),
        cursor_pos: 0,
    };
    let done_listener = engine.done.listen();

    let address = address::get_address()?;
    let conn = connection::Builder::address(address.as_str())?
        .auth_mechanisms(&[AuthMechanism::External, AuthMechanism::Cookie])
        .name(REQUEST_NAME)?
        .serve_at(SERVE_AT, engine)?
        .build()
        .await?;

    done_listener.wait();

    Ok(())
}
