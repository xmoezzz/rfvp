import Foundation
import AppKit
import SwiftUI

// -----------------------------
// NLS
// -----------------------------
enum NlsOption: String, CaseIterable, Identifiable, Codable {
    case sjis
    case gbk
    case utf8

    var id: String { rawValue }

    var label: String {
        switch self {
        case .sjis: return "SJIS"
        case .gbk: return "GBK"
        case .utf8: return "UTF-8"
        }
    }
}

// -----------------------------
// Model
// -----------------------------
struct GameEntry: Identifiable, Codable, Equatable {
    let id: String
    var title: String
    var rootPath: String
    var nls: String // "sjis" | "gbk" | "utf8"
    var addedAtUnix: Int64
    var lastPlayedAtUnix: Int64?
    var coverPath: String?

    init(id: String, title: String, rootPath: String, nls: String, addedAtUnix: Int64, lastPlayedAtUnix: Int64? = nil, coverPath: String? = nil) {
        self.id = id
        self.title = title
        self.rootPath = rootPath
        self.nls = nls
        self.addedAtUnix = addedAtUnix
        self.lastPlayedAtUnix = lastPlayedAtUnix
        self.coverPath = coverPath
    }

    // Backward compatibility: older library.json entries do not have `nls`.
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        rootPath = try c.decode(String.self, forKey: .rootPath)
        nls = try c.decodeIfPresent(String.self, forKey: .nls) ?? NlsOption.sjis.rawValue
        addedAtUnix = try c.decode(Int64.self, forKey: .addedAtUnix)
        lastPlayedAtUnix = try c.decodeIfPresent(Int64.self, forKey: .lastPlayedAtUnix)
        coverPath = try c.decodeIfPresent(String.self, forKey: .coverPath)
    }
}

struct PendingImport: Identifiable {
    let id: String
    let rootURL: URL
    let rootPath: String
    let title: String
}

@MainActor
final class GameLibrary: ObservableObject {
    @Published var games: [GameEntry] = []
    @Published var showError: Bool = false
    @Published var errorMessage: String = ""
    @Published var pendingImport: PendingImport? = nil
    // Set by main.swift (launcher host) to receive a launch request.
    // This must only stop the modal loop; the actual rfvp entry is called outside SwiftUI.
    var onLaunchRequest: ((GameEntry) -> Void)? = nil

    private let fm = FileManager.default

    private var libraryURL: URL {
        // ~/Library/Application Support/RFVPLauncher/library.json
        let appSupport = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = appSupport.appendingPathComponent("RFVPLauncher", isDirectory: true)
        if !fm.fileExists(atPath: dir.path) {
            try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        return dir.appendingPathComponent("library.json")
    }

    init() {
        load()
    }

    func load() {
        do {
            if fm.fileExists(atPath: libraryURL.path) {
                let data = try Data(contentsOf: libraryURL)
                games = try JSONDecoder().decode([GameEntry].self, from: data)
            } else {
                games = []
            }
        } catch {
            games = []
        }
        refreshValidation()
    }

    func save() {
        do {
            let data = try JSONEncoder().encode(games)
            try data.write(to: libraryURL, options: [.atomic])
        } catch {
            // best-effort
        }
    }

    // -----------------------------
    // Import
    // -----------------------------
    func importGameFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Import"

        panel.begin { [weak self] response in
            guard let self else { return }
            if response != .OK { return }
            guard let url = panel.url else { return }
            self.prepareImport(url: url)
        }
    }

    private func prepareImport(url: URL) {
        guard let hcb = findFirstHcb(root: url) else {
            showError("No *.hcb found in selected folder.")
            return
        }
        let title = probeTitleFromHcb(hcbURL: hcb) ?? url.lastPathComponent
        let path = url.path
        let id = stableId(for: path)
        pendingImport = PendingImport(id: id, rootURL: url, rootPath: path, title: title)
    }

