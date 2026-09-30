include!(concat!(env!("OUT_DIR"), "/pstack_tree.rs"));

pub fn tree_directory(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".local/share/ai-mission-manager/pstack")
        .join(PSTACK_TREE_HASH)
}

pub fn skill_snapshot() -> String {
    format!(
        "pstack version {}; upstream commit {}; tree hash {}",
        PSTACK_VERSION, PSTACK_UPSTREAM_COMMIT, PSTACK_TREE_HASH
    )
}
