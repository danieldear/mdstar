//! macOS integration that talks to AppKit directly.

use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use volt_ui_kit::native::UiSender;

use crate::app::Msg;

static SENDER: OnceLock<Mutex<UiSender<Msg>>> = OnceLock::new();

/// Receive the files Finder asks MD Star to open — a double-click, Open
/// With, or a drop on the Dock icon — as [`Msg::Open`], including the file
/// that launched the app. Call once the application object exists and
/// before its run loop starts (`Application::before_launch`).
pub fn receive_opened_files(sender: UiSender<Msg>) {
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{ffi, sel};
    use objc2_foundation::{NSArray, NSURL};

    if SENDER.set(Mutex::new(sender)).is_err() {
        return;
    }
    unsafe extern "C-unwind" fn open_urls(
        _this: *mut AnyObject,
        _cmd: Sel,
        _app: *mut AnyObject,
        urls: *mut NSArray<NSURL>,
    ) {
        // SAFETY: AppKit passes an NSArray of NSURL for this selector.
        let Some(urls) = (unsafe { urls.as_ref() }) else {
            return;
        };
        let paths: Vec<PathBuf> = urls
            .iter()
            .filter(|url| url.isFileURL())
            .filter_map(|url| url.path())
            .map(|path| PathBuf::from(path.to_string()))
            .filter(|path| path.is_absolute())
            .collect();
        if paths.is_empty() {
            return;
        }
        if let Some(sender) = SENDER.get().and_then(|s| s.lock().ok()) {
            let _ = sender.send(Msg::Open(paths));
        }
    }
    // winit's application delegate; it does not implement this selector.
    let Some(class) = AnyClass::get(c"WinitApplicationDelegate") else {
        return;
    };
    // SAFETY: `open_urls` has the signature of
    // `-(void)application:(NSApplication *)app openURLs:(NSArray<NSURL *> *)urls`
    // ("v@:@@"), and adding a method the class lacks changes nothing else.
    unsafe {
        let imp = std::mem::transmute::<
            unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject, *mut NSArray<NSURL>),
            Imp,
        >(open_urls);
        ffi::class_addMethod(
            (class as *const AnyClass).cast_mut(),
            sel!(application:openURLs:),
            imp,
            c"v@:@@".as_ptr(),
        );
    }
}
