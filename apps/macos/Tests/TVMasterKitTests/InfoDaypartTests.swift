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

final class ChannelGuideTests: XCTestCase {
    private func world() -> ChannelGuideAnswers.World {
        let classic = AdPreset(id: "classic", name: "Classic TV", about: "", commercials: true, podSeconds: 120, maxBreaks: 4, minutesPerHour: 8, midBreaks: true)
        let none = AdPreset(id: "none", name: "None", about: "", commercials: false, podSeconds: 60, maxBreaks: 0, minutesPerHour: 0, midBreaks: false)
        return .init(timezone: "America/Toronto", showLibraryIds: [1], commercialLibraryIds: [2], presets: [none, classic])
    }

    func testEverythingShuffledWithNoBreaksIsTheSimplestChannel() {
        var a = ChannelGuideAnswers(); a.name = "Cartoons"; a.id = "cartoons"; a.number = 5
        let b = a.body(world())
        XCTAssertEqual(b["strategy"]?.stringValue, "shuffle")
        XCTAssertEqual(b["kind"]?.stringValue, "programming")
        XCTAssertEqual(b["library_ids"]?.arrayValue?.compactMap(\.intValue), [1])
        XCTAssertEqual(b["lineup"]?.get(["timezone"])?.stringValue, "America/Toronto")
        XCTAssertEqual(b["lineup"]?.get(["fill"]), .null)
        XCTAssertEqual(b["lineup"]?.get(["ads"]), .null)
        XCTAssertNil(b["options"], "following the station writes no look")
    }

    func testPickedShowsBecomeAPoolInTheOrderAskedAndBreaksUseTheCommercialsFound() {
        var a = ChannelGuideAnswers(); a.name = "Kids"; a.id = "kids"; a.shows = ["Clifford", "Franklin"]; a.style = .inOrder; a.breaks = "classic"
        let b = a.body(world())
        XCTAssertEqual(b["strategy"]?.stringValue, "sequential")
        XCTAssertEqual(b["lineup"]?.get(["fill", "type"])?.stringValue, "pool")
        XCTAssertEqual(b["lineup"]?.get(["fill", "order"])?.stringValue, "sequential")
        XCTAssertEqual(b["lineup"]?.get(["fill", "shows"])?.stringArray, ["Clifford", "Franklin"])
        XCTAssertEqual(b["lineup"]?.get(["ads", "pod_seconds"])?.intValue, 120)
        XCTAssertEqual(b["lineup"]?.get(["ads", "library_ids"])?.arrayValue?.compactMap(\.intValue), [2])
    }

    func testAWeeklyScheduleStartsEmptyOnAHalfHourGrid() {
        var a = ChannelGuideAnswers(); a.name = "Week"; a.id = "week"; a.style = .weekly; a.shows = ["A"]; a.chaos = 40
        let b = a.body(world())
        XCTAssertEqual(b["strategy"]?.stringValue, "block_grid")
        XCTAssertEqual(b["chaos"]?.intValue, 0)
        XCTAssertEqual(b["lineup"]?.get(["grid_minutes"])?.intValue, 30)
        // time nobody scheduled shows the title card, not more programmes
        XCTAssertEqual(b["lineup"]?.get(["fill", "type"])?.stringValue, "off_air")
    }

    func testTheKidsLookIsWrittenOnlyWhenAsked() {
        var a = ChannelGuideAnswers(); a.name = "Kids"; a.id = "kids"; a.look = .kids
        let b = a.body(world())
        XCTAssertEqual(b["options"]?.get(["theme", "theme"])?.stringValue, "kids")
        XCTAssertEqual(b["options"]?.get(["theme", "gentle"])?.boolValue, true)
    }

