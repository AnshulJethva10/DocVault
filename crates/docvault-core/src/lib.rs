use wasm_bindgen::prelude::*;

mod crypto;
mod decoder;
mod encoder;
mod metadata;

// Re-export the WASM-bound functions
pub use decoder::{decode_file, decode_file_multi};
pub use encoder::{encode_file, encode_file_multi};

/// Initialize panic hook for better error messages in the browser console
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// Returns the current version of docvault-core
#[wasm_bindgen]
pub fn get_version() -> String {
    "1.1.0".to_string()
}
