pub mod parse;
pub mod serialize;

pub use parse::*;
pub use serialize::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Profile;
    use crate::store::to_toml_string;
    use crate::PanteonError;

    #[test]
    fn toml_roundtrip() {
        let profile = Profile::default();
        let toml_str = to_toml_string(&profile);
        let parsed = parse(&toml_str).expect("Failed to parse serialized TOML");
        assert_eq!(profile, parsed);
    }

    #[test]
    fn test_expanded_toml_sections_parse() {
        let toml_str = r##"
id = 1
name = "High-DPI Preset"
active_dpi_index = 2
polling_rate = 1000

[dpi]
slots = [8000, 10400, 11200, 8800, 12800, 9600]
active = 2
colors = ["#ff0000", "#00ff00", "#0000ff", "#ffff00", "#00ffff", "#ff00ff"]

[fire]
delay_ms = 8

[led]
effect = "Выключено"
speed = 7
brightness = 0
all_colors = false
"##;
        let profile = parse(toml_str).expect("parse expanded toml");
        assert_eq!(profile.name, "High-DPI Preset");
        assert_eq!(profile.active_dpi_index, 2);
        assert_eq!(profile.fire_delay_ms, 8);
        assert_eq!(profile.led_effect, "Выключено");
        assert_eq!(profile.led_speed, 7);
        assert_eq!(profile.led_brightness, 0);
        assert!(!profile.led_all_colors);
        assert_eq!(profile.dpi_levels.len(), 6);
        assert_eq!(profile.dpi_levels[0].dpi, 8000);
        assert_eq!(profile.dpi_levels[0].main, [255, 0, 0]);
        assert_eq!(profile.dpi_levels[4].dpi, 12800);
    }

    #[test]
    fn parse_toml_without_dpi_fails() {
        let toml_missing_dpi = r#"
id = 0
name = "Config0"
active_dpi_index = 1
polling_rate = 1000

[[dpi_levels]]
level_index = 1
enabled = true
color = [255, 0, 0]
"#;
        let err = parse(toml_missing_dpi).unwrap_err();
        assert_eq!(
            err,
            PanteonError::ProtocolDetailed {
                context: "DpiLevel",
                detail: "missing required field 'dpi'".to_string(),
            }
        );
    }

    #[test]
    fn parse_toml_action_typo_fails() {
        let toml_typo = r#"
id = 0
name = "Config0"
active_dpi_index = 1
polling_rate = 1000

[[dpi_levels]]
level_index = 1
dpi = 800
enabled = true
color = [255, 0, 0]

[[button_bindings]]
button_index = 1
action = "LeftClck"
"#;
        let err = parse(toml_typo).unwrap_err();
        assert_eq!(
            err,
            PanteonError::ProtocolDetailed {
                context: "ButtonAction",
                detail: "unknown button action 'LeftClck'".to_string(),
            }
        );
        assert!(format!("{}", err).contains("LeftClck"));
    }
}
