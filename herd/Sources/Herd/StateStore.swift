import Foundation

@MainActor
final class StateStore: ObservableObject {
    @Published var pastures: [PastureEntry] = []
    @Published var loadError: String?

    private let stateURL: URL
    private var watcher: FileWatcher?

    init() {
        let home = FileManager.default.homeDirectoryForCurrentUser
        stateURL = home.appendingPathComponent(".cow/state.json")
        load()

        let watchDir = home.appendingPathComponent(".cow").path
        watcher = FileWatcher(directory: watchDir) { [weak self] in
            Task { @MainActor [weak self] in
                self?.load()
            }
        }
        watcher?.start()
    }

    func load() {
        guard FileManager.default.fileExists(atPath: stateURL.path) else {
            pastures = []
            loadError = nil
            return
        }
        do {
            let data = try Data(contentsOf: stateURL)
            let state = try JSONDecoder().decode(CowState.self, from: data)
            pastures = state.pastures.sorted { $0.name < $1.name }
            loadError = nil
        } catch {
            loadError = error.localizedDescription
        }
    }
}
