import AppKit
import Darwin
import Foundation

private enum Product {
    static let name = "CoPets"
    static let appName = "CoPets.app"
    static let bundleIdentifier = "dev.copets.sidecar"
    static let installerIdentifier = "dev.copets.installer"
    static let executableName = "copets"
    static let cleanupPrefix = "copets-installer-cleanup-"
    static let testModeVariable = "COPETS_INSTALLER_TEST_MODE"
}

private struct InstallerFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

private struct CommandResult {
    let status: Int32
    let output: String
    let error: String
}

@discardableResult
private func runCommand(_ executable: String, _ arguments: [String]) throws -> CommandResult {
    let process = Process()
    let stdout = Pipe()
    let stderr = Pipe()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    process.standardOutput = stdout
    process.standardError = stderr
    try process.run()
    process.waitUntilExit()
    return CommandResult(
        status: process.terminationStatus,
        output: String(decoding: stdout.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self),
        error: String(decoding: stderr.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
    )
}

private func itemKind(at url: URL) throws -> mode_t? {
    var info = stat()
    if lstat(url.path, &info) == 0 {
        return info.st_mode
    }
    if errno == ENOENT {
        return nil
    }
    throw InstallerFailure(message: "Could not inspect \(url.path): \(String(cString: strerror(errno)))")
}

private func requireRealDirectory(_ url: URL, label: String) throws {
    guard let mode = try itemKind(at: url) else {
        throw InstallerFailure(message: "\(label) does not exist: \(url.path)")
    }
    guard (mode & S_IFMT) == S_IFDIR else {
        let kind = (mode & S_IFMT) == S_IFLNK ? "a symbolic link" : "not a directory"
        throw InstallerFailure(message: "\(label) is \(kind); refusing to continue: \(url.path)")
    }
}

private func requireRegularFile(_ url: URL, label: String) throws {
    guard let mode = try itemKind(at: url) else {
        throw InstallerFailure(message: "\(label) does not exist: \(url.path)")
    }
    guard (mode & S_IFMT) == S_IFREG else {
        let kind = (mode & S_IFMT) == S_IFLNK ? "a symbolic link" : "not a regular file"
        throw InstallerFailure(message: "\(label) is \(kind); refusing to continue: \(url.path)")
    }
}

private func rejectBundleSymlinks(_ appURL: URL) throws {
    let keys: Set<URLResourceKey> = [.isSymbolicLinkKey]
    guard let enumerator = FileManager.default.enumerator(
        at: appURL,
        includingPropertiesForKeys: Array(keys)
    ) else {
        throw InstallerFailure(message: "Could not inspect application contents: \(appURL.path)")
    }
    for case let itemURL as URL in enumerator {
        if try itemURL.resourceValues(forKeys: keys).isSymbolicLink == true {
            throw InstallerFailure(
                message: "Application bundle contains a symbolic link and was refused: \(itemURL.path)"
            )
        }
    }
}

private func bundleMetadata(at appURL: URL) throws -> (identifier: String, executable: String) {
    try requireRealDirectory(appURL, label: "Application bundle")
    try rejectBundleSymlinks(appURL)
    try requireRealDirectory(
        appURL.appendingPathComponent("Contents"),
        label: "Application Contents directory"
    )
    let infoURL = appURL.appendingPathComponent("Contents/Info.plist")
    try requireRegularFile(infoURL, label: "Application Info.plist")
    guard let data = try? Data(contentsOf: infoURL),
          let plist = try? PropertyListSerialization.propertyList(from: data, format: nil),
          let dictionary = plist as? [String: Any],
          let identifier = dictionary["CFBundleIdentifier"] as? String,
          let executable = dictionary["CFBundleExecutable"] as? String
    else {
        throw InstallerFailure(message: "Application metadata is missing or invalid: \(infoURL.path)")
    }
    return (identifier, executable)
}

private func validateBundle(at appURL: URL) throws {
    let metadata = try bundleMetadata(at: appURL)
    guard metadata.identifier == Product.bundleIdentifier else {
        throw InstallerFailure(
            message: "Expected bundle identifier \(Product.bundleIdentifier), found \(metadata.identifier)."
        )
    }
    guard metadata.executable == Product.executableName else {
        throw InstallerFailure(
            message: "Expected executable \(Product.executableName), found \(metadata.executable)."
        )
    }
    let executableURL = appURL
        .appendingPathComponent("Contents/MacOS")
        .appendingPathComponent(metadata.executable)
    try requireRealDirectory(
        appURL.appendingPathComponent("Contents/MacOS"),
        label: "Application executable directory"
    )
    guard let executableMode = try itemKind(at: executableURL),
          (executableMode & S_IFMT) == S_IFREG,
          access(executableURL.path, X_OK) == 0
    else {
        throw InstallerFailure(message: "Application executable is missing, unsafe, or not executable.")
    }
    let result = try runCommand(
        "/usr/bin/codesign",
        ["--verify", "--deep", "--strict", "--verbose=2", appURL.path]
    )
    guard result.status == 0 else {
        let detail = result.error.trimmingCharacters(in: .whitespacesAndNewlines)
        throw InstallerFailure(message: "Application signature validation failed. \(detail)")
    }
}

private func stopRunningApplication() throws {
    let applications = NSRunningApplication.runningApplications(
        withBundleIdentifier: Product.bundleIdentifier
    )
    guard !applications.isEmpty else { return }
    applications.forEach { _ = $0.terminate() }
    let deadline = Date().addingTimeInterval(10)
    while Date() < deadline {
        if applications.allSatisfy({ $0.isTerminated }) {
            return
        }
        RunLoop.current.run(until: Date().addingTimeInterval(0.1))
    }
    throw InstallerFailure(
        message: "CoPets is still running. Quit it from the menu bar, then try again."
    )
}

private enum InstallLocation {
    static func productionRoot() throws -> URL {
        let fileManager = FileManager.default
        let systemRoot = URL(fileURLWithPath: "/Applications", isDirectory: true)
        let userRoot = fileManager.homeDirectoryForCurrentUser
            .appendingPathComponent("Applications", isDirectory: true)
        let systemTarget = systemRoot.appendingPathComponent(Product.appName)
        let userTarget = userRoot.appendingPathComponent(Product.appName)

        if try itemKind(at: systemTarget) != nil {
            return systemRoot
        }
        if try itemKind(at: userTarget) != nil {
            return userRoot
        }
        if fileManager.isWritableFile(atPath: systemRoot.path) {
            return systemRoot
        }
        if try itemKind(at: userRoot) == nil {
            try fileManager.createDirectory(at: userRoot, withIntermediateDirectories: false)
        }
        return userRoot
    }

    static func existingProductionTargets() throws -> [URL] {
        let roots = [
            URL(fileURLWithPath: "/Applications", isDirectory: true),
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Applications", isDirectory: true),
        ]
        return try roots.compactMap { root in
            let target = root.appendingPathComponent(Product.appName)
            return try itemKind(at: target) == nil ? nil : target
        }
    }
}

private enum Lifecycle {
    static func install(payload: URL, applicationsRoot: URL) throws -> URL {
        let fileManager = FileManager.default
        try validateBundle(at: payload)
        try requireRealDirectory(applicationsRoot, label: "Applications destination")

        let target = applicationsRoot.appendingPathComponent(Product.appName, isDirectory: true)
        let token = UUID().uuidString
        let staging = applicationsRoot
            .appendingPathComponent(".CoPets.installing-\(token).app", isDirectory: true)
        let backup = applicationsRoot
            .appendingPathComponent(".CoPets.backup-\(token).app", isDirectory: true)

        let existingMode = try itemKind(at: target)
        if let mode = existingMode {
            guard (mode & S_IFMT) == S_IFDIR else {
                let kind = (mode & S_IFMT) == S_IFLNK ? "a symbolic link" : "not an app directory"
                throw InstallerFailure(
                    message: "\(target.path) is \(kind). It was preserved and installation was stopped."
                )
            }
            try validateBundle(at: target)
        }

        do {
            try fileManager.copyItem(at: payload, to: staging)
            try validateBundle(at: staging)
        } catch {
            if try itemKind(at: staging) != nil {
                try? fileManager.removeItem(at: staging)
            }
            throw error
        }

        if existingMode == nil {
            do {
                try fileManager.moveItem(at: staging, to: target)
            } catch {
                if try itemKind(at: staging) != nil {
                    try? fileManager.removeItem(at: staging)
                }
                throw InstallerFailure(message: "Installation failed before CoPets was placed: \(error)")
            }
            return target
        }

        try fileManager.moveItem(at: target, to: backup)
        do {
            try fileManager.moveItem(at: staging, to: target)
        } catch {
            var restorationDetail = ""
            do {
                try fileManager.moveItem(at: backup, to: target)
            } catch {
                restorationDetail = " The previous version remains recoverable at \(backup.path): \(error)"
            }
            if try itemKind(at: staging) != nil {
                try? fileManager.removeItem(at: staging)
            }
            throw InstallerFailure(
                message: "Upgrade failed and the previous version was restored.\(restorationDetail)"
            )
        }
        do {
            try fileManager.removeItem(at: backup)
        } catch {
            throw InstallerFailure(
                message: "CoPets was upgraded, but the verified backup could not be removed: \(backup.path)"
            )
        }
        return target
    }

    static func uninstall(target: URL, testTrashRoot: URL? = nil) throws -> URL? {
        guard try itemKind(at: target) != nil else {
            return nil
        }
        try validateBundle(at: target)

        if let testTrashRoot {
            try requireRealDirectory(testTrashRoot, label: "Test Trash destination")
            let destination = testTrashRoot.appendingPathComponent(Product.appName)
            guard try itemKind(at: destination) == nil else {
                throw InstallerFailure(message: "Test Trash destination already exists: \(destination.path)")
            }
            try FileManager.default.moveItem(at: target, to: destination)
            return destination
        }

        var resultingURL: NSURL?
        try FileManager.default.trashItem(at: target, resultingItemURL: &resultingURL)
        return resultingURL as URL?
    }
}

private struct MountedImage {
    let mountURL: URL
    let imageURL: URL
    let deviceEntry: String
    let imageDevice: UInt64
    let imageInode: UInt64
}

private func fileIdentity(at url: URL) throws -> (device: UInt64, inode: UInt64) {
    var info = stat()
    guard lstat(url.path, &info) == 0, (info.st_mode & S_IFMT) == S_IFREG else {
        throw InstallerFailure(message: "Expected a regular file: \(url.path)")
    }
    return (UInt64(info.st_dev), UInt64(info.st_ino))
}

private func mountedImage(containing bundleURL: URL) throws -> MountedImage {
    let result = try runCommand("/usr/bin/hdiutil", ["info", "-plist"])
    guard result.status == 0,
          let data = result.output.data(using: .utf8),
          let plist = try? PropertyListSerialization.propertyList(from: data, format: nil),
          let root = plist as? [String: Any],
          let images = root["images"] as? [[String: Any]]
    else {
        throw InstallerFailure(message: "Could not inspect mounted disk images.")
    }

    let bundlePath = bundleURL.resolvingSymlinksInPath().path
    var matches: [MountedImage] = []
    for image in images {
        guard let imagePath = image["image-path"] as? String,
              URL(fileURLWithPath: imagePath).pathExtension.lowercased() == "dmg",
              let entities = image["system-entities"] as? [[String: Any]]
        else { continue }
        for entity in entities {
            guard let mountPath = entity["mount-point"] as? String,
                  let deviceEntry = entity["dev-entry"] as? String
            else { continue }
            let normalizedMount = URL(fileURLWithPath: mountPath).resolvingSymlinksInPath()
            if bundlePath == normalizedMount.path
                || bundlePath.hasPrefix(normalizedMount.path + "/")
            {
                let imageURL = URL(fileURLWithPath: imagePath).standardizedFileURL
                let identity = try fileIdentity(at: imageURL)
                matches.append(
                    MountedImage(
                        mountURL: normalizedMount,
                        imageURL: imageURL,
                        deviceEntry: deviceEntry,
                        imageDevice: identity.device,
                        imageInode: identity.inode
                    )
                )
            }
        }
    }
    guard let match = matches.max(by: { $0.mountURL.path.count < $1.mountURL.path.count }) else {
        throw InstallerFailure(message: "This installer is not running from a mounted DMG.")
    }
    guard let imageMode = try itemKind(at: match.imageURL),
          (imageMode & S_IFMT) == S_IFREG
    else {
        throw InstallerFailure(message: "The backing DMG is missing or unsafe: \(match.imageURL.path)")
    }
    return match
}

private func revalidateMountedImage(
    mountURL: URL,
    imageURL: URL,
    deviceEntry: String,
    imageDevice: UInt64,
    imageInode: UInt64
) throws {
    let result = try runCommand("/usr/bin/hdiutil", ["info", "-plist"])
    guard result.status == 0,
          let data = result.output.data(using: .utf8),
          let plist = try? PropertyListSerialization.propertyList(from: data, format: nil),
          let root = plist as? [String: Any],
          let images = root["images"] as? [[String: Any]]
    else {
        throw InstallerFailure(message: "Could not revalidate the mounted installer image.")
    }
    let normalizedMount = mountURL.resolvingSymlinksInPath().path
    let normalizedImage = imageURL.standardizedFileURL.path
    let matched = images.contains { image in
        guard let currentImagePath = image["image-path"] as? String,
              URL(fileURLWithPath: currentImagePath).standardizedFileURL.path == normalizedImage,
              let entities = image["system-entities"] as? [[String: Any]]
        else { return false }
        return entities.contains { entity in
            guard let currentMount = entity["mount-point"] as? String,
                  let currentDevice = entity["dev-entry"] as? String
            else { return false }
            return URL(fileURLWithPath: currentMount).resolvingSymlinksInPath().path
                == normalizedMount
                && currentDevice == deviceEntry
        }
    }
    guard matched else {
        throw InstallerFailure(message: "The mounted installer identity changed; cleanup was stopped.")
    }
    let currentIdentity = try fileIdentity(at: imageURL)
    guard currentIdentity.device == imageDevice, currentIdentity.inode == imageInode else {
        throw InstallerFailure(message: "The backing DMG changed; cleanup was stopped.")
    }
}

private func showWarning(title: String, message: String) {
    let application = NSApplication.shared
    application.setActivationPolicy(.regular)
    application.activate(ignoringOtherApps: true)
    let alert = NSAlert()
    alert.alertStyle = .warning
    alert.messageText = title
    alert.informativeText = message
    alert.addButton(withTitle: "OK")
    alert.runModal()
}

private func waitForParentToExit(_ parentPID: pid_t) -> Bool {
    let deadline = Date().addingTimeInterval(20)
    while Date() < deadline {
        if kill(parentPID, 0) != 0 && errno == ESRCH {
            return true
        }
        Thread.sleep(forTimeInterval: 0.2)
    }
    return false
}

private func cleanupHelper(arguments: [String]) -> Never {
    guard arguments.count == 7,
          let parentPID = pid_t(arguments[0]),
          let imageDevice = UInt64(arguments[4]),
          let imageInode = UInt64(arguments[5]),
          arguments[6] == "trash" || arguments[6] == "keep"
    else {
        fputs("Invalid cleanup-helper arguments.\n", stderr)
        exit(2)
    }
    let mountURL = URL(fileURLWithPath: arguments[1]).standardizedFileURL
    let imageURL = URL(fileURLWithPath: arguments[2]).standardizedFileURL
    let deviceEntry = arguments[3]
    let helperURL = URL(fileURLWithPath: CommandLine.arguments[0]).standardizedFileURL
    let temporaryRoot = URL(fileURLWithPath: NSTemporaryDirectory()).resolvingSymlinksInPath()
    let helperParent = helperURL.deletingLastPathComponent().resolvingSymlinksInPath()

    guard helperParent == temporaryRoot,
          helperURL.lastPathComponent.hasPrefix(Product.cleanupPrefix),
          imageURL.pathExtension.lowercased() == "dmg"
    else {
        fputs("Cleanup helper refused an unsafe path.\n", stderr)
        exit(2)
    }
    guard waitForParentToExit(parentPID) else {
        showWarning(
            title: "CoPets installer cleanup paused",
            message: "The installer did not exit in time. The DMG remains mounted at \(mountURL.path)."
        )
        try? FileManager.default.removeItem(at: helperURL)
        exit(1)
    }

    do {
        try revalidateMountedImage(
            mountURL: mountURL,
            imageURL: imageURL,
            deviceEntry: deviceEntry,
            imageDevice: imageDevice,
            imageInode: imageInode
        )
        let detach = try runCommand("/usr/bin/hdiutil", ["detach", mountURL.path])
        guard detach.status == 0 else {
            throw InstallerFailure(
                message: detach.error.trimmingCharacters(in: .whitespacesAndNewlines)
            )
        }
    } catch {
        showWarning(
            title: "Could not eject the CoPets installer",
            message: "The DMG remains mounted at \(mountURL.path). \(error.localizedDescription)"
        )
        try? FileManager.default.removeItem(at: helperURL)
        exit(1)
    }

    if arguments[6] == "trash" {
        do {
            let currentIdentity = try fileIdentity(at: imageURL)
            guard currentIdentity.device == imageDevice, currentIdentity.inode == imageInode else {
                throw InstallerFailure(message: "The verified DMG changed after eject.")
            }
            var resultingURL: NSURL?
            try FileManager.default.trashItem(at: imageURL, resultingItemURL: &resultingURL)
        } catch {
            showWarning(
                title: "DMG kept",
                message: "The installer was ejected, but the DMG could not be moved to Trash: \(error.localizedDescription)"
            )
        }
    }
    try? FileManager.default.removeItem(at: helperURL)
    exit(0)
}

private func spawnCleanupHelper(moveImageToTrash: Bool) throws {
    let mounted = try mountedImage(containing: Bundle.main.bundleURL)
    guard let executableURL = Bundle.main.executableURL else {
        throw InstallerFailure(message: "Installer executable could not be located.")
    }
    let helperURL = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
        .appendingPathComponent(Product.cleanupPrefix + UUID().uuidString)
    try FileManager.default.copyItem(at: executableURL, to: helperURL)
    guard chmod(helperURL.path, S_IRUSR | S_IWUSR | S_IXUSR) == 0 else {
        try? FileManager.default.removeItem(at: helperURL)
        throw InstallerFailure(message: "Could not secure the temporary cleanup helper.")
    }
    let process = Process()
    process.executableURL = helperURL
    process.arguments = [
        "--cleanup-helper",
        String(ProcessInfo.processInfo.processIdentifier),
        mounted.mountURL.path,
        mounted.imageURL.path,
        mounted.deviceEntry,
        String(mounted.imageDevice),
        String(mounted.imageInode),
        moveImageToTrash ? "trash" : "keep",
    ]
    do {
        try process.run()
    } catch {
        try? FileManager.default.removeItem(at: helperURL)
        throw error
    }
}

private func embeddedPayloadURL() throws -> URL {
    let payload = Bundle.main.bundleURL
        .appendingPathComponent("Contents/Helpers")
        .appendingPathComponent(Product.appName)
    try validateBundle(at: payload)
    return payload
}

private final class InstallerDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        do {
            let payload = try embeddedPayloadURL()
            let targets = try InstallLocation.existingProductionTargets()
            let alert = NSAlert()
            alert.alertStyle = .informational
            alert.messageText = "Install CoPets"
            alert.informativeText = targets.isEmpty
                ? "CoPets will be installed for this Mac. Pet packages in ~/.codex/pets are never removed."
                : "A verified CoPets installation was found. Installing will quit it before a transactional upgrade. Pet packages are preserved."
            alert.addButton(withTitle: targets.isEmpty ? "Install" : "Upgrade")
            alert.addButton(withTitle: "Uninstall Existing…")
            alert.addButton(withTitle: "Cancel")
            let response = alert.runModal()

            if response == .alertFirstButtonReturn {
                try stopRunningApplication()
                let root = try InstallLocation.productionRoot()
                let installed = try Lifecycle.install(payload: payload, applicationsRoot: root)
                try finish(
                    title: "CoPets installed",
                    message: "Installed safely at \(installed.path)."
                )
                return
            }
            if response == .alertSecondButtonReturn {
                guard !targets.isEmpty else {
                    throw InstallerFailure(message: "No CoPets installation was found.")
                }
                guard targets.count == 1, let target = targets.first else {
                    throw InstallerFailure(
                        message: "Multiple CoPets installations were found. Keep only the copy you want to remove, then retry."
                    )
                }
                let confirm = NSAlert()
                confirm.alertStyle = .warning
                confirm.messageText = "Uninstall CoPets?"
                confirm.informativeText =
                    "The app will quit and move to Trash. Pet packages in ~/.codex/pets and Codex data will be preserved."
                confirm.addButton(withTitle: "Move CoPets to Trash")
                confirm.addButton(withTitle: "Cancel")
                guard confirm.runModal() == .alertFirstButtonReturn else {
                    NSApp.terminate(nil)
                    return
                }
                try stopRunningApplication()
                _ = try Lifecycle.uninstall(target: target)
                try finish(
                    title: "CoPets uninstalled",
                    message: "The application was moved to Trash. Your pet packages were preserved."
                )
                return
            }
            NSApp.terminate(nil)
        } catch {
            let alert = NSAlert(error: error)
            alert.alertStyle = .critical
            alert.messageText = "CoPets installer stopped safely"
            alert.runModal()
            NSApp.terminate(nil)
        }
    }

    private func finish(title: String, message: String) throws {
        let alert = NSAlert()
        alert.alertStyle = .informational
        alert.messageText = title
        alert.informativeText = message + "\n\nThe mounted installer can now eject itself."
        alert.addButton(withTitle: "Eject and Move DMG to Trash")
        alert.addButton(withTitle: "Eject and Keep DMG")
        let moveToTrash = alert.runModal() == .alertFirstButtonReturn
        try spawnCleanupHelper(moveImageToTrash: moveToTrash)
        NSApp.terminate(nil)
    }
}

