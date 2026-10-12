#![allow(dead_code)]
mod engine;

// Buffers stay owned by Rust. JS copies the response before the next request.
#[cfg(target_arch = "wasm32")]
mod browser_api {
    use std::cell::RefCell;
    thread_local! {
        static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
    #[no_mangle]
    pub extern "C" fn input_buffer(len: usize) -> *mut u8 {
        if len > 8192 {
            return std::ptr::null_mut();
        }
        INPUT.with(|input| {
            let mut input = input.borrow_mut();
            input.resize(len, 0);
            input.as_mut_ptr()
        })
    }
    #[no_mangle]
    pub extern "C" fn game_request() -> *const u8 {
        let result = INPUT.with(|input| {
            let input = input.borrow();
            std::str::from_utf8(&input)
                .map_err(|_| "Invalid UTF-8")
                .and_then(crate::engine::game::state)
        });
        OUTPUT.with(|output| {
            let mut output = output.borrow_mut();
            *output = result
                .unwrap_or_else(|_| "{\"error\":\"Invalid game request\"}".into())
                .into_bytes();
            output.as_ptr()
        })
    }
    #[no_mangle]
    pub extern "C" fn output_len() -> usize {
        OUTPUT.with(|output| output.borrow().len())
    }
}
