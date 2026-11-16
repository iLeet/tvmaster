import CoreImage
import CoreImage.CIFilterBuiltins
import Foundation

public enum QRCode {
    /// A QR code for `text` as a square bitmap of about `size` pixels, scaled in whole
    /// steps so the modules stay sharp.
    public static func cgImage(for text: String, size: CGFloat) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(text.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        let scale = max(1, floor(size / output.extent.width))
        let scaled = output.transformed(by: CGAffineTransform(scaleX: scale, y: scale))
        return CIContext().createCGImage(scaled, from: scaled.extent)
    }

    /// Read the text back out of a QR image (used to prove what we draw can be scanned).
    public static func decode(_ image: CGImage) -> [String] {
        guard let detector = CIDetector(ofType: CIDetectorTypeQRCode, context: nil, options: [CIDetectorAccuracy: CIDetectorAccuracyHigh]) else { return [] }
        return detector.features(in: CIImage(cgImage: image)).compactMap { ($0 as? CIQRCodeFeature)?.messageString }
    }
}
