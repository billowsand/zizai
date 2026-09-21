use qingjian_platform::Config;

fn temp(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(name);
    let _ = std::fs::remove_file(&path);
    path
}

#[test]
fn polish_level_round_trips_through_toml_save_and_load() {
    let path = temp("qingjian-polish-roundtrip.toml");
    Config::set_value(&path, "voice", "polish", "written").unwrap();
    let config = Config::load(&path).unwrap();
    assert_eq!(
        config.voice.polish_level(),
        qingjian_platform::PolishLevel::Written
    );
    let _ = std::fs::remove_file(&path);
}

/// 旧配置只有总开关时按 spoken 迁移；新键优先。
#[test]
fn old_polish_enabled_true_maps_to_spoken() {
    let path = temp("qingjian-polish-legacy.toml");
    std::fs::write(&path, "[voice]\npolish_enabled = true\n").unwrap();
    let config = Config::load(&path).unwrap();
    assert_eq!(
        config.voice.polish_level(),
        qingjian_platform::PolishLevel::Spoken
    );
    let _ = std::fs::remove_file(&path);
}
