// Draws the Kafkalens app icon (1024×1024 PNG): a topic log (stacked
// records) under a magnifying lens, on a rounded square following the macOS
// icon grid. Run through scripts/make-icon.sh, which builds the .icns.
//
// Usage: swift scripts/make-icon.swift <output.png>

import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let size = 1024
let output = CommandLine.arguments.dropFirst().first ?? "AppIcon.png"

func color(_ hex: UInt32, _ alpha: CGFloat = 1) -> CGColor {
    CGColor(
        srgbRed: CGFloat((hex >> 16) & 0xff) / 255,
        green: CGFloat((hex >> 8) & 0xff) / 255,
        blue: CGFloat(hex & 0xff) / 255,
        alpha: alpha
    )
}

let space = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(
    data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
    space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
)!
// Work top-down like a design tool.
ctx.translateBy(x: 0, y: CGFloat(size))
ctx.scaleBy(x: 1, y: -1)

// macOS grid: 824pt body centred on the 1024 canvas, with a soft drop shadow.
let body = CGRect(x: 100, y: 100, width: 824, height: 824)
let bodyPath = CGPath(roundedRect: body, cornerWidth: 185, cornerHeight: 185, transform: nil)

ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -12), blur: 28, color: color(0x000000, 0.35))
ctx.addPath(bodyPath)
ctx.setFillColor(color(0x0B3D4F))
ctx.fillPath()
ctx.restoreGState()

// Background gradient: deep petrol blue to teal.
ctx.saveGState()
ctx.addPath(bodyPath)
ctx.clip()
let background = CGGradient(
    colorsSpace: space,
    colors: [color(0x0E7490), color(0x0B3D5C)] as CFArray,
    locations: [0, 1]
)!
ctx.drawLinearGradient(background, start: CGPoint(x: 200, y: 100), end: CGPoint(x: 824, y: 924), options: [])

// Topic log: three records of different lengths, like messages on a partition.
let records: [(y: CGFloat, width: CGFloat, alpha: CGFloat)] = [
    (300, 520, 0.95),
    (430, 420, 0.80),
    (560, 470, 0.65),
]
for record in records {
    let bar = CGRect(x: 230, y: record.y, width: record.width, height: 80)
    ctx.addPath(CGPath(roundedRect: bar, cornerWidth: 40, cornerHeight: 40, transform: nil))
    ctx.setFillColor(color(0xFFFFFF, record.alpha))
    ctx.fillPath()
}
ctx.restoreGState()

// Lens over the lower right of the log.
let lensCenter = CGPoint(x: 610, y: 600)
let lensRadius: CGFloat = 150
let lensRect = CGRect(
    x: lensCenter.x - lensRadius, y: lensCenter.y - lensRadius,
    width: lensRadius * 2, height: lensRadius * 2
)

// Handle first, starting inside the rim so the rim hides its square end;
// only the outer end is rounded.
let handleStart = CGPoint(x: lensCenter.x + 100, y: lensCenter.y + 100)
let handleEnd = CGPoint(x: lensCenter.x + 215, y: lensCenter.y + 215)
ctx.saveGState()
ctx.setLineCap(.butt)
ctx.setLineWidth(70)
ctx.setStrokeColor(color(0xF59E0B))
ctx.move(to: handleStart)
ctx.addLine(to: handleEnd)
ctx.strokePath()
ctx.addEllipse(in: CGRect(x: handleEnd.x - 35, y: handleEnd.y - 35, width: 70, height: 70))
ctx.setFillColor(color(0xF59E0B))
ctx.fillPath()
ctx.restoreGState()

// Glass: a light tint so the records behind still read through.
ctx.addEllipse(in: lensRect)
ctx.setFillColor(color(0xE0F2FE, 0.28))
ctx.fillPath()

ctx.addEllipse(in: lensRect)
ctx.setLineWidth(46)
ctx.setStrokeColor(color(0xFBBF24))
ctx.strokePath()

// Glint.
ctx.saveGState()
ctx.setLineCap(.round)
ctx.setLineWidth(22)
ctx.setStrokeColor(color(0xFFFFFF, 0.7))
ctx.addArc(center: lensCenter, radius: lensRadius - 62, startAngle: .pi * 1.1, endAngle: .pi * 1.45, clockwise: false)
ctx.strokePath()
ctx.restoreGState()

let image = ctx.makeImage()!
let url = URL(fileURLWithPath: output) as CFURL
let destination = CGImageDestinationCreateWithURL(url, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(destination, image, nil)
guard CGImageDestinationFinalize(destination) else {
    fatalError("could not write \(output)")
}
