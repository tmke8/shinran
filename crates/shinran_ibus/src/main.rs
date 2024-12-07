use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use async_std::task;
use event_listener::{Event, Listener};
use log::{debug, error, info};
use shinran_backend::{Backend, Configuration};
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
    backend: Arc<Backend<'static>>,
}

#[interface(name = "org.freedesktop.IBus.Factory")]
impl Factory {
    async fn create_engine(
        &mut self,
        #[zbus(object_server)] server: &ObjectServer,
        engine_name: &str,
    ) -> fdo::Result<OwnedObjectPath> {
        info!("CreateEngine: {}", engine_name);
        let path: OwnedObjectPath = ObjectPath::try_from(SERVE_AT)
            .map_err(|_| fdo::Error::BadAddress(SERVE_AT.to_string()))?
            .into();
        info!("Path: {}", path);
        let engine = ShinranEngine::new(self.done.clone(), self.backend.clone());
        server.at(&path, engine).await?;
        Ok(path)
    }
}

// TODO: Replace with a `OnceLock` when we want to actually parse CLI arguments.
static CONFIG: LazyLock<(Configuration, PathBuf)> = LazyLock::new(|| {
    let cli_overrides = HashMap::new();
    Configuration::new(&cli_overrides)
});

#[async_std::main]
async fn main() -> zbus::Result<()> {
    // Set up the logger.
    env_logger::init();
    info!("Program started!");

    // Save config in cache file if it wasn't loaded from cache.
    let cache_write_handle = if !CONFIG.0.loaded_from_cache {
        Some(task::spawn(async {
            let bytes = CONFIG.0.serialize(); // (this is blocking)
            match async_std::fs::write(&CONFIG.1, &bytes).await {
                Ok(()) => debug!("Cache written successfully to '{}'.", CONFIG.1.display()),
                Err(err) => error!("Failed to write cache: {:?}", err),
            }
        }))
    } else {
        None
    };

    // Set up the backend.
    let backend = Backend::new(&CONFIG.0).unwrap();
    // Set up the factory.
    let event = Arc::new(Event::new());
    let factory = Factory {
        done: event.clone(),
        backend: Arc::new(backend),
    };
    let done_listener = event.listen();

    let address: Address = get_ibus_address()?;
    info!("Address: {}", address);
    let _conn = connection::Builder::address(address)?
        .auth_mechanisms(&[AuthMechanism::External, AuthMechanism::Cookie])
        .name(REQUESTED_NAME)?
        .serve_at(FACTORY_PATH, factory)? // To start with, only the factory is registered.
        .build()
        .await?;

    done_listener.wait();

    // Wait for the cache write to finish.
    if let Some(handle) = cache_write_handle {
        handle.await;
    }

    Ok(())
}
