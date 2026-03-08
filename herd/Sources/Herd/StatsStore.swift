import Foundation

struct StatsData {
    let onDisk: String
    let delta: String
    let savedNpm: String
}

private let kStatsCache = "herd.statsCache"

@MainActor
final class StatsStore: ObservableObject {
    @Published var stats: StatsData?
    @Published var isLoading = false

    init() {
        // Show cached value immediately so the strip is never blank on open
        if let cached = UserDefaults.standard.dictionary(forKey: kStatsCache),
           let onDisk = cached["onDisk"] as? String,
           let delta  = cached["delta"]  as? String,
           let npm    = cached["savedNpm"] as? String {
            stats = StatsData(onDisk: onDisk, delta: delta, savedNpm: npm)
        }
        // Start fetching fresh data immediately in the background
        Task { await refresh() }
    }

    func refresh() async {
        guard !isLoading else { return }
        isLoading = true
        defer { isLoading = false }
        do {
            let output = try await CowRunner.run(["stats"])
            if let parsed = Self.parse(output) {
                stats = parsed
                UserDefaults.standard.set(
                    ["onDisk": parsed.onDisk, "delta": parsed.delta, "savedNpm": parsed.savedNpm],
                    forKey: kStatsCache
                )
            }
        } catch {
            // non-critical — cached value remains visible
        }
    }

    private static func parse(_ output: String) -> StatsData? {
        guard let totalLine = output.components(separatedBy: "\n")
            .first(where: { $0.contains("Total") }) else { return nil }
        // Split on │ and trim; columns: ["", "Total", count, onDisk, delta, savedNpm, savedPnpm, ""]
        let cells = totalLine.components(separatedBy: "│")
            .map { $0.trimmingCharacters(in: .whitespaces) }
        guard cells.count >= 7 else { return nil }
        return StatsData(
            onDisk: cells[3],
            delta: cells[4],
            savedNpm: cells[5]
        )
    }
}