private func requireTestMode() throws {
    guard ProcessInfo.processInfo.environment[Product.testModeVariable] == "1" else {
        throw InstallerFailure(message: "Installer test commands are disabled.")
    }
}

private func testCommand(arguments: [String]) throws -> Int32 {
    try requireTestMode()
    guard let command = arguments.first else {
        throw InstallerFailure(message: "Missing installer test command.")
    }
    switch command {
    case "--test-install":
        guard arguments.count == 3 else {
            throw InstallerFailure(message: "usage: --test-install <payload.app> <Applications>")
        }
        let installed = try Lifecycle.install(
            payload: URL(fileURLWithPath: arguments[1]),
            applicationsRoot: URL(fileURLWithPath: arguments[2])
        )
        print(installed.path)
        return 0
    case "--test-uninstall":
        guard arguments.count == 3,
              let trashPath = ProcessInfo.processInfo.environment["COPETS_INSTALLER_TEST_TRASH"]
        else {
            throw InstallerFailure(
                message: "usage: COPETS_INSTALLER_TEST_TRASH=... --test-uninstall <target.app> <ignored>"
            )
        }
        let removed = try Lifecycle.uninstall(
            target: URL(fileURLWithPath: arguments[1]),
            testTrashRoot: URL(fileURLWithPath: trashPath)
        )
        print(removed?.path ?? "already absent")
        return 0
    case "--test-resolve-image":
        guard arguments.count == 2 else {
            throw InstallerFailure(message: "usage: --test-resolve-image <bundle-path>")
        }
        let image = try mountedImage(containing: URL(fileURLWithPath: arguments[1]))
        print(
            "\(image.mountURL.path)\n\(image.imageURL.path)\n\(image.deviceEntry)\n"
                + "\(image.imageDevice)\n\(image.imageInode)"
        )
        return 0
    default:
        throw InstallerFailure(message: "Unknown installer test command: \(command)")
    }
}

private let arguments = Array(CommandLine.arguments.dropFirst())
if arguments.first == "--cleanup-helper" {
    cleanupHelper(arguments: Array(arguments.dropFirst()))
}
if arguments.first?.hasPrefix("--test-") == true {
    do {
        exit(try testCommand(arguments: arguments))
    } catch {
        fputs("\(error.localizedDescription)\n", stderr)
        exit(1)
    }
}

private let application = NSApplication.shared
private let delegate = InstallerDelegate()
application.delegate = delegate
application.run()
