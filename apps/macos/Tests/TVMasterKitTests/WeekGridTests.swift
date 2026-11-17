import XCTest
@testable import TVMasterKit

final class WeekGridTests: XCTestCase {
    private func slot(_ start: String, _ end: String, days: Set<Int> = [0]) -> TimeSlot {
        var s = TimeSlot(); s.start = start; s.end = end; s.days = days; return s
    }

    func testTimesParseAndFormat() {
        XCTAssertEqual(WeekGrid.toMin("20:30"), 1230)
        XCTAssertEqual(WeekGrid.toMin("24:00"), 1440)
        XCTAssertNil(WeekGrid.toMin("25:00"))
        XCTAssertNil(WeekGrid.toMin("nope"))
        XCTAssertEqual(WeekGrid.fmtMin(75), "01:15")
        XCTAssertEqual(WeekGrid.clockLabel(20 * 60), "8 PM")
        XCTAssertEqual(WeekGrid.clockLabel(0), "12 AM")
        XCTAssertEqual(WeekGrid.clockLabel(12 * 60 + 30), "12:30 PM")
    }

    func testAnOvernightSlotContinuesIntoTheNextDay() {
        let segs = WeekGrid.segments([slot("22:00", "02:00", days: [4])])
        XCTAssertEqual(segs.count, 2)
        XCTAssertEqual(segs[0], WeekGrid.Seg(slot: 0, day: 4, origin: 4, start: 1320, end: 1440, contFrom: false, contTo: true))
        XCTAssertEqual(segs[1], WeekGrid.Seg(slot: 0, day: 5, origin: 4, start: 0, end: 120, contFrom: true, contTo: false))
        XCTAssertEqual(WeekGrid.segments([slot("06:00", "07:00", days: [])]).count, 7, "no days means every day")
        XCTAssertTrue(WeekGrid.segments([slot("10:00", "10:00")]).isEmpty, "an empty slot has no rectangle")
    }

    func testANewSlotSnapsAndIsAtLeastOneStepLong() {
        let s = WeekGrid.newSlot(day: 2, from: 601, to: 612, step: 30)
        XCTAssertEqual([s.start, s.end], ["10:00", "10:30"])
        let late = WeekGrid.newSlot(day: 0, from: 1439, to: 1440, step: 30)
        XCTAssertEqual([late.start, late.end], ["23:30", "24:00"])
        XCTAssertEqual(s.days, [2])
    }

    func testShiftKeepsTheLengthAndStopsAtTheEdgesOrWrapsWhenOvernight() {
        let a = WeekGrid.shift(slot("20:00", "21:00"), by: 65, step: 30)
        XCTAssertEqual([a.start, a.end], ["21:00", "22:00"])
        let edge = WeekGrid.shift(slot("23:00", "24:00"), by: 300, step: 30)
        XCTAssertEqual([edge.start, edge.end], ["23:00", "24:00"])
        let wrap = WeekGrid.shift(slot("23:00", "01:00"), by: 60, step: 30)
        XCTAssertEqual([wrap.start, wrap.end], ["00:00", "02:00"])
    }

    func testMovingAnOccurrenceChangesOneDayOnly() {
        let s = slot("20:00", "21:00", days: [0, 1, 2])
        XCTAssertEqual(WeekGrid.moveOccurrence(s, from: 1, to: 4).days, [0, 2, 4])
        XCTAssertEqual(WeekGrid.moveOccurrence(s, from: 1, to: 2).days, [0, 1, 2], "onto a day it already has: unchanged")
    }

    func testResizingKeepsAtLeastOneStep() {
        let s = slot("20:00", "21:00")
        XCTAssertEqual(WeekGrid.resizeEnd(s, to: 20 * 60 + 5, step: 30).end, "20:30")
        XCTAssertEqual(WeekGrid.resizeEnd(s, to: 22 * 60, step: 30).end, "22:00")
        XCTAssertEqual(WeekGrid.resizeStart(s, to: 19 * 60 + 40, step: 30).start, "19:30")
        XCTAssertEqual(WeekGrid.resizeStart(s, to: 22 * 60, step: 30).start, "20:30")
    }

    func testOneDayCanBeSplitOffAMultiDaySlot() {
        let s = slot("20:00", "21:00", days: [0, 1, 2])
        let r = WeekGrid.detachDay([s], index: 0, day: 1)
        XCTAssertEqual(r.slots.count, 2)
        XCTAssertEqual(r.slots[0].days, [0, 2])
        XCTAssertEqual(r.slots[1].days, [1])
        XCTAssertEqual(r.index, 1)
        XCTAssertNotEqual(r.slots[0].id, r.slots[1].id)
        XCTAssertEqual(WeekGrid.detachDay([slot("20:00", "21:00")], index: 0, day: 0).slots.count, 1, "a single day stays as it is")
    }

    func testAColourIsStableForWhatAirs() {
        XCTAssertEqual(WeekGrid.hue(for: "Cheers"), WeekGrid.hue(for: "Cheers"))
        XCTAssertNotEqual(WeekGrid.hue(for: "Cheers"), WeekGrid.hue(for: "Taxi"))
        XCTAssertTrue((0..<360).contains(WeekGrid.hue(for: "x")))
    }
}
