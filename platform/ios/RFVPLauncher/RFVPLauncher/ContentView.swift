import SwiftUI

struct ContentView: View {
    @EnvironmentObject var library: GameLibrary

    @State private var showImporter: Bool = false
    @State private var showNlsPicker: Bool = false
    @State private var pendingZipURL: URL? = nil
    @State private var pendingNls: NlsOption = .sjis

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
                            GameTileView(game: game, isLaunching: $isLaunching)
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
                    pendingZipURL = url
                    pendingNls = .sjis
                    showNlsPicker = true
                }
            }
            .sheet(isPresented: $showNlsPicker) {
                NlsPickerSheet(
                    selected: $pendingNls,
                    onCancel: {
                        pendingZipURL = nil
                        showNlsPicker = false
                    },
                    onConfirm: {
                        guard let url = pendingZipURL else {
                            showNlsPicker = false
                            return
                        }
                        showNlsPicker = false
                        pendingZipURL = nil
                        library.importZip(url: url, nls: pendingNls)
                    }
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
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("rfvp")
                .font(.headline)

            Spacer()

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

private struct NlsPickerSheet: View {
    @Binding var selected: NlsOption

    let onCancel: () -> Void
    let onConfirm: () -> Void

    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("NLS")) {
                    Picker("Encoding", selection: $selected) {
                        ForEach(NlsOption.allCases) { opt in
                            Text(opt.displayName).tag(opt)
                        }
                    }
                    .pickerStyle(.segmented)

                    Text("Default is SJIS. You can change this later per game.")
                        .font(.footnote)
                        .foregroundColor(.secondary)
                }
            }
            .navigationTitle("Select NLS")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { onCancel() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Import") { onConfirm() }
                }
            }
        }
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
        .background(RoundedRectangle(cornerRadius: 14).fill(Color(UIColor.systemBackground)))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(Color(UIColor.separator).opacity(0.35)))
    }
}
