import XCTest
@testable import TVMasterKit

final class InfoDaypartTests: XCTestCase {
    func testTheKidsLookIsBrightSmoothAndGentleAndSurvivesTheServersJSON() {
        let k = ThemeChoice.kids
        XCTAssertEqual(k.theme, "kids")
        XCTAssertEqual(k.style, "modern")
        XCTAssertTrue(k.gentle)
        XCTAssertEqual(ThemeChoice(json: k.json), k)
        XCTAssertEqual(k.json.get(["gentle"])?.boolValue, true)
        // A theme saved before gentle alerts existed is not gentle.
        let old = ThemeChoice(json: .object(["theme": .string("neon"), "style": .string("retro")]))
        XCTAssertEqual(old?.gentle, false)
    }

    func testAnInfoChannelsPagesRoundTripAndKeepWhatTheAppDoesNotKnow() throws {
        var i = InfoSettings()
        i.title = "Community"
        i.slides = [InfoSettings.newSlide("text"), InfoSettings.newSlide("headlines"), InfoSettings.newSlide("clock")]
        i.slides[0].title = "Garage sale"
        i.slides[0].lines = ["Saturday", "8 to 2"]
        i.slides[1].seconds = 20
        i.feeds = ["https://example.com/rss"]
        i.ticker = ["Welcome", "  "]
        i.musicLibraryIds = [3]
        let written = i.apply(to: .object(["future_option": .bool(true)]))
        XCTAssertEqual(written.get(["future_option"])?.boolValue, true, "options a newer server adds are kept")
        XCTAssertEqual(written.get(["ticker"])?.stringArray, ["Welcome"], "blank ticker lines are dropped")
        let back = InfoSettings(info: written)
        XCTAssertEqual(back.slides.count, 3)
        XCTAssertEqual(back.slides[0].lines, ["Saturday", "8 to 2"])
        XCTAssertEqual(back.slides[1].seconds, 20)
        XCTAssertNil(back.slides[0].seconds)
        XCTAssertEqual(back.feeds, ["https://example.com/rss"])
        XCTAssertEqual(back.musicLibraryIds, [3])
        XCTAssertEqual(back.title, "Community")
        XCTAssertEqual(InfoSettings(info: nil), InfoSettings())
    }

    func testTheEditorSaysWhatTheServerWouldRefuse() {
        var i = InfoSettings()
        XCTAssertNil(i.problem)
        i.slides = [InfoSettings.newSlide("headlines")]
        XCTAssertTrue(i.problem?.contains("feed") == true)
        i.feeds = ["not a url"]
        XCTAssertTrue(i.problem?.contains("web address") == true)
        i.feeds = ["https://x.org/rss"]
        XCTAssertNil(i.problem)
        i.slides = [InfoSettings.newSlide("photo")]
        XCTAssertTrue(i.problem?.contains("Page 1") == true)
        i.slides = [{ var s = InfoSettings.newSlide("text"); s.title = ""; return s }()]
        XCTAssertTrue(i.problem?.contains("nothing in it") == true)
    }

    func testPagesMoveWithoutChangingTheOriginal() {
        let l = ["a", "b", "c"]
        XCTAssertEqual(InfoSettings.moved(l, from: 0, to: 2), ["b", "c", "a"])
        XCTAssertEqual(InfoSettings.moved(l, from: 2, to: 0), ["c", "a", "b"])
        XCTAssertEqual(InfoSettings.moved(l, from: 1, to: 9), l)
    }

