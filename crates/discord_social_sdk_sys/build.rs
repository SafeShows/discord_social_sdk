use std::env;
use std::path::{Path, PathBuf};

/// Locate the vendored SDK. `DISCORD_SOCIAL_SDK_DIR` wins; otherwise walk up from
/// the crate root looking for a directory that holds `include/cdiscord.h`.
fn find_sdk() -> PathBuf {
    if let Ok(dir) = env::var("DISCORD_SOCIAL_SDK_DIR") {
        let dir = PathBuf::from(dir);
        assert!(
            dir.join("include/cdiscord.h").exists(),
            "DISCORD_SOCIAL_SDK_DIR={} does not contain include/cdiscord.h",
            dir.display()
        );
        return dir;
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut cur: &Path = &manifest;
    loop {
        let candidate = cur.join("discord_social_sdk");
        if candidate.join("include/cdiscord.h").exists() {
            return candidate;
        }
        match cur.parent() {
            Some(p) => cur = p,
            None => panic!(
                "could not find the Discord Social SDK. Download it from \
                 https://discord.com/developers/applications and either place it at \
                 <workspace>/discord_social_sdk or set DISCORD_SOCIAL_SDK_DIR."
            ),
        }
    }
}

/// The SDK ships `debug` and `release` trees. Match the Cargo profile, but fall
/// back to release when a debug tree is absent.
fn sdk_profile(sdk: &Path) -> &'static str {
    let debug = env::var("DEBUG").map(|d| d != "false").unwrap_or(false);
    let opt_level = env::var("OPT_LEVEL").unwrap_or_else(|_| "0".into());
    if debug && opt_level == "0" && sdk.join("lib/debug").exists() {
        "debug"
    } else {
        "release"
    }
}

fn main() {
    let sdk = find_sdk();
    let profile = sdk_profile(&sdk);
    let include = sdk.join("include");
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=DISCORD_SOCIAL_SDK_DIR");
    println!("cargo:rerun-if-changed={}", include.join("cdiscord.h").display());
    // Downstream crates (and integration tests) need these to stage the runtime library.
    println!("cargo:root={}", sdk.display());
    println!("cargo:profile={}", profile);

    emit_link_flags(&sdk, profile, &target_os);
    generate_bindings(&include);
}

fn emit_link_flags(sdk: &Path, profile: &str, target_os: &str) {
    let lib_dir = sdk.join("lib").join(profile);
    let bin_dir = sdk.join("bin").join(profile);
    let krisp = env::var("CARGO_FEATURE_KRISP").is_ok();

    match target_os {
        // Windows: link the import library; the DLL lives in bin/<profile> and must sit
        // next to the final executable at runtime.
        "windows" => {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            println!("cargo:rustc-link-lib=dylib=discord_partner_sdk");
            stage_runtime_files(&bin_dir, &["discord_partner_sdk.dll"]);
            if krisp {
                stage_runtime_files(&bin_dir, &["discord_krisp.dll"]);
                stage_kef_models(&bin_dir);
            }
        }

        // Linux: shared object lives in lib/<profile>. $ORIGIN lets the built binary
        // find it after the .so is staged alongside it.
        "linux" | "android" => {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
            println!("cargo:rustc-link-lib=dylib=discord_partner_sdk");
            println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
            stage_runtime_files(&lib_dir, &["libdiscord_partner_sdk.so"]);
            if krisp {
                stage_kef_models(&lib_dir);
            }
        }

        // macOS/iOS: the SDK ships both a flat dylib and an .xcframework. Prefer the
        // flat dylib when present since it needs no slice selection.
        "macos" | "ios" => {
            let flat = lib_dir.join("libdiscord_partner_sdk.dylib");
            if flat.exists() {
                println!("cargo:rustc-link-search=native={}", lib_dir.display());
                stage_runtime_files(&lib_dir, &["libdiscord_partner_sdk.dylib"]);
                if krisp {
                    stage_runtime_files(&lib_dir, &["libdiscord_krisp.dylib"]);
                    stage_kef_models(&lib_dir);
                }
            } else {
                let fw = lib_dir.join("discord_partner_sdk.xcframework");
                let slice = xcframework_slice(&fw, target_os);
                println!("cargo:rustc-link-search=framework={}", slice.display());
            }
            println!("cargo:rustc-link-lib=dylib=discord_partner_sdk");
            println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path");
            println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
        }

        other => panic!(
            "the Discord Social SDK does not ship binaries for target_os = {other:?}. \
             Supported: windows, linux, macos, ios, android."
        ),
    }
}

