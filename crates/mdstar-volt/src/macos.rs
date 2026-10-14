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

// Native context menus.

use std::sync::atomic::{AtomicIsize, Ordering};

use objc2::{
    AnyThread, MainThreadMarker, MainThreadOnly, define_class, ffi, msg_send,
    rc::Retained,
    runtime::{AnyClass, AnyObject, Bool, Imp, NSObject, NSObjectProtocol, Sel},
    sel,
};
// NSSpeechSynthesizer is deprecated in favour of AVFoundation, but still
// works and keeps AVFoundation out of the build.
#[allow(deprecated)]
use objc2_app_kit::NSSpeechSynthesizer;
use objc2_app_kit::{
    NSApplication, NSMenu, NSMenuItem, NSPasteboard, NSPasteboardTypeString,
    NSSharingServicePicker, NSView,
};
use objc2_foundation::{
    NSArray, NSAttributedString, NSCopying, NSPoint, NSRect, NSRectEdge, NSSize, NSString,
};

/// The tag of the item picked in the open menu, or -1.
static CHOSEN: AtomicIsize = AtomicIsize::new(-1);
/// Selected text offered to Services and Look Up.
static SELECTED: Mutex<String> = Mutex::new(String::new());

thread_local! {
    #[allow(deprecated)]
    static SPEAKER: std::cell::RefCell<Option<Retained<NSSpeechSynthesizer>>> =
        const { std::cell::RefCell::new(None) };
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements; no Drop impl.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "MDStarMenuTarget"]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(pick:))]
        fn pick(&self, sender: &NSMenuItem) {
            CHOSEN.store(sender.tag(), Ordering::SeqCst);
        }
    }

    unsafe impl NSObjectProtocol for MenuTarget {}
);

impl MenuTarget {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// One entry of a native context menu.
pub enum Entry {
    Item {
        title: String,
        tag: isize,
        enabled: bool,
    },
    Separator,
    Submenu {
        title: String,
        items: Vec<Entry>,
    },
    /// The system Services for the selected text.
    Services,
}

/// Where a context menu was opened, for actions that anchor to it.
pub struct Anchor {
    view: Retained<NSView>,
    point: NSPoint,
}

fn build(
    entries: &[Entry],
    target: &MenuTarget,
    mtm: MainThreadMarker,
    services: &mut Option<Retained<NSMenu>>,
) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    for entry in entries {
        match entry {
            Entry::Separator => menu.addItem(&NSMenuItem::separatorItem(mtm)),
            Entry::Item {
                title,
                tag,
                enabled,
            } => {
                // SAFETY: `pick:` is implemented by the target.
                let item = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(mtm),
                        &NSString::from_str(title),
                        Some(sel!(pick:)),
                        &NSString::from_str(""),
                    )
                };
                // SAFETY: the target outlives the menu's tracking.
                unsafe { item.setTarget(Some(target)) };
                item.setTag(*tag);
                item.setEnabled(*enabled);
                menu.addItem(&item);
            }
            Entry::Submenu { title, items } => {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str(title));
                item.setSubmenu(Some(&build(items, target, mtm, services)));
                menu.addItem(&item);
            }
            Entry::Services => {
                let item = NSMenuItem::new(mtm);
                item.setTitle(&NSString::from_str("Services"));
                let list = NSMenu::new(mtm);
                item.setSubmenu(Some(&list));
                menu.addItem(&item);
                *services = Some(list);
            }
        }
    }
    menu
}

/// Show a context menu at the current right-click and return the picked
/// item's tag with where it opened. Blocks while the menu is open.
pub fn popup(entries: &[Entry]) -> Option<(isize, Anchor)> {
    let mtm = MainThreadMarker::new()?;
    let app = NSApplication::sharedApplication(mtm);
    let event = app.currentEvent()?;
    let window = event.window(mtm).or_else(|| app.keyWindow())?;
    let view = window.contentView()?;
    let point = view.convertPoint_fromView(event.locationInWindow(), None);
    let target = MenuTarget::new(mtm);
    let mut services = None;
    let menu = build(entries, &target, mtm, &mut services);
    CHOSEN.store(-1, Ordering::SeqCst);
    // AppKit fills the registered Services menu for the first responder;
    // lend it ours while this menu is open.
    let previous = app.servicesMenu();
    if let Some(list) = &services {
        app.setServicesMenu(Some(list));
    }
    NSMenu::popUpContextMenu_withEvent_forView(&menu, &event, &view);
    if services.is_some() {
        app.setServicesMenu(previous.as_deref());
    }
    let tag = CHOSEN.swap(-1, Ordering::SeqCst);
    (tag >= 0).then_some((tag, Anchor { view, point }))
}

/// The selection Services and Look Up act on.
pub fn set_selection(text: &str) {
    if let Ok(mut selected) = SELECTED.lock() {
        selected.clear();
        selected.push_str(text);
    }
}

