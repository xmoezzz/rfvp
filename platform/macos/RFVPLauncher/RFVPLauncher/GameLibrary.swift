import Foundation
import AppKit
import SwiftUI

// -----------------------------
// Rust entry point
// -----------------------------
//
// Unified entry point across macOS/iOS/Android:
@_silgen_name("rfvp_run_entry")
private func rfvp_run_entry(_ gameRootUtf8: UnsafePointer<CChar>, _ nlsUtf8: UnsafePointer<CChar>) -> Void

// -----------------------------
// Model
// -----------------------------
struct GameEntry: Identifiable, Codable, Equatable {
    let id: String
    var title: String
    var rootPath: String
    var addedAtUnix: Int64
    var lastPlayedAtUnix: Int64?
    var coverPath: String?

    init(id: String, title: String, rootPath: String, addedAtUnix: Int64, lastPlayedAtUnix: Int64? = nil, coverPath: String? = nil) {
        self.id = id
        self.title = title
        self.rootPath = rootPath
        self.addedAtUnix = addedAtUnix
        self.lastPlayedAtUnix = lastPlayedAtUnix
        self.coverPath = coverPath
    }
}

final class GameLibrary: ObservableObject {
    @Published var games: [GameEntry] = []
    @Published var showError: Bool = false
    @Published var errorMessage: String = ""

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
            self.tryAddGame(at: url)
        }
    }

    func tryAddGame(at url: URL) {
        let path = url.path

        guard let hcb = findFirstHcb(root: url) else {
            showError("No *.hcb found in selected folder.")
            return
        }

        let title = probeTitleFromHcb(hcbURL: hcb) ?? url.lastPathComponent
        let id = stableId(for: path)
        let now = Int64(Date().timeIntervalSince1970)

        if let idx = games.firstIndex(where: { $0.id == id }) {
            games[idx].rootPath = path
            games[idx].title = title
        } else {
            games.append(GameEntry(id: id, title: title, rootPath: path, addedAtUnix: now))
        }
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

    func launch(game: GameEntry, nls: String) {
        // Update last played
        if let idx = games.firstIndex(of: game) {
            games[idx].lastPlayedAtUnix = Int64(Date().timeIntervalSince1970)
            save()
        }

        // Close launcher windows. The process remains alive; rfvp will take over the UI.
        for w in NSApp.windows {
            w.close()
        }

        let gameC = strdup(game.rootPath)
        let nlsC = strdup(nls)
        guard let gameC, let nlsC else {
            if gameC != nil { free(gameC) }
            if nlsC != nil { free(nlsC) }
            showError("Failed to allocate argument strings.")
            return
        }

        // rfvp_run_entry is expected to start the winit loop (typically non-returning).
        DispatchQueue.main.async {
            rfvp_run_entry(gameC, nlsC)
            // If rfvp returns (unexpected), free memory to avoid leaks.
            free(gameC)
            free(nlsC)
        }
    }

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
        guard let data = try? Data(contentsOf: hcbURL) else { return nil }
        if data.count < 4 { return nil }

        func readU32LE(_ off: Int) -> UInt32? {
            if off + 4 > data.count { return nil }
            return data.withUnsafeBytes { ptr in
                ptr.load(fromByteOffset: off, as: UInt32.self).littleEndian
            }
        }
        func readU16LE(_ off: Int) -> UInt16? {
            if off + 2 > data.count { return nil }
            return data.withUnsafeBytes { ptr in
                ptr.load(fromByteOffset: off, as: UInt16.self).littleEndian
            }
        }
        func readU8(_ off: Int) -> UInt8? {
            if off + 1 > data.count { return nil }
            return data[off]
        }

        guard let sysDescOffU32 = readU32LE(0) else { return nil }
        let sysDescOff = Int(sysDescOffU32)
        if sysDescOff < 0 || sysDescOff >= data.count { return nil }

        var off = sysDescOff

        // entry_point u32
        guard readU32LE(off) != nil else { return nil }
        off += 4

        // non_volatile_global_count u16
        guard readU16LE(off) != nil else { return nil }
        off += 2

        // volatile_global_count u16
        guard readU16LE(off) != nil else { return nil }
        off += 2

        // game_mode u16
        guard readU16LE(off) != nil else { return nil }
        off += 2

        // title_len u8
        guard let titleLenU8 = readU8(off) else { return nil }
        off += 1
        let titleLen = Int(titleLenU8)
        if off + titleLen > data.count { return nil }

        let titleBytesAll = [UInt8](data[off..<(off + titleLen)])
        let end = titleBytesAll.firstIndex(of: 0) ?? titleBytesAll.count
        let raw = Array(titleBytesAll[0..<end])

        return decodeTitleGuess(raw)
    }

    private func decodeTitleGuess(_ bytes: [UInt8]) -> String? {
        if bytes.isEmpty { return nil }

        // Try Shift-JIS first, then GB18030 (and GBK as fallback), pick the best score.
        let cands: [String] = [
            decodeShiftJIS(bytes) ?? "",
            decodeGB18030(bytes) ?? "",
            decodeGBK(bytes) ?? ""
        ].filter { !$0.isEmpty }

        if cands.isEmpty { return nil }

        var best: (score: Int, s: String)? = nil
        for s in cands {
            let t = s.trimmingCharacters(in: .whitespacesAndNewlines)
            if t.isEmpty { continue }
            let sc = scoreText(t)
            if best == nil || sc > best!.score {
                best = (sc, t)
            }
        }
        return best?.s
    }

    private func scoreText(_ s: String) -> Int {
        // Heuristic:
        // - Penalize replacement chars strongly
        // - Reward letters/digits/CJK/kana and common punctuation
        var score = 0
        var repl = 0

        for ch in s {
            if ch == "\u{FFFD}" {
                repl += 1
                continue
            }
            if ch.isASCII {
                if ch.isLetter || ch.isNumber { score += 2 }
                else if ch.isWhitespace { score += 0 }
                else { score += 1 }
                continue
            }
            // CJK / Kana
            for scalar in String(ch).unicodeScalars {
                if (0x3040...0x30FF).contains(Int(scalar.value)) || (0x4E00...0x9FFF).contains(Int(scalar.value)) {
                    score += 3
                } else {
                    score += 1
                }
            }
        }

        return score - repl * 10
    }

    private func decodeShiftJIS(_ bytes: [UInt8]) -> String? {
        return String(data: Data(bytes), encoding: .shiftJIS)
    }

    private func decodeGBK(_ bytes: [UInt8]) -> String? {
        // GBK (CP936)
        let cfEnc = CFStringEncoding(CFStringEncodings.dosChineseSimplif.rawValue)
        let nsEnc = CFStringConvertEncodingToNSStringEncoding(cfEnc)
        return String(data: Data(bytes), encoding: String.Encoding(rawValue: nsEnc))
    }

    private func decodeGB18030(_ bytes: [UInt8]) -> String? {
        // GB18030-2000
        let cfEnc = CFStringEncoding(CFStringEncodings.GB_18030_2000.rawValue)
        let nsEnc = CFStringConvertEncodingToNSStringEncoding(cfEnc)
        return String(data: Data(bytes), encoding: String.Encoding(rawValue: nsEnc))
    }
}
