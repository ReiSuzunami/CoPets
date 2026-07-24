import AppKit
import Foundation

guard CommandLine.arguments.count == 3 else {
    fputs("usage: render_dmg_background.swift <icon.png> <output.png>\n", stderr)
    exit(2)
}

let iconURL = URL(fileURLWithPath: CommandLine.arguments[1])
let outputURL = URL(fileURLWithPath: CommandLine.arguments[2])
guard let icon = NSImage(contentsOf: iconURL) else {
    fputs("could not load icon: \(iconURL.path)\n", stderr)
    exit(1)
}

let canvas = NSImage(size: NSSize(width: 720, height: 440))
canvas.lockFocus()

let background = NSColor(
    calibratedRed: 0.98,
    green: 0.96,
    blue: 0.89,
    alpha: 1
)
background.setFill()
NSBezierPath(rect: NSRect(x: 0, y: 0, width: 720, height: 440)).fill()

icon.draw(
    in: NSRect(x: 304, y: 261, width: 112, height: 112),
    from: .zero,
    operation: .sourceOver,
    fraction: 1
)

let title = "Double-click to install or uninstall CoPets"
let subtitle = "The installer verifies, upgrades, ejects, and cleans up safely."
let paragraph = NSMutableParagraphStyle()
paragraph.alignment = .center
title.draw(
    in: NSRect(x: 110, y: 213, width: 500, height: 40),
    withAttributes: [
        .font: NSFont.systemFont(ofSize: 17, weight: .semibold),
        .foregroundColor: NSColor(calibratedWhite: 0.12, alpha: 1),
        .paragraphStyle: paragraph,
    ]
)
subtitle.draw(
    in: NSRect(x: 110, y: 186, width: 500, height: 26),
    withAttributes: [
        .font: NSFont.systemFont(ofSize: 10, weight: .regular),
        .foregroundColor: NSColor(calibratedWhite: 0.32, alpha: 1),
        .paragraphStyle: paragraph,
    ]
)

let arrow = NSBezierPath()
arrow.lineWidth = 4
arrow.lineCapStyle = .round
arrow.lineJoinStyle = .round
arrow.move(to: NSPoint(x: 360, y: 175))
arrow.line(to: NSPoint(x: 360, y: 118))
arrow.move(to: NSPoint(x: 342, y: 136))
arrow.line(to: NSPoint(x: 360, y: 118))
arrow.line(to: NSPoint(x: 378, y: 136))
NSColor(calibratedRed: 0.93, green: 0.62, blue: 0.11, alpha: 1).setStroke()
arrow.stroke()

canvas.unlockFocus()
guard let tiff = canvas.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:])
else {
    fputs("could not render background\n", stderr)
    exit(1)
}
try png.write(to: outputURL, options: .atomic)
