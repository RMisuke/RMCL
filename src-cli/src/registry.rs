//! Command registry — data-driven metadata for the `help` command, shell
//! completions and REPL tab-completion.
//!
//! The registry is the **extensibility contract**: to add a new command group
//! or subcommand you update [`crate::app`] (parsing) and this file (metadata),
//! plus add a handler module under [`crate::commands`]. A unit test
//! ([`registry_covers_all_clap_commands`](tests)) asserts that every clap
//! variant is documented here, so the two can never drift silently.

/// Description of a top-level command group.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub name: &'static str,
    pub about: &'static str,
    pub subcommands: &'static [SubcommandSpec],
}

/// Description of a subcommand (leaf action).
#[derive(Debug, Clone)]
pub struct SubcommandSpec {
    pub name: &'static str,
    pub about: &'static str,
    /// Short usage hint with example flags.
    pub usage: &'static str,
}

macro_rules! sub {
    ($name:literal, $about:literal, $usage:literal) => {
        SubcommandSpec {
            name: $name,
            about: $about,
            usage: $usage,
        }
    };
}

/// All documented command groups, in help-display order.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "auth",
        about: "Authentication: Microsoft OAuth2 / offline accounts",
        subcommands: &[
            sub!(
                "login",
                "Log in (microsoft device flow or offline)",
                "auth login -t microsoft|offline [--username <name>]"
            ),
            sub!(
                "logout",
                "Log out the active (or --account) account",
                "auth logout [--account <id>]"
            ),
            sub!(
                "refresh",
                "Refresh an expired token",
                "auth refresh [--account <id>]"
            ),
            sub!("status", "Show the active session", "auth status"),
            sub!(
                "list",
                "List persisted accounts (public metadata)",
                "auth list"
            ),
            sub!(
                "select",
                "Select an account as active",
                "auth select --account <id>"
            ),
        ],
    },
    CommandSpec {
        name: "config",
        about: "Launcher settings, runtime state and user preferences",
        subcommands: &[
            sub!(
                "get",
                "Read a settings field or the full document",
                "config get [<key>]"
            ),
            sub!(
                "set",
                "Update a structured settings field",
                "config set <field> <value>"
            ),
            sub!("path", "Show resolved data dir / game root", "config path"),
            sub!(
                "state",
                "Runtime state (loss-tolerant)",
                "config state get|set ..."
            ),
            sub!("user", "User preferences", "config user get|set ..."),
        ],
    },
    CommandSpec {
        name: "profile",
        about: "Game instance (profile) management",
        subcommands: &[
            sub!("list", "List all game instances", "profile list"),
            sub!("get", "Show one instance", "profile get --id <id>"),
            sub!(
                "create",
                "Create a new game instance",
                "profile create -n <name> [-v <mc>] [-l <loader>] [--memory <mb>]"
            ),
            sub!(
                "update",
                "Update instance fields",
                "profile update --id <id> [--name] [--memory] ..."
            ),
            sub!(
                "delete",
                "Delete an instance",
                "profile delete --id <id> [--cleanup]"
            ),
            sub!(
                "select",
                "Set the active instance",
                "profile select --id <id>"
            ),
            sub!("active", "Show the active instance", "profile active"),
        ],
    },
    CommandSpec {
        name: "version",
        about: "Version management (remote manifest + local repository)",
        subcommands: &[
            sub!(
                "list",
                "List remote Minecraft versions",
                "version list [--refresh]"
            ),
            sub!("get", "Show remote version metadata", "version get <id>"),
            sub!(
                "install",
                "Install a version into a profile",
                "version install --profile <id> -v <mc>"
            ),
            sub!(
                "local",
                "Local repository operations",
                "version local list|get|delete ..."
            ),
        ],
    },
    CommandSpec {
        name: "resource",
        about: "Game resource install / check / repair / verify",
        subcommands: &[
            sub!(
                "install",
                "Install game resources for a profile",
                "resource install --profile <id> -v <mc>"
            ),
            sub!(
                "check",
                "Check completeness of installed files",
                "resource check --profile <id>"
            ),
            sub!(
                "repair",
                "Auto-download missing / corrupted files",
                "resource repair --profile <id>"
            ),
            sub!(
                "verify",
                "Verify SHA1 integrity",
                "resource verify --profile <id>"
            ),
            sub!("status", "Download engine status", "resource status"),
        ],
    },
    CommandSpec {
        name: "modloader",
        about: "Mod loader discovery, install and analysis",
        subcommands: &[
            sub!(
                "list",
                "List available loader versions",
                "modloader list -t <loader> -v <mc>"
            ),
            sub!(
                "install",
                "Install a loader into a profile",
                "modloader install -p <profile> -t <loader> -v <version>"
            ),
            sub!(
                "remove",
                "Remove the installed loader",
                "modloader remove -p <profile>"
            ),
            sub!(
                "analyze",
                "Analyze installed loaders in a version",
                "modloader analyze -v <version>"
            ),
        ],
    },
    CommandSpec {
        name: "mod",
        about: "Mod management (Modrinth search / install / local files)",
        subcommands: &[
            sub!(
                "search",
                "Search Modrinth for mods",
                "mod search -q <query> [--game-version] [--loader]"
            ),
            sub!(
                "versions",
                "List versions of a mod",
                "mod versions --id <mod-id>"
            ),
            sub!(
                "install",
                "Install a mod (remote or local file)",
                "mod install --profile <id> (--version-id <vid> | --file <path>)"
            ),
            sub!("list", "List installed mods", "mod list --profile <id>"),
            sub!(
                "remove",
                "Remove an installed mod",
                "mod remove --profile <id> --file <name>"
            ),
            sub!(
                "toggle",
                "Enable / disable a mod",
                "mod toggle --profile <id> --file <name> --enabled <bool>"
            ),
            sub!(
                "download-url",
                "Resolve a version's download URL",
                "mod download-url --version-id <vid>"
            ),
        ],
    },
    CommandSpec {
        name: "plugin",
        about: "Declarative plugins: install / permissions / config (no third-party code)",
        subcommands: &[
            sub!(
                "list",
                "List installed plugins (--all includes removed tombstones)",
                "plugin list [--all]"
            ),
            sub!(
                "inspect",
                "Show manifest, permissions, dependencies and config",
                "plugin inspect --id <plugin-id>"
            ),
            sub!(
                "install",
                "Install a .rmcpkg package (disabled unless --enable)",
                "plugin install --path <file.rmcpkg> [--enable]"
            ),
            sub!(
                "enable",
                "Enable after permission review",
                "plugin enable --id <plugin-id>"
            ),
            sub!(
                "disable",
                "Disable an enabled plugin",
                "plugin disable --id <plugin-id>"
            ),
            sub!(
                "remove",
                "Uninstall (keeps config unless --delete-data)",
                "plugin remove --id <plugin-id> [--delete-data]"
            ),
            sub!(
                "rollback",
                "Roll back to the previous version",
                "plugin rollback --id <plugin-id>"
            ),
            sub!(
                "grant",
                "Grant a declared capability",
                "plugin grant --id <plugin-id> --capability <name>"
            ),
            sub!(
                "revoke",
                "Revoke a granted capability",
                "plugin revoke --id <plugin-id> --capability <name>"
            ),
            sub!(
                "config",
                "Get / set / reset plugin config",
                "plugin config get|set|reset|path --id <plugin-id>"
            ),
            sub!(
                "capabilities",
                "Show the capability catalog",
                "plugin capabilities"
            ),
            sub!(
                "pack",
                "Pack a directory into a .rmcpkg (dev helper)",
                "plugin pack --dir <dir> --out <file.rmcpkg>"
            ),
        ],
    },
    CommandSpec {
        name: "launch",
        about: "Launch games, observe processes, dry-run commands",
        subcommands: &[
            sub!(
                "run",
                "Launch a game (blocks until exit by default)",
                "launch run -p <profile> [--no-wait] [--no-repair]"
            ),
            sub!("status", "List running game processes", "launch status"),
            sub!(
                "stop",
                "Terminate a running game",
                "launch stop -p <profile>"
            ),
            sub!(
                "wait",
                "Block until a game exits",
                "launch wait -p <profile>"
            ),
            sub!("java", "Detect Java installations", "launch java"),
            sub!(
                "command",
                "Dry-run: print the generated command line",
                "launch command -p <profile>"
            ),
        ],
    },
    CommandSpec {
        name: "system",
        about: "Launcher-wide diagnostics",
        subcommands: &[
            sub!("info", "Launcher status snapshot", "system info"),
            sub!("paths", "Resolved paths", "system paths"),
        ],
    },
    CommandSpec {
        name: "mcp",
        about: "MCP stdio bridge: expose the running launcher to external AI tools (ADR-013)",
        subcommands: &[],
    },
    CommandSpec {
        name: "help",
        about: "Show help for a command",
        subcommands: &[],
    },
    CommandSpec {
        name: "completions",
        about: "Generate shell completions",
        subcommands: &[],
    },
];

