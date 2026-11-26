//! Native macOS NSWindow / NSView plug-in editor hosting for VST3 and Audio Units.
//!
//! Must be called on the macOS main thread (which AppKit requires for all
//! `NSWindow` and `NSView` operations).

use std::ffi::{CString, c_void};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, Ordering};

use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{Encode, Encoding, msg_send};
use vst3::Steinberg::Vst::{IEditControllerTrait, ViewType};
use vst3::Steinberg::{
    IPlugFrame, IPlugFrameTrait, IPlugView, IPlugViewTrait, ViewRect, kPlatformTypeNSView,
    kResultOk, tresult,
};
use vst3::{Class, ComPtr, ComWrapper};

use crate::{AuEffect, AuInstrument, Vst3Effect};

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "CoreAudioKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    fn AudioUnitGetProperty(
        unit: *mut c_void,
        property: u32,
        scope: u32,
        element: u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
}

#[repr(C)]
struct OpaqueAudioComponentInstance {
    _private: [u8; 0],
}

unsafe impl objc2::RefEncode for OpaqueAudioComponentInstance {
    const ENCODING_REF: Encoding =
        Encoding::Pointer(&Encoding::Struct("OpaqueAudioComponentInstance", &[]));
}

const K_AUDIO_UNIT_PROPERTY_COCOA_UI: u32 = 31;
const K_AUDIO_UNIT_SCOPE_GLOBAL: u32 = 0;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSPoint {
    x: f64,
    y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSSize {
    width: f64,
    height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
struct NSRect {
    origin: NSPoint,
    size: NSSize,
}

unsafe impl Encode for NSPoint {
    const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
}

unsafe impl Encode for NSSize {
    const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
}

unsafe impl Encode for NSRect {
    const ENCODING: Encoding = Encoding::Struct("CGRect", &[NSPoint::ENCODING, NSSize::ENCODING]);
}

fn ensure_main_thread() -> Result<(), String> {
    let cls = AnyClass::get(c"NSThread").ok_or_else(|| String::from("NSThread unavailable"))?;
    let is_main: Bool = unsafe { msg_send![cls, isMainThread] };
    if is_main.as_bool() {
        Ok(())
    } else {
        Err(String::from(
            "native plug-in editor windows must be opened on the main thread",
        ))
    }
}

unsafe fn nsstring(text: &str) -> Result<*mut AnyObject, String> {
    let cls = AnyClass::get(c"NSString").ok_or_else(|| String::from("NSString unavailable"))?;
    let cstr = CString::new(text).unwrap_or_else(|_| CString::new("Plug-in").unwrap_or_default());
    let obj: *mut AnyObject = msg_send![cls, stringWithUTF8String: cstr.as_ptr()];
    if obj.is_null() {
        Err(String::from("failed to create NSString"))
    } else {
        Ok(obj)
    }
}

/// Owned AppKit `NSWindow` hosting an embedded plug-in `NSView`.
struct NativeWindow {
    window: *mut AnyObject,
    content_view: *mut AnyObject,
}

impl NativeWindow {
    fn open(title: &str, width: f64, height: f64, resizable: bool) -> Result<Self, String> {
        ensure_main_thread()?;
        let width = width.clamp(240.0, 2400.0);
        let height = height.clamp(160.0, 1600.0);
        unsafe {
            if let Some(app_cls) = AnyClass::get(c"NSApplication") {
                let _: *mut AnyObject = msg_send![app_cls, sharedApplication];
            }
            let win_cls =
                AnyClass::get(c"NSWindow").ok_or_else(|| String::from("NSWindow unavailable"))?;
            let alloc: *mut AnyObject = msg_send![win_cls, alloc];
            if alloc.is_null() {
                return Err(String::from("NSWindow alloc failed"));
            }
            let rect = NSRect {
                origin: NSPoint { x: 120.0, y: 120.0 },
                size: NSSize { width, height },
            };
            // Titled (1) | Closable (2) | Miniaturizable (4) | optional Resizable (8)
            let style: usize = if resizable { 1 | 2 | 4 | 8 } else { 1 | 2 | 4 };
            let backing: usize = 2; // NSBackingStoreBuffered
            let window: *mut AnyObject = msg_send![
                alloc,
                initWithContentRect: rect,
                styleMask: style,
                backing: backing,
                defer: Bool::NO
            ];
            if window.is_null() {
                return Err(String::from("NSWindow init failed"));
            }
            let _: () = msg_send![window, setReleasedWhenClosed: Bool::NO];
            let title_ns = match nsstring(title) {
                Ok(title) => title,
                Err(error) => {
                    let _: () = msg_send![window, release];
                    return Err(error);
                }
            };
            let _: () = msg_send![window, setTitle: title_ns];
            let content_view: *mut AnyObject = msg_send![window, contentView];
            if content_view.is_null() {
                let _: () = msg_send![window, close];
                let _: () = msg_send![window, release];
                return Err(String::from("NSWindow contentView was null"));
            }
            let _: () = msg_send![window, center];
            let _: () = msg_send![window, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
            Ok(Self {
                window,
                content_view,
            })
        }
    }

    fn set_content_size(&self, width: f64, height: f64) {
        let size = NSSize {
            width: width.clamp(160.0, 2400.0),
            height: height.clamp(120.0, 1600.0),
        };
        unsafe {
            let _: () = msg_send![self.window, setContentSize: size];
        }
    }

    fn is_visible(&self) -> bool {
        if self.window.is_null() {
            return false;
        }
        let visible: Bool = unsafe { msg_send![self.window, isVisible] };
        visible.as_bool()
    }

    fn close_and_release(&mut self) {
        if !self.window.is_null() {
            unsafe {
                let _: () = msg_send![self.window, orderOut: std::ptr::null::<AnyObject>()];
                let _: () = msg_send![self.window, close];
                let _: () = msg_send![self.window, release];
            }
            self.window = std::ptr::null_mut();
            self.content_view = std::ptr::null_mut();
        }
    }
}

impl Drop for NativeWindow {
    fn drop(&mut self) {
        self.close_and_release();
    }
}

struct HostPlugFrame {
    /// Shared with the editor, which clears it before the window is freed, so
    /// a plug-in that keeps the frame never resizes a released window.
    window: Arc<AtomicPtr<AnyObject>>,
}

impl Class for HostPlugFrame {
    type Interfaces = (IPlugFrame,);
}

impl IPlugFrameTrait for HostPlugFrame {
    unsafe fn resizeView(&self, _view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
        if new_size.is_null() {
            return vst3::Steinberg::kInvalidArgument;
        }
        // AppKit may only be touched on the main thread.
        if ensure_main_thread().is_err() {
            return vst3::Steinberg::kResultFalse;
        }
        let window = self.window.load(Ordering::Acquire);
        if window.is_null() {
            return kResultOk;
        }
        let rect = unsafe { &*new_size };
        let size = NSSize {
            width: f64::from(rect.right.saturating_sub(rect.left).clamp(160, 8192)),
            height: f64::from(rect.bottom.saturating_sub(rect.top).clamp(120, 8192)),
        };
        unsafe {
            let _: () = msg_send![window, setContentSize: size];
        }
        kResultOk
    }
}

/// An open native macOS VST3 editor window hosting the plug-in's `IPlugView`.
pub struct Vst3Editor {
    // Drop order matters: detach view and close window BEFORE dropping controller/effect/library.
    view: Option<ComPtr<IPlugView>>,
    frame: Option<ComPtr<IPlugFrame>>,
    frame_window: Arc<AtomicPtr<AnyObject>>,
    window: NativeWindow,
    effect: Vst3Effect,
    size: (i32, i32),
}

impl std::fmt::Debug for Vst3Editor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Vst3Editor")
            .field("size", &self.size)
            .field("open", &self.is_open())
            .finish()
    }
}

impl Vst3Editor {
    /// Opens a native macOS editor window for a VST3 plug-in bundle and class ID.
    ///
    /// # Errors
    ///
    /// Returns a message when not on the main thread, the plug-in has no
    /// `NSView`-compatible `IPlugView`, or window creation fails.
    pub fn open(
        title: &str,
        bundle: &Path,
        class_id: &str,
        initial_state_base64: &str,
        sample_rate: f64,
    ) -> Result<Self, String> {
        ensure_main_thread()?;
        let mut effect = Vst3Effect::new(bundle, class_id, sample_rate, 512)?;
        if !initial_state_base64.trim().is_empty() {
            let _ = effect.load_state_base64(initial_state_base64);
        } else {
            effect.sync_controller_state();
        }
        // The view is set up inside a local pool, so anything the plug-in
        // autoreleases on an error path is freed before `effect` unloads it.
        let (view, frame, frame_window, window, size) =
            objc2::rc::autoreleasepool(|_| Self::attach(title, &effect))?;
        Ok(Self {
            view: Some(view),
            frame: Some(frame),
            frame_window,
            window,
            effect,
            size,
        })
    }

    #[allow(clippy::type_complexity)]
    fn attach(
        title: &str,
        effect: &Vst3Effect,
    ) -> Result<
        (
            ComPtr<IPlugView>,
            ComPtr<IPlugFrame>,
            Arc<AtomicPtr<AnyObject>>,
            NativeWindow,
            (i32, i32),
        ),
        String,
    > {
        let controller = effect
            .controller()
            .ok_or_else(|| String::from("VST3 plug-in has no edit controller"))?;
        let raw_view = unsafe { controller.createView(ViewType::kEditor) };
        let view = unsafe { ComPtr::<IPlugView>::from_raw(raw_view) }
            .ok_or_else(|| String::from("VST3 plug-in provides no native editor view"))?;
        if unsafe { view.isPlatformTypeSupported(kPlatformTypeNSView) } != kResultOk {
            return Err(String::from("VST3 plug-in view does not support NSView"));
        }
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 640,
            bottom: 420,
        };
        let _ = unsafe { view.getSize(&mut rect) };
        let width = rect.right.saturating_sub(rect.left).clamp(240, 2400);
        let height = rect.bottom.saturating_sub(rect.top).clamp(160, 1600);
        let resizable = unsafe { view.canResize() } == vst3::Steinberg::kResultTrue;
        let window = NativeWindow::open(title, f64::from(width), f64::from(height), resizable)?;
        let frame_window = Arc::new(AtomicPtr::new(window.window));
        let frame = ComWrapper::new(HostPlugFrame {
            window: Arc::clone(&frame_window),
        })
        .to_com_ptr::<IPlugFrame>()
        .ok_or_else(|| String::from("failed to create IPlugFrame"))?;
        unsafe {
            let _ = view.setFrame(frame.as_ptr());
        }
        let attached = unsafe { view.attached(window.content_view.cast(), kPlatformTypeNSView) };
        if attached != kResultOk {
            frame_window.store(std::ptr::null_mut(), Ordering::Release);
            unsafe {
                let _ = view.setFrame(std::ptr::null_mut());
            }
            return Err(format!("IPlugView::attached returned {attached}"));
        }
        Ok((view, frame, frame_window, window, (width, height)))
    }

    /// Initial editor width and height in points.
    #[must_use]
    pub fn size(&self) -> (i32, i32) {
        self.size
    }

    /// True while the native `NSWindow` remains visible.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.window.is_visible()
    }

    /// Captures the plug-in's current state as base64.
    ///
    /// # Errors
    ///
    /// Returns a message if the component reports no state.
    pub fn save_state_base64(&self) -> Result<String, String> {
        self.effect.save_state_base64()
    }

    /// Detaches the plug-in view and closes the native window.
    pub fn close(&mut self) {
        // Drain whatever the plug-in autoreleases here before its library
        // can be unloaded with `self.effect`.
        objc2::rc::autoreleasepool(|_| {
            self.frame_window
                .store(std::ptr::null_mut(), Ordering::Release);
            if let Some(view) = self.view.take() {
                unsafe {
                    let _ = view.removed();
                    let _ = view.setFrame(std::ptr::null_mut());
                }
            }
            self.frame = None;
            self.window.close_and_release();
        });
    }
}