/// Pick the architecture slice inside an `.xcframework` matching the build target.
fn xcframework_slice(fw: &Path, target_os: &str) -> PathBuf {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let wanted_arch = match arch.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x86_64",
        other => other,
    };
    let entries = std::fs::read_dir(fw)
        .unwrap_or_else(|e| panic!("cannot read xcframework at {}: {e}", fw.display()));

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        let platform_ok = match target_os {
            "ios" => name.starts_with("ios-"),
            _ => name.starts_with("macos-"),
        };
        if platform_ok && name.contains(wanted_arch) {
            return entry.path();
        }
    }
    panic!(
        "no slice in {} matches target_os={target_os} arch={wanted_arch}",
        fw.display()
    );
}

/// Copy runtime artifacts next to the built binary so `cargo run`/`cargo test`
/// work without the user manually placing shared libraries.
fn stage_runtime_files(src_dir: &Path, files: &[&str]) {
    for out in output_dirs() {
        for file in files {
            let src = src_dir.join(file);
            if !src.exists() {
                println!("cargo:warning=missing SDK runtime file {}", src.display());
                continue;
            }
            let dst = out.join(file);
            // Copying over a loaded DLL fails; treat an existing identical file as fine.
            if let Err(e) = std::fs::copy(&src, &dst)
                && !dst.exists()
            {
                println!("cargo:warning=failed to stage {}: {e}", src.display());
            }
        }
    }
}

/// Krisp loads its `.kef` model files from the working directory at runtime.
fn stage_kef_models(src_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(src_dir) else {
        return;
    };
    let models: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "kef"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let refs: Vec<&str> = models.iter().map(String::as_str).collect();
    stage_runtime_files(src_dir, &refs);
}

/// `OUT_DIR` is `target/<profile>/build/<pkg>-<hash>/out`; the binaries land in
/// `target/<profile>` and `target/<profile>/deps`.
fn output_dirs() -> Vec<PathBuf> {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let Some(profile_dir) = out.ancestors().nth(3) else {
        return Vec::new();
    };
    let mut dirs = vec![profile_dir.to_path_buf()];
    let deps = profile_dir.join("deps");
    if deps.exists() {
        dirs.push(deps);
    }
    dirs
}

fn generate_bindings(include: &Path) {
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("bindings.rs");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include.display()))
        // Only bind the SDK's own surface, not libc types pulled in by the header.
        .allowlist_item("Discord_.*")
        .allowlist_item("DISCORD_.*")
        // `Discord_Foo_forceint = 0x7FFFFFFF` padding members exist only to fix the
        // C enum width; newtypes keep them harmless instead of becoming Rust variants
        // that make matches non-exhaustive.
        .default_enum_style(bindgen::EnumVariation::NewType {
            is_bitfield: false,
            is_global: false,
        })
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        .derive_eq(true)
        .derive_hash(true)
        // Holds raw function pointers; comparing/hashing those is meaningless and
        // trips `unpredictable_function_pointer_comparisons`.
        .no_hash("Discord_Allocator")
        .no_partialeq("Discord_Allocator")
        .prepend_enum_name(false)
        .generate_comments(false)
        .layout_tests(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("failed to generate bindings for cdiscord.h");

    bindings.write_to_file(&out).expect("failed to write bindings.rs");
}
