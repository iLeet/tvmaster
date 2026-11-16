import Foundation

/// What the server says after receiving a file.
public struct UploadResult: Codable, Sendable, Hashable {
    public var path: String
    public var bytes: Int64
    /// The file was read and is on the schedule.
    public var indexed: Bool
}

/// Reports how much of an upload has been sent.
final class UploadProgress: NSObject, URLSessionTaskDelegate, @unchecked Sendable {
    private let report: @Sendable (Double) -> Void
    init(_ report: @escaping @Sendable (Double) -> Void) { self.report = report }

    func urlSession(_ session: URLSession, task: URLSessionTask, didSendBodyData bytesSent: Int64, totalBytesSent: Int64, totalBytesExpectedToSend: Int64) {
        guard totalBytesExpectedToSend > 0 else { return }
        report(Double(totalBytesSent) / Double(totalBytesExpectedToSend))
    }
}

/// Works out where each picked file goes inside a library.
public enum UploadPlan {
    /// `file` as a path relative to `base` (the folder the user picked, or its parent for a single file),
    /// with an optional sub-folder in front. Always forward slashes.
    public static func relativePath(file: URL, base: URL, subfolder: String) -> String {
        let f = file.standardizedFileURL.pathComponents
        let b = base.standardizedFileURL.pathComponents
        let rel = f.count > b.count && Array(f.prefix(b.count)) == b ? Array(f.dropFirst(b.count)) : [file.lastPathComponent]
        let sub = subfolder.split(whereSeparator: { $0 == "/" || $0 == "\\" }).map(String.init)
        return (sub + rel).joined(separator: "/")
    }

    /// `1.4 GB`, `820 MB`.
    public static func size(_ bytes: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }
}
