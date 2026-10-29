//! `rawbytes`: logs the bytes of every event and callback the client hands
//! it, as they arrived and without decoding them (`event <hex>`,
//! `callback <hex>`), so a test can see exactly what a plugin built for an
//! older API version is sent. It registers a loot classifier `rules`
//! (answering nothing) so another plugin can ask it a question.
//!
//! Its exports are written here instead of by `export_plugin!`: the SDK's
//! own would decode the bytes as this SDK's records first.

use openac_plugin::{Host, Plugin, StartInfo};

#[derive(Default)]
pub struct RawBytes;

impl Plugin for RawBytes {
    fn enable(&mut self, host: &mut Host<'_>, _info: &StartInfo) -> Result<(), String> {
        host.register_loot_classifier("rules", "Raw Rules")?;
        Ok(())
    }
}

/// `bytes` as lower-case hex.
#[must_use]
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(target_arch = "wasm32")]
#[allow(unsafe_code)]
mod exports {
    use std::cell::RefCell;

    use ac_plugin_abi::Wire;
    use openac_plugin::__export;
    use openac_plugin::{CallbackAnswer, Host, LogLevel};

    use super::{RawBytes, hex};

    std::thread_local! {
        static PLUGIN: RefCell<Option<RawBytes>> = const { RefCell::new(None) };
        static ANSWER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    #[unsafe(link_section = "openac.api")]
    #[used]
    static OPENAC_API: [u8; __export::API_SECTION_LEN] = __export::API_SECTION_BYTES;

    #[unsafe(link_section = "openac.layout")]
    #[used]
    static OPENAC_LAYOUT: [u8; __export::LAYOUT_SECTION_LEN] = __export::LAYOUT_SECTION_BYTES;

    /// The bytes the client wrote into a buffer `openac_alloc` gave it.
    fn take(ptr: i32, len: i32) -> Box<[u8]> {
        let len = usize::try_from(len).unwrap_or(0);
        let ptr = ptr as usize as *mut u8;
        // SAFETY: the client hands each export a buffer it got from
        // `openac_alloc` (the SDK's `alloc`, a boxed slice of this length),
        // exactly once.
        unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len)) }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn openac_alloc(len: i32) -> i32 {
        __export::alloc(len)
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn openac_enable(ptr: i32, len: i32) -> i64 {
        PLUGIN.with(|slot| __export::enable::<RawBytes>(slot, ptr, len))
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn openac_disable() {
        PLUGIN.with(|slot| __export::disable::<RawBytes>(slot));
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn openac_tick() {
        PLUGIN.with(|slot| __export::tick::<RawBytes>(slot));
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn openac_event(ptr: i32, len: i32) {
        let bytes = take(ptr, len);
        Host::wasm().log(LogLevel::Info, &format!("event {}", hex(&bytes)));
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn openac_callback(ptr: i32, len: i32) -> i64 {
        let bytes = take(ptr, len);
        Host::wasm().log(LogLevel::Info, &format!("callback {}", hex(&bytes)));
        ANSWER.with(|kept| {
            let mut kept = kept.borrow_mut();
            *kept = CallbackAnswer::Nothing.to_bytes();
            let ptr = kept.as_ptr() as usize as i64;
            let len = kept.len() as i64;
            (ptr << 32) | len
        })
    }
}
