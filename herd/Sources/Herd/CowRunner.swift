import Foundation

enum CowError: LocalizedError {
    case binaryNotFound
    case commandFailed(String)

    var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            return "cow binary not found. Install with: brew install joeinnes/tap/cow"
        case .commandFailed(let msg):
            return msg.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }
}

enum CowRunner {
    static func findBinary() -> String? {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let candidates = [
            "/opt/homebrew/bin/cow",
            "/usr/local/bin/cow",
            "\(home)/.cargo/bin/cow",
        ]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
    }

    @discardableResult
    static func run(_ args: [String]) async throws -> String {
        guard let binary = findBinary() else {
            throw CowError.binaryNotFound
        }
        return try await withCheckedThrowingContinuation { continuation in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    let process = Process()
                    process.executableURL = URL(fileURLWithPath: binary)
                    process.arguments = args
                    let outPipe = Pipe()
                    let errPipe = Pipe()
                    process.standardOutput = outPipe
                    process.standardError = errPipe
                    try process.run()
                    process.waitUntilExit()
                    if process.terminationStatus == 0 {
                        let data = outPipe.fileHandleForReading.readDataToEndOfFile()
                        continuation.resume(returning: String(data: data, encoding: .utf8) ?? "")
                    } else {
                        let errData = errPipe.fileHandleForReading.readDataToEndOfFile()
                        let msg = String(data: errData, encoding: .utf8) ?? "Command failed"
                        continuation.resume(throwing: CowError.commandFailed(msg))
                    }
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }
}
