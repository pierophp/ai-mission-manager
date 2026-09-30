use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .expect("read pstack tree")
        .map(|entry| entry.expect("read pstack entry").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        let metadata = fs::symlink_metadata(&path).expect("stat pstack entry");
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_files(root, &path, files);
        } else if metadata.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
            files.push(
                path.strip_prefix(root)
                    .expect("pstack-relative path")
                    .to_path_buf(),
            );
        }
    }
}

fn main() {
    tauri_build::build();

    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let tree_root = manifest
        .join("../agents/pstack")
        .canonicalize()
        .expect("pstack tree");
    let patches = fs::read_to_string(tree_root.join("PATCHES.md")).expect("pstack patches");
    let upstream = patches
        .lines()
        .find(|line| line.starts_with("Upstream source:"))
        .expect("pstack upstream metadata");
    let version = upstream
        .split("version `")
        .nth(1)
        .and_then(|value| value.split('`').next())
        .expect("pstack version");
    let commit = upstream
        .split("commit `")
        .nth(1)
        .and_then(|value| value.split('`').next())
        .expect("pstack commit");

    let mut files = Vec::new();
    collect_files(&tree_root, &tree_root, &mut files);
    files.sort();
    let mut hash = Sha256::new();
    for relative in &files {
        let name = relative.to_string_lossy().replace('\\', "/");
        let bytes = fs::read(tree_root.join(relative)).expect("read pstack file");
        hash.update(name.as_bytes());
        hash.update([0]);
        hash.update((bytes.len() as u64).to_be_bytes());
        hash.update(bytes);
        #[cfg(unix)]
        hash.update([u8::from(
            fs::metadata(tree_root.join(relative))
                .expect("stat pstack file")
                .permissions()
                .mode()
                & 0o111
                != 0,
        )]);
        #[cfg(not(unix))]
        hash.update([0]);
    }
    let tree_hash = format!("{:x}", hash.finalize());

    let mut generated = format!("pub const PSTACK_VERSION: &str = {version:?};\npub const PSTACK_UPSTREAM_COMMIT: &str = {commit:?};\npub const PSTACK_TREE_HASH: &str = {tree_hash:?};\npub const PSTACK_TREE: &[(&str, &[u8], bool)] = &[\n");
    for relative in files {
        let name = relative.to_string_lossy().replace('\\', "/");
        let absolute = tree_root.join(relative);
        #[cfg(unix)]
        let executable = fs::metadata(&absolute)
            .expect("stat pstack file")
            .permissions()
            .mode()
            & 0o111
            != 0;
        #[cfg(not(unix))]
        let executable = false;
        generated.push_str(&format!(
            "    ({name:?}, include_bytes!({:?}) as &[u8], {executable}),\n",
            absolute.to_string_lossy()
        ));
    }
    generated.push_str("];\n");
    let out = PathBuf::from(env::var("OUT_DIR").expect("output directory"));
    fs::write(out.join("pstack_tree.rs"), generated).expect("write embedded pstack tree");
}
