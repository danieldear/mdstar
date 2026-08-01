#!/usr/bin/env bash
# Generate the DMG installer background image.
#
# Renders assets/dmg/background.png with CoreGraphics (via Swift, which the
# native app already requires) so the installer needs no design tooling or
# binary asset checked into the repository. Re-run after changing the window
# geometry in make_dmg.sh.
#
# Usage:
#   ./script/make_dmg_background.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_DIR="$ROOT_DIR/assets/dmg"
OUTPUT="$OUTPUT_DIR/background.png"

# Must match the window geometry in make_dmg.sh.
WIDTH=620
HEIGHT=420

command -v swift >/dev/null 2>&1 || { echo "error: swift not found" >&2; exit 1; }
mkdir -p "$OUTPUT_DIR"

GENERATOR="$(mktemp -t dmgbg).swift"
trap 'rm -f "$GENERATOR"' EXIT

cat >"$GENERATOR" <<'SWIFT'
import AppKit
import CoreGraphics
import Foundation

let width = Int(CommandLine.arguments[1])!
let height = Int(CommandLine.arguments[2])!
let outputPath = CommandLine.arguments[3]

// Render at 2x so the background stays crisp on Retina displays.
let scale = 2
let pixelWidth = width * scale
let pixelHeight = height * scale

guard let context = CGContext(
    data: nil,
    width: pixelWidth,
    height: pixelHeight,
    bitsPerComponent: 8,
    bytesPerRow: 0,
    space: CGColorSpaceCreateDeviceRGB(),
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { exit(1) }

context.scaleBy(x: CGFloat(scale), y: CGFloat(scale))

let bounds = CGRect(x: 0, y: 0, width: width, height: height)

// Soft vertical gradient, cool neutral so both the app icon and the folder read
// clearly without competing with them.
let colorSpace = CGColorSpaceCreateDeviceRGB()
let gradient = CGGradient(
    colorsSpace: colorSpace,
    colors: [
        CGColor(red: 0.97, green: 0.975, blue: 0.985, alpha: 1),
        CGColor(red: 0.90, green: 0.915, blue: 0.945, alpha: 1)
    ] as CFArray,
    locations: [0, 1]
)!
context.drawLinearGradient(
    gradient,
    start: CGPoint(x: 0, y: height),
    end: CGPoint(x: 0, y: 0),
    options: []
)

// Arrow from the app icon toward the Applications folder. Icon centers sit at
// y = 200 measured from the window top, so flip for CoreGraphics' bottom-left
// origin, then drop below the icons to clear their labels.
let iconCenterY = CGFloat(height - 200)
let arrowY = iconCenterY - 6
let arrowStartX: CGFloat = 258
let arrowEndX: CGFloat = 372

context.setStrokeColor(CGColor(red: 0.42, green: 0.46, blue: 0.53, alpha: 0.55))
context.setLineWidth(2.5)
context.setLineCap(.round)
context.setLineDash(phase: 0, lengths: [9, 7])
context.move(to: CGPoint(x: arrowStartX, y: arrowY))
context.addLine(to: CGPoint(x: arrowEndX - 12, y: arrowY))
context.strokePath()

// Solid arrowhead.
context.setLineDash(phase: 0, lengths: [])
context.setFillColor(CGColor(red: 0.42, green: 0.46, blue: 0.53, alpha: 0.75))
context.move(to: CGPoint(x: arrowEndX + 2, y: arrowY))
context.addLine(to: CGPoint(x: arrowEndX - 15, y: arrowY + 8))
context.addLine(to: CGPoint(x: arrowEndX - 15, y: arrowY - 8))
context.closePath()
context.fillPath()

// Instruction line, sitting under the icons.
let text = "Drag MD Star to your Applications folder"
let paragraph = NSMutableParagraphStyle()
paragraph.alignment = .center
let attributes: [NSAttributedString.Key: Any] = [
    .font: NSFont.systemFont(ofSize: 13, weight: .medium),
    .foregroundColor: NSColor(calibratedRed: 0.35, green: 0.39, blue: 0.46, alpha: 1),
    .paragraphStyle: paragraph
]
let attributed = NSAttributedString(string: text, attributes: attributes)

let graphicsContext = NSGraphicsContext(cgContext: context, flipped: false)
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = graphicsContext
attributed.draw(in: CGRect(x: 0, y: 74, width: CGFloat(width), height: 24))
NSGraphicsContext.restoreGraphicsState()

guard let image = context.makeImage() else { exit(1) }
let bitmap = NSBitmapImageRep(cgImage: image)
bitmap.size = NSSize(width: width, height: height)
guard let data = bitmap.representation(using: .png, properties: [:]) else { exit(1) }
try! data.write(to: URL(fileURLWithPath: outputPath))
_ = bounds
SWIFT

swift "$GENERATOR" "$WIDTH" "$HEIGHT" "$OUTPUT"

printf '\033[32m  ✓\033[0m Background image: %s (%sx%s @2x)\n' "$OUTPUT" "$WIDTH" "$HEIGHT"
