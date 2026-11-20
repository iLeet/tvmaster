// Draws src/icon.png (512x423, the size Samsung TVs use for app tiles) with the app name in it,
// so the name shows even when the TV's launcher does not draw one. Run: swift tools/make-icon.swift
import AppKit

let w = 512, h = 423
let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: w, pixelsHigh: h, bitsPerSample: 8, samplesPerPixel: 4,
                           hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

let bounds = NSRect(x: 0, y: 0, width: w, height: h)
NSGradient(colors: [NSColor(red: 0.08, green: 0.26, blue: 0.78, alpha: 1), NSColor(red: 0.02, green: 0.05, blue: 0.24, alpha: 1)])!
    .draw(in: bounds, angle: -90)

// a tiny guide grid
let rows: [[(CGFloat, CGFloat)]] = [[(0, 0.42), (0.44, 0.56)], [(0, 0.7), (0.72, 0.28)], [(0, 0.3), (0.32, 0.68)]]
let gx: CGFloat = 56, gw: CGFloat = 400
for (i, r) in rows.enumerated() {
    let y = CGFloat(h) - 120 - CGFloat(i) * 44
    for (j, c) in r.enumerated() {
        let rect = NSRect(x: gx + c.0 * gw, y: y, width: c.1 * gw - 6, height: 36)
        let p = NSBezierPath(roundedRect: rect, xRadius: 6, yRadius: 6)
        (i == 0 && j == 0 ? NSColor(red: 1, green: 0.85, blue: 0.29, alpha: 1) : NSColor(red: 0.33, green: 0.52, blue: 0.95, alpha: 1)).setFill()
        p.fill()
    }
}

func text(_ s: String, size: CGFloat, color: NSColor, y: CGFloat) {
    let a: [NSAttributedString.Key: Any] = [.font: NSFont.systemFont(ofSize: size, weight: .heavy), .foregroundColor: color]
    let str = NSAttributedString(string: s, attributes: a)
    let sz = str.size()
    str.draw(at: NSPoint(x: (CGFloat(w) - sz.width) / 2, y: y))
}
text("TVMaster", size: 74, color: .white, y: 92)
text("GUIDE", size: 44, color: NSColor(red: 1, green: 0.85, blue: 0.29, alpha: 1), y: 26)

NSGraphicsContext.restoreGraphicsState()
let png = rep.representation(using: .png, properties: [:])!
try png.write(to: URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "src/icon.png"))
print("wrote icon")
