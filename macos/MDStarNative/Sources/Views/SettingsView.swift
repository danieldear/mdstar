import SwiftUI

struct SettingsView: View {
    @ObservedObject var settings: ReaderSettings

    var body: some View {
        TabView {
            AppearanceSettings(settings: settings)
                .tabItem { Label("Appearance", systemImage: "paintbrush") }
            ReadingSettings(settings: settings)
                .tabItem { Label("Reading", systemImage: "textformat") }
        }
        .frame(width: 460)
    }
}

// MARK: - Appearance

private struct AppearanceSettings: View {
    @ObservedObject var settings: ReaderSettings

    var body: some View {
        Form {
            Picker("Theme", selection: $settings.appearance) {
                ForEach(AppearancePreference.allCases) { preference in
                    Text(preference.title).tag(preference)
                }
            }
            .pickerStyle(.segmented)

            LabeledContent("Accent") {
                Text("Follows the system accent color")
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding(20)
    }
}

// MARK: - Reading

private struct ReadingSettings: View {
    @ObservedObject var settings: ReaderSettings

    var body: some View {
        Form {
            Section {
                Picker("Typeface", selection: $settings.fontFamily) {
                    ForEach(ReaderFontFamily.allCases) { family in
                        Text(family.title)
                            .font(.system(size: 13, design: family.design))
                            .tag(family)
                    }
                }

                LabeledContent("Text size") {
                    HStack(spacing: 10) {
                        Button {
                            settings.zoomOut()
                        } label: {
                            Image(systemName: "minus")
                        }
                        .disabled(!settings.canZoomOut)

                        Text("\(Int(settings.fontSize)) pt")
                            .font(.body.monospacedDigit())
                            .frame(width: 46)

                        Button {
                            settings.zoomIn()
                        } label: {
                            Image(systemName: "plus")
                        }
                        .disabled(!settings.canZoomIn)

                        Button("Reset") { settings.resetZoom() }
                            .padding(.leading, 4)
                    }
                }

                LabeledContent("Line spacing") {
                    Slider(value: $settings.lineSpacing, in: 0...12, step: 0.5)
                        .frame(width: 180)
                }

                LabeledContent("Line width") {
                    HStack(spacing: 10) {
                        Slider(value: $settings.contentWidth, in: 560...1000, step: 20)
                            .frame(width: 150)
                        Text("\(Int(settings.contentWidth))")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                            .frame(width: 34, alignment: .trailing)
                    }
                }
            } footer: {
                Text("Text size also responds to \u{2318}+ and \u{2318}\u{2212} while reading.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Section("Preview") {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Heading")
                        .font(settings.headingFont(level: 2))
                    Text("Body text renders at the size and typeface you choose, with `inline code` shown in a monospaced face.")
                        .font(settings.bodyFont)
                        .lineSpacing(settings.lineSpacing)
                    Text("let sample = \"code\"")
                        .font(settings.codeFont)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 5)
                        .background(Color.readerSubtleFill, in: RoundedRectangle(cornerRadius: 6))
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 4)
            }
        }
        .formStyle(.grouped)
        .padding(20)
    }
}
