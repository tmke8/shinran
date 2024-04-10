use std::sync::Arc;

use event_listener::{Event, Listener};
use zbus::zvariant::{ObjectPath, OwnedObjectPath};
use zbus::{connection, fdo, Address, ObjectServer};
use zbus::{interface, AuthMechanism};

use ibus_utils::get_ibus_address;

use crate::engine::ShinranEngine;

mod engine;

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
        let engine = ShinranEngine::new(self.done.clone());
        server.at(&path, engine).await?;
        Ok(path)
    }
}

#[async_std::main]
async fn main() -> zbus::Result<()> {
    println!("Program started!");
    let event = Arc::new(Event::new());
    let factory = Factory {
        done: event.clone(),
    };
    let done_listener = event.listen();

    let address: Address = get_ibus_address()?;
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