impl Drop for Vst3Editor {
    fn drop(&mut self) {
        self.close();
    }
}

enum AuHandle {
    Effect(AuEffect),
    Instrument(AuInstrument),
}

impl AuHandle {
    fn raw_unit(&self) -> *mut c_void {
        match self {
            Self::Effect(fx) => fx.raw_unit(),
            Self::Instrument(inst) => inst.raw_unit(),
        }
    }

    fn save_parameter_state(&self) -> String {
        match self {
            Self::Effect(fx) => fx.serialize_parameter_state(),
            Self::Instrument(inst) => inst.serialize_parameter_state(),
        }
    }

    fn apply_parameter_state(&self, state: &str) {
        match self {
            Self::Effect(fx) => fx.apply_parameter_state(state),
            Self::Instrument(inst) => inst.apply_parameter_state(state),
        }
    }
}

/// An open native macOS Audio Unit editor window (`AUCocoaUIBase` or `AUGenericView`).
pub struct AuEditor {
    au_view: *mut AnyObject,
    window: NativeWindow,
    handle: AuHandle,
    used_custom_view: bool,
}

impl std::fmt::Debug for AuEditor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuEditor")
            .field("open", &self.is_open())
            .field("custom_view", &self.used_custom_view)
            .finish()
    }
}

