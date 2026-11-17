import Foundation

/// Finds and runs a local `tvmaster` server for the app.
public enum DaemonLocator {
    /// Where a `tvmaster` binary might be, best first: bundled in the app, an
    /// explicit override, a development build, then the usual install folders.
    public static func candidates(
        bundleResources: URL? = Bundle.main.resourceURL,
        environment: [String: String] = ProcessInfo.processInfo.environment,
        home: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> [URL] {
        var urls: [URL] = []
        if let r = bundleResources { urls.append(r.appendingPathComponent("tvmaster")) }
        if let p = environment["TVMASTER_BIN"], !p.isEmpty { urls.append(URL(fileURLWithPath: p)) }
        urls += [
            URL(fileURLWithPath: "/opt/homebrew/bin/tvmaster"),
            URL(fileURLWithPath: "/usr/local/bin/tvmaster"),
            home.appendingPathComponent(".cargo/bin/tvmaster"),
            home.appendingPathComponent(".local/bin/tvmaster"),
        ]
        for dir in (environment["PATH"] ?? "").split(separator: ":") where !dir.isEmpty {
            urls.append(URL(fileURLWithPath: String(dir)).appendingPathComponent("tvmaster"))
        }
        return urls
    }

    public static func find(
        candidates: [URL]? = nil,
        isExecutable: (String) -> Bool = { FileManager.default.isExecutableFile(atPath: $0) }
    ) -> URL? {
        // Never the app itself: run unbundled on a case-insensitive disk, `TVMaster` and `tvmaster` are one file.
        let me = Bundle.main.executablePath?.lowercased()
        return (candidates ?? Self.candidates()).first { isExecutable($0.path) && $0.path.lowercased() != me }
    }

    /// A `tvmaster` started from a GUI app gets a minimal PATH; FFmpeg usually
    /// lives in Homebrew's folder, so put it there.
    public static func environment(base: [String: String] = ProcessInfo.processInfo.environment) -> [String: String] {
        var env = base
        let extra = ["/opt/homebrew/bin", "/usr/local/bin"]
        let existing = (env["PATH"] ?? "/usr/bin:/bin").split(separator: ":").map(String.init)
        env["PATH"] = (extra + existing.filter { !extra.contains($0) }).joined(separator: ":")
        return env
    }

    /// Where the local server keeps its database and caches. `TVMASTER_DATA_DIR` overrides it.
    public static var defaultDataDirectory: URL {
        if let p = ProcessInfo.processInfo.environment["TVMASTER_DATA_DIR"], !p.isEmpty { return URL(fileURLWithPath: p) }
        return FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent("Library/Application Support/TVMaster")
    }
}

/// Runs `tvmaster serve` as a child process and restarts it if it dies.
public actor DaemonSupervisor {
    public enum State: Sendable, Equatable {
        case stopped
        case running(pid: Int32)
        case failed(String)
    }

    public private(set) var state: State = .stopped
    private var process: Process?
    private var wantRunning = false
    private var restarts = 0
    private let logURL: URL
    private var onChange: (@Sendable (State) -> Void)?

    public init(logURL: URL = DaemonLocator.defaultDataDirectory.appendingPathComponent("tvmaster-app.log")) {
        self.logURL = logURL
    }

    public func observe(_ handler: @escaping @Sendable (State) -> Void) { onChange = handler }

    private func set(_ s: State) {
        state = s
        onChange?(s)
    }

    /// Start the server on `bind` (for example `127.0.0.1:8787`). Does nothing if it is already running.
    public func start(binary: URL, bind: String, dataDirectory: URL, token: String?, extraArguments: [String] = []) {
        guard process == nil else { return }
        wantRunning = true
        self.extraArguments = extraArguments
        launch(binary: binary, bind: bind, dataDirectory: dataDirectory, token: token)
    }

    /// Flags beyond the basics (HTTPS), kept so a crash restart uses the same ones.
    private var extraArguments: [String] = []

    private func launch(binary: URL, bind: String, dataDirectory: URL, token: String?) {
        try? FileManager.default.createDirectory(at: dataDirectory, withIntermediateDirectories: true)
        try? FileManager.default.createDirectory(at: logURL.deletingLastPathComponent(), withIntermediateDirectories: true)
        if !FileManager.default.fileExists(atPath: logURL.path) { FileManager.default.createFile(atPath: logURL.path, contents: nil) }

        let p = Process()
        p.executableURL = binary
        var args = ["serve", "--bind", bind, "--data-dir", dataDirectory.path]
        if let token, !token.isEmpty { args += ["--token", token] }
        args += extraArguments
        p.arguments = args
        p.environment = DaemonLocator.environment()
        if let log = try? FileHandle(forWritingTo: logURL) {
            _ = try? log.seekToEnd()
            p.standardOutput = log
            p.standardError = log
        }
        p.terminationHandler = { [weak self] proc in
            Task { await self?.exited(code: proc.terminationStatus, binary: binary, bind: bind, dataDirectory: dataDirectory, token: token) }
        }
        do {
            try p.run()
            process = p
            set(.running(pid: p.processIdentifier))
        } catch {
            set(.failed(error.localizedDescription))
        }
    }

    private func exited(code: Int32, binary: URL, bind: String, dataDirectory: URL, token: String?) {
        process = nil
        guard wantRunning else { set(.stopped); return }
        restarts += 1
        if restarts > 5 {
            wantRunning = false
            set(.failed("The server keeps stopping (exit code \(code)). See the log."))
            return
        }
        let delay = UInt64(min(restarts, 5)) * 1_000_000_000
        Task { [weak self] in
            try? await Task.sleep(nanoseconds: delay)
            await self?.relaunch(binary: binary, bind: bind, dataDirectory: dataDirectory, token: token)
        }
    }

    private func relaunch(binary: URL, bind: String, dataDirectory: URL, token: String?) {
        guard wantRunning, process == nil else { return }
        launch(binary: binary, bind: bind, dataDirectory: dataDirectory, token: token)
    }

    /// Stop the server (it shuts its encoders down cleanly on SIGINT).
    public func stop() {
        wantRunning = false
        restarts = 0
        if let p = process, p.isRunning { p.interrupt() }
        if process == nil { set(.stopped) }
    }

    /// The last lines of the server's log.
    public func logTail(lines: Int = 80) -> String {
        guard let data = try? Data(contentsOf: logURL), let text = String(data: data.suffix(64 * 1024), encoding: .utf8) else { return "" }
        let all = text.split(separator: "\n", omittingEmptySubsequences: false)
        return all.suffix(lines).joined(separator: "\n")
    }
}