    func testTheFirstStepSaysWhatIsWrongAndTheSummaryIsPlain() {
        var a = ChannelGuideAnswers(); a.number = 5
        XCTAssertTrue(a.problem(step: 0, takenNumbers: [])?.contains("name") == true)
        a.name = "X"; a.id = "x"
        XCTAssertTrue(a.problem(step: 0, takenNumbers: [5])?.contains("already used") == true)
        XCTAssertNil(a.problem(step: 0, takenNumbers: [1]))
        XCTAssertNil(a.problem(step: 3, takenNumbers: [5]))
        a.shows = ["A", "B", "C", "D", "E"]; a.breaks = "classic"; a.alwaysOn = true
        let text = a.summary(world(), totalShows: 19).joined(separator: " ")
        XCTAssertTrue(text.contains("Plays 5 shows: A, B, C, D and 1 more"))
        XCTAssertTrue(text.contains("Breaks: Classic TV"))
        XCTAssertTrue(text.contains("Always on"))
    }
}

final class ProgrammingWordsTests: XCTestCase {
    private func slot() -> TimeSlot { var t = TimeSlot(); t.source.kind = .show; t.source.show = "A"; return t }

    func testAnUnsetFillKeepsPlayingUnlessTheOldTestScreenPolicyApplies() {
        var ls = LineupSettings(); ls.slots = [slot()]
        XCTAssertEqual(ls.gapChoice(runOut: .nextInRotation), .keep)
        XCTAssertEqual(ls.gapChoice(runOut: .offAir), .screen)
        XCTAssertEqual(LineupSettings().gapChoice(runOut: .offAir), .keep)
    }

    func testEveryChoiceRoundTripsThroughTheFill() {
        for c in GapChoice.allCases {
            var ls = LineupSettings(); ls.slots = [slot()]
            ls.setGap(c, strategy: .blockGrid, runOut: .keepGoing)
            XCTAssertEqual(ls.gapChoice(runOut: .keepGoing), c)
            // and through the JSON the server stores
            let back = LineupSettings(lineup: ls.apply(to: nil))
            XCTAssertEqual(back.gapChoice(runOut: .keepGoing), c)
        }
    }

    func testTheOffAirScreenIsAFillTheServerUnderstands() {
        var ls = LineupSettings()
        ls.setGap(.screen, strategy: .blockGrid, runOut: .keepGoing)
        XCTAssertEqual(ls.apply(to: nil).get(["fill", "type"])?.stringValue, "off_air")
    }

    func testKeepPlayingIsSaidOutrightUnderTheOldTestScreenPolicy() {
        var ls = LineupSettings(); ls.slots = [slot()]
        ls.setGap(.keep, strategy: .shuffle, runOut: .offAir)
        XCTAssertEqual(ls.fill?.kind, .pool)
        XCTAssertEqual(ls.fill?.order, "shuffle")
        XCTAssertEqual(ls.gapChoice(runOut: .offAir), .these)
    }

    func testChoosingTheseShowsKeepsShowsAlreadyChosen() {
        var ls = LineupSettings()
        var f = SourceChoice(); f.kind = .show; f.show = "B"; ls.fill = f
        ls.setGap(.these, strategy: .blockGrid, runOut: .keepGoing)
        XCTAssertEqual(ls.fill?.show, "B")
    }

    func testHybridIsShownAsSomethingRandomAndSummariesAreOneLine() {
        XCTAssertEqual(RunOut.hybrid.choice, .chaosFill)
        var ls = LineupSettings(); ls.slots = [slot(), slot()]
        ls.setGap(.screen, strategy: .blockGrid, runOut: .nextInRotation)
        XCTAssertEqual(ls.summary(strategy: .blockGrid, runOut: .nextInRotation, breaks: "commercial breaks: short break between shows"),
                       "Weekly schedule · 2 slots · nothing scheduled: show the off-air screen · when a show runs out: move to the next show · commercial breaks: short break between shows")
    }

    func testBreaksAreNamedAfterThePresetInForce() {
        let short = try! JSONDecoder.snake.decode(AdPreset.self, from: Data(#"{"id":"short","name":"Short break between shows","about":"","commercials":true,"pod_seconds":60,"max_breaks":0,"minutes_per_hour":4,"mid_breaks":false}"#.utf8))
        var a = AdSettings(); a.apply(short)
        XCTAssertEqual(a.breaksLabel(in: [short]), "commercial breaks: short break between shows")
        a.podSeconds = 77
        XCTAssertEqual(a.breaksLabel(in: [short]), "commercial breaks: custom")
    }
}
