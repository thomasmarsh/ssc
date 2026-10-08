#!/usr/bin/env swift
// Run from any directory: swift scripts/generate-macos-art.swift
// Native vector drawing; no external libraries. Hull coordinates match shipview::outline.
import AppKit

let source = URL(fileURLWithPath: #filePath)
let output = source.deletingLastPathComponent().deletingLastPathComponent()
    .appendingPathComponent("packaging/macos")
let cyan = NSColor(srgbRed: 0.38, green: 0.88, blue: 1, alpha: 1)
let green = NSColor(srgbRed: 0.45, green: 1, blue: 0.68, alpha: 1)
let amber = NSColor(srgbRed: 1, green: 0.65, blue: 0.24, alpha: 1)
let dark = NSColor(srgbRed: 0.025, green: 0.045, blue: 0.09, alpha: 1)

func line(_ points: [NSPoint], _ color: NSColor, _ width: CGFloat) {
    let path = NSBezierPath()
    path.move(to: points[0])
    for p in points.dropFirst() { path.line(to: p) }
    path.lineWidth = width
    path.lineJoinStyle = .round
    path.lineCapStyle = .round
    color.setStroke()
    path.stroke()
}

func ship(_ center: NSPoint, _ radius: CGFloat) {
    // Point the in-game +X hull upward for the icon.
    func p(_ x: CGFloat, _ y: CGFloat) -> NSPoint {
        NSPoint(x: center.x - y * radius, y: center.y + x * radius)
    }
    let hull: [(CGFloat, CGFloat)] = [
        (1.05, 0.4), (0.35, 0.85), (-0.7, 0.85), (-1.05, 0.45),
        (-1.05, -0.45), (-0.7, -0.85), (0.35, -0.85), (1.05, -0.4), (1.05, 0.4)
    ]
    let ring = NSBezierPath()
    ring.appendArc(withCenter: center, radius: radius * 1.85,
                   startAngle: -40, endAngle: 245)
    ring.lineWidth = radius * 0.045
    cyan.setStroke(); ring.stroke()
    for i in 0..<10 {
        let arc = NSBezierPath()
        arc.appendArc(withCenter: center, radius: radius * 1.63,
                      startAngle: CGFloat(i) * 36 + 3, endAngle: CGFloat(i) * 36 + 29)
        arc.lineWidth = radius * 0.035
        green.withAlphaComponent(0.8).setStroke(); arc.stroke()
    }
    line(hull.map { p($0.0, $0.1) }, cyan, radius * 0.065)
    line([p(-1.05, 0.32), p(-1.55, 0), p(-1.05, -0.32)], amber, radius * 0.055)
    line([p(-0.6, 0), p(1.28, 0)], .white, radius * 0.045)
    line([p(0.8, 0.26), p(1.28, 0), p(0.8, -0.26)], .white, radius * 0.045)
    let core = NSBezierPath(ovalIn: NSRect(x: center.x - radius * 0.15,
        y: center.y - radius * 0.15, width: radius * 0.3, height: radius * 0.3))
    amber.setFill(); core.fill()
}

func text(_ value: String, _ x: CGFloat, _ y: CGFloat, _ size: CGFloat, _ color: NSColor) {
    (value as NSString).draw(at: NSPoint(x: x, y: y), withAttributes: [
        .font: NSFont.monospacedSystemFont(ofSize: size, weight: .medium),
        .foregroundColor: color
    ])
}

func render(_ width: Int, _ height: Int, _ name: String, _ draw: () -> Void) throws {
    let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: width, pixelsHigh: height,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: bitmap)
    NSGraphicsContext.current?.imageInterpolation = .high
    draw()
    NSGraphicsContext.restoreGraphicsState()
    try bitmap.representation(using: .png, properties: [:])!
        .write(to: output.appendingPathComponent(name))
}

try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)
try render(1024, 1024, "AppIcon.png") {
    let tile = NSBezierPath(roundedRect: NSRect(x: 32, y: 32, width: 960, height: 960),
                           xRadius: 210, yRadius: 210)
    dark.setFill(); tile.fill()
    ship(NSPoint(x: 512, y: 530), 225)
}
try render(800, 400, "DMG-background.png") {
    dark.setFill(); NSRect(x: 0, y: 0, width: 800, height: 400).fill()
    for i in 0..<85 {
        let x = CGFloat((i * 137 + 43) % 800)
        let y = CGFloat((i * 97 + 19) % 400)
        cyan.withAlphaComponent(i % 4 == 0 ? 0.3 : 0.1).setFill()
        NSBezierPath(ovalIn: NSRect(x: x, y: y, width: 2, height: 2)).fill()
    }
    text("SSC", 54, 322, 32, cyan)
    text("AN ENDLESS, LIVING UNIVERSE", 55, 300, 11, .lightGray)
    // Finder places icons at (200, 190) and (600, 190), measured from the top.
    line([NSPoint(x: 340, y: 210), NSPoint(x: 460, y: 210)], cyan, 2)
    line([NSPoint(x: 448, y: 222), NSPoint(x: 460, y: 210), NSPoint(x: 448, y: 198)], cyan, 2)
    text("Drag SSC to Applications", 277, 74, 16, .white)
    text("SPACE COMBAT / PROCEDURAL WORLDS", 55, 28, 10, cyan.withAlphaComponent(0.6))
}
print("Wrote AppIcon.png and DMG-background.png to \(output.path)")
