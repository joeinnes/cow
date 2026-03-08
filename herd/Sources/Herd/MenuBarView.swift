import SwiftUI

struct MenuBarView: View {
    @StateObject private var store = StateStore()
    @StateObject private var statsStore = StatsStore()
    @State private var runningOp: String?
    @State private var alertMessage: String?
    @State private var showDeltaWink = false

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            // ── Header ──────────────────────────────────────────
            HStack {
                Text("Herd")
                    .font(.headline)
                Spacer()
                if store.pastures.isEmpty {
                    Text("no pastures")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Text("\(store.pastures.count) pasture\(store.pastures.count == 1 ? "" : "s")")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)

            Divider()

            // ── Pasture list ─────────────────────────────────────
            if store.pastures.isEmpty {
                Text("No pastures yet. Run cow create to get started.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(20)
                    .frame(maxWidth: .infinity)
            } else {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0, pinnedViews: .sectionHeaders) {
                        ForEach(groupedKeys, id: \.self) { project in
                            Section {
                                ForEach(grouped[project] ?? []) { pasture in
                                    PastureRowView(
                                        pasture: pasture,
                                        runningOp: $runningOp,
                                        alertMessage: $alertMessage
                                    )
                                }
                            } header: {
                                Text(project)
                                    .font(.caption)
                                    .fontWeight(.semibold)
                                    .foregroundStyle(.secondary)
                                    .padding(.horizontal, 12)
                                    .padding(.top, 8)
                                    .padding(.bottom, 2)
                                    .frame(maxWidth: .infinity, alignment: .leading)
                                    .background(.regularMaterial)
                            }
                        }
                    }
                }
                .frame(maxHeight: 380)
            }

            // ── Error banner ─────────────────────────────────────
            if let err = alertMessage {
                Divider()
                HStack(alignment: .top, spacing: 6) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.red)
                        .font(.caption)
                        .padding(.top, 1)
                    Text(err)
                        .font(.caption)
                        .foregroundStyle(.red)
                        .lineLimit(4)
                    Spacer()
                    Button {
                        alertMessage = nil
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
            }

            // ── Progress bar ─────────────────────────────────────
            if let op = runningOp {
                Divider()
                HStack(spacing: 6) {
                    ProgressView()
                        .scaleEffect(0.65)
                        .frame(width: 14, height: 14)
                    Text(op)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 7)
            }

            // ── Stats strip ───────────────────────────────────────
            if let s = statsStore.stats {
                Divider()
                HStack(spacing: 0) {
                    statCell(label: "source", value: s.onDisk)
                    Divider().frame(height: 28)
                    statCell(label: "pastures", value: s.delta)
                        .onTapGesture {
                            withAnimation(.spring(response: 0.2)) { showDeltaWink = true }
                            DispatchQueue.main.asyncAfter(deadline: .now() + 1.8) {
                                withAnimation(.easeOut(duration: 0.3)) { showDeltaWink = false }
                            }
                        }
                        .overlay(alignment: .top) {
                            if showDeltaWink {
                                Text("yes, really 🐄")
                                    .font(.system(size: 9, weight: .semibold))
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 3)
                                    .background(Capsule().fill(Color(nsColor: .systemGreen)))
                                    .transition(.scale(scale: 0.7).combined(with: .opacity))
                                    .offset(y: -22)
                                    .zIndex(10)
                            }
                        }
                    Divider().frame(height: 28)
                    statCell(label: "saved vs npm", value: s.savedNpm)
                }
                .frame(maxWidth: .infinity)
            }

            // ── Footer ────────────────────────────────────────────
            Divider()
            HStack {
                Button("Open ~/.cow") {
                    NSWorkspace.shared.open(
                        FileManager.default.homeDirectoryForCurrentUser
                            .appendingPathComponent(".cow")
                    )
                }
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }
            }
            .buttonStyle(.plain)
            .font(.callout)
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 9)
        }
        .background(.regularMaterial)
        .task { await statsStore.refresh() }
        .onChange(of: store.pastures.count) { _ in
            Task { await statsStore.refresh() }
        }
    }

    private var grouped: [String: [PastureEntry]] {
        Dictionary(grouping: store.pastures, by: \.projectName)
    }

    private var groupedKeys: [String] {
        grouped.keys.sorted()
    }

    @ViewBuilder
    private func statCell(label: String, value: String) -> some View {
        VStack(spacing: 2) {
            Text(value)
                .font(.system(size: 10, weight: .semibold, design: .monospaced))
                .lineLimit(1)
                .minimumScaleFactor(0.7)
            Text(label)
                .font(.system(size: 9))
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 7)
    }
}

// ── Pasture row ───────────────────────────────────────────────────────────────

struct PastureRowView: View {
    let pasture: PastureEntry
    @Binding var runningOp: String?
    @Binding var alertMessage: String?

    @State private var isHovered = false
    @State private var showRemoveConfirm = false

    var body: some View {
        HStack(spacing: 8) {
            // VCS colour dot
            Circle()
                .fill(pasture.vcs == .git ? Color.orange : Color.purple)
                .frame(width: 7, height: 7)

            // Name + branch
            VStack(alignment: .leading, spacing: 1) {
                Text(pasture.pastureName)
                    .font(.callout)
                    .lineLimit(1)
                if let branch = pasture.branch {
                    Text(branch)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer()

            // Hover action buttons
            if isHovered {
                HStack(spacing: 2) {
                    actionButton(icon: "folder", help: "Open in Finder") {
                        NSWorkspace.shared.open(pasture.pathURL)
                    }
                    actionButton(icon: "terminal", help: "Open in Terminal") {
                        openInTerminal(pasture.path)
                    }
                    actionButton(icon: "arrow.triangle.2.circlepath", help: "Sync from source") {
                        Task { await runSync() }
                    }
                    actionButton(icon: "trash", help: "Remove pasture") {
                        showRemoveConfirm = true
                    }
                    .foregroundStyle(.red)
                }
                .transition(.opacity.animation(.easeInOut(duration: 0.1)))
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(isHovered ? Color.primary.opacity(0.07) : Color.clear)
                .padding(.horizontal, 4)
        )
        .onHover { isHovered = $0 }
        .confirmationDialog(
            "Remove \u{201C}\(pasture.pastureName)\u{201D}?",
            isPresented: $showRemoveConfirm,
            titleVisibility: .visible
        ) {
            Button("Remove", role: .destructive) {
                Task { await runRemove() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("This will permanently delete the pasture directory at \(pasture.path).")
        }
    }

    @ViewBuilder
    private func actionButton(
        icon: String,
        help: String,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            Image(systemName: icon)
                .frame(width: 22, height: 22)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(help)
        .disabled(runningOp != nil)
    }

    private func openInTerminal(_ path: String) {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/open")
        p.arguments = ["-a", "Terminal", path]
        try? p.run()
    }

    private func runSync() async {
        runningOp = "Syncing \(pasture.pastureName)…"
        do {
            try await CowRunner.run(["sync", pasture.name])
        } catch {
            alertMessage = "Sync failed: \(error.localizedDescription)"
        }
        runningOp = nil
    }

    private func runRemove() async {
        runningOp = "Removing \(pasture.pastureName)…"
        do {
            try await CowRunner.run(["remove", pasture.name, "--yes"])
        } catch {
            alertMessage = "Remove failed: \(error.localizedDescription)"
        }
        runningOp = nil
    }
}
