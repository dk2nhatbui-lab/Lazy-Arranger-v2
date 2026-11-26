//! A loaded VST3 binary with the platform entry and exit calls the SDK asks
//! hosts to make: `ModuleEntry`/`ModuleExit` on Linux and `InitDll`/`ExitDll`
//! on Windows. macOS uses `bundleEntry`, which needs a `CFBundleRef`; plug-ins
//! there load without it, as before.

use std::path::Path;

/// Owns the library; exits the module before unloading it. Keep it as the
/// last field of its owner so every COM pointer into the plug-in drops first.
pub(crate) struct Module {
    library: Option<libloading::Library>,
    entered: bool,
}

impl Module {
    /// Loads `binary` and calls its entry function when it has one.
    pub(crate) fn open(binary: &Path) -> Result<Self, String> {
        // SAFETY: loading runs the plug-in's static initialisers. The caller
        // asked for this plug-in; nothing else can make it safe.
        let library = unsafe { libloading::Library::new(binary) }
            .map_err(|error| format!("load {}: {error}", binary.display()))?;
        let (library, entered) = enter(library)?;
        Ok(Self {
            library: Some(library),
            entered,
        })
    }

    /// Looks up a symbol in the binary.
    ///
    /// # Safety
    /// `T` must match the symbol's real type.
    pub(crate) unsafe fn get<T>(&self, name: &[u8]) -> Result<libloading::Symbol<'_, T>, String> {
        let library = self
            .library
            .as_ref()
            .ok_or_else(|| String::from("module closed"))?;
        // SAFETY: the caller promises `T` is the symbol's type.
        unsafe { library.get(name) }.map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "linux")]
fn enter(library: libloading::Library) -> Result<(libloading::Library, bool), String> {
    type Entry = unsafe extern "C" fn(*mut std::ffi::c_void) -> bool;
    let raw = libloading::os::unix::Library::from(library).into_raw();
    // SAFETY: `raw` came from `into_raw` just above and is used once.
    let library =
        libloading::Library::from(unsafe { libloading::os::unix::Library::from_raw(raw) });
    // SAFETY: the VST3 SDK declares `bool ModuleEntry(void*)`.
    let entry: Option<Entry> = unsafe { library.get::<Entry>(b"ModuleEntry\0") }
        .ok()
        .map(|symbol| *symbol);
    match entry {
        // SAFETY: the plug-in gets its own dlopen handle, as the SDK asks.
        Some(entry) => {
            if unsafe { entry(raw) } {
                Ok((library, true))
            } else {
                Err(String::from("ModuleEntry failed"))
            }
        }
        None => Ok((library, false)),
    }
}

#[cfg(target_os = "windows")]
fn enter(library: libloading::Library) -> Result<(libloading::Library, bool), String> {
    type Entry = unsafe extern "system" fn() -> bool;
    // SAFETY: the VST3 SDK declares `bool InitDll()`.
    let entry: Option<Entry> = unsafe { library.get::<Entry>(b"InitDll\0") }
        .ok()
        .map(|symbol| *symbol);
    match entry {
        // SAFETY: called once, before any other plug-in call.
        Some(entry) => {
            if unsafe { entry() } {
                Ok((library, true))
            } else {
                Err(String::from("InitDll failed"))
            }
        }
        None => Ok((library, false)),
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn enter(library: libloading::Library) -> Result<(libloading::Library, bool), String> {
    Ok((library, false))
}

#[cfg(target_os = "linux")]
const EXIT: &[u8] = b"ModuleExit\0";
#[cfg(target_os = "windows")]
const EXIT: &[u8] = b"ExitDll\0";

impl Drop for Module {
    fn drop(&mut self) {
        let Some(library) = self.library.take() else {
            return;
        };
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if self.entered {
            #[cfg(target_os = "linux")]
            type Exit = unsafe extern "C" fn() -> bool;
            #[cfg(target_os = "windows")]
            type Exit = unsafe extern "system" fn() -> bool;
            // SAFETY: the SDK declares the exit function as `bool f()`; the
            // owner dropped every COM pointer before this field.
            if let Ok(exit) = unsafe { library.get::<Exit>(EXIT) } {
                let _ = unsafe { exit() };
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        let _ = self.entered;
        drop(library);
    }
}
