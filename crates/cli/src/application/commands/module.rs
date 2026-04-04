use super::{init::to_pascal_case, init::write_file, load_project};
use crate::infrastructure::ui;
use std::{fs, path::PathBuf};

pub fn module_add(name: String, project_dir: Option<PathBuf>) {
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        ui::die(format!(
            "module name '{}' must only contain letters, digits, hyphens or underscores",
            name
        ));
    }

    let proj = load_project(project_dir);

    if proj.project.modules.contains(&name) {
        ui::die(format!("module '{name}' already exists in this project."));
    }

    let root = proj.root().to_path_buf();
    let src_main = root.join("modules").join(&name).join("src").join("main");
    let src_test = root.join("modules").join(&name).join("src").join("test");

    fs::create_dir_all(&src_main)
        .unwrap_or_else(|e| ui::die(format!("cannot create directory: {e}")));
    fs::create_dir_all(&src_test)
        .unwrap_or_else(|e| ui::die(format!("cannot create directory: {e}")));

    let module_json = format!(
        r#"{{
  "name": "{name}",
  "main": "app.nx",
  "dependencies": {{}}
}}
"#
    );
    write_file(
        &root.join("modules").join(&name).join("module.json"),
        &module_json,
    );

    let app_class = to_pascal_case(&name);
    let app_nx = format!(
        r#"package {pkg};

app {app} {{
  server {{ port: 3000; }}

  public window HomePage {{
    public render() => Component {{
      return Page {{
        Heading("Module {app}")
      }};
    }}
  }}

  route "/" => HomePage;
}}
"#,
        pkg = name.replace('-', "_"),
        app = app_class,
    );
    write_file(&src_main.join("app.nx"), &app_nx);

    // Add module to project.json
    let proj_path = root.join("project.json");
    if let Ok(text) = fs::read_to_string(&proj_path) {
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(modules) = val.get_mut("modules").and_then(|m| m.as_array_mut()) {
                modules.push(serde_json::Value::String(name.clone()));
            }
            if let Ok(updated) = serde_json::to_string_pretty(&val) {
                let _ = fs::write(&proj_path, updated);
            }
        }
    }

    ui::blank();
    ui::success(format!("Module \x1b[1m{name}\x1b[0m added"));
    ui::blank();
    ui::hint(format!("  modules/{name}/"));
    ui::hint("  ├── module.json");
    ui::hint("  └── src/main/app.nx");
    ui::blank();
    ui::hint(format!(
        "  Set as main:  nexa-compiler.yaml → main_module: \"{name}\""
    ));
    ui::blank();
}
