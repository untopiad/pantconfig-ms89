use crate::model::Profile;

/// Serialize Profile into TOML string format with expanded sections
pub fn profile_to_toml_string(profile: &Profile) -> String {
    let mut out = String::new();
    out.push_str(&format!("id = {}\n", profile.id));
    out.push_str(&format!("name = \"{}\"\n", profile.name));
    out.push_str(&format!("active_dpi_index = {}\n", profile.active_dpi_index));
    out.push_str(&format!("polling_rate = {}\n\n", profile.polling_rate.as_hz()));

    out.push_str("[dpi]\n");
    let slots_str: Vec<String> = profile.dpi_levels.iter().map(|d| d.dpi.to_string()).collect();
    out.push_str(&format!("slots = [{}]\n", slots_str.join(", ")));
    out.push_str(&format!("active = {}\n", profile.active_dpi_index));
    let colors_str: Vec<String> = profile
        .dpi_levels
        .iter()
        .map(|d| format!("\"#{:02x}{:02x}{:02x}\"", d.main[0], d.main[1], d.main[2]))
        .collect();
    out.push_str(&format!("colors = [{}]\n\n", colors_str.join(", ")));

    out.push_str("[fire]\n");
    out.push_str(&format!("delay_ms = {}\n\n", profile.fire_delay_ms));

    out.push_str("[led]\n");
    out.push_str(&format!("effect = \"{}\"\n", profile.led_effect));
    out.push_str(&format!("speed = {}\n", profile.led_speed));
    out.push_str(&format!("brightness = {}\n", profile.led_brightness));
    out.push_str(&format!("all_colors = {}\n\n", profile.led_all_colors));

    for dpi in &profile.dpi_levels {
        out.push_str("[[dpi_levels]]\n");
        out.push_str(&format!("level_index = {}\n", dpi.level_index));
        out.push_str(&format!("dpi = {}\n", dpi.dpi));
        out.push_str(&format!("enabled = {}\n", dpi.enabled));
        out.push_str(&format!(
            "color = [{}, {}, {}]\n",
            dpi.main[0], dpi.main[1], dpi.main[2]
        ));
        out.push_str(&format!(
            "second_color = [{}, {}, {}]\n\n",
            dpi.second[0], dpi.second[1], dpi.second[2]
        ));
    }

    for btn in &profile.button_bindings {
        out.push_str("[[button_bindings]]\n");
        out.push_str(&format!("button_index = {}\n", btn.button_index));
        out.push_str(&format!("action = \"{}\"\n\n", btn.action.as_str()));
    }

    out
}
