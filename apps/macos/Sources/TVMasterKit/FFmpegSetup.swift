import Foundation

/// TVMaster streams with FFmpeg. A downloaded app cannot carry it (licensing,
/// and architecture), so the app checks for it and can install it with Homebrew.
public enum FFmpegSetup {
    public enum Failure: Error, LocalizedError {
        case noHomebrew
        case failed(String)
        public var errorDescription: String? {
            switch self {
            case .noHomebrew: "Homebrew isn't installed."
            case .failed(let tail): "Installing FFmpeg failed: \(tail)"
            }
        }
    }

    public static let downloadPage = URL(string: "https://ffmpeg.org/download.html")!

    public static func brew(candidates: [String] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]) -> URL? {
        candidates.first { FileManager.default.isExecutableFile(atPath: $0) }.map { URL(fileURLWithPath: $0) }
    }

    /// Are ffmpeg and ffprobe on the PATH the server will get?
    public static func isInstalled(environment: [String: String] = DaemonLocator.environment()) -> Bool {
        ["ffmpeg", "ffprobe"].allSatisfy { name in
            (environment["PATH"] ?? "").split(separator: ":").contains {
                FileManager.default.isExecutableFile(atPath: "\($0)/\(name)")
            }
        }
    }

    /// `brew install ffmpeg`. Takes a minute or two; throws with the last lines of output on failure.
    public static func install(brew: URL) async throws {
        try await withCheckedThrowingContinuation { (cont: CheckedContinuation<Void, Error>) in
            let p = Process()
            p.executableURL = brew
            p.arguments = ["install", "ffmpeg"]
            var env = DaemonLocator.environment()
            env["HOMEBREW_NO_AUTO_UPDATE"] = "1"
            env["HOMEBREW_NO_ENV_HINTS"] = "1"
            p.environment = env
            let pipe = Pipe()
            p.standardOutput = pipe
            p.standardError = pipe
            p.terminationHandler = { proc in
                let out = String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
                if proc.terminationStatus == 0 {
                    cont.resume()
                } else {
                    let tail = out.split(separator: "\n").suffix(3).joined(separator: " ")
                    cont.resume(throwing: Failure.failed(tail.isEmpty ? "exit code \(proc.terminationStatus)" : tail))
                }
            }
            do { try p.run() } catch { cont.resume(throwing: error) }
        }
    }
}
