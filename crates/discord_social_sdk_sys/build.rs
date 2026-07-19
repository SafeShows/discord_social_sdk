use std::env;
use std::path::{Path, PathBuf};

/// Locate the SDK, or report that it is absent.
///
/// `DISCORD_SOCIAL_SDK_DIR` wins; otherwise walk up from the crate root looking
/// for a directory holding `include/cdiscord.h`.
///
/// Absence is not automatically fatal. Generating bindings needs only the header,
/// and a documentation build never links — see [`main`] for how those are split.
fn find_sdk() -> Option<PathBuf> {
    if let Ok(dir) = env::var("DISCORD_SOCIAL_SDK_DIR") {
        let dir = PathBuf::from(dir);
        assert!(
            dir.join("include/cdiscord.h").exists(),
            "DISCORD_SOCIAL_SDK_DIR={} does not contain include/cdiscord.h",
            dir.display()
        );
        return Some(dir);
    }

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let mut cur: &Path = &manifest;
    loop {
        let candidate = cur.join("discord_social_sdk");
        if candidate.join("include/cdiscord.h").exists() {
            return Some(candidate);
        }
        cur = cur.parent()?;
    }
}

/// A copy of `cdiscord.h` checked into this crate, if one has been vendored.
///
/// This exists for environments that can build documentation but cannot supply
/// the SDK — docs.rs above all, which has neither the SDK nor network access.
/// Bindings need only the header; the libraries matter solely to linking.
///
/// Vendoring is opt-in and never automatic: the header is Discord's, and whether
/// it may be redistributed inside a published crate is a licensing decision for
/// whoever publishes. See the crate README.
fn vendored_header() -> Option<PathBuf> {
    let path = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("vendor")
        .join("cdiscord.h");
    path.exists().then_some(path)
}

/// Whether this build only has to produce documentation.
///
/// docs.rs sets `DOCS_RS`. Nothing is linked or run in that build, so the SDK
/// libraries are not needed — only the header, to generate the bindings that
/// `cargo doc` type-checks against.
fn docs_only() -> bool {
    env::var_os("DOCS_RS").is_some()
}

fn missing_sdk_panic(what: &str) -> ! {
    panic!(
        "{}",
        concat!(
            "could not find the Discord Social SDK ({what}).\n",
            "\n",
            "The SDK is a prebuilt package distributed by Discord and is not vendored ",
            "into this crate, so it must be supplied out of band. Download it from ",
            "https://discord.com/developers/applications, then either:\n",
            "\n",
            "  - place it at <workspace>/discord_social_sdk, or\n",
            "  - set DISCORD_SOCIAL_SDK_DIR to its absolute path.\n",
            "\n",
            "The directory search only finds the SDK inside your own workspace. When ",
            "this crate is built from the registry, or from the extracted copy that ",
            "`cargo publish` verifies, it lies outside your workspace and the search ",
            "cannot reach it, so DISCORD_SOCIAL_SDK_DIR (absolute, not relative) is ",
            "required there."
        )
        .replace("{what}", what)
    )
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
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=vendor/cdiscord.h");
    println!("cargo:rerun-if-env-changed=DISCORD_SOCIAL_SDK_DIR");
    println!("cargo:rerun-if-env-changed=DOCS_RS");

    let sdk = find_sdk();

    // Bindings need a header from somewhere; prefer the real SDK, fall back to a
    // vendored copy. Linking needs the full SDK and is skipped when only
    // documenting, which is what lets docs.rs succeed with the header alone.
    let include_dir = match (&sdk, vendored_header()) {
        (Some(sdk), _) => sdk.join("include"),
        (None, Some(header)) => header
            .parent()
            .expect("vendored header always has a parent")
            .to_path_buf(),
        (None, None) => missing_sdk_panic(if docs_only() {
            "documenting without the SDK requires a vendored header at vendor/cdiscord.h"
        } else {
            "no SDK directory and no vendored header"
        }),
    };

    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("cdiscord.h").display()
    );

    match sdk {
        Some(sdk) => {
            let profile = sdk_profile(&sdk);
            // Downstream crates and integration tests use these to stage the runtime library.
            println!("cargo:root={}", sdk.display());
            println!("cargo:profile={}", profile);

            if docs_only() {
                // Deliberately skip linking even though the SDK is present: a docs
                // build produces no binary to link, and emitting link flags would
                // only risk failing on a platform whose libraries are absent.
                println!("cargo:warning=DOCS_RS set; skipping link configuration");
            } else {
                let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
                emit_link_flags(&sdk, profile, &target_os);
            }
        }
        None => {
            // Header-only build. Fine for `cargo doc`; anything that links will fail
            // at link time with an unresolved-symbol error, so say so plainly now.
            assert!(
                docs_only(),
                concat!(
                    "found a vendored header at vendor/cdiscord.h but no SDK.\n",
                    "\n",
                    "That is enough to generate bindings and build documentation, but ",
                    "not enough to link. Set DISCORD_SOCIAL_SDK_DIR to an absolute SDK ",
                    "path to build anything runnable."
                )
            );
            println!("cargo:warning=building documentation against the vendored header; not linking");
        }
    }

    generate_bindings(&include_dir);
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