/// Show the dictionary popover for `text` where the menu opened.
pub fn look_up(text: &str, at: &Anchor) {
    let string =
        NSAttributedString::initWithString(NSAttributedString::alloc(), &NSString::from_str(text));
    at.view
        .showDefinitionForAttributedString_atPoint(Some(&string), at.point);
}

/// The system share sheet for `text`, from where the menu opened.
pub fn share(text: &str, at: &Anchor) {
    let item: Retained<AnyObject> =
        Retained::into_super(Retained::into_super(NSString::from_str(text)));
    let items = NSArray::from_retained_slice(&[item]);
    // SAFETY: the picker takes an array of shareable objects; strings are.
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    picker.showRelativeToRect_ofView_preferredEdge(
        NSRect::new(at.point, NSSize::new(1., 1.)),
        &at.view,
        NSRectEdge::MinY,
    );
}

#[allow(deprecated)]
pub fn speak(text: &str) {
    SPEAKER.with(|speaker| {
        let mut speaker = speaker.borrow_mut();
        let synth = speaker.get_or_insert_with(|| {
            NSSpeechSynthesizer::initWithVoice(NSSpeechSynthesizer::alloc(), None)
                .unwrap_or_default()
        });
        synth.startSpeakingString(&NSString::from_str(text));
    });
}

#[allow(deprecated)]
pub fn stop_speaking() {
    SPEAKER.with(|speaker| {
        if let Some(synth) = speaker.borrow().as_ref() {
            synth.stopSpeaking();
        }
    });
}

/// Offer the selection to the system Services menu: teach winit's view the
/// NSServicesMenuRequestor methods and register plain text as sendable.
pub fn offer_selection_to_services() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    unsafe extern "C-unwind" fn valid_requestor(
        this: *mut AnyObject,
        _cmd: Sel,
        send: *mut NSString,
        ret: *mut NSString,
    ) -> *mut AnyObject {
        // SAFETY: AppKit passes pasteboard type strings or nil.
        let send = unsafe { send.as_ref() };
        let has_text = SELECTED.lock().is_ok_and(|s| !s.is_empty());
        // SAFETY: the pasteboard type constant is a static NSString.
        let plain = unsafe { NSPasteboardTypeString };
        if ret.is_null() && has_text && send.is_some_and(|t| t.isEqualToString(plain)) {
            this
        } else {
            std::ptr::null_mut()
        }
    }
    unsafe extern "C-unwind" fn write_selection(
        _this: *mut AnyObject,
        _cmd: Sel,
        pasteboard: *mut NSPasteboard,
        _types: *mut AnyObject,
    ) -> Bool {
        // SAFETY: AppKit passes the services pasteboard.
        let Some(pasteboard) = (unsafe { pasteboard.as_ref() }) else {
            return Bool::NO;
        };
        let text = SELECTED.lock().map(|s| s.clone()).unwrap_or_default();
        if text.is_empty() {
            return Bool::NO;
        }
        pasteboard.clearContents();
        // SAFETY: the pasteboard type constant is a static NSString.
        Bool::new(
            pasteboard.setString_forType(&NSString::from_str(&text), unsafe {
                NSPasteboardTypeString
            }),
        )
    }
    let Some(class) = AnyClass::get(c"WinitView") else {
        return;
    };
    // SAFETY: the functions match `validRequestorForSendType:returnType:`
    // ("@@:@@") and `writeSelectionToPasteboard:types:` ("c@:@@"); the
    // first replaces NSResponder's inherited default only for WinitView.
    unsafe {
        let valid = std::mem::transmute::<
            unsafe extern "C-unwind" fn(
                *mut AnyObject,
                Sel,
                *mut NSString,
                *mut NSString,
            ) -> *mut AnyObject,
            Imp,
        >(valid_requestor);
        let write = std::mem::transmute::<
            unsafe extern "C-unwind" fn(
                *mut AnyObject,
                Sel,
                *mut NSPasteboard,
                *mut AnyObject,
            ) -> Bool,
            Imp,
        >(write_selection);
        let class = (class as *const AnyClass).cast_mut();
        ffi::class_addMethod(
            class,
            sel!(validRequestorForSendType:returnType:),
            valid,
            c"@@:@@".as_ptr(),
        );
        ffi::class_addMethod(
            class,
            sel!(writeSelectionToPasteboard:types:),
            write,
            c"c@:@@".as_ptr(),
        );
    }
    let app = NSApplication::sharedApplication(mtm);
    // SAFETY: the pasteboard type constant is a static NSString.
    let plain: Retained<NSString> = unsafe { NSPasteboardTypeString }.copy();
    app.registerServicesMenuSendTypes_returnTypes(
        &NSArray::from_retained_slice(&[plain]),
        &NSArray::new(),
    );
}

/// The macOS accent colour (System Settings › Appearance), as sRGB.
pub fn system_accent() -> Option<[f32; 4]> {
    use objc2_app_kit::{NSColor, NSColorSpace};
    MainThreadMarker::new()?;
    let color =
        NSColor::controlAccentColor().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    Some([
        color.redComponent() as f32,
        color.greenComponent() as f32,
        color.blueComponent() as f32,
        1.,
    ])
}