/// Look up a command group spec by name.
pub fn command(name: &str) -> Option<&'static CommandSpec> {
    COMMANDS.iter().find(|c| c.name == name)
}

/// All top-level command names (help / completion / REPL).
pub fn command_names() -> Vec<&'static str> {
    COMMANDS.iter().map(|c| c.name).collect()
}

/// Subcommand names for a group (REPL completion).
pub fn subcommand_names(group: &str) -> Vec<&'static str> {
    command(group)
        .map(|c| c.subcommands.iter().map(|s| s.name).collect())
        .unwrap_or_default()
}

/// Render a `help <command> [<sub>]` document into a string.
pub fn render_help(path: &[String]) -> String {
    let mut out = String::new();
    if path.is_empty() {
        out.push_str("RMCCC CLI — available commands\n\n");
        for c in COMMANDS {
            out.push_str(&format!("  {:<12} {}\n", c.name, c.about));
            for s in c.subcommands {
                out.push_str(&format!(
                    "    {:<10} {}\n",
                    format!("{} {}", c.name, s.name),
                    s.about
                ));
            }
        }
        out.push_str("\nGlobal options: -d/--data-dir <dir>  -o/--output <auto|plain|json>  --event-stream  -i/--interactive\n");
        out.push_str(
            "\nUse `help <command> [<subcommand>]` for details, or `rmcli <command> --help`.\n",
        );
        return out;
    }

    let group = path[0].as_str();
    match command(group) {
        None => out.push_str(&format!("Unknown command: {group}\n")),
        Some(spec) => {
            out.push_str(&format!("{}\n\n", spec.about));
            if let Some(sub) = path.get(1) {
                if let Some(s) = spec.subcommands.iter().find(|s| s.name == *sub) {
                    out.push_str(&format!("Usage: {}\n\n{}\n", s.usage, s.about));
                } else {
                    out.push_str(&format!(
                        "Unknown subcommand '{sub}' under '{group}'. Subcommands: {}\n",
                        spec.subcommands
                            .iter()
                            .map(|s| s.name)
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                }
            } else {
                for s in spec.subcommands {
                    out.push_str(&format!("  {:<12} {}\n", s.name, s.about));
                    out.push_str(&format!("      {}\n", s.usage));
                }
            }
        }
    }
    out
}

/// Render shell completions for the given shell (simple, stable contract).
pub fn render_completions(shell: &str) -> String {
    let names = command_names();
    let mut out = String::new();
    match shell {
        "bash" => {
            out.push_str("_rmcli_completions() {\n");
            out.push_str("  local cur prev words cword\n");
            out.push_str("  cur=\"${COMP_WORDS[COMP_CWORD]}\"\n");
            out.push_str(&format!(
                "  COMPREPLY=( $(compgen -W \"{} \" -- \"$cur\") )\n",
                names.join(" ")
            ));
            out.push_str("  return 0\n}\n");
            out.push_str("complete -F _rmcli_completions rmcli\n");
        }
        "zsh" => {
            out.push_str("#compdef rmcli\n");
            out.push_str(&format!(
                "_rmcli() {{ _arguments '1:command:({})' }}\n",
                names.join(" ")
            ));
            out.push_str("compdef _rmcli rmcli\n");
        }
        "fish" => {
            for name in &names {
                out.push_str(&format!(
                    "complete -c rmcli -n \"__fish_use_subcommand\" -a '{}'\n",
                    name
                ));
            }
        }
        "powershell" => {
            out.push_str("Register-ArgumentCompleter -Native -CommandName rmcli -ScriptBlock {\n");
            out.push_str("  param($wordToComplete, $commandAst, $cursorPosition)\n");
            out.push_str(&format!(
                "  '{}'.Split(' ') | Where-Object {{ $_ -like \"$wordToComplete*\" }}\n",
                names.join(" ")
            ));
            out.push_str("}\n");
        }
        "elvish" => {
            out.push_str(&format!(
                "set edit:completion:arg-completer[rmcli] = [@words]{{ put {} }}\n",
                names.join(" ")
            ));
        }
        _ => {
            out.push_str(&format!(
                "Unsupported shell '{shell}'. Supported: bash, zsh, fish, powershell, elvish\n"
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry must document every clap top-level command (extensibility
    /// invariant — the two can never silently drift). Verified against the
    /// actual clap `Command` tree.
    #[test]
    fn registry_covers_all_clap_commands() {
        use clap::CommandFactory;
        let cmd = crate::app::Cli::command();
        let clap_names: Vec<String> = cmd
            .get_subcommands()
            .map(|s| s.get_name().to_string())
            .collect();

        for spec in COMMANDS {
            assert!(
                clap_names.iter().any(|n| n == spec.name),
                "registry command '{}' is missing from clap Commands",
                spec.name
            );
        }
        for name in &clap_names {
            assert!(
                command(name).is_some(),
                "clap command '{name}' is missing from registry::COMMANDS"
            );
        }
    }

    #[test]
    fn help_renders_group_and_subcommand() {
        let group = render_help(&["profile".to_string()]);
        assert!(group.contains("create"));
        assert!(group.contains("Game instance"));
        let sub = render_help(&["profile".to_string(), "create".to_string()]);
        assert!(sub.contains("Usage: profile create"));
        let unknown = render_help(&["nope".to_string()]);
        assert!(unknown.contains("Unknown command"));
    }

    #[test]
    fn completions_cover_all_groups() {
        for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
            let out = render_completions(shell);
            assert!(!out.is_empty());
            for name in ["auth", "profile", "launch"] {
                assert!(out.contains(name), "completions for {shell} miss {name}");
            }
        }
    }
}
