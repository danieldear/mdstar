import Foundation

/// Watches a single file on disk and invokes a handler when it changes.
///
/// Uses a GCD vnode `DispatchSource` on a file descriptor opened with
/// `O_EVTONLY`. Events are debounced (~250ms) to coalesce the bursts that
/// editors produce while saving. Because many editors save atomically by
/// writing a temporary file and renaming it over the original (which
/// invalidates our file descriptor), a `.rename`/`.delete` event tears the
/// current watch down and, after the debounce, re-establishes it on the same
/// path if the file still exists. The change handler is always delivered on
/// the main actor.
/// `@unchecked Sendable`: every mutable stored property is accessed only on
/// the private serial `queue`, so the type is concurrency-safe by construction
/// even though the compiler cannot prove it.
final class FileWatcher: @unchecked Sendable {
    private let queue = DispatchQueue(label: "com.mdstar.filewatcher")
    private var source: DispatchSourceFileSystemObject?
    private var fileDescriptor: CInt = -1
    private var debounceWorkItem: DispatchWorkItem?

    /// Path currently being watched. Accessed only on `queue`.
    private var watchedPath: String?
    /// Handler invoked on the main actor when a change is detected.
    private var onChange: (@MainActor @Sendable () -> Void)?

    private let debounceInterval: DispatchTimeInterval = .milliseconds(250)

    deinit {
        // Tear down synchronously without hopping queues from deinit.
        source?.cancel()
        if fileDescriptor >= 0 { close(fileDescriptor) }
    }

    /// Begin watching `url`. Any previous watch is stopped first, so this is
    /// safe to call repeatedly (e.g. when switching documents).
    func start(url: URL, onChange: @escaping @MainActor @Sendable () -> Void) {
        let path = url.path
        queue.async { [weak self] in
            guard let self else { return }
            self.teardownLocked()
            self.onChange = onChange
            self.watchedPath = path
            self.beginWatchingLocked(path: path)
        }
    }

    /// Stop watching and release the file descriptor.
    func stop() {
        queue.async { [weak self] in
            guard let self else { return }
            self.onChange = nil
            self.watchedPath = nil
            self.teardownLocked()
        }
    }

    // MARK: - Queue-isolated helpers (must run on `queue`)

    private func beginWatchingLocked(path: String) {
        let descriptor = open(path, O_EVTONLY)
        guard descriptor >= 0 else {
            // File not currently openable; leave the watch inactive. A later
            // start() (e.g. reopening the document) will retry.
            return
        }
        let source = DispatchSource.makeFileSystemObjectSource(
            fileDescriptor: descriptor,
            eventMask: [.write, .rename, .delete, .extend],
            queue: queue
        )
        source.setEventHandler { [weak self] in
            guard let self, let source = self.source else { return }
            self.handleEventLocked(flags: source.data)
        }
        source.setCancelHandler { [weak self, descriptor] in
            close(descriptor)
            if self?.fileDescriptor == descriptor {
                self?.fileDescriptor = -1
            }
        }
        self.fileDescriptor = descriptor
        self.source = source
        source.resume()
    }

    /// Cancel the current source (its cancel handler closes the fd) and reset
    /// state. Must be called on `queue`.
    private func teardownLocked() {
        debounceWorkItem?.cancel()
        debounceWorkItem = nil
        if let source {
            self.source = nil
            source.cancel()
        } else if fileDescriptor >= 0 {
            close(fileDescriptor)
            fileDescriptor = -1
        }
    }

    private func handleEventLocked(flags: DispatchSource.FileSystemEvent) {
        // Atomic saves replace the inode via rename/delete, invalidating our
        // fd. Drop the current source now; the debounced work re-establishes
        // the watch on the same path.
        let needsRewatch = flags.contains(.rename) || flags.contains(.delete)
        if needsRewatch, let source {
            self.source = nil
            source.cancel()
        }

        debounceWorkItem?.cancel()
        let work = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.debounceWorkItem = nil

            if needsRewatch, let path = self.watchedPath, self.source == nil,
               FileManager.default.fileExists(atPath: path) {
                self.beginWatchingLocked(path: path)
            }

            guard let handler = self.onChange else { return }
            Task { @MainActor in handler() }
        }
        debounceWorkItem = work
        queue.asyncAfter(deadline: .now() + debounceInterval, execute: work)
    }
}
