import XCTest
@testable import TVMasterKit

final class PromoTests: XCTestCase {
    func testADraftIsReadyWhenItHasWhatItNeeds() {
        var d = PromoDraft()
        XCTAssertFalse(d.ready)
        d.title = "Mel's Diner"
        XCTAssertTrue(d.ready)
        d.kind = "slides"; d.title = ""
        XCTAssertFalse(d.ready, "slides need a slide")
        d.slides = ["", "Pie"]
        XCTAssertTrue(d.ready)
    }

    func testTheDraftIsSentTheWayTheServerReadsIt() {
        var d = PromoDraft()
        d.kind = "slides"; d.slides = ["Pie", "  ", "Coffee"]; d.look = "vhs"; d.year = 1987; d.seconds = 30
        let j = d.json
        XCTAssertEqual(j.get(["slides"])?.stringArray, ["Pie", "Coffee"], "blank slides are dropped")
        XCTAssertEqual(j.get(["look"])?.stringValue, "vhs")
        XCTAssertEqual(j.get(["year"])?.intValue, 1987)
        XCTAssertEqual(j.get(["seconds"])?.intValue, 30)
        XCTAssertEqual(j.get(["picture"]), .null)
        XCTAssertNil(j.get(["theme"]), "no theme of its own means the station's")
        d.look = "none"; d.theme = ThemeChoice()
        XCTAssertEqual(d.json.get(["look"]), .null)
        XCTAssertEqual(d.json.get(["theme", "style"])?.stringValue, "retro")
    }
}
