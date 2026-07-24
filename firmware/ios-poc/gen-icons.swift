#!/usr/bin/env swift
// 程序化生成 App 图标 + 启动屏 logo（深色·盾牌+₿，与应用内 BrandMark 视觉一致）。
// 每个尺寸矢量重绘（非位图缩放），质量最佳；图标无 alpha 通道以满足 App Store 校验。
// 用 CoreGraphics + CoreText + ImageIO（不用 AppKit 颜色，避免命令行 headless 下 NSColor 回退成黑）。
// 用法： cd firmware/ios-poc && swift gen-icons.swift
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

/// 渲染一枚正方形图标；transparent=true 用于启动屏 logo（透明外底）。
func render(_ px: Int, transparent: Bool = false) -> CGImage {
    let s = CGFloat(px)
    let info: UInt32 = transparent ? CGImageAlphaInfo.premultipliedLast.rawValue
                                    : CGImageAlphaInfo.noneSkipLast.rawValue
    let ctx = CGContext(data: nil, width: px, height: px, bitsPerComponent: 8, bytesPerRow: 0,
                        space: sRGB, bitmapInfo: info)!
    if !transparent { ctx.setFillColor(bg); ctx.fill(CGRect(x: 0, y: 0, width: s, height: s)) }

    // 盾牌（CG 原点左下，y 向上）
    let cx = s/2, cy = s/2, sw = s*0.60, sh = s*0.70
    let p = CGMutablePath()
    p.move(to: CGPoint(x: cx - sw/2, y: cy + sh/2))
    p.addLine(to: CGPoint(x: cx + sw/2, y: cy + sh/2))
    p.addLine(to: CGPoint(x: cx + sw/2, y: cy - sh*0.10))
    p.addCurve(to: CGPoint(x: cx, y: cy - sh/2),
               control1: CGPoint(x: cx + sw/2, y: cy - sh*0.34),
               control2: CGPoint(x: cx + sw*0.22, y: cy - sh/2))
    p.addCurve(to: CGPoint(x: cx - sw/2, y: cy - sh*0.10),
               control1: CGPoint(x: cx - sw*0.22, y: cy - sh/2),
               control2: CGPoint(x: cx - sw/2, y: cy - sh*0.34))
    p.closeSubpath()
    ctx.addPath(p)
    ctx.setLineWidth(max(2, s*0.045)); ctx.setLineJoin(.round)
    ctx.setFillColor(card); ctx.setStrokeColor(brand)
    ctx.drawPath(using: .fillStroke)

    // 居中 ₿（CoreText）
    let font = CTFontCreateWithName("HelveticaNeue-Bold" as CFString, s*0.42, nil)
    let attr = NSAttributedString(string: "₿", attributes: [
        .init(kCTFontAttributeName as String): font,
        .init(kCTForegroundColorAttributeName as String): brand])
    let line = CTLineCreateWithAttributedString(attr)
    let b = CTLineGetBoundsWithOptions(line, .useOpticalBounds)
    ctx.textPosition = CGPoint(x: cx - b.width/2 - b.minX, y: cy - b.height/2 - b.minY)
    CTLineDraw(line, ctx)

    return ctx.makeImage()!
}

func writePNG(_ img: CGImage, _ path: String) {
    let url = URL(fileURLWithPath: path) as CFURL
    let dest = CGImageDestinationCreateWithURL(url, "public.png" as CFString, 1, nil)!
    CGImageDestinationAddImage(dest, img, nil)
    _ = CGImageDestinationFinalize(dest)
}

let base = "Assets.xcassets"
let iconDir = "\(base)/AppIcon.appiconset", logoDir = "\(base)/LaunchLogo.imageset"
let fm = FileManager.default
try? fm.createDirectory(atPath: iconDir, withIntermediateDirectories: true)
try? fm.createDirectory(atPath: logoDir, withIntermediateDirectories: true)

struct Spec { let size: String; let scale: String; let idiom: String; let px: Int }
let specs: [Spec] = [
    Spec(size: "20x20", scale: "2x", idiom: "iphone", px: 40),
    Spec(size: "20x20", scale: "3x", idiom: "iphone", px: 60),
    Spec(size: "29x29", scale: "2x", idiom: "iphone", px: 58),
    Spec(size: "29x29", scale: "3x", idiom: "iphone", px: 87),
    Spec(size: "40x40", scale: "2x", idiom: "iphone", px: 80),
    Spec(size: "40x40", scale: "3x", idiom: "iphone", px: 120),
    Spec(size: "60x60", scale: "2x", idiom: "iphone", px: 120),
    Spec(size: "60x60", scale: "3x", idiom: "iphone", px: 180),
    Spec(size: "1024x1024", scale: "1x", idiom: "ios-marketing", px: 1024),
]
var imagesJSON: [String] = []
for sp in specs {
    let fn = "icon-\(sp.size)@\(sp.scale).png"
    writePNG(render(sp.px), "\(iconDir)/\(fn)")
    imagesJSON.append("    { \"size\":\"\(sp.size)\", \"idiom\":\"\(sp.idiom)\", \"filename\":\"\(fn)\", \"scale\":\"\(sp.scale)\" }")
}
try! "{ \"images\": [\n\(imagesJSON.joined(separator: ",\n"))\n], \"info\": { \"version\": 1, \"author\": \"xcode\" } }"
    .write(toFile: "\(iconDir)/Contents.json", atomically: true, encoding: .utf8)

for (scale, px) in [("1x", 120), ("2x", 240), ("3x", 360)] {
    writePNG(render(px, transparent: true), "\(logoDir)/logo@\(scale).png")
}
try! """
{ "images": [
  { "idiom":"universal", "filename":"logo@1x.png", "scale":"1x" },
  { "idiom":"universal", "filename":"logo@2x.png", "scale":"2x" },
  { "idiom":"universal", "filename":"logo@3x.png", "scale":"3x" }
], "info": { "version": 1, "author": "xcode" } }
""".write(toFile: "\(logoDir)/Contents.json", atomically: true, encoding: .utf8)

print("✅ 生成完成：\(iconDir)（\(specs.count) 尺寸）+ \(logoDir)（logo @1x/2x/3x）")
