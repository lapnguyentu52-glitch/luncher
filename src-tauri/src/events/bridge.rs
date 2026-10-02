//! Event bridge — bơm envelope từ EventHub (Rust core) sang webview qua Tauri emit.
//!
//! §93/§200: producer publish vào hub (bounded, không block); bridge thread drain
//! theo interval và emit batch lên UI. UI side có ingestion pipeline riêng với QoS.

use std::sync::Arc;
use std::time::Duration;

use tauri::Emitter;

use crate::protocol::event::EVENT_CHANNEL;

pub fn spawn(app: tauri::AppHandle, state: Arc<antares_core::AppState>) {
    std::thread::Builder::new()
        .name("antares-event-bridge".into())
        .spawn(move || loop {
            let batch = state.events.drain();
            if !batch.is_empty() {
                for envelope in batch {
                    if let Err(err) = app.emit(EVENT_CHANNEL, &envelope) {
                        log::warn!("event emit failed: {err}");
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        })
        .expect("spawn event bridge thread");
}
