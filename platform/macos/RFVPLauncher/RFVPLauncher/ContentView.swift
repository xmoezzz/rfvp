import SwiftUI
import AppKit

struct ContentView: View {
    @EnvironmentObject var library: GameLibrary

    @AppStorage("rfvp.default_nls") private var defaultNls: String = "ShiftJIS"

    private let columns: [GridItem] = [
        GridItem(.flexible(), spacing: 16),
        GridItem(.flexible(), spacing: 16),
        GridItem(.flexible(), spacing: 16),
    ]

    var body: some View {
        VStack(spacing: 12) {
            header
            Divider()
            ScrollView(.vertical) {
                LazyVGrid(columns: columns, spacing: 16) {
                    ForEach(library.games) { game in
                        GameTileView(game: game, defaultNls: defaultNls)
                    }
                }
                .padding(16)
            }
        }
        .frame(minWidth: 920, minHeight: 560)
        .alert(isPresented: $library.showError) {
            Alert(
                title: Text("Import failed"),
                message: Text(library.errorMessage),
                dismissButton: .default(Text("OK"))
            )
        }
}

    private var header: some View {
        HStack(spacing: 12) {
            Text("rfvp")
                .font(.title2)
                .bold()

            Spacer()

            Text("NLS:")
                .foregroundColor(.secondary)

            TextField("ShiftJIS", text: $defaultNls)
                .textFieldStyle(.roundedBorder)
                .frame(width: 120)

            Button("Import…") {
                library.importGameFolder()
            }
            .keyboardShortcut("i", modifiers: [.command])

            Button("Refresh") {
                library.refreshValidation()
            }
            .keyboardShortcut("r", modifiers: [.command])
        }
        .padding(.horizontal, 16)
        .padding(.top, 12)
    }
}

struct GameTileView: View {
    @EnvironmentObject var library: GameLibrary
    let game: GameEntry
    let defaultNls: String

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ZStack {
                RoundedRectangle(cornerRadius: 12)
                    .strokeBorder(Color.secondary.opacity(0.35), lineWidth: 1)
                    .background(RoundedRectangle(cornerRadius: 12).fill(Color.secondary.opacity(0.08)))

                if let cover = library.loadCoverImage(game: game) {
                    Image(nsImage: cover)
                        .resizable()
                        .scaledToFill()
                        .clipShape(RoundedRectangle(cornerRadius: 12))
                } else {
                    Text(game.title)
                        .font(.headline)
                        .multilineTextAlignment(.center)
                        .padding(12)
                }
            }
            .frame(height: 160)
            .clipped()

            Text(game.title)
                .font(.headline)
                .lineLimit(2)

            Text(game.rootPath)
                .font(.caption)
                .foregroundColor(.secondary)
                .lineLimit(1)

            HStack {
                Button("Play") {
                    library.launch(game: game, nls: defaultNls)
                }
                .keyboardShortcut(.defaultAction)

                Spacer()

                Menu {
                    Button("Remove from Library") {
                        library.remove(game: game)
                    }
                    Button("Reveal in Finder") {
                        library.revealInFinder(game: game)
                    }
                } label: {
                    Text("…")
                        .font(.headline)
                        .frame(width: 28, height: 22)
                }
                .menuStyle(.borderlessButton)
            }
        }
        .padding(12)
        .background(RoundedRectangle(cornerRadius: 16).fill(Color(NSColor.windowBackgroundColor)))
        .overlay(RoundedRectangle(cornerRadius: 16).strokeBorder(Color.secondary.opacity(0.20)))
    }
}
