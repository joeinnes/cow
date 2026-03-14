import Foundation

enum Vcs: String, Codable {
    case git
    case jj
}

struct PastureEntry: Codable, Identifiable {
    var id: String { name }
    let name: String
    let path: String
    let source: String
    let vcs: Vcs
    let branch: String?
    let initialCommit: String?
    let createdAt: String
    let symlinkedDirs: [String]
    let linkedDirs: [String]
    let isWorktree: Bool

    enum CodingKeys: String, CodingKey {
        case name, path, source, vcs, branch
        case initialCommit = "initial_commit"
        case createdAt = "created_at"
        case symlinkedDirs = "symlinked_dirs"
        case linkedDirs = "linked_dirs"
        case isWorktree = "is_worktree"
    }

    /// The project prefix, e.g. "jazz2" from "jazz2/feature-foo".
    var projectName: String {
        name.components(separatedBy: "/").first ?? name
    }

    /// The branch/workspace portion after the project prefix.
    var pastureName: String {
        let parts = name.components(separatedBy: "/")
        return parts.count > 1 ? parts.dropFirst().joined(separator: "/") : name
    }

    var pathURL: URL { URL(fileURLWithPath: path) }
}

struct CowState: Codable {
    let pastures: [PastureEntry]
}