    func commitImport(p: PendingImport, nls: NlsOption) {
        let now = Int64(Date().timeIntervalSince1970)

        if let idx = games.firstIndex(where: { $0.id == p.id }) {
            games[idx].rootPath = p.rootPath
            games[idx].title = p.title
            games[idx].nls = nls.rawValue
        } else {
            games.append(GameEntry(id: p.id, title: p.title, rootPath: p.rootPath, nls: nls.rawValue, addedAtUnix: now))
        }
        pendingImport = nil
        save()
        refreshValidation()
    }

    func refreshValidation() {
        var changed = false
        games.removeAll { g in
            let root = URL(fileURLWithPath: g.rootPath)
            let ok = fm.fileExists(atPath: g.rootPath) && findFirstHcb(root: root) != nil
            if !ok { changed = true }
            return !ok
        }
        if changed { save() }
    }

    // -----------------------------
    // Per-game NLS
    // -----------------------------
    func updateNls(game: GameEntry, nls: NlsOption) {
        guard let idx = games.firstIndex(of: game) else { return }
        games[idx].nls = nls.rawValue
        save()
    }

    // -----------------------------
    // Launch request (blocking entry is called from main.swift)
    // -----------------------------
    func launch(game: GameEntry) {
        // Update last played
        if let idx = games.firstIndex(of: game) {
            games[idx].lastPlayedAtUnix = Int64(Date().timeIntervalSince1970)
            save()
        }

        guard let cb = onLaunchRequest else {
            showError("Launcher host is not ready.")
            return
        }
        cb(game)
    }

    // -----------------------------
    // UI actions
    // -----------------------------
    func remove(game: GameEntry) {
        games.removeAll { $0.id == game.id }
        save()
    }

    func revealInFinder(game: GameEntry) {
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: game.rootPath)])
    }

    func loadCoverImage(game: GameEntry) -> NSImage? {
        guard let p = game.coverPath else { return nil }
        let url = URL(fileURLWithPath: p)
        guard fm.fileExists(atPath: url.path) else { return nil }
        return NSImage(contentsOf: url)
    }

    // -----------------------------
    // Helpers
    // -----------------------------
    private func showError(_ msg: String) {
        errorMessage = msg
        showError = true
    }

    private func stableId(for path: String) -> String {
        // Stable enough for local library usage.
        return String(path.hashValue, radix: 16)
    }

    private func findFirstHcb(root: URL) -> URL? {
        // Deterministic scan: root files, then one-level subdirs.
        let options: FileManager.DirectoryEnumerationOptions = [.skipsHiddenFiles, .skipsPackageDescendants]

        if let items = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil, options: options) {
            for u in items.sorted(by: { $0.path < $1.path }) {
                if u.pathExtension.lowercased() == "hcb" { return u }
            }
        }

        if let items = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: [.isDirectoryKey], options: options) {
            for u in items.sorted(by: { $0.path < $1.path }) {
                let isDir = (try? u.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) ?? false
                if !isDir { continue }
                if let sub = try? fm.contentsOfDirectory(at: u, includingPropertiesForKeys: nil, options: options) {
                    for q in sub.sorted(by: { $0.path < $1.path }) {
                        if q.pathExtension.lowercased() == "hcb" { return q }
                    }
                }
            }
        }

        return nil
    }

    private func probeTitleFromHcb(hcbURL: URL) -> String? {
        // Best-effort: look for a UTF-8-ish title string in the header.
        // (The rfvp core will handle NLS correctly; this is just for the launcher list.)
        guard let data = try? Data(contentsOf: hcbURL) else { return nil }
        // Very conservative probe: search for a long-ish ASCII/UTF-8 run.
        if let s = String(data: data.prefix(4096), encoding: .utf8) {
            let candidates = s
                .split(whereSeparator: { $0.isNewline })
                .map { String($0) }
                .filter { $0.count >= 2 && $0.count <= 64 }
            return candidates.first
        }
        return nil
    }
}
