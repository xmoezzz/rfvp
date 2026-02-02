import Foundation
import SwiftUI
import ZIPFoundation
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

    // MARK: - Storage
    private var appSupportDir: URL {
        let base = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let dir = base.appendingPathComponent("RFVPLauncher", isDirectory: true)
        if !fm.fileExists(atPath: dir.path) {
            try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        return dir
    }

    private var gamesDir: URL {
        let dir = appSupportDir.appendingPathComponent("Games", isDirectory: true)
        if !fm.fileExists(atPath: dir.path) {
            try? fm.createDirectory(at: dir, withIntermediateDirectories: true)
        }
        return dir
    }

    private var libraryURL: URL {
        appSupportDir.appendingPathComponent("library.json")
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

    // MARK: - Import ZIP
    func importZip(url: URL, nls: NlsOption) {
        // Import flow:
        // 1) copy zip into a temp file
        // 2) unzip into temp dir
        // 3) find first *.hcb
        // 4) determine gameRoot = directory containing the hcb
        // 5) move/copy the entire extracted root into sandbox Games/<id>/
        // 6) update library.json
        let now = Int64(Date().timeIntervalSince1970)

        let tmpRoot = fm.temporaryDirectory.appendingPathComponent("rfvp_import_\(UUID().uuidString)", isDirectory: true)
        let tmpZip = tmpRoot.appendingPathComponent("import.zip")

        do {
            try fm.createDirectory(at: tmpRoot, withIntermediateDirectories: true)

            // Copy picked file to temp location (DocumentPicker can give security-scoped URLs)
            try fm.copyItem(at: url, to: tmpZip)

            let unzipDir = tmpRoot.appendingPathComponent("unzipped", isDirectory: true)
            try fm.createDirectory(at: unzipDir, withIntermediateDirectories: true)

            try fm.unzipItem(at: tmpZip, to: unzipDir)

            guard let hcb = findFirstHcb(root: unzipDir) else {
                cleanup(url: tmpRoot)
                showError("No *.hcb found in imported ZIP.")
                return
            }

            let gameRoot = hcb.deletingLastPathComponent()
            let title = probeTitleFromHcb(hcbURL: hcb) ?? gameRoot.lastPathComponent
            let id = stableId(for: gameRoot.path)

            // Final install location:
            let installDir = gamesDir.appendingPathComponent(id, isDirectory: true)
            if fm.fileExists(atPath: installDir.path) {
                try? fm.removeItem(at: installDir)
            }
            try fm.createDirectory(at: installDir, withIntermediateDirectories: true)

            // Copy the entire gameRoot folder into installDir/<folderName>
            let leaf = gameRoot.lastPathComponent
            let destRoot = installDir.appendingPathComponent(leaf, isDirectory: true)
            try fm.copyItem(at: gameRoot, to: destRoot)

            // Update library with rootPath = destRoot
            if let idx = games.firstIndex(where: { $0.id == id }) {
                games[idx].title = title
                games[idx].rootPath = destRoot.path
                games[idx].nls = nls.rawValue
            } else {
                games.append(GameEntry(id: id, title: title, rootPath: destRoot.path, nls: nls.rawValue, addedAtUnix: now))
            }
            save()

            cleanup(url: tmpRoot)
            refreshValidation()
        } catch {
            cleanup(url: tmpRoot)
            showError("ZIP import failed: \(error.localizedDescription)")
        }
    }

    func refreshValidation() {
        var changed = false
        games.removeAll { g in
            let ok = fm.fileExists(atPath: g.rootPath) && findFirstHcb(root: URL(fileURLWithPath: g.rootPath)) != nil
            if !ok { changed = true }
            return !ok
        }
        if changed { save() }
    }

    func remove(game: GameEntry) {
        // Remove from library and delete files.
        games.removeAll { $0.id == game.id }
        save()

        let installDir = gamesDir.appendingPathComponent(game.id, isDirectory: true)
        try? fm.removeItem(at: installDir)
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

    private func findFirstHcb(root: URL) -> URL? {
        let keys: [URLResourceKey] = [.isDirectoryKey]
        guard let en = fm.enumerator(at: root, includingPropertiesForKeys: keys, options: [.skipsHiddenFiles]) else {
            return nil
        }
        for case let url as URL in en {
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
