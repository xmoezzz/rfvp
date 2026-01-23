import SwiftUI

struct ContentView: View {
    @EnvironmentObject var library: GameLibrary
    @AppStorage("rfvp.default_nls") private var defaultNls: String = "ShiftJIS"

    @State private var showImporter: Bool = false
    @State private var isLaunching: Bool = false

    private let columns: [GridItem] = [
        GridItem(.flexible(), spacing: 12),
        GridItem(.flexible(), spacing: 12),
        GridItem(.flexible(), spacing: 12),
    ]

    var body: some View {
        ZStack {
            VStack(spacing: 10) {
                header
                Divider()
                ScrollView(.vertical) {
                    LazyVGrid(columns: columns, spacing: 12) {
                        ForEach(library.games) { game in
                            GameTileView(game: game, defaultNls: defaultNls, isLaunching: $isLaunching)
                        }
                    }
                    .padding(12)
                }
            }
            .alert(isPresented: $library.showError) {
                Alert(
                    title: Text("Import failed"),
                    message: Text(library.errorMessage),
                    dismissButton: .default(Text("OK"))
                )
            }
            .sheet(isPresented: $showImporter) {
                ZipDocumentPicker { url in
                    showImporter = false
                    guard let url else { return }
                    library.importZip(url: url)
                }
            }

            if isLaunching {
                Color.black.opacity(0.35).ignoresSafeArea()
                VStack(spacing: 12) {
                    ProgressView()
                    Text("Launching…")
                        .foregroundColor(.white)
                }
                .padding(18)
                .background(RoundedRectangle(cornerRadius: 12).fill(Color.black.opacity(0.6)))
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("rfvp")
                .font(.headline)

            Spacer()

            Text("NLS:")
                .foregroundColor(.secondary)

            TextField("ShiftJIS", text: $defaultNls)
                .textFieldStyle(RoundedBorderTextFieldStyle())
                .frame(width: 110)

            Button("Import ZIP") {
                showImporter = true
            }

            Button("Refresh") {
                library.refreshValidation()
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 8)
    }
}

struct GameTileView: View {
    @EnvironmentObject var library: GameLibrary
    let game: GameEntry
    let defaultNls: String
    @Binding var isLaunching: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ZStack {
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(UIColor.secondarySystemBackground))
                Text(game.title)
                    .font(.headline)
                    .multilineTextAlignment(.center)
                    .padding(10)
            }
            .frame(height: 130)

            Text(game.title)
                .font(.subheadline)
                .lineLimit(2)

            Text(game.rootPath)
                .font(.caption2)
                .foregroundColor(.secondary)
                .lineLimit(1)

            HStack {
                Button("Play") {
                    isLaunching = true
                    DispatchQueue.main.async {
                        library.launch(game: game, nls: defaultNls)
                    }
                }
                Spacer()
                Button("Remove") {
                    library.remove(game: game)
                }
            }
        }
        .padding(10)
        .background(RoundedRectangle(cornerRadius: 14).fill(Color(UIColor.systemBackground)))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(Color(UIColor.separator).opacity(0.35)))
    }
}
