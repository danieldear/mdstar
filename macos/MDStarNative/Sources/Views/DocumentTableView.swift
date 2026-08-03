import SwiftUI

/// Tables stay inside the reading pane. Cells wrap rather than forcing the
/// entire document into horizontal scrolling; code blocks retain independent
/// horizontal scrolling where preserving source layout is useful.
struct DocumentTableView: View {
    let headers: [[InlineIR]]
    let rows: [[[InlineIR]]]
    let origin: String

    private var columnCount: Int {
        max(headers.count, rows.map(\.count).max() ?? 0)
    }

    var body: some View {
        if columnCount > 0 {
            VStack(spacing: 0) {
                if !headers.isEmpty {
                    tableRow(headers, header: true, zebra: false)
                }
                ForEach(rows.indices, id: \.self) { rowIndex in
                    tableRow(rows[rowIndex], header: false, zebra: rowIndex.isMultiple(of: 2))
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadius))
            .overlay(RoundedRectangle(cornerRadius: Theme.cornerRadius).stroke(Color.readerHairline))
            .padding(.vertical, 2)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Table with \(rows.count) rows and \(columnCount) columns")
        }
    }

    private func cellInlines(_ source: [[InlineIR]], _ column: Int) -> [InlineIR] {
        source.indices.contains(column) ? source[column] : []
    }

    /// An eager stack is intentional here. The reader itself is a LazyVStack;
    /// nesting LazyVGrid inside it can leave table body cells unmaterialized.
    /// Each cell gets an equal flexible share of the available table width and
    /// its text wraps before the document needs to scroll horizontally.
    private func tableRow(_ source: [[InlineIR]], header: Bool, zebra: Bool) -> some View {
        HStack(alignment: .top, spacing: 0) {
            ForEach(0..<columnCount, id: \.self) { column in
                cell(cellInlines(source, column), header: header, zebra: zebra)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func cell(_ inlines: [InlineIR], header: Bool, zebra: Bool) -> some View {
        Text(InlineRenderer.attributedString(
            inlines,
            base: header ? .system(.callout, weight: .semibold) : .callout,
            origin: origin
        ))
        .textSelection(.enabled)
        .fixedSize(horizontal: false, vertical: true)
        .frame(maxWidth: .infinity, minHeight: 20, alignment: .leading)
        .padding(.horizontal, 13)
        .padding(.vertical, 9)
        .background(header ? Color.readerStrongFill : (zebra ? Color.readerSubtleFill : Color.clear))
    }
}