/// The CF objects an Audio Unit wrote into a `CocoaUI` property buffer,
/// released on drop so every return path frees them.
struct CfBuffer {
    words: Vec<*const c_void>,
    written: usize,
}

impl Drop for CfBuffer {
    fn drop(&mut self) {
        for &ptr in &self.words[..self.written.min(self.words.len())] {
            if !ptr.is_null() {
                unsafe { CFRelease(ptr) };
            }
        }
    }
}

unsafe fn try_custom_cocoa_au_view(unit: *mut c_void) -> Option<*mut AnyObject> {
    let mut size = 0_u32;
    let status = unsafe {
        AudioUnitGetProperty(
            unit,
            K_AUDIO_UNIT_PROPERTY_COCOA_UI,
            K_AUDIO_UNIT_SCOPE_GLOBAL,
            0,
            std::ptr::null_mut(),
            &mut size,
        )
    };
    let min_size = 2 * std::mem::size_of::<*const c_void>();
    if status != 0 || (size as usize) < min_size {
        return None;
    }
    let words = (size as usize).div_ceil(std::mem::size_of::<*const c_void>());
    let mut buffer = CfBuffer {
        words: vec![std::ptr::null::<c_void>(); words],
        written: 0,
    };
    let status = unsafe {
        AudioUnitGetProperty(
            unit,
            K_AUDIO_UNIT_PROPERTY_COCOA_UI,
            K_AUDIO_UNIT_SCOPE_GLOBAL,
            0,
            buffer.words.as_mut_ptr().cast(),
            &mut size,
        )
    };
    // Only words the unit actually wrote are CF objects to release.
    buffer.written = (size as usize / std::mem::size_of::<*const c_void>()).min(words);
    if status != 0 || (size as usize) < min_size {
        return None;
    }
    let bundle_url = buffer.words[0].cast::<AnyObject>();
    let class_name = buffer.words[1].cast::<AnyObject>();
    let ns_bundle_cls = AnyClass::get(c"NSBundle")?;
    if bundle_url.is_null() || class_name.is_null() {
        return None;
    }
    // Objects the plug-in autoreleases here are freed when this pool drains,
    // not in an outer pool that may outlive the Audio Unit. The view itself
    // is retained below, so it survives the drain.
    objc2::rc::autoreleasepool(|_| {
        let bundle: *mut AnyObject = msg_send![ns_bundle_cls, bundleWithURL: bundle_url];
        if bundle.is_null() {
            return None;
        }
        let view_cls: *const AnyClass = msg_send![bundle, classNamed: class_name];
        if view_cls.is_null() {
            return None;
        }
        let factory_alloc: *mut AnyObject = msg_send![view_cls, alloc];
        let factory: *mut AnyObject = msg_send![factory_alloc, init];
        if factory.is_null() {
            return None;
        }
        // An unknown selector would raise an Objective-C exception across
        // the Rust boundary, so ask first.
        let responds: Bool =
            msg_send![factory, respondsToSelector: objc2::sel!(uiViewForAudioUnit:withSize:)];
        if !responds.as_bool() {
            let _: () = msg_send![factory, release];
            return None;
        }
        let preferred = NSSize {
            width: 640.0,
            height: 420.0,
        };
        let au_ptr = unit.cast::<OpaqueAudioComponentInstance>();
        let view: *mut AnyObject = msg_send![
            factory,
            uiViewForAudioUnit: au_ptr,
            withSize: preferred
        ];
        if !view.is_null() {
            let _: *mut AnyObject = msg_send![view, retain];
        }
        let _: () = msg_send![factory, release];
        (!view.is_null()).then_some(view)
    })
}

