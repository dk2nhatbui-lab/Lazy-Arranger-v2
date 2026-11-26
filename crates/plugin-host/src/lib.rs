//! Isolated FFI boundary for third-party plug-ins.
//!
//! This is the only crate allowed to contain `unsafe` code. Every block here is
//! a C ABI boundary; the rest of the workspace keeps `unsafe_code = "forbid"`.
//! Nothing in this crate loads a plug-in unless a caller explicitly asks for it.
//! Each module states the rules its `unsafe` blocks rely on under `# Safety`.

#![allow(unsafe_code)]

mod instance;
mod module;
mod vst3_instrument;
pub use instance::{Vst3Effect, Vst3ViewInfo};
pub use vst3_instrument::Vst3Instrument;

#[cfg(target_os = "macos")]
mod au;
#[cfg(target_os = "macos")]
pub use au::{AuEffect, AuInstrument};
#[cfg(not(target_os = "macos"))]
mod au_stub;
#[cfg(not(target_os = "macos"))]
pub use au_stub::{AuEffect, AuInstrument};

#[cfg(target_os = "macos")]
mod editor;
#[cfg(target_os = "macos")]
pub use editor::{AuEditor, Vst3Editor};
#[cfg(not(target_os = "macos"))]
mod editor_stub;
#[cfg(not(target_os = "macos"))]
pub use editor_stub::{AuEditor, Vst3Editor};

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

/// The bundle sub-folder that holds this platform's VST3 binary.
fn vst3_arch_folder() -> &'static str {
    if cfg!(target_os = "macos") {
        "MacOS"
    } else if cfg!(target_os = "windows") {
        if cfg!(target_arch = "aarch64") {
            "arm64-win"
        } else {
            "x86_64-win"
        }
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-linux"
    } else {
        "x86_64-linux"
    }
}

/// The plug-in binary inside a VST3 bundle, for this platform.
///
/// The VST3 bundle layout names the binary after the bundle: `MacOS/<name>`,
/// `x86_64-linux/<name>.so` or `x86_64-win/<name>.vst3`. Only that file is
/// loaded, never some other file that happens to sit in the folder.
pub(crate) fn vst3_binary(path: &Path) -> Option<PathBuf> {
    let folder = path.join("Contents").join(vst3_arch_folder());
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    let name = if cfg!(target_os = "macos") {
        stem
    } else if cfg!(target_os = "windows") {
        format!("{stem}.vst3")
    } else {
        format!("{stem}.so")
    };
    let binary = folder.join(name);
    binary.is_file().then_some(binary)
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
        IPluginFactory, IPluginFactory2, IPluginFactory2Trait, IPluginFactoryTrait, PClassInfo,
        PClassInfo2, PFactoryInfo,
    };

    type GetFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
    let binary = vst3_binary(path).ok_or_else(|| String::from("no VST3 binary in bundle"))?;
    let library = module::Module::open(&binary)?;
    // SAFETY: the VST3 SDK declares `IPluginFactory* GetPluginFactory()`.
    let get_factory: libloading::Symbol<'_, GetFactory> =
        unsafe { library.get(b"GetPluginFactory\0") }
            .map_err(|error| format!("GetPluginFactory: {error}"))?;
    let factory = unsafe { ComPtr::from_raw(get_factory()) }
        .ok_or_else(|| String::from("GetPluginFactory returned null"))?;
    let mut classes = Vec::new();
    if let Some(factory2) = factory.cast::<IPluginFactory2>() {
        let count = unsafe { factory2.countClasses() }.max(0);
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
    } else {
        let mut factory_info: PFactoryInfo = unsafe { std::mem::zeroed() };
        let vendor = if unsafe { factory.getFactoryInfo(&mut factory_info) } == 0 {
            text(&factory_info.vendor)
        } else {
            String::new()
        };
        let count = unsafe { factory.countClasses() }.max(0);
        for index in 0..count {
            let mut info: PClassInfo = unsafe { std::mem::zeroed() };
            let result = unsafe { factory.getClassInfo(index, &mut info) };
            if result != 0 {
                continue;
            }
            if text(&info.category) != "Audio Module Class" {
                continue;
            }
            classes.push(Vst3Class {
                cid: cid_hex(&info.cid),
                name: text(&info.name),
                vendor: vendor.clone(),
                version: String::new(),
                subcategories: String::new(),
            });
        }
    }
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
