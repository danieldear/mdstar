import Foundation
import XCTest
@testable import MDStarNative

@MainActor
final class SlashCommandEngineTests: XCTestCase {
    func testSlashAtLineStartProducesCommandContext() {
        let source = "# Notes\n\n/hea"
        let context = SlashCommandEngine.context(
            in: source,
            selectedRange: NSRange(location: (source as NSString).length, length: 0)
        )

        XCTAssertEqual(context?.query, "hea")
        XCTAssertEqual(
            context?.replacementRange,
            NSRange(location: ("# Notes\n\n" as NSString).length, length: 4)
        )
    }

    func testIndentedSlashIsAllowed() {
        let source = "    /task"
        let context = SlashCommandEngine.context(
            in: source,
            selectedRange: NSRange(location: (source as NSString).length, length: 0)
        )

        XCTAssertEqual(context?.query, "task")
        XCTAssertEqual(context?.replacementRange, NSRange(location: 4, length: 5))
    }

    func testSlashInProseOrPathDoesNotOpenPalette() {
        for source in ["Read /this", "https://example.com", "docs/readme/"] {
            XCTAssertNil(
                SlashCommandEngine.context(
                    in: source,
                    selectedRange: NSRange(location: (source as NSString).length, length: 0)
                ),
                source
            )
        }
    }

    func testSelectionDoesNotOpenPalette() {
        XCTAssertNil(
            SlashCommandEngine.context(
                in: "/head",
                selectedRange: NSRange(location: 1, length: 2)
            )
        )
    }

    func testCommandFilteringSearchesTitlesAndKeywords() {
        XCTAssertEqual(
            SlashCommandEngine.filteredCommands(matching: "diagram").map(\.id),
            ["mermaid"]
        )
        XCTAssertEqual(
            SlashCommandEngine.filteredCommands(matching: "checkbox").map(\.id),
            ["task"]
        )
        XCTAssertEqual(
            SlashCommandEngine.filteredCommands(matching: "h2").map(\.id),
            ["heading-2"]
        )
    }

    func testEmptyQueryShowsAllCommands() {
        XCTAssertEqual(
            SlashCommandEngine.filteredCommands(matching: ""),
            SlashCommand.markdownCommands
        )
    }

    func testEveryCommandSelectionFallsWithinItsInsertion() {
        for command in SlashCommand.markdownCommands {
            let insertionLength = command.insertion.utf16.count
            XCTAssertLessThanOrEqual(command.selection.location, insertionLength, command.id)
            XCTAssertLessThanOrEqual(
                command.selection.location + command.selection.length,
                insertionLength,
                command.id
            )
        }
    }

    func testPaletteSelectionSupportsArrowNavigationAndWrapping() {
        let model = SlashCommandPaletteModel()
        let commands = Array(SlashCommand.markdownCommands.prefix(3))
        model.update(commands: commands)

        XCTAssertEqual(model.selectedCommand?.id, commands[0].id)
        model.moveSelection(by: 1)
        XCTAssertEqual(model.selectedCommand?.id, commands[1].id)
        model.moveSelection(by: -1)
        XCTAssertEqual(model.selectedCommand?.id, commands[0].id)
        model.moveSelection(by: -1)
        XCTAssertEqual(model.selectedCommand?.id, commands[2].id)
    }

}