unsafe fn generic_au_view(unit: *mut c_void) -> Result<*mut AnyObject, String> {
    let cls =
        AnyClass::get(c"AUGenericView").ok_or_else(|| String::from("AUGenericView unavailable"))?;
    let alloc: *mut AnyObject = msg_send![cls, alloc];
    if alloc.is_null() {
        return Err(String::from("AUGenericView alloc failed"));
    }
    let au_ptr = unit.cast::<OpaqueAudioComponentInstance>();
    let view: *mut AnyObject = msg_send![alloc, initWithAudioUnit: au_ptr];
    if view.is_null() {
        Err(String::from("AUGenericView initWithAudioUnit failed"))
    } else {
        Ok(view)
    }
}

impl AuEditor {
    /// Opens a native macOS Audio Unit editor window for `manufacturer:subtype`.
    ///
    /// Tries the plug-in's custom Cocoa UI (`kAudioUnitProperty_CocoaUI`) first
    /// and falls back to Apple's `AUGenericView`.
    ///
    /// # Errors
    ///
    /// Returns a message when not on the main thread, the component cannot be
    /// opened, or view/window creation fails.
    pub fn open(
        title: &str,
        identifier: &str,
        initial_state: &str,
        sample_rate: f64,
    ) -> Result<Self, String> {
        ensure_main_thread()?;
        let handle = match AuEffect::new(identifier, sample_rate) {
            Ok(effect) => AuHandle::Effect(effect),
            Err(_) => AuHandle::Instrument(AuInstrument::new(identifier, sample_rate)?),
        };
        if !initial_state.trim().is_empty() {
            handle.apply_parameter_state(initial_state);
        }
        let unit = handle.raw_unit();
        // A local pool, so nothing the view setup autoreleases outlives the
        // unit if this returns an error.
        let (au_view, used_custom_view, window) = objc2::rc::autoreleasepool(|_| {
            let (au_view, used_custom_view) = unsafe {
                if let Some(view) = try_custom_cocoa_au_view(unit) {
                    (view, true)
                } else {
                    (generic_au_view(unit)?, false)
                }
            };
            let frame: NSRect = unsafe { msg_send![au_view, frame] };
            let width = frame.size.width.max(420.0);
            let height = frame.size.height.max(260.0);
            let window = match NativeWindow::open(title, width, height, true) {
                Ok(window) => window,
                Err(error) => {
                    // The view is +1 retained; release it before the unit goes.
                    unsafe {
                        let _: () = msg_send![au_view, release];
                    }
                    return Err(error);
                }
            };
            unsafe {
                let _: () = msg_send![window.window, setContentView: au_view];
            }
            window.set_content_size(width, height);
            Ok((au_view, used_custom_view, window))
        })?;
        Ok(Self {
            au_view,
            window,
            handle,
            used_custom_view,
        })
    }

