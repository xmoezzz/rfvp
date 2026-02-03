import SwiftUI

struct ContentView: View {
    @EnvironmentObject var library: GameLibrary

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
                Text("Copy game folders into Files → On My iPhone → RFVPLauncher → rfvp, then tap Rescan.")
                    .font(.footnote)
                    .foregroundColor(.secondary)
                    .padding(.horizontal, 12)
                ScrollView(.vertical) {
                    LazyVGrid(columns: columns, spacing: 12) {
                        ForEach(library.games) { game in
                            GameTileView(game: game, isLaunching: $isLaunching)
                        }
                    }
                    .padding(12)
                }
            }
            .alert(isPresented: $library.showError) {
                Alert(
                    title: Text("Error"),
                    message: Text(library.errorMessage),
                    dismissButton: .default(Text("OK"))
                )
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
        .fullScreenCover(item: $library.activeGame, onDismiss: {
            isLaunching = false
        }) { game in
            RFVPPlayerScreen(game: game)
                .environmentObject(library)
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("rfvp")
                .font(.headline)

            Spacer()

            Button("Rescan") {
                library.rescanFromDocuments()
            }
        }
        .padding(.horizontal, 12)
        .padding(.top, 8)
    }
}

struct GameTileView: View {
    @EnvironmentObject var library: GameLibrary

    let game: GameEntry
    @Binding var isLaunching: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ZStack(alignment: .topTrailing) {
                RoundedRectangle(cornerRadius: 12)
                    .fill(Color(UIColor.secondarySystemBackground))

                VStack(spacing: 0) {
                    Text(game.title)
                        .font(.headline)
                        .multilineTextAlignment(.center)
                        .padding(10)
                    Spacer()
                }

                Text(game.nlsOption.displayName)
                    .font(.caption2)
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(RoundedRectangle(cornerRadius: 8).fill(Color.black.opacity(0.10)))
                    .padding(8)
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
                        library.launch(game: game)
                    }
                }

                Menu {
                    ForEach(NlsOption.allCases) { opt in
                        Button(opt.displayName) {
                            library.updateNls(game: game, nls: opt)
                        }
                    }
                } label: {
                    Text("NLS")
                }

                Spacer()

                Button("Remove") {
                    library.remove(game: game)
                }
            }
        }
        .padding(10)
    }
}
