// Draws resources/icons/icon-macos.png, the macOS app icon, from the boar
// of resources/icons/icon.png. Rerun after changing icon.png:
//
//   swift scripts/macos-icon.swift resources/icons/icon.png resources/icons/icon-macos.png
//
// macOS 26 boxes any icon that doesn't fill Apple's rounded square into a
// grey tile. This one follows Apple's grid: 1024 px canvas, 824 px body,
// ~185 px corner radius, drop shadow. Dark ground, like the app's theme.

import AppKit

let args = CommandLine.arguments
guard args.count == 3, let sprite = NSImage(contentsOfFile: args[1]) else {
    print("usage: macos-icon.swift <icon.png> <out.png>")
    exit(1)
}

let size: CGFloat = 1024
let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size), bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0)!
let context = NSGraphicsContext(bitmapImageRep: rep)!
context.imageInterpolation = .high
NSGraphicsContext.current = context

let body = NSRect(x: 100, y: 100, width: 824, height: 824)
let shape = NSBezierPath(roundedRect: body, xRadius: 185, yRadius: 185)
let top = NSColor(srgbRed: 0.20, green: 0.20, blue: 0.18, alpha: 1)
let bottom = NSColor(srgbRed: 0.08, green: 0.08, blue: 0.07, alpha: 1)

func withShadow(alpha: CGFloat, offset: CGFloat, blur: CGFloat, _ draw: () -> Void) {
    NSGraphicsContext.saveGraphicsState()
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(alpha)
    shadow.shadowOffset = NSSize(width: 0, height: -offset)
    shadow.shadowBlurRadius = blur
    shadow.set()
    draw()
    NSGraphicsContext.restoreGraphicsState()
}

withShadow(alpha: 0.30, offset: 10, blur: 20) {
    top.setFill()
    shape.fill()
}
NSGradient(starting: top, ending: bottom)!.draw(in: shape, angle: -90)

// Faint highlight along the edge, so the tile reads on a dark Dock.
NSColor.white.withAlphaComponent(0.08).setStroke()
let edge = NSBezierPath(roundedRect: body.insetBy(dx: 2, dy: 2), xRadius: 183, yRadius: 183)
edge.lineWidth = 4
edge.stroke()

// The boar fills ~72 % of the body, slightly above centre. The source is
// 256 px: the upscale stays sharp at Dock sizes.
let side: CGFloat = 600
let spriteRect = NSRect(x: (size - side) / 2, y: (size - side) / 2 + 12, width: side, height: side)
withShadow(alpha: 0.35, offset: 8, blur: 16) {
    sprite.draw(in: spriteRect, from: .zero, operation: .sourceOver, fraction: 1)
}

try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[2]))
