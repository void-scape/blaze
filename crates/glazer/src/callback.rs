pub struct FnPtrs {
    #[allow(unused)]
    reloading: hot_reloading::HotReloading,
    handle_input: *mut core::ffi::c_void,
    update_and_render: *mut core::ffi::c_void,
    #[cfg(feature = "opengl")]
    initialize_opengl: *mut core::ffi::c_void,
}

impl FnPtrs {
    #[cfg(feature = "software")]
    pub fn new<Memory, Pixels>(
        handle_input: fn(crate::PlatformInput<Memory>),
        update_and_render: fn(crate::PlatformUpdate<Memory, Pixels>),
        path: Option<&str>,
    ) -> Self {
        Self {
            reloading: hot_reloading::HotReloading::from_path(path),
            handle_input: handle_input as *mut core::ffi::c_void,
            update_and_render: update_and_render as *mut core::ffi::c_void,
            #[cfg(feature = "opengl")]
            initialize_opengl: core::ptr::null_mut(),
        }
    }

    #[cfg(feature = "opengl")]
    pub fn new<Memory>(
        handle_input: fn(crate::PlatformInput<Memory>),
        update_and_render: fn(crate::PlatformUpdate<Memory>),
        initialize_opengl: fn(&dyn Fn(&'static str) -> *const core::ffi::c_void),
        path: Option<&str>,
    ) -> Self {
        Self {
            reloading: hot_reloading::HotReloading::from_path(path),
            handle_input: handle_input as *mut core::ffi::c_void,
            update_and_render: update_and_render as *mut core::ffi::c_void,
            initialize_opengl: initialize_opengl as *mut core::ffi::c_void,
        }
    }

    pub fn handle_input<Memory>(&self, input: crate::PlatformInput<Memory>) {
        unsafe {
            let handle_input = core::mem::transmute::<
                *mut core::ffi::c_void,
                fn(crate::PlatformInput<Memory>),
            >(self.handle_input);
            handle_input(input);
        }
    }

    pub fn update_and_render<Input>(&self, input: Input) {
        unsafe {
            let update_and_render =
                core::mem::transmute::<*mut core::ffi::c_void, fn(Input)>(self.update_and_render);
            update_and_render(input);
        }
    }

    #[cfg(feature = "opengl")]
    pub fn initialize_opengl(&self, loader: &dyn Fn(&'static str) -> *const core::ffi::c_void) {
        unsafe {
            let initialize_opengl = core::mem::transmute::<
                *mut core::ffi::c_void,
                fn(&dyn Fn(&'static str) -> *const core::ffi::c_void),
            >(self.initialize_opengl);
            initialize_opengl(loader);
        }
    }
}

#[cfg(not(feature = "hot-reload"))]
mod hot_reloading {
    use super::*;
    pub struct HotReloading;
    impl HotReloading {
        pub fn from_path(_: Option<&str>) -> Self {
            Self
        }
    }
    impl FnPtrs {
        pub fn reload(&mut self) -> bool {
            false
        }
    }
}

#[cfg(feature = "hot-reload")]
mod hot_reloading {
    use super::*;
    use alloc::ffi::CString;
    extern crate std;
    pub struct HotReloading {
        dylib: *mut core::ffi::c_void,
        path: Option<alloc::string::String>,
        loaded: std::time::SystemTime,
    }
    impl HotReloading {
        pub fn from_path(path: Option<&str>) -> Self {
            use alloc::string::ToString;
            Self {
                dylib: core::ptr::null_mut(),
                path: path.map(|inner| inner.to_string()),
                loaded: std::time::SystemTime::now(),
            }
        }
    }
    impl FnPtrs {
        pub fn reload(&mut self) -> bool {
            let Some(path) = self.reloading.path.as_deref() else {
                return false;
            };
            let Some(modified) = std::fs::metadata(path).ok().and_then(|meta| {
                meta.modified().ok().and_then(|modified| {
                    modified
                        .duration_since(self.reloading.loaded)
                        .is_ok_and(|dur| !dur.is_zero())
                        .then_some(modified)
                })
            }) else {
                return false;
            };

            if !self.reloading.dylib.is_null() {
                // NOTE: This does nothing on macos.
                debug_assert_eq!(unsafe { libc::dlclose(self.reloading.dylib) }, 0);
            }
            self.reloading.loaded = modified;

            crate::log!("loading functions from {path}");
            let mut copy = std::path::PathBuf::from(path);
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap();
            copy.pop();
            copy.push(alloc::format!("{}", time.as_millis()));
            // NOTE: need to copy path on macos to prevent dylib caching
            std::fs::copy(path, &copy).expect("failed to copy dynamic library");
            let filename = CString::new(copy.to_str().unwrap()).unwrap();

            let dylib =
                unsafe { libc::dlopen(filename.as_ptr(), libc::RTLD_LOCAL | libc::RTLD_LAZY) };
            if !dylib.is_null() {
                let symbol = unsafe { libc::dlsym(dylib, c"update_and_render".as_ptr().cast()) };
                if !symbol.is_null() {
                    self.update_and_render = symbol;

                    let symbol = unsafe { libc::dlsym(dylib, c"handle_input".as_ptr().cast()) };
                    if !symbol.is_null() {
                        self.handle_input = symbol;

                        #[cfg(feature = "opengl")]
                        if !self.initialize_opengl.is_null() {
                            let symbol =
                                unsafe { libc::dlsym(dylib, c"initialize_opengl".as_ptr().cast()) };
                            if !symbol.is_null() {
                                self.initialize_opengl = symbol;
                            } else {
                                err("failed to load symbol initialize_opengl");
                            }
                        }
                    } else {
                        err("failed to load symbol handle_input");
                    }
                } else {
                    err("failed to load symbol update_and_render");
                }
            } else {
                err(&alloc::format!("failed to open {path}"));
            }

            fn err(msg: &str) {
                let str = unsafe { core::ffi::CStr::from_ptr(libc::dlerror()) };
                crate::log!("ERROR: {}: {}", msg, str.to_str().unwrap());
            }

            true
        }
    }
}
