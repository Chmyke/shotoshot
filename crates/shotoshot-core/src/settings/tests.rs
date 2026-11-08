use shotoshot_text::Language;
#[test]
fn writes_the_shotoshot_header_and_still_reads_the_pre_rename_one() {
    let settings = Settings {
        last_custom_color: [1, 2, 3],
        ..Default::default()
    };
    let encoded = settings.encode();
    assert!(encoded.starts_with("shotoshot-settings-21\n"));
    let legacy = encoded.replacen("shotoshot-settings-", "shotshot-settings-", 1);
    assert_eq!(Settings::decode(&legacy).unwrap(), settings);
    let oldest = Settings::decode("shotshot-settings-1\n3\n65\n1\n43,3a,2f,e9\n").unwrap();
    assert_eq!((oldest.modifiers, oldest.key), (3, 65));
    assert!(Settings::decode(&encoded.replacen("shotoshot-", "other-", 1)).is_err());
    assert!(Settings::decode(&encoded.replacen("-21\n", "-22\n", 1)).is_err());
}
#[test]
fn board_fields_roundtrip_and_older_files_get_defaults() {
    let fields: Vec<String> = Settings::default()
        .encode()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(fields.len(), 34);
    assert_eq!(&fields[25..29], &["1,66", "0", "14", "auto"]);
    let mut old = fields[..25].to_vec();
    old[0] = "shotoshot-settings-13".into();
    let migrated = Settings::decode(&old.join("\n")).unwrap();
    assert_eq!(
        migrated.board_shortcut,
        Shortcut {
            modifiers: 1,
            key: 66
        }
    );
    assert!(!migrated.board_visible);
    let settings = Settings {
        board_shortcut: Shortcut {
            modifiers: 3,
            key: 78,
        },
        board_visible: true,
        ..Default::default()
    };
    assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    for (index, invalid) in [
        (25, "0,66"),
        (25, "3,27"),
        (25, "1,0"),
        (25, "16,66"),
        (26, "2"),
        (26, ""),
    ] {
        let mut damaged = fields.clone();
        damaged[index] = invalid.into();
        assert!(
            Settings::decode(&damaged.join("\n")).is_err(),
            "accepted {invalid:?}"
        );
    }
}
#[test]
fn language_roundtrips_in_version_16() {
    let settings = Settings {
        language: LanguageChoice::Fixed(Language::Ja),
        ..Default::default()
    };
    let encoded = settings.encode();
    assert!(encoded.starts_with("shotoshot-settings-21\n"));
    assert_eq!(encoded.lines().nth(28), Some("ja"));
    assert_eq!(
        Settings::decode(&encoded).unwrap().language,
        settings.language
    );
    for language in Language::ALL {
        let settings = Settings {
            language: LanguageChoice::Fixed(language),
            ..Default::default()
        };
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    assert_eq!(Settings::default().language, LanguageChoice::Auto);
    assert_eq!(Settings::default().encode().lines().nth(28), Some("auto"));
}
#[test]
fn version_14_and_15_files_get_auto_language() {
    let fields: Vec<String> = Settings {
        language: LanguageChoice::Fixed(Language::De),
        capture_eraser_size: 32,
        ..Default::default()
    }
    .encode()
    .lines()
    .map(str::to_owned)
    .collect();
    assert_eq!(fields.len(), 34);
    // Format 14 ends before the eraser size, 15 before the language.
    for (version, len, eraser) in [("14", 27, crate::ERASER_SIZE as u32), ("15", 28, 32)] {
        let mut old = fields[..len].to_vec();
        old[0] = format!("shotoshot-settings-{version}");
        let migrated = Settings::decode(&old.join("\n")).unwrap();
        assert_eq!(migrated.language, LanguageChoice::Auto, "{version}");
        assert_eq!(migrated.capture_eraser_size, eraser, "{version}");
        // The legacy header still works, and a file with one line too many is damaged.
        old[0] = format!("shotshot-settings-{version}");
        assert!(Settings::decode(&old.join("\n")).is_ok(), "{version}");
        let mut too_long = fields[..=len].to_vec();
        too_long[0] = format!("shotoshot-settings-{version}");
        assert!(Settings::decode(&too_long.join("\n")).is_err(), "{version}");
        // The migrated file is written as version 20.
        assert!(migrated.encode().starts_with("shotoshot-settings-21\n"));
    }
}
#[test]
fn unknown_language_tag_is_auto_and_keeps_other_fields() {
    let settings = Settings {
        capture_stroke_width: 9,
        ..Default::default()
    };
    for tag in ["ar", "", "xx-YY", "FR-fr "] {
        let mut fields: Vec<String> = settings.encode().lines().map(str::to_owned).collect();
        fields[28] = tag.into();
        let read = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
        assert_eq!(read.capture_stroke_width, 9, "{tag:?}");
        assert_eq!(read.board_shortcut, settings.board_shortcut, "{tag:?}");
        assert_eq!(read.language, LanguageChoice::Auto, "{tag:?}");
    }
    // A known tag with different case or spaces is still understood.
    let mut fields: Vec<String> = settings.encode().lines().map(str::to_owned).collect();
    fields[28] = "FR ".into();
    assert_eq!(
        Settings::decode(&fields.join("\n")).unwrap().language,
        LanguageChoice::Fixed(Language::Fr)
    );
}
#[test]
fn board_visibility_is_saved_alone() {
    let dir = std::env::temp_dir().join(format!("shotoshot-board-visible-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.txt");
    let settings = Settings {
        board_visible: false,
        capture_stroke_width: 9,
        ..Default::default()
    };
    std::fs::write(&path, settings.encode()).unwrap();
    Settings::save_board_visible_to(&path, true, rename).unwrap();
    let read = Settings::load_from(&path).unwrap();
    assert!(read.board_visible);
    assert_eq!(read.capture_stroke_width, 9, "other fields are kept");
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn the_learned_wheel_is_saved_alone_and_older_files_still_hint() {
    let dir = std::env::temp_dir().join(format!("shotoshot-wheel-learned-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.txt");
    let settings = Settings {
        capture_stroke_width: 9,
        ..Default::default()
    };
    std::fs::write(&path, settings.encode()).unwrap();
    assert!(!Settings::load_from(&path).unwrap().wheel_learned);
    Settings::save_wheel_learned_to(&path, rename).unwrap();
    let read = Settings::load_from(&path).unwrap();
    assert!(read.wheel_learned);
    assert_eq!(read.capture_stroke_width, 9, "other fields are kept");
    assert_eq!(Settings::decode(&read.encode()).unwrap(), read);
    std::fs::remove_dir_all(dir).unwrap();

    let mut fields: Vec<_> = settings.encode().lines().map(String::from).collect();
    fields[0] = "shotoshot-settings-17".into();
    fields.truncate(30);
    let older = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
    assert!(
        !older.wheel_learned,
        "a version 17 file has never seen the hint"
    );
    fields[0] = "shotoshot-settings-18".into();
    fields.push("2".into());
    assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
}
#[test]
fn the_obs_mirror_roundtrips_and_older_files_keep_it_off() {
    let settings = Settings {
        presentation: PresentationSettings {
            obs_mirror: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let encoded = settings.encode();
    assert_eq!(encoded.lines().nth(33), Some("1"));
    assert_eq!(Settings::decode(&encoded).unwrap(), settings);
    let mut fields: Vec<_> = encoded.lines().map(String::from).collect();
    fields[0] = "shotoshot-settings-20".into();
    fields.pop();
    let older = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
    assert!(
        !older.presentation.obs_mirror,
        "a version 20 file has no mirror"
    );
    fields[0] = "shotoshot-settings-21".into();
    fields.push("2".into());
    assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
}
#[test]
fn the_mini_toolbar_roundtrips_and_older_files_keep_the_full_one() {
    let settings = Settings {
        mini_toolbar: true,
        ..Default::default()
    };
    let encoded = settings.encode();
    assert_eq!(encoded.lines().nth(31), Some("1"));
    assert_eq!(Settings::decode(&encoded).unwrap(), settings);
    let mut fields: Vec<_> = encoded.lines().map(String::from).collect();
    fields[0] = "shotoshot-settings-18".into();
    fields.truncate(31);
    let older = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
    assert!(
        !older.mini_toolbar,
        "a version 18 file shows the full toolbar"
    );
    fields[0] = "shotoshot-settings-19".into();
    fields.push("2".into());
    assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
}
#[test]
fn the_unfolded_mini_toolbar_is_saved_alone_and_older_files_open_folded() {
    let dir = std::env::temp_dir().join(format!("shotoshot-unfolded-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.txt");
    let settings = Settings {
        mini_toolbar: true,
        capture_stroke_width: 9,
        ..Default::default()
    };
    std::fs::write(&path, settings.encode()).unwrap();
    Settings::save_toolbar_unfolded_to(&path, true, rename).unwrap();
    let read = Settings::load_from(&path).unwrap();
    assert!(read.toolbar_unfolded && read.mini_toolbar);
    assert_eq!(read.capture_stroke_width, 9, "other fields are kept");
    Settings::save_toolbar_unfolded_to(&path, false, rename).unwrap();
    assert!(!Settings::load_from(&path).unwrap().toolbar_unfolded);
    std::fs::remove_dir_all(dir).unwrap();

    let mut fields: Vec<_> = read.encode().lines().map(String::from).collect();
    assert_eq!(fields[32], "1");
    fields[0] = "shotoshot-settings-19".into();
    fields.truncate(32);
    let older = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
    assert!(!older.toolbar_unfolded, "a version 19 file opens folded");
    fields[0] = "shotoshot-settings-21".into();
    fields.push("2".into());
    assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
}
#[test]
fn save_format_roundtrips_and_older_files_keep_png() {
    let fields: Vec<String> = Settings::default()
        .encode()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(&fields[24..25], &["0"]);
    let mut old = fields[..24].to_vec();
    old[0] = "shotoshot-settings-12".into();
    let migrated = Settings::decode(&old.join("\n")).unwrap();
    assert_eq!(migrated.save_format, SaveFormat::Png);
    for format in SaveFormat::ALL {
        let settings = Settings {
            save_format: format,
            ..Default::default()
        };
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    for invalid in ["2", "jpg", "00", " 1"] {
        let mut damaged = fields.clone();
        damaged[24] = invalid.into();
        assert!(
            Settings::decode(&damaged.join("\n")).is_err(),
            "accepted save format {invalid:?}"
        );
    }
}
#[test]
fn full_and_editor_saves_keep_the_stored_board_visibility() {
    let path =
        std::env::temp_dir().join(format!("shotoshot-board-merge-{}.txt", std::process::id()));
    // Another process shows the board while the resident still holds `false`.
    let resident = Settings {
        key: 65,
        ..Default::default()
    };
    assert!(!resident.board_visible);
    resident.save_to(&path, None, None, rename).unwrap();
    Settings::save_board_visible_to(&path, true, rename).unwrap();
    resident.save_to(&path, None, None, rename).unwrap();
    let stored = Settings::load_from(&path).unwrap();
    assert!(
        stored.board_visible,
        "a resident save keeps the board shown"
    );
    assert_eq!(stored.key, 65, "the resident's own fields are still saved");
    Settings::save_editor_to(&path, Some([1, 2, 3]), Some(7), None, None, rename).unwrap();
    let stored = Settings::load_from(&path).unwrap();
    assert!(stored.board_visible, "an editor save keeps the board shown");
    assert_eq!(stored.capture_stroke_width, 7);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn an_invalid_board_shortcut_is_never_written() {
    let path = std::env::temp_dir().join(format!(
        "shotoshot-board-shortcut-{}.txt",
        std::process::id()
    ));
    let invalid = Settings {
        board_shortcut: Shortcut {
            modifiers: 0,
            key: 66,
        },
        ..Default::default()
    };
    assert!(invalid.save_to(&path, None, None, rename).is_err());
    assert!(!path.exists(), "nothing is written");
}
#[test]
fn save_format_follows_the_file_extension() {
    let of = |name: &str| SaveFormat::of(Path::new(name));
    assert_eq!(of("C:/été/Shot 001.PNG"), Some(SaveFormat::Png));
    assert_eq!(of("photo.jpg"), Some(SaveFormat::Jpeg));
    assert_eq!(of("photo.JPEG"), Some(SaveFormat::Jpeg));
    assert_eq!(of("notes.txt"), None);
    assert_eq!(of("png"), None);
    for format in SaveFormat::ALL {
        let path = Path::new("Shot 001").with_extension(format.extension());
        assert_eq!(SaveFormat::of(&path), Some(format));
    }
}
#[test]
fn input_size_roundtrips_and_the_retired_layout_still_loads() {
    let fields: Vec<String> = Settings::default()
        .encode()
        .lines()
        .map(str::to_owned)
        .collect();
    assert_eq!(&fields[22..24], &["0", "100"]);
    for version in ["10", "11"] {
        let mut old = fields[..22].to_vec();
        old[0] = format!("shotoshot-settings-{version}");
        let migrated = Settings::decode(&old.join("\n")).unwrap();
        assert_eq!(migrated.presentation.input_scale, 100);
    }
    for scale in INPUT_SCALES {
        let mut settings = Settings::default();
        settings.presentation.input_scale = scale;
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    // A file saved with the compact or gaming keyboard still loads.
    for layout in ["1", "2"] {
        let mut saved = fields.clone();
        saved[22] = layout.into();
        assert!(
            Settings::decode(&(saved.join("\n") + "\n")).is_ok(),
            "{layout}"
        );
    }
    for (index, invalid) in [
        (22, "3"),
        (22, "full"),
        (22, "00"),
        (23, "0"),
        (23, "125"),
        (23, "0100"),
        (23, "200 "),
    ] {
        let mut damaged = fields.clone();
        damaged[index] = invalid.into();
        assert!(
            Settings::decode(&damaged.join("\n")).is_err(),
            "accepted field {index}: {invalid:?}"
        );
    }
}
#[test]
fn instant_capture_migration_defaults_and_custom_settings_roundtrip() {
    let mut old: Vec<String> = Settings::default()
        .encode()
        .lines()
        .take(20)
        .map(str::to_owned)
        .collect();
    old[0] = "shotoshot-settings-9".into();
    let migrated = Settings::decode(&old.join("\n")).unwrap().encode();
    let fields: Vec<_> = migrated.lines().collect();
    assert_eq!(fields[0], "shotoshot-settings-21");
    assert_eq!(&fields[20..22], &["0,145", "1"]);
    let mut custom: Vec<_> = fields.iter().map(|v| v.to_string()).collect();
    custom[20] = "6,122".into();
    let roundtrip = |custom: &[String]| {
        Settings::decode(&custom.join("\n"))
            .unwrap()
            .encode()
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    assert_eq!(roundtrip(&custom), custom);
    // A sound switched off before it became permanent is read, then written back on.
    custom[21] = "0".into();
    assert_eq!(roundtrip(&custom)[21], "1");
    custom[21] = "2".into();
    assert!(Settings::decode(&custom.join("\n")).is_err());
    custom[21] = "1".into();
    custom[20] = "0,0".into();
    assert!(Settings::decode(&custom.join("\n")).is_err());
}
use super::*;
#[test]
fn shortcut_refusals_are_listed_keys() {
    let shortcut = |modifiers, key| Shortcut { modifiers, key };
    let shapes = |shortcuts| PresentationSettings {
        shape_shortcuts: shortcuts,
        ..PresentationSettings::default()
    };
    let [a, r, c] = DEFAULT_SHAPE_SHORTCUTS;
    let cases = [
        (shapes([shortcut(16, 0x41), r, c]), Key::SetupInvalidShape),
        (
            shapes([shortcut(4, 0x41), r, c]),
            Key::SetupShapeNeedsModifier,
        ),
        (shapes([shortcut(3, 27), r, c]), Key::SetupReserved),
        (shapes([a, a, c]), Key::SetupDuplicateShape),
    ];
    for (settings, expected) in cases {
        assert_eq!(settings.validate_shape_shortcuts(), Err(expected));
        assert!(SHAPE_SHORTCUT_REFUSALS.contains(&expected));
    }
    assert_eq!(
        PresentationSettings::default().validate_shape_shortcuts(),
        Ok(())
    );
    for (board, expected) in [
        (shortcut(1, 0), Key::SetupInvalidBoard),
        (shortcut(0, 0x42), Key::SetupBoardNeedsModifier),
        (shortcut(3, 27), Key::SetupReserved),
    ] {
        assert_eq!(Settings::validate_board_shortcut(&board), Err(expected));
        assert!(BOARD_SHORTCUT_REFUSALS.contains(&expected));
    }
    assert_eq!(
        Settings::validate_board_shortcut(&Settings::default().board_shortcut),
        Ok(())
    );
}
#[test]
fn startup_and_last_width_survive_reload_without_losing_diagram_position() {
    let path =
        std::env::temp_dir().join(format!("shotoshot-last-width-{}.txt", std::process::id()));
    let mut resident = Settings {
        start_with_windows: false,
        ..Default::default()
    };
    resident.presentation.input_offset = Some([1234, 8765]);
    resident.save_to(&path, None, Some(7), rename).unwrap();
    resident.save_to(&path, None, None, rename).unwrap();
    Settings::save_custom_color_to(&path, [10, 20, 30], rename).unwrap();
    let loaded = Settings::load_from(&path).unwrap();
    assert!(!loaded.start_with_windows);
    assert_eq!(loaded.capture_stroke_width, 7);
    assert_eq!(loaded.presentation.input_offset, Some([1234, 8765]));
    std::fs::remove_file(path).unwrap();
    assert!(Settings::default().start_with_windows);
    let old = Settings::default()
        .encode()
        .lines()
        .take(18)
        .collect::<Vec<_>>()
        .join("\n")
        .replacen("settings-21", "settings-8", 1);
    assert!(Settings::decode(&old).unwrap().start_with_windows);
}
#[test]
fn custom_color_roundtrips_and_old_versions_keep_the_default() {
    let settings = Settings {
        last_custom_color: [0, 128, 255],
        ..Default::default()
    };
    assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    for (version, count) in [(1, 5), (2, 6), (3, 9), (4, 10), (5, 13), (6, 15), (7, 17)] {
        let encoded = settings.encode();
        let mut fields: Vec<_> = encoded.lines().take(count).map(str::to_owned).collect();
        fields[0] = format!("shotoshot-settings-{version}");
        let migrated = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
        assert_eq!(migrated.last_custom_color, [237, 79, 92]);
        assert_eq!(Settings::decode(&migrated.encode()).unwrap(), migrated);
    }
    for invalid in ["", "#123456", "12345", "1234567", "12gg34", "é1234"] {
        let encoded = settings.encode();
        let mut fields: Vec<_> = encoded.lines().collect();
        fields[17] = invalid;
        assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
    }
}
#[test]
fn the_eraser_size_is_kept_across_saves_and_read_back_from_format_15_only() {
    let path = std::env::temp_dir().join(format!("shotoshot-eraser-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&path);
    Settings::save_editor_to(&path, None, None, Some(32), None, rename).unwrap();
    assert_eq!(Settings::load_from(&path).unwrap().capture_eraser_size, 32);
    // A full save from a copy loaded before keeps the editor's size.
    Settings::default()
        .save_to(&path, None, Some(5), rename)
        .unwrap();
    let stored = Settings::load_from(&path).unwrap();
    assert_eq!(
        (stored.capture_eraser_size, stored.capture_stroke_width),
        (32, 5)
    );
    std::fs::remove_file(&path).unwrap();
    // Older formats have no size: the default.
    let older = Settings::default()
        .encode()
        .replacen("settings-21", "settings-14", 1);
    let older = older.lines().take(27).collect::<Vec<_>>().join("\n") + "\n";
    assert_eq!(
        Settings::decode(&older).unwrap().capture_eraser_size,
        crate::ERASER_SIZE as u32
    );
    let encoded = Settings::default().encode();
    for invalid in ["0", "3", "129", "014", "x", ""] {
        let mut fields: Vec<_> = encoded.lines().collect();
        fields[27] = invalid;
        assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
    }
}
#[test]
fn editor_field_merge_keeps_current_resident_preferences_and_reports_failure() {
    let path =
        std::env::temp_dir().join(format!("shotoshot-editor-merge-{}.txt", std::process::id()));
    let resident = Settings {
        key: 77,
        live_capture: true,
        start_with_windows: false,
        ..Default::default()
    };
    resident.save_to(&path, None, Some(9), rename).unwrap();
    let folder = std::env::temp_dir().join("editor-last-folder");
    let destination = Some((folder.as_path(), SaveFormat::Jpeg));
    Settings::save_editor_to(&path, Some([10, 20, 30]), None, None, destination, rename).unwrap();
    let stored = Settings::load_from(&path).unwrap();
    assert_eq!(stored.save_format, SaveFormat::Jpeg);
    assert_eq!(stored.key, 77);
    assert!(stored.live_capture);
    assert!(!stored.start_with_windows);
    assert_eq!(stored.capture_stroke_width, 9);
    assert_eq!(stored.last_custom_color, [10, 20, 30]);
    assert_eq!(stored.folder, folder);
    let before = std::fs::read(&path).unwrap();
    let failed = Settings::save_editor_to(&path, None, Some(17), None, None, |_, _| {
        Err("replace failed".into())
    });
    assert_eq!(failed.unwrap_err(), "replace failed");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    std::fs::write(&path, "damaged settings").unwrap();
    assert!(Settings::save_editor_to(&path, None, Some(17), None, None, rename).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "damaged settings");
    std::fs::remove_file(path.with_extension(format!("{}.tmp", std::process::id()))).unwrap();
    std::fs::remove_file(path).unwrap();
}
#[test]
fn stale_resident_save_preserves_child_color_and_color_save_preserves_preferences() {
    let path = std::env::temp_dir().join(format!(
        "shotoshot-color-settings-{}.txt",
        std::process::id()
    ));
    let mut resident = Settings::default();
    resident.save_to(&path, None, None, rename).unwrap();
    Settings::save_custom_color_to(&path, [12, 34, 56], rename).unwrap();
    resident.key = 65;
    resident.save_to(&path, None, None, rename).unwrap();
    let stored = Settings::decode(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(stored.key, 65);
    assert_eq!(stored.last_custom_color, [12, 34, 56]);
    Settings::save_custom_color_to(&path, [78, 90, 12], rename).unwrap();
    let stored = Settings::decode(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(stored.key, 65);
    assert_eq!(stored.last_custom_color, [78, 90, 12]);
    std::fs::write(&path, "damaged settings").unwrap();
    assert!(resident.save_to(&path, None, None, rename).is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "damaged settings");
    std::fs::remove_file(path).unwrap();
}
#[test]
fn free_input_position_and_lock_roundtrip_and_validate() {
    for offset in [None, Some([0, 0]), Some([10000, 10000]), Some([3210, 7890])] {
        let mut settings = Settings::default();
        settings.presentation.input_offset = offset;
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    let valid = Settings::default().encode();
    for (index, values) in [
        (
            15,
            &[
                "", "10001,0", "0,10001", "65536,0", "-1,0", "+1,0", "1,", "1,2,3", "x,0",
            ][..],
        ),
        (16, &["", "2", "true", "01"][..]),
    ] {
        for invalid in values {
            let mut fields: Vec<_> = valid.lines().collect();
            fields[index] = invalid;
            assert!(
                Settings::decode(&(fields.join("\n") + "\n")).is_err(),
                "accepted {index}: {invalid:?}"
            );
        }
    }
}
#[test]
fn version_six_keeps_custom_settings_and_defaults_drag_preferences() {
    let text = "shotoshot-settings-6\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n2\n6,112\n2,113\n9,114\n12\n40\n";
    let settings = Settings::decode(text).unwrap();
    assert_eq!(settings.presentation.input_offset, None);
    assert_eq!(settings.presentation.stroke_width, 12);
    assert_eq!(settings.presentation.arrow_size, 40);
    assert_eq!(
        settings.presentation.input_position,
        DisplayPosition::TopRight
    );
    assert_eq!(
        settings.presentation.shape_shortcuts[0],
        Shortcut {
            modifiers: 6,
            key: 112
        }
    );
    assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
}
#[test]
fn stroke_preferences_roundtrip_and_reject_unsupported_values() {
    for width in [2, 3, 5, 8, 12] {
        for size in [16, 28, 40] {
            let mut settings = Settings::default();
            settings.presentation.stroke_width = width;
            settings.presentation.arrow_size = size;
            assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
        }
    }
    let valid = Settings::default().encode();
    for (index, values) in [
        (13, &["0", "41", "-5", "05", "+5", ""][..]),
        (14, &["0", "15", "29", "41", "028", "+28", ""][..]),
    ] {
        for invalid in values {
            let mut fields: Vec<_> = valid.lines().collect();
            fields[index] = invalid;
            assert!(
                Settings::decode(&(fields.join("\n") + "\n")).is_err(),
                "accepted field {index}: {invalid:?}"
            );
        }
    }
}
#[test]
fn version_five_keeps_custom_shortcuts_and_adds_stroke_defaults() {
    let text = "shotoshot-settings-5\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n2\n6,112\n2,113\n9,114\n";
    let settings = Settings::decode(text).unwrap();
    assert_eq!(settings.presentation.stroke_width, 5);
    assert_eq!(settings.presentation.arrow_size, 28);
    assert_eq!(
        settings.presentation.input_position,
        DisplayPosition::TopRight
    );
    assert_eq!(
        settings.presentation.shape_shortcuts,
        [
            Shortcut {
                modifiers: 6,
                key: 112
            },
            Shortcut {
                modifiers: 2,
                key: 113
            },
            Shortcut {
                modifiers: 9,
                key: 114
            }
        ]
    );
    assert_eq!(settings.presentation.duration_ms, 5000);
    assert_eq!(settings.presentation.halo_color, [1, 2, 3]);
    assert_eq!(settings.presentation.shape_color, [4, 5, 6]);
    assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
}
#[test]
fn version_four_preserves_position_and_adds_default_shape_shortcuts() {
    let text = "shotoshot-settings-4\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n2\n";
    let settings = Settings::decode(text).unwrap();
    assert_eq!(
        settings.presentation.input_position,
        DisplayPosition::TopRight
    );
    assert_eq!(
        settings.presentation.shape_shortcuts,
        DEFAULT_SHAPE_SHORTCUTS
    );
    assert_eq!(settings.presentation.duration_ms, 5000);
    assert_eq!(settings.presentation.halo_color, [1, 2, 3]);
    assert_eq!(settings.presentation.shape_color, [4, 5, 6]);
    assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
}
#[test]
fn older_formats_move_an_untouched_arrow_shortcut_to_alt_a() {
    let stored = |version: &str, arrow: &str, rectangle: &str| {
        let mut fields: Vec<String> = Settings::default()
            .encode()
            .lines()
            .map(str::to_owned)
            .collect();
        fields[0] = format!("shotoshot-settings-{version}");
        fields[10] = arrow.into();
        fields[11] = rectangle.into();
        // Formats 10 and 11 end before the input layout and size, 12 before the save
        // format, 13 before the board shortcut and visibility, 14 before the eraser size,
        // 15 before the language.
        match version {
            "10" | "11" => fields.truncate(22),
            "12" => fields.truncate(24),
            "13" => fields.truncate(25),
            "14" => fields.truncate(27),
            "15" => fields.truncate(28),
            "16" => fields.truncate(29),
            _ => {}
        }
        let settings = Settings::decode(&(fields.join("\n") + "\n")).unwrap();
        settings.presentation.shape_shortcuts[0]
    };
    let alt = |key| Shortcut { modifiers: 1, key };
    assert_eq!(DEFAULT_SHAPE_SHORTCUTS, [alt(0x41), alt(0x52), alt(0x43)]);
    assert_eq!(stored("10", "1,70", "1,82"), alt(0x41));
    assert_eq!(
        stored("10", "6,112", "1,82"),
        Shortcut {
            modifiers: 6,
            key: 112
        },
        "a chosen shortcut is kept"
    );
    assert_eq!(
        stored("10", "1,70", "1,65"),
        alt(0x46),
        "Alt+A already draws another shape"
    );
    assert_eq!(
        stored("11", "1,70", "1,82"),
        alt(0x46),
        "Alt+F chosen again after the change is kept"
    );
    assert_eq!(stored("12", "1,70", "1,82"), alt(0x46));
    assert_eq!(stored("13", "1,70", "1,82"), alt(0x46));
    assert_eq!(stored("14", "1,70", "1,82"), alt(0x46));
    assert_eq!(stored("15", "1,70", "1,82"), alt(0x46));
    assert_eq!(stored("16", "1,70", "1,82"), alt(0x46));
}
#[test]
fn custom_shortcuts_roundtrip_and_invalid_shortcuts_are_rejected() {
    let mut settings = Settings::default();
    settings.presentation.shape_shortcuts = [
        Shortcut {
            modifiers: 6,
            key: 112,
        },
        Shortcut {
            modifiers: 2,
            key: 113,
        },
        Shortcut {
            modifiers: 9,
            key: 114,
        },
    ];
    assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    let valid = settings.encode();
    for invalid in [
        "16,70", "1,0", "1,255", "3,27", "2,113", "", "1", "1,2,3", "x,70", "1,-1",
    ] {
        let mut fields: Vec<_> = valid.lines().collect();
        fields[10] = invalid;
        assert!(
            Settings::decode(&(fields.join("\n") + "\n")).is_err(),
            "accepted {invalid:?}"
        );
    }
    settings.presentation.shape_shortcuts[2] = settings.presentation.shape_shortcuts[1];
    assert!(settings.presentation.validate_shape_shortcuts().is_err());
}
#[test]
fn every_input_position_roundtrips_and_rejects_invalid_values() {
    for position in DisplayPosition::ALL {
        let mut settings = Settings::default();
        settings.presentation.input_position = position;
        assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    }
    let valid = Settings::default().encode();
    for invalid in ["", "6", "-1", "04", "+4", "bottom-center"] {
        let mut fields: Vec<_> = valid.lines().collect();
        fields[9] = invalid;
        assert!(Settings::decode(&(fields.join("\n") + "\n")).is_err());
    }
}
#[test]
fn version_three_preserves_preferences_and_defaults_to_bottom_center() {
    let text = "shotoshot-settings-3\n3\n65\n1\n43,3a,2f,e9\n1\n5000\n010203\n040506\n";
    let settings = Settings::decode(text).unwrap();
    assert_eq!(
        settings.presentation.input_position,
        DisplayPosition::BottomCenter
    );
    assert_eq!(settings.presentation.duration_ms, 5000);
    assert_eq!(settings.presentation.halo_color, [1, 2, 3]);
    assert_eq!(settings.presentation.shape_color, [4, 5, 6]);
    assert_eq!(Settings::decode(&settings.encode()).unwrap(), settings);
    assert!(settings.live_capture);
    assert!(settings.legacy_style);
    assert_eq!(settings.modifiers, 3);
    assert_eq!(settings.key, 65);
    assert_eq!(settings.folder, PathBuf::from("C:/é"));
}
#[test]
fn old_versions_upgrade_with_default_presentation_settings() {
    for (text, live_capture) in [
        ("shotoshot-settings-1\n3\n65\n1\n43,3a,2f,e9\n", "0"),
        ("shotoshot-settings-2\n3\n65\n1\n43,3a,2f,e9\n1\n", "1"),
    ] {
        let settings = Settings::decode(text).unwrap();
        assert_eq!(
            settings.encode(),
            format!(
                "shotoshot-settings-21\n3\n65\n1\n43,3a,2f,e9\n{live_capture}\n3000\nffd340\ned4f5c\n4\n1,65\n1,82\n1,67\n5\n28\n-\n1\ned4f5c\n1\n3\n0,145\n1\n0\n100\n0\n1,66\n0\n14\nauto\n1,48,100,2,0,0,1,0,ed4f5c,409cff,0,1,-,0,240,60,0,-,0,0,240,2,-,0\n0\n0\n0\n0\n"
            )
        );
    }
}
#[test]
fn presentation_preferences_roundtrip_for_each_supported_duration() {
    for duration in [1000, 3000, 5000] {
        let text =
            format!("shotoshot-settings-3\n3\n65\n0\n43,3a,2f,e9\n1\n{duration}\n00aAFF\nFf0001\n");
        let settings = Settings::decode(&text).unwrap();
        assert_eq!(
            settings.encode(),
            format!(
                "shotoshot-settings-21\n3\n65\n0\n43,3a,2f,e9\n1\n{duration}\n00aaff\nff0001\n4\n1,65\n1,82\n1,67\n5\n28\n-\n1\ned4f5c\n1\n3\n0,145\n1\n0\n100\n0\n1,66\n0\n14\nauto\n1,48,100,2,0,0,1,0,ed4f5c,409cff,0,1,-,0,240,60,0,-,0,0,240,2,-,0\n0\n0\n0\n0\n"
            )
        );
    }
}
#[test]
fn damaged_presentation_preferences_are_rejected() {
    let valid = "shotoshot-settings-3\n0\n19\n0\n\n1\n3000\nffd340\ned4f5c\n";
    assert!(Settings::decode(valid).is_ok());
    for (index, invalid_values) in [
        (0, vec!["shotoshot-settings-4"]),
        (5, vec!["2", "true", ""]),
        (
            6,
            vec!["0", "999", "2000", "5001", "-1000", "+1000", "01000", ""],
        ),
        (
            7,
            vec!["#ffd340", "ffd34", "ffd3400", "ggd340", "ffé40", ""],
        ),
        (
            8,
            vec!["#ed4f5c", "ed4f5", "ed4f5c0", "ed4f5g", " ed4f5c", ""],
        ),
    ] {
        for invalid in invalid_values {
            let mut fields: Vec<_> = valid.lines().collect();
            fields[index] = invalid;
            assert!(
                Settings::decode(&(fields.join("\n") + "\n")).is_err(),
                "accepted field {index}: {invalid:?}"
            );
        }
    }
    assert!(Settings::decode(&format!("{valid}1\n")).is_err());
    assert!(Settings::decode("shotoshot-settings-3\n0\n19\n0\n\n1\n3000\nffd340\n").is_err());
}
#[test]
fn unicode_settings_roundtrip() {
    let s = Settings {
        instant_shortcut: Shortcut {
            modifiers: 0,
            key: 145,
        },
        start_with_windows: true,
        capture_stroke_width: 3,
        capture_eraser_size: 24,
        live_capture: true,
        modifiers: 3,
        key: 65,
        legacy_style: true,
        folder: PathBuf::from("C:/été/图像"),
        save_format: SaveFormat::Jpeg,
        last_custom_color: [237, 79, 92],
        presentation: PresentationSettings {
            stroke_width: 5,
            arrow_size: 28,
            shape_shortcuts: DEFAULT_SHAPE_SHORTCUTS,
            input_position: DisplayPosition::TopRight,
            input_offset: None,
            input_scale: 150,
            duration_ms: 1000,
            halo_color: [0, 128, 255],
            shape_color: [4, 17, 92],
            pointer: crate::pointer::PointerSettings {
                halo_shape: crate::pointer::HaloShape::Ring,
                trail: crate::pointer::TrailStyle::Stars,
                spotlight_shortcut: Some(Shortcut {
                    modifiers: 6,
                    key: 0x20,
                }),
                ..Default::default()
            },
            obs_mirror: true,
        },
        board_shortcut: Shortcut {
            modifiers: 2,
            key: 78,
        },
        board_visible: true,
        language: LanguageChoice::Fixed(Language::Ja),
        wheel_learned: true,
        mini_toolbar: true,
        toolbar_unfolded: true,
    };
    assert_eq!(Settings::decode(&s.encode()).unwrap(), s);
}
#[test]
fn rejects_future_and_invalid_shortcuts() {
    assert!(Settings::decode("shotoshot-settings-9\n0\n65\n0\n\n").is_err());
    assert!(Settings::decode("shotoshot-settings-1\n99\n65\n0\n\n").is_err());
}
#[test]
fn old_settings_keep_frozen_default_and_new_mode_roundtrips() {
    let old = Settings::decode("shotoshot-settings-1\n0\n19\n1\n\n").unwrap();
    assert!(!old.live_capture);
    assert_eq!(old.key, 19);
    assert!(old.legacy_style);
    assert_eq!(Settings::decode(&old.encode()).unwrap(), old);
    assert!(Settings::decode("shotoshot-settings-2\n0\n19\n0\n\nx\n").is_err());
}
#[test]
fn empty_folder_roundtrip() {
    assert_eq!(
        Settings::decode(&Settings::default().encode()).unwrap(),
        Settings::default()
    );
}
