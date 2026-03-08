import Foundation
import CoreServices

/// Watches a directory with FSEvents and fires a callback on any change.
final class FileWatcher {
    private var streamRef: FSEventStreamRef?
    private let callback: () -> Void

    init(directory: String, callback: @escaping () -> Void) {
        self.callback = callback

        let paths = [directory] as CFArray
        var ctx = FSEventStreamContext(
            version: 0,
            info: Unmanaged.passRetained(self).toOpaque(),
            retain: nil,
            release: { ptr in
                guard let ptr else { return }
                Unmanaged<FileWatcher>.fromOpaque(ptr).release()
            },
            copyDescription: nil
        )
        streamRef = FSEventStreamCreate(
            nil,
            { _, info, _, _, _, _ in
                guard let info else { return }
                Unmanaged<FileWatcher>.fromOpaque(info).takeUnretainedValue().callback()
            },
            &ctx,
            paths,
            FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
            0.3,  // latency in seconds
            FSEventStreamCreateFlags(kFSEventStreamCreateFlagFileEvents)
        )
    }

    func start() {
        guard let stream = streamRef else { return }
        FSEventStreamSetDispatchQueue(stream, DispatchQueue.main)
        FSEventStreamStart(stream)
    }

    deinit {
        guard let stream = streamRef else { return }
        FSEventStreamStop(stream)
        FSEventStreamInvalidate(stream)
        FSEventStreamRelease(stream)
    }
}
