#[derive(Clone, Copy)]
pub struct Time(inner::Time);

#[cfg(target_arch = "wasm32")]
mod inner {
    pub type Time = f64;
    impl super::Time {
        pub fn now() -> Self {
            Self(web_sys::window().unwrap().performance().unwrap().now())
        }

        pub fn elapsed_secs(self, earlier: Self) -> f32 {
            ((self.0 - earlier.0).abs() / 1_000.0) as f32
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod inner {
    extern crate std;
    pub type Time = std::time::SystemTime;
    impl super::Time {
        pub fn now() -> Self {
            Self(std::time::SystemTime::now())
        }

        pub fn elapsed_secs(self, earlier: Self) -> f32 {
            self.0
                .duration_since(earlier.0)
                .unwrap_or_default()
                .as_secs_f32()
        }
    }
}
