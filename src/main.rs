use std::sync::Arc;
use std::time::Duration;

use async_std::task::sleep;
use event_listener::{Event, Listener};
use zbus::object_server::SignalContext;
use zbus::zvariant::{ObjectPath, OwnedObjectPath};
use zbus::{connection, fdo, Address, ObjectServer};
use zbus::{interface, AuthMechanism};

mod address;
mod constants;
mod text;

const REQUESTED_NAME: &str = "org.freedesktop.IBus.Shinran";
const SERVE_AT: &str = "/org/freedesktop/IBus/Engine/Shinran";
const FACTORY_PATH: &str = "/org/freedesktop/IBus/Factory";

struct Factory {
    done: Arc<Event>,
}

#[interface(name = "org.freedesktop.IBus.Factory")]
impl Factory {
    async fn create_engine(
        &mut self,
        #[zbus(object_server)] server: &ObjectServer,
        engine_name: &str,
    ) -> fdo::Result<OwnedObjectPath> {
        println!("CreateEngine: {}", engine_name);
        let path: OwnedObjectPath = ObjectPath::try_from(SERVE_AT)
            .map_err(|_| fdo::Error::BadAddress(SERVE_AT.to_string()))?
            .into();
        println!("Path: {}", path);
        let engine = ShinranEngine {
            done: self.done.clone(),
            text: "".to_string(),
            cursor_pos: 0,
        };
        server.at(&path, engine).await?;
        return Ok(path);
    }
}

struct ShinranEngine {
    done: Arc<Event>,
    text: String,
    cursor_pos: u32,
}

#[interface(name = "org.freedesktop.IBus.Engine")]
impl ShinranEngine {
    /// ProcessKeyEvent method
    async fn process_key_event(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        keyval: u32,
        keycode: u32,
        state: u32,
    ) -> fdo::Result<bool> {
        println!(
            "ProcessKeyEvent: keyval={}, keycode={}, state={}",
            keyval, keycode, state
        );
        if state & constants::RELEASE_MASK != 0 {
            println!("Key released");
            return Ok(true);
        }
        match keyval {
            constants::KEY_BACK_SPACE => {
                if self.cursor_pos > 0 {
                    self.text.remove(self.cursor_pos as usize - 1);
                    self.cursor_pos -= 1;
                    update_text(self, ctxt).await?;
                }
                return Ok(true);
            }
            _ => {}
        }

        let character = char::from_u32(keyval);

        if let Some(character) = character {
        }
        Ok(true)
    }

    /// FocusIn method
    fn focus_in(&self) {}

    /// FocusOut method
    async fn focus_out(&self) {
        println!("FocusOut");
        sleep(Duration::from_millis(100)).await;
        self.done.notify(1);
    }

    /// Destroy method
    fn destroy(&self) {
        println!("Destroy");
        self.done.notify(1);
    }

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

async fn update_text(engine: &ShinranEngine, ctxt: SignalContext<'_>) -> zbus::Result<()> {
    println!(
        "UpdateText(text = '{}', cursorPos = {})",
        engine.text, engine.cursor_pos,
    );

    let attr = text::Attribute {
        type_: text::IBUS_ATTR_TYPE_UNDERLINE,
        value: text::IBUS_ATTR_UNDERLINE_SINGLE,
        start_index: 0,
        end_index: engine.text.len() as u32,
    };
    let attr_list: [text::Attribute; 1] = [attr];
    let ibus_text = text::text(&engine.text, &attr_list);

    ShinranEngine::update_preedit_text(&ctxt, ibus_text, engine.cursor_pos, engine.text != "")
        .await?;
    Ok(())
}

async fn clear_text(engine: &mut ShinranEngine, ctxt: SignalContext<'_>) -> zbus::Result<()> {
    engine.text.clear();
    engine.cursor_pos = 0;
    update_text(engine, ctxt).await?;
    Ok(())
}

#[async_std::main]
async fn main() -> zbus::Result<()> {
    println!("Program started!");
    let event = Arc::new(Event::new());
    let factory = Factory {
        done: event.clone(),
    };
    let done_listener = event.listen();

    let address: Address = address::get_ibus_address()?.as_str().try_into()?;
    println!("Address: {}", address);
    let _conn = connection::Builder::address(address)?
        .auth_mechanisms(&[AuthMechanism::External, AuthMechanism::Cookie])
        .name(REQUESTED_NAME)?
        .serve_at(FACTORY_PATH, factory)? // To start with, only the factory is registered.
        .build()
        .await?;

    done_listener.wait();

    Ok(())
}
