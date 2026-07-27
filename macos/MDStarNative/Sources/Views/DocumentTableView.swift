import SwiftUI

struct DocumentTableView: View {
    let headers: [[InlineIR]]
    let rows: [[[InlineIR]]]
    let origin: String

    private var columnCount: Int {
        max(headers.count, rows.map(\.count).max() ?? 0)
    }

    var body: some View {
        if columnCount > 0 {
            ScrollView(.horizontal, showsIndicators: false) {
                Grid(alignment: .topLeading, horizontalSpacing: 0, verticalSpacing: 0) {
                    if !headers.isEmpty {
                        GridRow {
                            ForEach(0..<columnCount, id: \.self) { column in
                                cell(cellInlines(headers, column), header: true, zebra: false)
                            }
                        }
                        Divider().gridCellUnsizedAxes(.horizontal)
                    }
                    ForEach(rows.indices, id: \.self) { rowIndex in
                        GridRow {
                            ForEach(0..<columnCount, id: \.self) { column in
                                cell(cellInlines(rows[rowIndex], column), header: false, zebra: rowIndex.isMultiple(of: 2))
                            }
                        }
                        if rowIndex < rows.count - 1 {
                            Divider().gridCellUnsizedAxes(.horizontal)
                        }
                    }
                }
                .clipShape(RoundedRectangle(cornerRadius: Theme.cornerRadius))
                .overlay(RoundedRectangle(cornerRadius: Theme.cornerRadius).stroke(Color.readerHairline))
            }
            .padding(.vertical, 2)
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Table with \(rows.count) rows and \(columnCount) columns")
        }
    }

    private func cellInlines(_ source: [[InlineIR]], _ column: Int) -> [InlineIR] {
        source.indices.contains(column) ? source[column] : []
    }

    private func cell(_ inlines: [InlineIR], header: Bool, zebra: Bool) -> some View {
        Text(InlineRenderer.attributedString(
            inlines,
            base: header ? .system(.callout, weight: .semibold) : .callout,
            origin: origin
        ))
        .textSelection(.enabled)
        .fixedSize(horizontal: false, vertical: true)
        .frame(minWidth: 80, maxWidth: 340, minHeight: 20, alignment: .leading)
        .padding(.horizontal, 13)
        .padding(.vertical, 9)
        .background(header ? Color.readerStrongFill : (zebra ? Color.readerSubtleFill : Color.clear))
    }
}
