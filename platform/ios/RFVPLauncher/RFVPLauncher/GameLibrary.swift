import Foundation
import SwiftUI
import Darwin

// Canonical strings must match Rust `Nls::from_str`.
enum NlsOption: String, CaseIterable, Identifiable, Codable {
    case sjis = "sjis"
    case gbk = "gbk"
    case utf8 = "utf8"

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .sjis: return "SJIS"
        case .gbk: return "GBK"
        case .utf8: return "UTF-8"
        }
    }
}

struct GameEntry: Identifiable, Codable, Equatable {
    let id: String
    var title: String
    var rootPath: String

    // Stored as canonical string ("sjis" | "gbk" | "utf8").
    var nls: String

    var addedAtUnix: Int64
    var lastPlayedAtUnix: Int64?

    init(
        id: String,
        title: String,
        rootPath: String,
        nls: String = NlsOption.sjis.rawValue,
        addedAtUnix: Int64,
        lastPlayedAtUnix: Int64? = nil
    ) {
        self.id = id
        self.title = title
        self.rootPath = rootPath
        self.nls = GameEntry.normalizeNls(nls)
        self.addedAtUnix = addedAtUnix
        self.lastPlayedAtUnix = lastPlayedAtUnix
    }

    enum CodingKeys: String, CodingKey {
        case id
        case title
        case rootPath
        case nls
        case addedAtUnix
        case lastPlayedAtUnix
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        rootPath = try c.decode(String.self, forKey: .rootPath)
        let nlsOpt = try c.decodeIfPresent(String.self, forKey: .nls) ?? NlsOption.sjis.rawValue
        nls = GameEntry.normalizeNls(nlsOpt)
        addedAtUnix = try c.decode(Int64.self, forKey: .addedAtUnix)
        lastPlayedAtUnix = try c.decodeIfPresent(Int64.self, forKey: .lastPlayedAtUnix)
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(title, forKey: .title)
        try c.encode(rootPath, forKey: .rootPath)
        try c.encode(GameEntry.normalizeNls(nls), forKey: .nls)
        try c.encode(addedAtUnix, forKey: .addedAtUnix)
        try c.encodeIfPresent(lastPlayedAtUnix, forKey: .lastPlayedAtUnix)
    }

    var nlsOption: NlsOption {
        NlsOption(rawValue: GameEntry.normalizeNls(nls)) ?? .sjis
    }

    static func normalizeNls(_ s: String) -> String {
        let t = s.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if NlsOption(rawValue: t) != nil {
            return t
        }
        // Back-compat for old UI values.
        if t == "shiftjis" || t == "shift-jis" || t == "sjis" {
            return NlsOption.sjis.rawValue
        }
        if t == "utf-8" || t == "utf8" {
            return NlsOption.utf8.rawValue
        }
        return NlsOption.sjis.rawValue
    }
}

final class GameLibrary: ObservableObject {
    @Published var games: [GameEntry] = []
    @Published var showError: Bool = false
    @Published var errorMessage: String = ""

    // When non-nil, present the in-app player (iOS host-mode).
    @Published var activeGame: GameEntry? = nil

    private let fm = FileManager.default

