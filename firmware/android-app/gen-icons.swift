#!/usr/bin/env swift
// 生成 Android launcher 图标（盾牌+₿，与 iOS 同视觉）到各 mipmap 密度目录。
// PNG 由 macOS 字体渲染 ₿，不受设备字体缺字形影响。
// 用法： cd firmware/android-app && swift gen-icons.swift
import Foundation
import CoreGraphics
import CoreText
import ImageIO

let sRGB = CGColorSpace(name: CGColorSpace.sRGB)!
func rgb(_ hex: UInt32) -> CGColor {
    CGColor(colorSpace: sRGB, components: [CGFloat((hex>>16)&0xFF)/255, CGFloat((hex>>8)&0xFF)/255,
                                           CGFloat(hex&0xFF)/255, 1])!
}
let bg = rgb(0x0E1116), card = rgb(0x1A1F2A), brand = rgb(0xF7931A)

func render(_ px: Int) -> CGImage {
    let s = CGFloat(px)
    let ctx = CGContext(data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
                        space: sRGB, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)!
    ctx.setFillColor(bg); ctx.fill(CGRect(x: 0, y: 0, width: s, height: s))
    let cx = s/2, cy = s/2, sw = s*0.60, sh = s*0.70
    let p = CGMutablePath()
    p.move(to: CGPoint(x: cx - sw/2, y: cy + sh/2))
    p.addLine(to: CGPoint(x: cx + sw/2, y: cy + sh/2))
    p.addLine(to: CGPoint(x: cx + sw/2, y: cy - sh*0.10))
    p.addCurve(to: CGPoint(x: cx, y: cy - sh/2), control1: CGPoint(x: cx + sw/2, y: cy - sh*0.34), control2: CGPoint(x: cx + sw*0.22, y: cy - sh/2))
    p.addCurve(to: CGPoint(x: cx - sw/2, y: cy - sh*0.10), control1: CGPoint(x: cx - sw*0.22, y: cy - sh/2), control2: CGPoint(x: cx - sw/2, y: cy - sh*0.34))
    p.closeSubpath()
    ctx.addPath(p); ctx.setLineWidth(max(2, s*0.045)); ctx.setLineJoin(.round)
    ctx.setFillColor(card); ctx.setStrokeColor(brand); ctx.drawPath(using: .fillStroke)
    let font = CTFontCreateWithName("HelveticaNeue-Bold" as CFString, s*0.42, nil)
    let attr = NSAttributedString(string: "₿", attributes: [.init(kCTFontAttributeName as String): font, .init(kCTForegroundColorAttributeName as String): brand])
    let line = CTLineCreateWithAttributedString(attr)
    let b = CTLineGetBoundsWithOptions(line, .useOpticalBounds)
    ctx.textPosition = CGPoint(x: cx - b.width/2 - b.minX, y: cy - b.height/2 - b.minY)
    CTLineDraw(line, ctx)
    return ctx.makeImage()!
}
func writePNG(_ img: CGImage, _ path: String) {
    let url = URL(fileURLWithPath: path) as CFURL
    let dest = CGImageDestinationCreateWithURL(url, "public.png" as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, img, nil); _ = CGImageDestinationFinalize(dest)
}

let fm = FileManager.default
for (d, px) in [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)] {
    let dir = "app/src/main/res/mipmap-\(d)"
    try? fm.createDirectory(atPath: dir, withIntermediateDirectories: true)
    writePNG(render(px), "\(dir)/ic_launcher.png")
}
print("✅ 生成 Android launcher 图标（mdpi…xxxhdpi）")
