#[link(wasm_import_module = "env")]
extern "C" {
    fn now_ms() -> f64;
}
pub struct Instant(f64);
impl Instant {
    pub fn now() -> Self {
        Self(unsafe { now_ms() })
    }
    pub fn elapsed(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(((unsafe { now_ms() } - self.0) / 1000.0).max(0.0))
    }
}
