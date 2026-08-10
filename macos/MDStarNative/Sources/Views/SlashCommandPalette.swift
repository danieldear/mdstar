import SwiftUI

@MainActor
final class SlashCommandPaletteModel: ObservableObject {
    @Published var commands: [SlashCommand] = []
    @Published var selectedIndex = 0

    var selectedCommand: SlashCommand? {
        commands.indices.contains(selectedIndex) ? commands[selectedIndex] : nil
    }

    func update(commands: [SlashCommand]) {
        let commandIDsChanged = self.commands.map(\.id) != commands.map(\.id)
        self.commands = commands
        selectedIndex = commandIDsChanged
            ? 0
            : min(selectedIndex, max(0, commands.count - 1))
    }

    func moveSelection(by offset: Int) {
        guard !commands.isEmpty else { return }
        selectedIndex = (selectedIndex + offset + commands.count) % commands.count
    }
}

struct SlashCommandPalette: View {
    @ObservedObject var model: SlashCommandPaletteModel
    let choose: (SlashCommand) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("INSERT MARKDOWN")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)
                .padding(.top, 10)
                .padding(.bottom, 6)

            if model.commands.isEmpty {
                Text("No matching commands")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(12)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 2) {
                            ForEach(Array(model.commands.enumerated()), id: \.element.id) { index, command in
                                Button {
                                    choose(command)
                                } label: {
                                    HStack(spacing: 10) {
                                        Image(systemName: command.systemImage)
                                            .font(.system(size: 15))
                                            .frame(width: 24, height: 24)
                                            .foregroundStyle(index == model.selectedIndex ? Color.white : Color.accentColor)

                                        VStack(alignment: .leading, spacing: 1) {
                                            Text(command.title)
                                                .font(.callout.weight(.medium))
                                            Text(command.subtitle)
                                                .font(.caption)
                                                .foregroundStyle(index == model.selectedIndex ? AnyShapeStyle(Color.white.opacity(0.82)) : AnyShapeStyle(.secondary))
                                        }
                                        Spacer(minLength: 8)
                                    }
                                    .padding(.horizontal, 9)
                                    .padding(.vertical, 6)
                                    .contentShape(Rectangle())
                                    .background(
                                        index == model.selectedIndex ? Color.accentColor : Color.clear,
                                        in: RoundedRectangle(cornerRadius: 7, style: .continuous)
                                    )
                                }
                                .buttonStyle(.plain)
                                .id(command.id)
                            }
                        }
                        .padding(.horizontal, 6)
                        .padding(.bottom, 6)
                    }
                    .onChange(of: model.selectedIndex) { index in
                        guard model.commands.indices.contains(index) else { return }
                        proxy.scrollTo(model.commands[index].id, anchor: .center)
                    }
                }
            }
        }
        .frame(width: 310, height: min(390, max(88, CGFloat(model.commands.count) * 51 + 38)))
        .background(.regularMaterial)
    }
}
