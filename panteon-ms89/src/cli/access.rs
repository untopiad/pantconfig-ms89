use panteon_ms89::device::{
    self, default_rule_dirs, report_is_ok, rule_dirs_from_env, AccessReport, NodeAccess, RuleState,
};
use panteon_ms89::service::discover;
use panteon_ms89::PanteonError;

/// `check-access [--print-rule]`: read-only access diagnostics.
///
/// Returns normally (exit code 0) when access is fully configured. When
/// something is missing it prints the installation command and exits with
/// code 1 so scripts can detect the condition.
pub fn run_check_access(print_rule: bool) -> Result<(), PanteonError> {
    if print_rule {
        print!("{}", device::UDEV_RULE);
        return Ok(());
    }

    let devices = discover()?;
    let report = device::diagnose(&devices);

    for line in report_lines(&report) {
        println!("{}", line);
    }

    if report_is_ok(&report) {
        println!();
        println!("Доступ настроен: правило установлено, узлы открываются.");
        return Ok(());
    }

    println!();
    println!("Установите правило (офлайн, без curl):");
    println!();
    print!("{}", device::install_command());
    println!();
    println!("После установки нажмите «Перепроверить» в GUI или повторите `panteon-ms89 check-access`.");

    std::process::exit(1);
}

/// Human-readable report lines (also the contract used by the CLI tests).
pub fn report_lines(report: &AccessReport) -> Vec<String> {
    let searched = rule_dirs_from_env().unwrap_or_else(default_rule_dirs);
    let searched_str = searched
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let rule_line = match report.rule {
        RuleState::Installed => format!(
            "udev-правило: установлено ({})",
            report
                .rule_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "путь неизвестен".to_string())
        ),
        RuleState::Outdated => format!(
            "udev-правило: устарело ({}) — содержимое отличается от поставляемого",
            report
                .rule_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "путь неизвестен".to_string())
        ),
        RuleState::Missing => format!("udev-правило: отсутствует (искали: {})", searched_str),
    };

    vec![
        rule_line,
        format!("hidraw: {}", node_text(report.hidraw)),
        format!("USB reset: {}", node_text(report.usb_reset)),
    ]
}

fn node_text(access: NodeAccess) -> &'static str {
    match access {
        NodeAccess::Ok => "доступ есть",
        NodeAccess::Denied => "нет доступа",
        NodeAccess::NoNode => "устройство не подключено",
    }
}