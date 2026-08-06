import AppKit
import SwiftUI

@main
struct MDStarApp: App {
    @StateObject private var workspace = WorkspaceStore()
    @StateObject private var navigation = NavigationStore()
    @StateObject private var inspector = InspectorStore()
    @StateObject private var settings = ReaderSettings()
    @StateObject private var annotations = AnnotationStore()
    @AppStorage("mdstar.native.appearance") private var appearance = AppearancePreference.system.rawValue

    var body: some Scene {
        WindowGroup("MD Star") {
            WorkspaceWindow(
                workspace: workspace,
                navigation: navigation,
                inspector: inspector,
                settings: settings,
                annotations: annotations
            )
                .frame(minWidth: 880, minHeight: 620)
                .onOpenURL { url in workspace.openFile(url, recordHistory: true) }
                .preferredColorScheme(AppearancePreference(rawValue: appearance)?.colorScheme)
        }
        .windowToolbarStyle(.unified(showsTitle: false))
        .commands {
            CommandGroup(after: .newItem) {
                Button("Open File…", action: workspace.chooseFile)
                    .keyboardShortcut("o", modifiers: .command)
                Button("Open Workspace…", action: workspace.chooseWorkspace)
                    .keyboardShortcut("o", modifiers: [.command, .shift])
                Divider()
                Button("Save") { _ = workspace.save() }
                    .keyboardShortcut("s", modifiers: .command)
                    .disabled(!workspace.isDirty)
                Button("Reload", action: workspace.reload)
                    .keyboardShortcut("r", modifiers: .command)
                    .disabled(workspace.selectedURL == nil)
            }
            CommandMenu("Navigate") {
                Button("Back") { if let url = navigation.back() { workspace.openFile(url, recordHistory: false) } }
                    .keyboardShortcut("[", modifiers: .command)
                    .disabled(!navigation.canGoBack)
                Button("Forward") { if let url = navigation.forward() { workspace.openFile(url, recordHistory: false) } }
                    .keyboardShortcut("]", modifiers: .command)
                    .disabled(!navigation.canGoForward)
            }
            CommandGroup(after: .textEditing) {
                Button("Find in Document…") { if workspace.document != nil { workspace.presentFind() } }
                    .keyboardShortcut("f", modifiers: .command)
                    .disabled(workspace.document == nil)
            }
            CommandMenu("View") {
                Picker("Reading Mode", selection: $workspace.viewMode) {
                    ForEach(ReaderViewMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.inline)
                .disabled(workspace.document == nil)
                Divider()
                Button("Toggle Source") { if workspace.document != nil { workspace.toggleSourcePreview() } }
                    .keyboardShortcut("/", modifiers: .command)
                    .disabled(workspace.document == nil)
                Divider()
                Button(inspector.isVisible ? "Hide Inspector" : "Show Inspector") {
                    inspector.isVisible.toggle()
                }
                .keyboardShortcut("i", modifiers: [.command, .option])
            }
        }

        Settings {
            SettingsView(settings: settings)
        }
    }
}