    /// True when the Audio Unit provided a custom Cocoa UI rather than `AUGenericView`.
    #[must_use]
    pub fn used_custom_view(&self) -> bool {
        self.used_custom_view
    }

    /// True while the native `NSWindow` remains visible.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.window.is_visible()
    }

    /// Reads the Audio Unit's current parameter values as `id=value;`.
    #[must_use]
    pub fn save_parameter_state(&self) -> String {
        self.handle.save_parameter_state()
    }

    /// Closes the native window and releases the Cocoa view.
    pub fn close(&mut self) {
        // Detach and release the view inside one pool, so an autoreleased
        // window or view cannot outlive the Audio Unit it draws.
        objc2::rc::autoreleasepool(|_| {
            if !self.au_view.is_null() {
                unsafe {
                    if !self.window.window.is_null() {
                        let _: () = msg_send![
                            self.window.window,
                            setContentView: std::ptr::null::<AnyObject>()
                        ];
                    }
                    let _: () = msg_send![self.au_view, removeFromSuperview];
                }
            }
            self.window.close_and_release();
            if !self.au_view.is_null() {
                unsafe {
                    let _: () = msg_send![self.au_view, release];
                }
                self.au_view = std::ptr::null_mut();
            }
        });
    }
}

impl Drop for AuEditor {
    fn drop(&mut self) {
        self.close();
    }
}
