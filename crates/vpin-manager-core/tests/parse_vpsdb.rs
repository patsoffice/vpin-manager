use vpin_manager_core::vpsdb::models::Game;

/// Fast unit test with inline JSON covering all resource types and edge cases.
#[test]
fn parse_sample_game() {
    let json = r#"[
        {
            "id": "test123",
            "name": "Test Game",
            "manufacturer": "Williams",
            "year": 1992,
            "type": "SS",
            "players": 4,
            "theme": ["Sci-Fi"],
            "designers": ["Steve Ritchie"],
            "features": ["VPX"],
            "ipdbUrl": "http://www.ipdb.org/machine.cgi?id=1234",
            "MPU": "WPC",
            "imgUrl": "https://example.com/img.webp",
            "broken": false,
            "updatedAt": 1745823539995,
            "lastCreatedAt": 1744675200000,
            "tableFiles": [{
                "id": "tf1",
                "tableFormat": "VPX",
                "version": "1.0",
                "authors": ["Author1"],
                "features": ["MOD", "SSF"],
                "comment": "A mod",
                "edition": "Premium",
                "theme": ["Sci-Fi"],
                "gameFileName": "test_game.vpx",
                "parentId": "tf0",
                "urls": [{"url": "https://example.com/dl", "broken": false}],
                "imgUrl": "https://example.com/table.webp",
                "createdAt": 1744675200000,
                "updatedAt": 1745823539995,
                "game": {"id": "test123", "name": "Test Game"}
            }],
            "b2sFiles": [{
                "id": "b2s1",
                "version": "2.0",
                "authors": ["Author2"],
                "features": ["FullDMD", "2Screens"],
                "urls": [{"url": "https://example.com/b2s"}],
                "imgUrl": "https://example.com/b2s.webp",
                "createdAt": 1681257600000,
                "updatedAt": 1710274420247,
                "game": {"id": "test123", "name": "Test Game"}
            }],
            "romFiles": [{
                "id": "rom1",
                "version": "hook_501",
                "name": "hook_501",
                "authors": ["RomAuthor"],
                "urls": [{"url": "https://example.com/rom"}],
                "createdAt": 1521849600000,
                "updatedAt": 1688945018550,
                "game": {"id": "test123", "name": "Test Game"}
            }],
            "altColorFiles": [{
                "id": "ac1",
                "version": "1.0",
                "authors": ["ColorAuthor"],
                "type": "Pin2DMD",
                "fileName": "test.pal",
                "folder": "altcolor/test",
                "urls": [{"url": "https://example.com/altcolor", "broken": true}],
                "createdAt": 1521849600000,
                "updatedAt": 1707993541782,
                "game": {"id": "test123", "name": "Test Game"}
            }],
            "tutorialFiles": [{
                "id": "tut1",
                "title": "Test Game Tutorial",
                "authors": ["TutAuthor"],
                "url": "https://example.com/tutorial",
                "urls": [{"url": "https://example.com/tutorial"}],
                "youtubeId": "abc123",
                "createdAt": 1722607915333,
                "updatedAt": 1722607968996,
                "game": {"id": "test123", "name": "Test Game"}
            }],
            "wheelArtFiles": [],
            "topperFiles": [],
            "pupPackFiles": [],
            "altSoundFiles": [],
            "povFiles": [],
            "mediaPackFiles": [],
            "ruleFiles": [],
            "soundFiles": []
        }
    ]"#;

    let games: Vec<Game> = serde_json::from_str(json).expect("failed to parse sample JSON");
    assert_eq!(games.len(), 1);

    let game = &games[0];
    assert_eq!(game.name, "Test Game");
    assert_eq!(game.manufacturer.as_deref(), Some("Williams"));
    assert_eq!(game.year, Some(1992));
    assert_eq!(game.players, Some(4));
    assert_eq!(game.game_type.as_deref(), Some("SS"));
    assert_eq!(game.mpu.as_deref(), Some("WPC"));
    assert_eq!(game.resource_count(), 5);

    // Table file
    let table = &game.table_files[0];
    assert_eq!(table.table_format.as_deref(), Some("VPX"));
    assert_eq!(table.edition.as_deref(), Some("Premium"));
    assert_eq!(table.features, vec!["MOD", "SSF"]);
    assert_eq!(table.urls[0].broken, Some(false));

    // B2S file
    let b2s = &game.b2s_files[0];
    assert_eq!(b2s.features, vec!["FullDMD", "2Screens"]);

    // ROM file
    let rom = &game.rom_files[0];
    assert_eq!(rom.name.as_deref(), Some("hook_501"));

    // Alt color file
    let altcolor = &game.alt_color_files[0];
    assert_eq!(altcolor.color_type.as_deref(), Some("Pin2DMD"));
    assert_eq!(altcolor.folder.as_deref(), Some("altcolor/test"));
    assert_eq!(altcolor.urls[0].broken, Some(true));

    // Tutorial file
    let tutorial = &game.tutorial_files[0];
    assert_eq!(tutorial.youtube_id.as_deref(), Some("abc123"));
    assert_eq!(tutorial.title.as_deref(), Some("Test Game Tutorial"));
}

/// Handles dirty data: negative players, negative timestamps, missing optional fields.
#[test]
fn parse_dirty_data() {
    let json = r#"[{
        "id": "dirty1",
        "name": "Dirty Game",
        "players": -14,
        "updatedAt": -61953724800000,
        "lastCreatedAt": -61953724800000,
        "tableFiles": [{
            "id": "t1",
            "urls": [],
            "createdAt": -61953724800000,
            "updatedAt": -61953724800000,
            "game": {"id": "dirty1", "name": "Dirty Game"}
        }]
    }]"#;

    let games: Vec<Game> = serde_json::from_str(json).expect("failed to parse dirty data");
    assert_eq!(games[0].players, Some(-14));
    assert_eq!(games[0].updated_at, Some(-61953724800000));
}

/// Verifies parsing against the real VPS database.
/// Run with: cargo test -p vpin-manager-core -- --ignored --nocapture
#[test]
#[ignore]
fn parse_real_vpsdb() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let url = "https://virtualpinballspreadsheet.github.io/vps-db/db/vpsdb.json";
        let resp = reqwest::get(url).await.expect("failed to fetch vpsdb.json");
        let bytes = resp.bytes().await.expect("failed to read response body");

        let games: Vec<Game> =
            serde_json::from_slice(&bytes).expect("failed to parse vpsdb.json");

        assert!(games.len() > 2000, "expected 2000+ games, got {}", games.len());

        let total_resources: usize = games.iter().map(|g| g.resource_count()).sum();
        assert!(total_resources > 10000);

        println!("Parsed {} games with {} total resources", games.len(), total_resources);
    });
}
