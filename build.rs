use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    if env::var_os("CARGO_CFG_WINDOWS").is_some() {
        let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

        let png_path = manifest_dir.join("resources/icons/icon.png");
        println!("cargo:rerun-if-changed={}", png_path.display());

        let ico_path = out_dir.join("icon.ico");
        generate_ico(&png_path, &ico_path);

        let manifest_src = manifest_dir.join("resources/manifest.xml");
        println!("cargo:rerun-if-changed={}", manifest_src.display());
        let manifest_dst = out_dir.join("manifest.xml");
        fs::copy(&manifest_src, &manifest_dst).expect("copy manifest.xml to OUT_DIR");

        let rc_path = out_dir.join("app.rc");
        // ID 1 (literal numeric) — Explorer / Alt+Tab pick the lowest-ID icon
        // resource for the .exe icon. Symbolic identifiers like IDI_ICON1
        // would be treated as string IDs and ignored by Explorer.
        let rc_contents = format!(
            "#include <winuser.h>\r\n\r\n1 24 \"manifest.xml\"\r\n1 ICON \"{}\"\r\n",
            ico_path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace('\\', "\\\\"),
        );
        fs::write(&rc_path, rc_contents).expect("write app.rc");

        // embed-resource 3.x returns a CompilationResult that must be
        // consumed. The shipped exe relies on the embedded icon resource
        // (see CLAUDE.md: Explorer picks the lowest numeric ICON ID), so
        // an unattempted or failed compile must abort the build.
        embed_resource::compile(&rc_path, embed_resource::NONE)
            .manifest_required()
            .expect("embed-resource: failed to compile app.rc");
    }
}

fn generate_ico(png_path: &std::path::Path, ico_path: &std::path::Path) {
    let img = image::open(png_path)
        .unwrap_or_else(|e| panic!("failed to open {}: {}", png_path.display(), e));
    let mut icon_dir = ico::IconDir::new(ico::ResourceType::Icon);
    for size in [16u32, 32, 48, 64, 128, 256] {
        let resized = img.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let rgba = resized.to_rgba8();
        let image = ico::IconImage::from_rgba_data(size, size, rgba.into_raw());
        icon_dir.add_entry(
            ico::IconDirEntry::encode(&image).expect("encode ico entry"),
        );
    }
    let mut file = fs::File::create(ico_path).expect("create icon.ico");
    icon_dir.write(&mut file).expect("write icon.ico");
}