    // MARK: - Storage (settings only)
    private var appSupportDir: URL {
        let base = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = base.appendingPathComponent("RFVPLauncher", isDirectory: true)
        if !fm.fileExists(atPath: dir.path) {
            try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        return dir
    }

    // Games live in Documents/rfvp so the user can copy folders in via the Files app.
    private var documentsDir: URL {
        fm.urls(for: .documentDirectory, in: .userDomainMask).first!
    }

    private var documentsGamesDir: URL {
        let dir = documentsDir.appendingPathComponent("rfvp", isDirectory: true)
        if !fm.fileExists(atPath: dir.path) {
            try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        return dir
    }

    private var libraryURL: URL {
        appSupportDir.appendingPathComponent("library.json")
    }

    init() {
        // Ensure the Files-visible folder exists as early as possible.
        _ = documentsGamesDir
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
        // Always rebuild the list from Documents/rfvp.
        rescanFromDocuments()
    }

    func save() {
        do {
            let data = try JSONEncoder().encode(games)
            try data.write(to: libraryURL, options: [.atomic])
        } catch {
            // best-effort
        }
    }

    // MARK: - Scan games in Documents/rfvp
    func rescanFromDocuments() {
        // Preserve per-game settings (NLS, last played, etc.) from library.json.
        let savedById: [String: GameEntry] = Dictionary(uniqueKeysWithValues: games.map { ($0.id, $0) })
        var out: [GameEntry] = []

        let now = Int64(Date().timeIntervalSince1970)

        let root = documentsGamesDir
        guard let items = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: [.isDirectoryKey], options: [.skipsHiddenFiles]) else {
            games = []
            save()
            return
        }

        for url in items {
            let isDir = (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) ?? false
            if !isDir { continue }

            // Only consider HCB files placed directly under the game folder.
            // Do NOT scan subfolders (e.g. updatedata/*.hcb), because the game root is the folder itself.
            guard let hcb = findTopLevelHcb(root: url) else { continue }

            // Heuristic root selection:
            // If HCB is inside e.g. "updatedata", climb until we hit something that looks like the real game root.
            let gameRoot = chooseGameRoot(hcbURL: hcb, containerRoot: url)
            let id = stableId(for: gameRoot.path)

            let saved = savedById[id]
            let title = probeTitleFromHcb(hcbURL: hcb) ?? saved?.title ?? url.lastPathComponent
            let nls = saved?.nls ?? NlsOption.sjis.rawValue
            let addedAt = saved?.addedAtUnix ?? now
            let lastPlayed = saved?.lastPlayedAtUnix

            out.append(GameEntry(id: id, title: title, rootPath: gameRoot.path, nls: nls, addedAtUnix: addedAt, lastPlayedAtUnix: lastPlayed))
        }

        // Stable-ish ordering: recently played first, then newest.
        out.sort { a, b in
            let ap = a.lastPlayedAtUnix ?? 0
            let bp = b.lastPlayedAtUnix ?? 0
            if ap != bp { return ap > bp }
            return a.addedAtUnix > b.addedAtUnix
        }

        games = out
        save()
    }

    func remove(game: GameEntry) {
        // Remove from library and delete the game folder (Documents/rfvp/...)
        games.removeAll { $0.id == game.id }
        save()

        // Best-effort: remove the folder pointed by rootPath.
        let root = URL(fileURLWithPath: game.rootPath)
        try? fm.removeItem(at: root)
    }

    func updateNls(game: GameEntry, nls: NlsOption) {
        if let idx = games.firstIndex(of: game) {
            games[idx].nls = nls.rawValue
            save()
        }
    }

    // MARK: - Launch
    func launch(game: GameEntry) {
        if let idx = games.firstIndex(of: game) {
            games[idx].lastPlayedAtUnix = Int64(Date().timeIntervalSince1970)
            save()
        }
        // Present the in-app player (SwiftUI owns the main loop).
        activeGame = game
    }

    // MARK: - Helpers
    private func cleanup(url: URL) {
        try? fm.removeItem(at: url)
    }

    private func stableId(for path: String) -> String {
        // Stable enough for local library usage.
        return String(path.hashValue, radix: 16)
    }

    private func chooseGameRoot(hcbURL: URL, containerRoot: URL) -> URL {
        // Climb from the HCB directory upward until we hit a directory that looks like a real game root.
        // If nothing matches, fall back to the top-level container folder.
        let containerPath = containerRoot.standardizedFileURL.path
        var cur = hcbURL.deletingLastPathComponent().standardizedFileURL
        while cur.path.hasPrefix(containerPath) {
            if looksLikeGameRoot(cur) {
                return cur
            }
            let parent = cur.deletingLastPathComponent().standardizedFileURL
            if parent.path == cur.path { break }
            cur = parent
        }
        return containerRoot
    }

    private func looksLikeGameRoot(_ dir: URL) -> Bool {
        // Common pack locations.
        let direct = dir.appendingPathComponent("se_sys.bin")
        if fm.fileExists(atPath: direct.path) { return true }

        let dataDir = dir.appendingPathComponent("data", isDirectory: true)
        if fm.fileExists(atPath: dataDir.appendingPathComponent("se_sys.bin").path) { return true }

        // Generic fallback: any .bin at the directory root.
        if let items = try? fm.contentsOfDirectory(atPath: dir.path) {
            if items.contains(where: { $0.lowercased().hasSuffix(".bin") }) { return true }
            if items.contains(where: { $0.lowercased() == "data" || $0.lowercased() == "savedata" }) { return true }
        }
        return false
    }

    private func findTopLevelHcb(root: URL) -> URL? {
        guard let items = try? fm.contentsOfDirectory(
            at: root,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        ) else {
            return nil
        }

        for url in items {
            let isDir = (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) ?? false
            if isDir { continue }
            if url.pathExtension.lowercased() == "hcb" {
                return url
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

        guard readU32LE(off) != nil else { return nil }
        off += 4
        guard readU16LE(off) != nil else { return nil }
        off += 2
        guard readU16LE(off) != nil else { return nil }
        off += 2
        guard readU16LE(off) != nil else { return nil }
        off += 2

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
        let cfEnc = CFStringEncoding(CFStringEncodings.dosChineseSimplif.rawValue)
        let nsEnc = CFStringConvertEncodingToNSStringEncoding(cfEnc)
        return String(data: Data(bytes), encoding: String.Encoding(rawValue: nsEnc))
    }

    private func decodeGB18030(_ bytes: [UInt8]) -> String? {
        let cfEnc = CFStringEncoding(CFStringEncodings.GB_18030_2000.rawValue)
        let nsEnc = CFStringConvertEncodingToNSStringEncoding(cfEnc)
        return String(data: Data(bytes), encoding: String.Encoding(rawValue: nsEnc))
    }

    private func showError(_ msg: String) {
        errorMessage = msg
        showError = true
    }
}
