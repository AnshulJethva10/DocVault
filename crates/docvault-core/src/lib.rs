#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

pub mod crypto;
pub mod decoder;
pub mod encoder;
pub mod metadata;
pub mod png;

// Re-export WASM-bound entry points only when the "wasm" feature is active
#[cfg(feature = "wasm")]
pub use decoder::{decode_file, decode_file_multi, PngFrameDecoder};
#[cfg(feature = "wasm")]
pub use encoder::{encode_file, encode_file_multi, encode_file_png};

/// Initialise the panic hook for better error messages in the browser console.
/// Only relevant (and compiled) for WASM targets.
#[cfg(feature = "wasm")]
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Returns the current version of docvault-core.
#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
