import Foundation

/// Watches directory trees and reports when their contents change.
///
/// Uses FSEvents rather than a vnode `DispatchSource`: a vnode source only
/// reports changes to the directory it is opened on, so a file added inside a
/// nested folder would never surface. FSEvents watches recursively, which is
/// what a workspace tree needs.
///
/// `@unchecked Sendable`: all mutable state is confined to the private serial
/// queue that also drives the event stream.
final class DirectoryWatcher: @unchecked Sendable {
    private let queue = DispatchQueue(label: "com.mdstar.directorywatcher")
    private var stream: FSEventStreamRef?
    private var handler: (@MainActor @Sendable () -> Void)?
    private var debounceWorkItem: DispatchWorkItem?

    /// Coalescing window. Editors and package managers write in bursts; without
    /// this the tree would rescan dozens of times for one logical change.
    private let debounceInterval: DispatchTimeInterval = .milliseconds(400)

    deinit {
        if let stream {
            FSEventStreamStop(stream)
            FSEventStreamInvalidate(stream)
            FSEventStreamRelease(stream)
        }
    }

    /// Watch `urls` recursively. Replaces any previous watch.
    func watch(_ urls: [URL], onChange: @escaping @MainActor @Sendable () -> Void) {
        queue.async { [weak self] in
            guard let self else { return }
            self.teardownLocked()
            guard !urls.isEmpty else { return }
            self.handler = onChange

            let paths = urls.map(\.path) as CFArray
            var context = FSEventStreamContext(
                version: 0,
                info: Unmanaged.passUnretained(self).toOpaque(),
                retain: nil,
                release: nil,
                copyDescription: nil
            )

            let callback: FSEventStreamCallback = { _, info, _, _, _, _ in
                guard let info else { return }
                let watcher = Unmanaged<DirectoryWatcher>.fromOpaque(info).takeUnretainedValue()
                watcher.scheduleNotification()
            }

            guard let stream = FSEventStreamCreate(
                kCFAllocatorDefault,
                callback,
                &context,
                paths,
                FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
                0.3,
                FSEventStreamCreateFlags(
                    kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagNoDefer
                )
            ) else { return }

            FSEventStreamSetDispatchQueue(stream, self.queue)
            FSEventStreamStart(stream)
            self.stream = stream
        }
    }

    func stop() {
        queue.async { [weak self] in
            self?.handler = nil
            self?.teardownLocked()
        }
    }

    // MARK: - Queue-isolated helpers

    /// Called from the FSEvents callback, already on `queue`.
    fileprivate func scheduleNotification() {
        debounceWorkItem?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.debounceWorkItem = nil
            guard let handler = self.handler else { return }
            Task { @MainActor in handler() }
        }
        debounceWorkItem = work
        queue.asyncAfter(deadline: .now() + debounceInterval, execute: work)
    }

    private func teardownLocked() {
        debounceWorkItem?.cancel()
        debounceWorkItem = nil
        if let stream {
            FSEventStreamStop(stream)
            FSEventStreamInvalidate(stream)
            FSEventStreamRelease(stream)
            self.stream = nil
        }
    }
}