    func testADayPartReadsAndWritesTheServersJSONAndDescribesItself() {
        var p = DaypartSetting()
        p.name = "Kids' morning"; p.start = "06:00"; p.end = "09:30"; p.days = [4, 0, 1, 2, 3]
        p.maxRating = "kids"; p.breaksOff = true; p.look = .kids
        XCTAssertEqual(p.summary, "6:00 AM – 9:30 AM, weekdays")
        let j = p.json
        XCTAssertEqual(j.get(["breaks"])?.stringValue, "off")
        XCTAssertEqual(j.get(["max_rating"])?.stringValue, "kids")
        XCTAssertEqual(j.get(["days"])?.arrayValue?.compactMap(\.intValue), [0, 1, 2, 3, 4], "days are sorted")
        XCTAssertEqual(DaypartSetting(json: j), { var q = p; q.days = [0, 1, 2, 3, 4]; return q }())
        // No limit and no look are null, not empty strings.
        let plain = DaypartSetting()
        XCTAssertEqual(plain.json.get(["max_rating"]), .null)
        XCTAssertEqual(plain.json.get(["look"]), .null)
        XCTAssertNil(DaypartSetting(json: plain.json).look)
        XCTAssertEqual(DaypartSetting.twelve("00:05"), "12:05 AM")
        XCTAssertEqual(DaypartSetting.twelve("13:30"), "1:30 PM")
    }

    func testTheCreatorsAndTheDayPartListsAreReadFromTheServer() {
        let json: JSONValue = .object([
            "templates": .array([
                .object(["id": .string("tv"), "group": .string("TV channels"), "kind": .string("programming"), "name": .string("TV channel"), "about": .string("x"),
                         "needs": .array([.string("libraries")]), "suggested": .object(["id": .string("channel-1"), "number": .number(1), "name": .string("Channel 1")]), "body": .object([:])]),
                .object(["id": .string("info_news"), "group": .string("Info channels"), "kind": .string("info"), "name": .string("News"), "about": .string("y"),
                         "needs": .array([]), "suggested": .object(["id": .string("news"), "number": .number(2), "name": .string("News")]),
                         "body": .object(["info": .object(["about": .string("News and headlines")])])]),
            ]),
            "feeds": .array([.object(["name": .string("BBC"), "url": .string("https://feeds.bbci.co.uk/news/rss.xml")])]),
        ])
        let t = ChannelTemplates(json: json)
        XCTAssertEqual(t.groups, ["TV channels", "Info channels"])
        XCTAssertEqual(t.templates[1].suggestedNumber, 2)
        XCTAssertEqual(InfoSettings(info: t.templates[1].body.get(["info"])).about, "News and headlines")
        XCTAssertEqual(t.feeds.first?.name, "BBC")

        let state = DaypartsState(json: .object([
            "parts": .array([DaypartSetting().json]),
            "check": .array([.object(["shows": .number(10), "airing": .number(4), "held_back": .number(6), "unrated": .number(2)])]),
            "channels": .array([.object(["id": .string("a"), "number": .number(4), "name": .string("Retro")])]),
            "ratings": .array([.object(["id": .string("kids"), "name": .string("Kids 7+"), "about": .string("TV-Y7")])]),
            "presets": .array([.object(["about": .string("Morning"), "part": DaypartSetting().json])]),
            "timezone": .string("America/Toronto"),
        ]))
        XCTAssertEqual(state.parts.count, 1)
        XCTAssertEqual(state.check[0], DaypartsState.Check(shows: 10, airing: 4, heldBack: 6, unrated: 2))
        XCTAssertEqual(state.channels[0].name, "4 Retro")
        XCTAssertEqual(state.ratings[0].id, "kids")
        XCTAssertEqual(state.presets[0].about, "Morning")
        XCTAssertEqual(state.timezone, "America/Toronto")
    }

    func testAnInfoChannelInTheListSaysWhatItIs() throws {
        let data = #"{"id":"news","number":3,"name":"News","strategy":"sequential","run_out":"keep_going","autopilot":true,"chaos":0,"always_on":false,"library_ids":[],"lineup":{},"kind":"info","group":null,"guide":{"hours":3,"title":null,"ticker":[],"music_library_ids":[]},"info":{"about":"Local news","slides":[{"kind":"clock"},{"kind":"headlines"}]}}"#.data(using: .utf8)!
        let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase
        let c = try d.decode(Channel.self, from: data)
        XCTAssertTrue(c.isInfo)
        XCTAssertFalse(c.isScheduled)
        XCTAssertEqual(c.infoSummary, "Local news · 2 pages")
    }
}
