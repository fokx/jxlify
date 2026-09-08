pub mod cache;
pub mod config;
pub mod encoder;
pub mod handler;
pub mod helper;
pub mod negotiation;

pub fn init() {
    jxl_image_rs_integration::register_image_decoding_hook();
}

