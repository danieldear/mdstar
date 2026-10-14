//! MD Star Preview: the Quick Look extension behind Finder's Space-bar
//! preview of Markdown files. It draws the document with the app's own
//! renderer (`export::pdf`) and hands Quick Look the pages as a PDF, so the
//! preview looks exactly like the reader.

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("MD Star Preview is a macOS Quick Look extension.");
}

#[cfg(target_os = "macos")]
fn main() {
    quicklook::run()
}

// `extern_protocol!` generates the protocol trait without passing on its
// `# Safety` section, which is written in its doc comment below.
#[cfg(target_os = "macos")]
#[allow(clippy::missing_safety_doc)]
mod quicklook {
    use std::{
        ffi::{c_char, c_int},
        path::PathBuf,
        ptr::null_mut,
    };

    use block2::{Block, RcBlock};
    use objc2::{
        ClassType, define_class, extern_class, extern_protocol, msg_send,
        rc::{Allocated, Retained},
        runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol},
    };
    use objc2_foundation::{NSData, NSError, NSSize, NSString, NSURL, ns_string};

    use mdstar_volt::export;

    #[link(name = "QuickLookUI", kind = "framework")]
    unsafe extern "C" {}
    #[link(name = "UniformTypeIdentifiers", kind = "framework")]
    unsafe extern "C" {}
    unsafe extern "C" {
        /// Foundation's entry point for app extensions: it sets up the
        /// extension's connection and instantiates the principal class.
        fn NSExtensionMain(argc: c_int, argv: *mut *mut c_char) -> c_int;
        fn NSLog(format: &NSString, ...);
    }

    extern_class!(
        /// QuickLookUI's base class for data-based preview extensions.
        #[unsafe(super(NSObject))]
        #[name = "QLPreviewProvider"]
        pub struct QLPreviewProvider;
    );

    extern_protocol!(
        /// QuickLookUI's protocol for preview extensions; data-based ones
        /// implement `providePreviewForFileRequest:completionHandler:`.
        ///
        /// # Safety
        ///
        /// Implementors must provide that method with Quick Look's signature.
        #[name = "QLPreviewingController"]
        unsafe trait QLPreviewingController: NSObjectProtocol {}
    );

    /// The longest Quick Look waits on is a few seconds; very long documents
    /// show their first pages.
    const MAX_PAGES: usize = 40;

    define_class!(
        // SAFETY: QLPreviewProvider has no subclassing requirements beyond
        // implementing the QLPreviewingController method below; no Drop impl.
        #[unsafe(super(QLPreviewProvider, NSObject))]
        #[name = "MDStarPreviewProvider"]
        struct Provider;

        unsafe impl NSObjectProtocol for Provider {}

        // Quick Look checks the conformance, not just the method.
        unsafe impl QLPreviewingController for Provider {
            /// `-[QLPreviewingController providePreviewForFileRequest:completionHandler:]`
            #[unsafe(method(providePreviewForFileRequest:completionHandler:))]
            fn provide_preview(
                &self,
                request: &AnyObject,
                done: &Block<dyn Fn(*mut AnyObject, *mut NSError)>,
            ) {
                let url: Option<Retained<NSURL>> = unsafe { msg_send![request, fileURL] };
                let path = url
                    .and_then(|u| u.path())
                    .map(|p| PathBuf::from(p.to_string()));
                let options = export::Options {
                    max_pages: MAX_PAGES,
                    ..export::Options::default()
                };
                let started = std::time::Instant::now();
                let pdf = path
                    .ok_or_else(|| "no file to preview".to_owned())
                    .and_then(|p| export::pdf(&p, options));
                match pdf {
                    Ok(bytes) => {
                        // A literal format keeps the line readable in the
                        // system log, which redacts string arguments.
                        unsafe {
                            NSLog(
                                ns_string!("MD Star Preview: rendered %ld KB in %ld ms"),
                                (bytes.len() / 1024) as std::ffi::c_long,
                                started.elapsed().as_millis() as std::ffi::c_long,
                            )
                        };
                        let reply = pdf_reply(bytes, options.width, options.page);
                        done.call((Retained::as_ptr(&reply) as *mut AnyObject, null_mut()));
                    }
                    Err(_) => {
                        let error = error();
                        done.call((null_mut(), Retained::as_ptr(&error) as *mut NSError));
                    }
                }
            }
        }
    );

    /// A reply that hands Quick Look the PDF bytes.
    fn pdf_reply(bytes: Vec<u8>, width: f32, height: f32) -> Retained<AnyObject> {
        let data = NSData::with_bytes(&bytes);
        // The block returns the data at +0; the block keeps it alive and
        // Quick Look keeps the block for the reply's lifetime.
        let block = RcBlock::new(move |_reply: *mut AnyObject, _error: *mut *mut NSError| {
            Retained::as_ptr(&data) as *mut NSData
        });
        let pdf_type: Retained<AnyObject> = unsafe {
            msg_send![
                AnyClass::get(c"UTType").expect("UniformTypeIdentifiers is linked"),
                typeWithIdentifier: ns_string!("com.adobe.pdf")
            ]
        };
        let class = AnyClass::get(c"QLPreviewReply").expect("QuickLookUI is linked");
        unsafe {
            let reply: Allocated<AnyObject> = msg_send![class, alloc];
            let reply: Option<Retained<AnyObject>> = msg_send![
                reply,
                initWithDataOfContentType: &*pdf_type,
                contentSize: NSSize::new(f64::from(width), f64::from(height)),
                dataCreationBlock: &*block
            ];
            reply.expect("QLPreviewReply initialises")
        }
    }

    /// Quick Look shows its own message for a failed preview. Only rendered
    /// previews are logged, so a line in the log means one succeeded.
    fn error() -> Retained<NSError> {
        let domain = NSString::from_str("dev.mdstar.volt.preview");
        unsafe { NSError::errorWithDomain_code_userInfo(&domain, 1, None) }
    }

    pub fn run() {
        // Register the principal class before Foundation looks it up by name.
        let _ = Provider::class();
        let args: Vec<std::ffi::CString> = std::env::args()
            .map(|a| std::ffi::CString::new(a).unwrap_or_default())
            .collect();
        let mut argv: Vec<*mut c_char> = args.iter().map(|a| a.as_ptr() as *mut c_char).collect();
        argv.push(null_mut());
        let code = unsafe { NSExtensionMain(args.len() as c_int, argv.as_mut_ptr()) };
        std::process::exit(code);
    }
}
