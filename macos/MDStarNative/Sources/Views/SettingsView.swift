import SwiftUI

struct SettingsView: View {
    @ObservedObject var settings: ReaderSettings
    @AppStorage("mdstar.native.appearance") private var appearance = AppearancePreference.system.rawValue

    var body: some View {
        Form {
            Picker("Appearance", selection: $appearance) {
                ForEach(AppearancePreference.allCases) { preference in
                    Text(preference.title).tag(preference.rawValue)
                }
            }
            Text("MD Star uses macOS system materials and adapts to the selected appearance.")
                .font(.callout)
                .foregroundStyle(.secondary)
            Section("Reading") {
                Picker("Typeface", selection: $settings.fontFamily) {
                    ForEach(ReaderFontFamily.allCases) { family in
                        Text(family.title).tag(family)
                    }
                }
                LabeledContent("Text size") {
                    HStack {
                        Button(action: settings.zoomOut) { Image(systemName: "minus") }
                            .disabled(!settings.canZoomOut)
                        Text("\(Int(settings.fontSize)) pt").monospacedDigit()
                        Button(action: settings.zoomIn) { Image(systemName: "plus") }
                            .disabled(!settings.canZoomIn)
                    }
                }
            }
        }
        .formStyle(.grouped)
        .padding(20)
        .frame(width: 390)
    }
}
