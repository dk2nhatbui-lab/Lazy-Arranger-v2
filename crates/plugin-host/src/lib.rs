//! Isolated FFI boundary for third-party plug-ins.
//!
//! This is the only crate allowed to contain `unsafe` code. Every block here is
//! a C ABI boundary; the rest of the workspace keeps `unsafe_code = "forbid"`.
//! Nothing in this crate loads a plug-in unless a caller explicitly asks for it.

#![allow(unsafe_code)]

mod instance;
mod vst3_instrument;
pub use instance::Vst3Effect;
pub use vst3_instrument::Vst3Instrument;

#[cfg(target_os = "macos")]
mod au;
#[cfg(target_os = "macos")]
pub use au::{AuEffect, AuInstrument};
#[cfg(not(target_os = "macos"))]
mod au_stub;
#[cfg(not(target_os = "macos"))]
pub use au_stub::{AuEffect, AuInstrument};

use std::path::{Path, PathBuf};

/// One VST3 audio-module class read from a bundle.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Vst3Class {
    /// Class id as 32 uppercase hex digits.
    pub cid: String,
    /// Vendor name.
    pub name: String,
    /// Vendor.
    pub vendor: String,
    /// Version string.
    pub version: String,
    /// Subcategories, space separated.
    pub subcategories: String,
}

/// Loads a VST3 bundle and reads its audio-module classes.
///
/// This loads third-party code in-process. A panicking plug-in is caught and
/// returned as an error; a segfaulting or hanging one is not recoverable here,
/// which is why production scanning should use a subprocess.
///
/// # Errors
///
/// Returns a message when the bundle binary, factory, class table or COM
/// interface cannot be read.
pub fn scan_vst3(path: &Path) -> Result<Vec<Vst3Class>, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| scan_vst3_inner(path)))
        .unwrap_or_else(|_| Err(String::from("VST3 scan panicked")))
}

fn vst3_binary(path: &Path) -> Option<PathBuf> {
    let macos = path.join("Contents").join("MacOS");
    let stem = path
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
    let direct = macos.join(&stem);
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(&macos)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|entry| entry.is_file())
}

fn text<const N: usize>(chars: &[std::ffi::c_char; N]) -> String {
    let bytes: Vec<u8> = chars
        .iter()
        .take_while(|value| **value != 0)
        .map(|value| *value as u8)
        .collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Formats a VST3 TUID as uppercase hex.
#[must_use]
pub fn cid_hex(cid: &vst3::Steinberg::TUID) -> String {
    cid.iter()
        .map(|byte| format!("{:02X}", *byte as u8))
        .collect()
}

fn scan_vst3_inner(path: &Path) -> Result<Vec<Vst3Class>, String> {
    use vst3::ComPtr;
    use vst3::Steinberg::{
        IPluginFactory, IPluginFactory2, IPluginFactory2Trait, IPluginFactoryTrait, PClassInfo2,
    };

    type GetFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
    let binary = vst3_binary(path).ok_or_else(|| String::from("no VST3 binary in bundle"))?;
    let library = unsafe { libloading::Library::new(&binary) }
        .map_err(|error| format!("load {}: {error}", binary.display()))?;
    let get_factory: libloading::Symbol<'_, GetFactory> =
        unsafe { library.get(b"GetPluginFactory\0") }
            .map_err(|error| format!("GetPluginFactory: {error}"))?;
    let factory = unsafe { ComPtr::from_raw(get_factory()) }
        .ok_or_else(|| String::from("GetPluginFactory returned null"))?;
    let factory2 = factory
        .cast::<IPluginFactory2>()
        .ok_or_else(|| String::from("factory has no IPluginFactory2"))?;
    let count = unsafe { factory2.countClasses() }.max(0);
    let mut classes = Vec::new();
    for index in 0..count {
        let mut info: PClassInfo2 = unsafe { std::mem::zeroed() };
        let result = unsafe { factory2.getClassInfo2(index, &mut info) };
        if result != 0 {
            continue;
        }
        if text(&info.category) != "Audio Module Class" {
            continue;
        }
        classes.push(Vst3Class {
            cid: cid_hex(&info.cid),
            name: text(&info.name),
            vendor: text(&info.vendor),
            version: text(&info.version),
            subcategories: text(&info.subCategories),
        });
    }
    drop(factory2);
    drop(factory);
    drop(library);
    if classes.is_empty() {
        Err(String::from("no audio module class"))
    } else {
        Ok(classes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cids_are_uppercase_hex() {
        let cid = [0_u8 as std::ffi::c_char; 16];
        assert_eq!(cid_hex(&cid), "00000000000000000000000000000000");
    }

    #[test]
    #[ignore = "loads a plug-in from this machine"]
    fn a_real_vst3_bundle_reports_classes() {
        let classes =
            scan_vst3(Path::new("/Library/Audio/Plug-Ins/VST3/R0Verb.vst3")).expect("scan");
        assert!(!classes.is_empty());
        assert!(!classes[0].cid.is_empty());
    }
}
