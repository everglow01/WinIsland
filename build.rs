fn main() {
    println!("cargo:rerun-if-env-changed=WINISLAND_BUILD_NUMBER");
    let build_number = std::env::var("WINISLAND_BUILD_NUMBER")
        .ok()
        .filter(|value| value.parse::<u64>().is_ok())
        .unwrap_or_else(|| "0".to_string());
    println!("cargo:rustc-env=WINISLAND_BUILD_NUMBER={build_number}");

    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        println!("cargo:rerun-if-env-changed=WINISLAND_PACKAGE_CHANNEL");
        if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg=/EXPORT:RTSSHooksCompatibility,DATA");
        }

        let mut res = winres::WindowsResource::new();

        let icon_path = "resources/icon-dark.ico";
        if std::path::Path::new(icon_path).exists() {
            res.set_icon(icon_path);
        } else {
            println!(
                "cargo:warning=Icon file not found: {icon_path}, executable will use default icon"
            );
        }

        res.set("CompanyName", "Eatgrapes");
        res.set("FileDescription", "WinIsland");
        res.set("ProductName", "WinIsland");
        res.set("LegalCopyright", "Copyright (c) Eatgrapes");

        let package = package_identity();
        let manifest = format!(
            include_str!("resources/WinIsland.manifest"),
            package_name = package.name
        );
        res.set_manifest(&manifest);
        res.compile().unwrap();
    }
}

struct PackageIdentity {
    name: &'static str,
}

fn package_identity() -> PackageIdentity {
    match std::env::var("WINISLAND_PACKAGE_CHANNEL").as_deref() {
        Ok("nightly" | "stable") => PackageIdentity {
            name: "Eatgrapes.WinIsland",
        },
        _ => PackageIdentity {
            name: "Eatgrapes.WinIsland.Dev",
        },
    }
}
