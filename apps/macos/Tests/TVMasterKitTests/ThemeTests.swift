import XCTest
@testable import TVMasterKit

final class ThemeTests: XCTestCase {
    func testAThemeRoundTripsThroughTheServersJSON() {
        var t = ThemeChoice()
        t.theme = "custom"; t.primary = "#112233"; t.accent = "#445566"; t.style = "modern"
        let back = ThemeChoice(json: t.json)
        XCTAssertEqual(back, t)
        XCTAssertNil(ThemeChoice(json: .null), "no theme of its own means following the station")
        XCTAssertNil(ThemeChoice(json: nil))
    }

    func testAChannelsOwnThemeIsWrittenAndRemoved() {
        var ps = PictureSoundSettings(options: .object([:]), lineup: nil)
        XCTAssertNil(ps.theme)
        ps.theme = ThemeChoice()
        ps.theme?.theme = "midnight"
        let written = ps.apply(to: .object([:]))
        XCTAssertEqual(written.get(["theme", "theme"])?.stringValue, "midnight")
        XCTAssertEqual(PictureSoundSettings(options: written, lineup: nil).theme?.theme, "midnight")
        ps.theme = nil
        XCTAssertEqual(ps.apply(to: written).get(["theme"]), .null, "following the station again clears it")
    }

    func testBreakSettingsKeepTheirYearsAndRecogniseAPreset() throws {
        var ls = LineupSettings()
        ls.ads = AdSettings()
        ls.ads?.years = 1980...1989
        let back = LineupSettings(lineup: ls.apply(to: .object([:])))
        XCTAssertEqual(back.ads?.years, 1980...1989)
        ls.ads?.years = nil
        XCTAssertEqual(LineupSettings(lineup: ls.apply(to: .object([:]))).ads?.years, nil)
        let classic = AdPreset(id: "classic", name: "Classic", about: "", commercials: true, podSeconds: 120, maxBreaks: 4, minutesPerHour: 8, midBreaks: true)
        XCTAssertEqual(AdSettings().presetID(in: [classic]), "classic")
        var custom = AdSettings(); custom.podSeconds = 77
        XCTAssertEqual(custom.presetID(in: [classic]), "custom")
        custom.apply(classic)
        XCTAssertEqual(custom.presetID(in: [classic]), "classic")
    }
}
