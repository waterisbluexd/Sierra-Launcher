use layer_shika::slint_interpreter::{ComponentInstance, Struct, Value};
use slint::{Image, ModelRc, SharedString, VecModel};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
struct DesktopApp {
    name: String,
    exec: String,
    icon: Option<String>,
}

pub struct AppGrid {
    apps: HashMap<String, DesktopApp>,
}

impl AppGrid {
    pub fn load() -> Self {
        let apps = scan_applications()
            .into_iter()
            .map(|app| (app.name.clone(), app))
            .collect::<HashMap<_, _>>();

        eprintln!("[appgrid] loaded {} applications", apps.len());

        Self { apps }
    }

    pub fn value(&self) -> Value {
        self.filtered_value("")
    }

    pub fn filtered_value(&self, query: &str) -> Value {
        let q = query.trim().to_lowercase();

        let mut names: Vec<&String> = self.apps.keys().collect();
        names.sort_by_key(|name| name.to_lowercase());

        let items: Vec<Value> = names
            .into_iter()
            .filter(|name| q.is_empty() || name.to_lowercase().contains(&q))
            .map(|name| {
                let app = &self.apps[name];

                let icon = app
                    .icon
                    .as_deref()
                    .and_then(find_icon_path)
                    .and_then(|path| Image::load_from_path(&path).ok());

                let has_icon = icon.is_some();

                let initial = app
                    .name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default();

                let mut entry = Struct::default();

                entry.set_field(
                    "name".into(),
                    Value::String(SharedString::from(app.name.as_str())),
                );

                entry.set_field("initial".into(), Value::String(SharedString::from(initial)));

                entry.set_field("icon".into(), Value::Image(icon.unwrap_or_default()));

                entry.set_field("has-icon".into(), Value::Bool(has_icon));

                Value::Struct(entry)
            })
            .collect();

        eprintln!("[appgrid] sending {} applications to UI", items.len());

        Value::Model(ModelRc::new(VecModel::from(items)))
    }

    pub fn launch(&self, name: &str) {
        if let Some(app) = self.apps.get(name) {
            launch_command(&app.exec);
        }
    }
}

pub fn push_apps_state(instance: &ComponentInstance, grid: &AppGrid) {
    push_filtered_apps_state(instance, grid, "");
}

pub fn push_filtered_apps_state(instance: &ComponentInstance, grid: &AppGrid, query: &str) {
    let value = grid.filtered_value(query);

    match instance.set_property("apps", value) {
        Ok(_) => {
            eprintln!("[appgrid] apps property updated successfully");
        }
        Err(e) => {
            eprintln!("[appgrid] FAILED to update apps property: {e:?}");
        }
    }
}

fn desktop_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
    ];

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);

        dirs.push(home.join(".local/share/applications"));
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }

    dirs
}

fn parse_desktop_entry(path: &Path) -> Option<DesktopApp> {
    let content = fs::read_to_string(path).ok()?;

    let mut in_entry_section = false;
    let mut name = None;
    let mut exec = None;
    let mut icon = None;
    let mut no_display = false;
    let mut is_application = true;

    for line in content.lines() {
        let line = line.trim();

        if line.starts_with('[') {
            in_entry_section = line == "[Desktop Entry]";
            continue;
        }

        if !in_entry_section || line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        match key {
            "Name" if name.is_none() => {
                name = Some(value.to_string());
            }

            "Exec" => {
                exec = Some(value.to_string());
            }

            "Icon" => {
                icon = Some(value.to_string());
            }

            "NoDisplay" => {
                no_display = value.eq_ignore_ascii_case("true");
            }

            "Type" => {
                is_application = value == "Application";
            }

            _ => {}
        }
    }

    if no_display || !is_application {
        return None;
    }

    Some(DesktopApp {
        name: name?,
        exec: exec?,
        icon,
    })
}

fn scan_applications() -> Vec<DesktopApp> {
    let mut by_name: HashMap<String, DesktopApp> = HashMap::new();

    for dir in desktop_dirs() {
        eprintln!("[appgrid] scanning {}", dir.display());

        let Ok(read_dir) = fs::read_dir(&dir) else {
            eprintln!("[appgrid] directory unavailable: {}", dir.display());
            continue;
        };

        for entry in read_dir.flatten() {
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("desktop") {
                continue;
            }

            if let Some(app) = parse_desktop_entry(&path) {
                by_name.insert(app.name.clone(), app);
            }
        }
    }

    let apps: Vec<DesktopApp> = by_name.into_values().collect();

    eprintln!("[appgrid] scan complete: {} applications", apps.len());

    apps
}

fn icon_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(home) = std::env::var("HOME") {
        let home = PathBuf::from(home);

        roots.push(home.join(".local/share/icons"));
        roots.push(home.join(".icons"));
    }

    roots.push(PathBuf::from("/usr/share/icons"));
    roots.push(PathBuf::from("/usr/local/share/icons"));

    roots
}

const ICON_SIZES: &[&str] = &[
    "scalable", "512x512", "256x256", "128x128", "96x96", "64x64", "48x48", "32x32",
];

const ICON_THEMES: &[&str] = &["hicolor", "Adwaita", "breeze", "Papirus"];

fn find_icon_path(icon_name: &str) -> Option<PathBuf> {
    let direct = Path::new(icon_name);

    if direct.is_absolute() && direct.exists() {
        return Some(direct.to_path_buf());
    }

    for root in icon_search_roots() {
        for theme in ICON_THEMES {
            for size in ICON_SIZES {
                for ext in ["svg", "png"] {
                    let candidate = root
                        .join(theme)
                        .join(size)
                        .join("apps")
                        .join(format!("{icon_name}.{ext}"));

                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }
    }

    for ext in ["png", "svg", "xpm"] {
        let candidate = PathBuf::from("/usr/share/pixmaps").join(format!("{icon_name}.{ext}"));

        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

fn launch_command(exec_line: &str) {
    let cleaned: Vec<&str> = exec_line
        .split_whitespace()
        .filter(|token| !token.starts_with('%'))
        .collect();

    let Some((cmd, args)) = cleaned.split_first() else {
        return;
    };

    let _ = Command::new(cmd).args(args).spawn();
}
