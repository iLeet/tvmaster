import XCTest
@testable import TVMasterKit

final class BackupTests: XCTestCase {
    func testTheServersBackupListDecodes() throws {
        let json = #"{"auto":true,"keep":7,"backups":[{"name":"tvmaster-20261116-224000.tar.gz","size":123456,"created_ms":1794890400000,"label":"before","auto":false,"version":"1.0.0","counts":{"channels":4,"libraries":1,"files":2255}}]}"#
        let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase
        let list = try d.decode(BackupList.self, from: Data(json.utf8))
        XCTAssertEqual(list.keep, 7)
        XCTAssertEqual(list.backups.first?.counts.files, 2255)
        XCTAssertEqual(list.backups.first?.label, "before")
        XCTAssertEqual(list.backups.first?.created.timeIntervalSince1970, 1794890400)
        let r = try d.decode(BackupRestored.self, from: Data(#"{"channels":1,"libraries":1,"files":1,"pictures":0,"missing_folders":["/x"],"safety_copy":"a.tar.gz"}"#.utf8))
        XCTAssertEqual(r.missingFolders, ["/x"])
        XCTAssertEqual(r.safetyCopy, "a.tar.gz")
    }
}
