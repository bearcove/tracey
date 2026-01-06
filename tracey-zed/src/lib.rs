use zed_extension_api::{self as zed, LanguageServerId, Result};

struct TraceyExtension;

impl zed::Extension for TraceyExtension {
    fn new() -> Self {
        TraceyExtension
    }

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        // For development, use hardcoded path
        // TODO: In production, look for tracey in PATH or download it
        let binary_path = "/Users/amos/bearcove/tracey/target/release/tracey-lsp-proto";

        Ok(zed::Command {
            command: binary_path.to_string(),
            args: vec![],
            env: Default::default(),
        })
    }
}

zed::register_extension!(TraceyExtension);
